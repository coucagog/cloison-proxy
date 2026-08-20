//! État global partagé + handlers des 3 routes.
//!
//! Un moteur `cloison-core` **par requête** (`RequestEngine`) : le registre
//! d'émission est exactement le périmètre de la requête en cours (I2), sans
//! verrou global ni purge explicite.

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

use axum::body::Bytes;
use axum::extract::{Extension, State};
use axum::response::{IntoResponse, Response};
use axum::Json;
use cloison_core::{Policy, SessionKeys};
use zeroize::Zeroizing;

use crate::auth::CompositeKey;
use crate::config::{Config, StreamConfig};
use crate::engine::{self, RequestEngine};
use crate::errors::{ErrorKind, ProxyError};
use crate::openai::{ChatCompletionRequest, CompletionRequest};
use crate::stream;
use crate::upstream::UpstreamClient;

/// Compteurs atomiques globaux du processus (visibles dans les logs structurés).
#[derive(Debug, Default)]
pub struct Metrics {
    pub requests_total: AtomicU64,
    pub auth_failures: AtomicU64,
    pub upstream_errors: AtomicU64,
    pub unresolved_tokens: AtomicU64,
}

/// État global partagé.
pub struct AppState {
    /// Clés de session (dérivées une fois par boot → rotation par redémarrage).
    pub keys: SessionKeys,
    /// Politique de détection PII.
    pub policy: Policy,
    /// Client amont.
    pub upstream: UpstreamClient,
    /// Configuration du flux SSE.
    pub stream_cfg: StreamConfig,
    /// Compteurs fail-loud.
    pub metrics: Metrics,
    /// Jeton d'accès attendu (validation à temps constant), si configuré.
    pub expected_access_token: Option<Zeroizing<String>>,
}

impl AppState {
    /// Construit l'état : `SessionKeys::derive(tenant_key, salt)` + client amont.
    pub fn new(config: &Config) -> Result<Self, ProxyError> {
        let keys = SessionKeys::derive(config.tenant_key, config.session_salt)
            .map_err(|e| ProxyError::new(ErrorKind::Internal, "failed to derive session keys").with_field("detail", e.to_string()))?;
        let upstream = UpstreamClient::new(&config.upstream)?;
        Ok(Self {
            keys,
            policy: Policy::default(),
            upstream,
            stream_cfg: config.stream.clone(),
            metrics: Metrics::default(),
            expected_access_token: config.expected_access_token.clone(),
        })
    }
}

/// Id court de requête (log uniquement, jamais de secret).
fn new_request_id() -> String {
    use rand::Rng;
    const CHARS: &[u8] = b"abcdefghijklmnopqrstuvwxyz0123456789";
    let mut rng = rand::thread_rng();
    (0..8).map(|_| CHARS[rng.gen_range(0..CHARS.len())] as char).collect()
}

/// POST /v1/chat/completions — aiguille stream / non-stream sur `req.stream`.
///
/// Le corps aller est TOUJOURS entièrement tokenisé avant l'appel amont ; la
/// restauration ne touche que les jetons émis par cette requête.
pub async fn chat_completions(
    State(state): State<Arc<AppState>>,
    Extension(key): Extension<CompositeKey>,
    body: Bytes,
) -> Result<Response, ProxyError> {
    state.metrics.requests_total.fetch_add(1, Ordering::Relaxed);
    let request_id = new_request_id();

    let mut req: ChatCompletionRequest = serde_json::from_slice(&body).map_err(|e| {
        ProxyError::new(ErrorKind::BadRequest, "invalid request body")
            .with_field("request_id", &request_id)
            .with_field("detail", e.to_string())
    })?;

    let req_engine = Arc::new(Mutex::new(RequestEngine::new(&state.keys, &request_id)?));

    // Phase aller : tokenisation complète du corps (registre = cette requête).
    let tokenized = {
        let mut guard = req_engine.lock().expect("engine mutex poisoned");
        engine::tokenize_chat_request(&mut req, &mut guard, &state.policy)?;
        serde_json::to_value(&req).map_err(|e| {
            ProxyError::new(ErrorKind::Internal, "failed to serialize request")
                .with_field("request_id", &request_id)
                .with_field("detail", e.to_string())
        })?
    };

    if req.stream {
        let upstream = match state.upstream.chat_completions_stream(&key.upstream_key, tokenized).await {
            Ok(u) => u,
            Err(e) => {
                state.metrics.upstream_errors.fetch_add(1, Ordering::Relaxed);
                return Err(e.with_field("request_id", &request_id));
            }
        };
        let content_type = upstream
            .headers()
            .get("content-type")
            .and_then(|v| v.to_str().ok())
            .unwrap_or("")
            .to_ascii_lowercase();
        if !content_type.contains("text/event-stream") {
            return Err(ProxyError::new(ErrorKind::Upstream, "upstream did not respond with an event stream")
                .with_field("request_id", &request_id));
        }
        Ok(stream::sse_response(upstream, state, request_id, req_engine).into_response())
    } else {
        let upstream = match state.upstream.chat_completions(&key.upstream_key, tokenized).await {
            Ok(u) => u,
            Err(e) => {
                state.metrics.upstream_errors.fetch_add(1, Ordering::Relaxed);
                return Err(e.with_field("request_id", &request_id));
            }
        };
        let restored = {
            let guard = req_engine.lock().expect("engine mutex poisoned");
            engine::restore_chat_response_value(upstream, &guard, &state.stream_cfg.neutral_marker, &state.metrics, &request_id)
        }?;
        Ok(Json(restored).into_response())
    }
}

/// POST /v1/completions (legacy) — non-stream uniquement. Le contrat SSE du
/// legacy est identique à chat/completions ; son implémentation est
/// explicitement hors périmètre, l'erreur est nette, jamais silencieuse.
pub async fn completions_legacy(
    State(state): State<Arc<AppState>>,
    Extension(key): Extension<CompositeKey>,
    body: Bytes,
) -> Result<Response, ProxyError> {
    state.metrics.requests_total.fetch_add(1, Ordering::Relaxed);
    let request_id = new_request_id();

    let mut req: CompletionRequest = serde_json::from_slice(&body).map_err(|e| {
        ProxyError::new(ErrorKind::BadRequest, "invalid request body")
            .with_field("request_id", &request_id)
            .with_field("detail", e.to_string())
    })?;

    if req.stream {
        return Err(ProxyError::new(
            ErrorKind::BadRequest,
            "streaming is not supported on the legacy /v1/completions endpoint",
        )
        .with_field("request_id", &request_id));
    }

    let mut req_engine = RequestEngine::new(&state.keys, &request_id)?;
    engine::tokenize_completion_request(&mut req, &mut req_engine, &state.policy)?;
    let tokenized = serde_json::to_value(&req).map_err(|e| {
        ProxyError::new(ErrorKind::Internal, "failed to serialize request")
            .with_field("request_id", &request_id)
            .with_field("detail", e.to_string())
    })?;

    let mut upstream = match state.upstream.completions(&key.upstream_key, tokenized).await {
        Ok(u) => u,
        Err(e) => {
            state.metrics.upstream_errors.fetch_add(1, Ordering::Relaxed);
            return Err(e.with_field("request_id", &request_id));
        }
    };

    let agg = engine::restore_completion_response(&mut upstream, &req_engine, &state.stream_cfg.neutral_marker)?;
    if agg.unresolved > 0 {
        state
            .metrics
            .unresolved_tokens
            .fetch_add(agg.unresolved as u64, Ordering::Relaxed);
        tracing::warn!(
            request_id = %request_id,
            restored = agg.restored,
            unresolved = agg.unresolved,
            "legacy completion restore: fail-loud redaction applied"
        );
    }
    Ok(Json(upstream).into_response())
}

/// GET /v1/models — pass-through (aucune tokenisation) après auth.
pub async fn models(
    State(state): State<Arc<AppState>>,
    Extension(key): Extension<CompositeKey>,
) -> Result<Json<serde_json::Value>, ProxyError> {
    state.metrics.requests_total.fetch_add(1, Ordering::Relaxed);
    let upstream = match state.upstream.models(&key.upstream_key).await {
        Ok(u) => u,
        Err(e) => {
            state.metrics.upstream_errors.fetch_add(1, Ordering::Relaxed);
            return Err(e);
        }
    };
    Ok(Json(upstream))
}
