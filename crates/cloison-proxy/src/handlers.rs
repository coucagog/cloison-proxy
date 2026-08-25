//! État global partagé + handlers des 3 routes.
//!
//! Un moteur `cloison-core` **par requête** (`RequestEngine`) : le registre
//! d'émission est exactement le périmètre de la requête en cours (I2), sans
//! verrou global ni purge explicite.

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

use axum::body::{Body, Bytes};
use axum::extract::{Extension, Query, State};
use axum::http::HeaderValue;
use axum::response::{IntoResponse, Response};
use axum::Json;
use cloison_audit::receipt::{self, Counters, Receipt};
use cloison_core::{
    derive_key_from_passphrase, Policy, SessionContext, SessionKeys, SessionOptions, Vault,
    VaultConfig,
};
use futures_util::StreamExt;
use serde_json::Value;
use tokio::sync::Mutex as AsyncMutex;
use zeroize::Zeroizing;

use crate::auth::{AuthenticatedTenant, CompositeKey};
use crate::config::{Config, ControlConfig, StreamConfig};
use crate::control::{flush_pending_audit, ControlClient, TokenVerifier};
use crate::detect::DetectClient;
use crate::engine::{self, AuditEngine, RequestEngine};
use crate::errors::{ErrorKind, ProxyError};
use crate::light_ner::LightNer;
use crate::openai::{ChatCompletionRequest, CompletionRequest, Content, Prompt};
use crate::stream;
use crate::upstream::UpstreamClient;

/// Compteurs atomiques globaux du processus (visibles dans les logs structurés).
#[derive(Debug, Default)]
pub struct Metrics {
    pub requests_total: AtomicU64,
    pub auth_failures: AtomicU64,
    pub upstream_errors: AtomicU64,
    pub unresolved_tokens: AtomicU64,
    /// Jauge quasi-id (N0 v1.1) : requêtes avec densité signalée — compteur
    /// uniquement, jamais de texte (charte §6.1 couche 6).
    pub quasi_id_flags: AtomicU64,
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
    /// Moteur d'audit observe-only (STACK-4) ; `Some` uniquement si
    /// `CLOISON_AUDIT_MODE=1`. Quand il est présent, chaque requête est
    /// **comptée sans être masquée** et produit un reçu signé.
    pub audit: Option<Arc<AuditEngine>>,
    /// Client du sidecar detect (wiring B.1) ; `None` = détection embarquée
    /// seule. Une panne du sidecar dégrade gracieusement (jamais d'erreur).
    pub detect: Option<DetectClient>,
    /// Vérificateur de jetons par hash auprès du contrôle (wiring C) ; `Some`
    /// uniquement si `CLOISON_CONTROL_URL` est configuré. Quand il est présent,
    /// l'auth passe par le contrôle (fail-closed) ; sinon le jeton local
    /// statique `expected_access_token` s'applique (mode N0).
    pub token_verifier: Option<Arc<TokenVerifier>>,
    /// Client du contrôle (ingest des reçus d'audit, wiring C).
    pub control: Option<ControlClient>,
    /// Configuration du wiring contrôle (tenant, intervalles — pour les tâches
    /// de fond et la construction des reçus).
    pub control_cfg: ControlConfig,
    /// Seuil k-anonyme du rapport (porté aussi par l'ingest).
    pub audit_k: usize,
    /// Coffre persistant local partagé (N0) ; `None` hors mode N0. Tous les
    /// `RequestEngine` de la session l'utilisent (clé dérivée de la passphrase,
    /// fail-loud au boot).
    pub vault: Option<Arc<Vault>>,
    /// Session N0 v1.1 : mentions canoniques accumulées (alias intra-session
    /// et jauge quasi-id in-core). Toujours allouée, consultée uniquement en
    /// mode N0 (vault actif) — le serveur garde son comportement historique.
    pub session: Arc<AsyncMutex<SessionContext>>,
    /// Options de session (alias/jauge) — `Some` uniquement en mode N0.
    pub session_options: Option<SessionOptions>,
    /// NER léger embarqué (chantier ④, N0 v1.2) ; `None` = N0 v1 inchangé
    /// (ou modèle non configuré). Dégradation gracieuse au chargement.
    pub light_ner: Option<Arc<LightNer>>,
}

impl AppState {
    /// Construit l'état : `SessionKeys::derive(tenant_key, salt)` + client amont.
    pub fn new(config: &Config) -> Result<Self, ProxyError> {
        let keys = SessionKeys::derive(config.tenant_key, config.session_salt).map_err(|e| {
            ProxyError::new(ErrorKind::Internal, "failed to derive session keys")
                .with_field("detail", e.to_string())
        })?;
        let upstream = UpstreamClient::new(&config.upstream)?;
        // Locataire : la politique porte l'identifiant (`CLOISON_TENANT_ID`) —
        // il alimente les reçus d'audit et le hash de session. En mode N0, la
        // politique N0 s'applique (généralisation des faibles cardinalités
        // explicite — charte §6.1, N0-PREP §4.6).
        let policy = if config.vault.is_active() {
            Policy::n0_for(&config.control.tenant_id)
        } else {
            Policy::default_for(&config.control.tenant_id)
        };
        // N0 — coffre persistant local : clé dérivée de la passphrase (HKDF,
        // jamais persistée), fail-loud au boot (mauvaise passphrase ou coffre
        // corrompu → refus de démarrer, jamais de recréation silencieuse).
        // Chantier ② : la passphrase vient du keychain OS (si configuré) avec
        // repli `CLOISON_VAULT_PASSPHRASE` — jamais persistée en clair.
        let vault = match &config.vault.path {
            Some(path) => {
                let passphrase = crate::passphrase::PassphraseProvider::from_config(&config.vault)
                    .load(config.vault.passphrase.as_ref())?;
                let key = derive_key_from_passphrase(passphrase.as_bytes()).map_err(|e| {
                    ProxyError::new(ErrorKind::Internal, "failed to derive N0 vault key")
                        .with_field("detail", e.to_string())
                })?;
                let vcfg = VaultConfig {
                    db_path: path.clone(),
                    max_db_size: 64 * 1024 * 1024,
                    ttl_secs: config.vault.ttl_secs,
                };
                let v = Vault::open(&vcfg, &key).map_err(|e| {
                    ProxyError::new(ErrorKind::Internal, "failed to open N0 vault (fail-loud)")
                        .with_field("path", path.display().to_string())
                        .with_field("detail", e.to_string())
                })?;
                tracing::info!(
                    path = %path.display(),
                    ttl_s = config.vault.ttl_secs,
                    "mode N0 actif : coffre persistant local (clé dérivée de la passphrase, jamais persistée)"
                );
                Some(Arc::new(v))
            }
            None => None,
        };
        let audit = if config.audit_mode {
            Some(Arc::new(AuditEngine::new(
                &policy,
                config.audit_keys.as_deref(),
                config.audit_k,
                config.audit_ledger_file.as_deref(),
            )?))
        } else {
            None
        };
        let detect = match &config.detect.url {
            Some(_) => {
                let client = DetectClient::new(&config.detect)?;
                tracing::info!(
                    url = %config.detect.url.as_ref().map(|u| u.as_str()).unwrap_or(""),
                    timeout_ms = config.detect.timeout.as_millis(),
                    "wiring edge→detect actif (B.1) — sidecar NER consommé"
                );
                Some(client)
            }
            None => None,
        };
        // N0 v1.2 — chantier ④ : NER léger embarqué (mode N0 uniquement).
        // Dégradation gracieuse : modèle/lib absents → `None` + warn (le
        // daemon reste N0 v1 : gazetteers + alias — jamais d'erreur).
        let light_ner = match (&config.light_ner, config.vault.is_active()) {
            (Some(cfg), true) => match LightNer::try_new(cfg.clone()) {
                Some(ner) => {
                    tracing::info!(
                        model = %cfg.model_path.display(),
                        threshold = cfg.threshold,
                        "NER léger embarqué chargé (chantier ④) — PERSON/LOC in-core"
                    );
                    Some(Arc::new(ner))
                }
                None => None,
            },
            (Some(_), false) => {
                tracing::warn!(
                    "NER léger embarqué configuré mais hors mode N0 (CLOISON_VAULT_PATH absent) — ignoré"
                );
                None
            }
            (None, _) => None,
        };
        // C — wiring edge → contrôle : vérification des jetons + ingest d'audit.
        let (control, token_verifier) = match &config.control.url {
            Some(_) => {
                let client = ControlClient::new(&config.control)?;
                let verifier = TokenVerifier::new(
                    client.clone(),
                    config.control.tenant_id.clone(),
                    config.control.verify_cache_ttl,
                );
                tracing::info!(
                    url = %config.control.url.as_ref().map(|u| u.as_str()).unwrap_or(""),
                    tenant_id = %config.control.tenant_id,
                    ingest_interval_s = config.control.ingest_interval.as_secs(),
                    poll_interval_s = config.control.poll_interval.as_secs(),
                    "wiring edge→contrôle actif (C) — auth par hash, ingest audit, long-poll rotation"
                );
                (Some(client), Some(Arc::new(verifier)))
            }
            None => (None, None),
        };
        Ok(Self {
            keys,
            policy,
            upstream,
            stream_cfg: config.stream.clone(),
            metrics: Metrics::default(),
            expected_access_token: config.expected_access_token.clone(),
            audit,
            detect,
            token_verifier,
            control,
            control_cfg: config.control.clone(),
            audit_k: config.audit_k,
            vault,
            // N0 v1.1 — session (alias + jauge) : allouée au boot, active en
            // mode N0 uniquement (les options sont `None` hors N0 → le
            // serveur ne consulte jamais la session).
            session: Arc::new(AsyncMutex::new(SessionContext::with_max(
                config.session.mentions_max,
            ))),
            session_options: if config.vault.is_active() {
                Some(SessionOptions {
                    enable_alias_expansion: config.session.enable_alias_expansion,
                    enable_quasiid_gauge: config.session.enable_quasiid_gauge,
                    quasiid_threshold: config.session.quasiid_threshold,
                    // N0 v1.2 — chantier ④ : la fusion englobante NER est
                    // active en mode N0 **si le NER léger est configuré**
                    // (sinon aucun span NER embarqué n'existe — no-op).
                    enable_enclosing_ner_fusion: config.light_ner.is_some(),
                })
            } else {
                None
            },
            light_ner,
        })
    }

    /// Démarre les tâches de fond du wiring contrôle (C) :
    /// - flush périodique des reçus d'audit vers `POST /v1/control/ingest` ;
    /// - long-poll de `GET /v1/control/version` (purge du cache de jetons).
    ///
    /// Appelé par `main.rs` uniquement — les tests exercent la logique
    /// synchroniquement (pas de tâches fuyantes).
    pub fn start_background_tasks(&self) {
        // Ingest automatique des reçus d'audit (chaînon manquant audit → transparence).
        if let (Some(audit), Some(control)) = (&self.audit, &self.control) {
            let audit = audit.clone();
            let control = control.clone();
            let tenant_id = self.control_cfg.tenant_id.clone();
            let k = self.audit_k;
            let interval = self.control_cfg.ingest_interval;
            tracing::info!(
                tenant_id = %tenant_id,
                interval_s = interval.as_secs(),
                "tâche de fond : ingest automatique des reçus d'audit"
            );
            tokio::spawn(async move {
                let mut ticker = tokio::time::interval(interval);
                loop {
                    ticker.tick().await;
                    match flush_pending_audit(&audit, &control, &tenant_id, k).await {
                        Ok(0) => {}
                        Ok(n) => tracing::info!(receipts = n, "lot d'audit ingéré"),
                        Err(e) => tracing::warn!(
                            error = %e,
                            "ingest audit échoué (retry au prochain tick — reçu persisté, aucune perte)"
                        ),
                    }
                }
            });
        }
        // Long-poll de la version des jetons (rotation/révocation → purge du cache).
        if let Some(verifier) = &self.token_verifier {
            let verifier = verifier.clone();
            let interval = self.control_cfg.poll_interval;
            tracing::info!(
                interval_s = interval.as_secs(),
                "tâche de fond : long-poll /v1/control/version (rotation des jetons)"
            );
            tokio::spawn(async move {
                let mut ticker = tokio::time::interval(interval);
                loop {
                    ticker.tick().await;
                    if let Err(e) = verifier.poll_version().await {
                        tracing::warn!(
                            error = %e,
                            "long-poll version contrôle échoué (cache intact — fail-closed inchangé)"
                        );
                    }
                }
            });
        }
    }
}

/// Id court de requête (log uniquement, jamais de secret).
fn new_request_id() -> String {
    use rand::Rng;
    const CHARS: &[u8] = b"abcdefghijklmnopqrstuvwxyz0123456789";
    let mut rng = rand::thread_rng();
    (0..8)
        .map(|_| CHARS[rng.gen_range(0..CHARS.len())] as char)
        .collect()
}

/// POST /v1/chat/completions — aiguille stream / non-stream sur `req.stream`.
///
/// Le corps aller est TOUJOURS entièrement tokenisé avant l'appel amont ; la
/// restauration ne touche que les jetons émis par cette requête.
pub async fn chat_completions(
    State(state): State<Arc<AppState>>,
    Extension(key): Extension<CompositeKey>,
    Extension(tenant): Extension<AuthenticatedTenant>,
    body: Bytes,
) -> Result<Response, ProxyError> {
    state.metrics.requests_total.fetch_add(1, Ordering::Relaxed);
    let request_id = new_request_id();

    let mut req: ChatCompletionRequest = serde_json::from_slice(&body).map_err(|e| {
        ProxyError::new(ErrorKind::BadRequest, "invalid request body")
            .with_field("request_id", &request_id)
            .with_field("detail", e.to_string())
    })?;

    // STACK-4 : mode audit observe-only — détecter + compter SANS masquer.
    // Le corps est transmis amont tel quel ; un reçu signé est généré.
    if state.audit.is_some() {
        return audit_chat_completions(state, key, tenant.0, req, request_id).await;
    }

    let mut req_engine = RequestEngine::new(&state.keys, &request_id, state.vault.clone())?;

    // Phase aller : tokenisation complète du corps (registre = cette requête).
    // B.1 : le sidecar detect est consulté (dégradation gracieuse s'il est
    // indisponible — jamais d'erreur).
    // N0 v1.1 : en mode N0, la session (alias intra-session + jauge quasi-id)
    // est verrouillée pour la requête — les mentions canoniques persistent
    // entre les requêtes du daemon (jamais de texte dans les logs).
    let mut session_guard = match &state.session_options {
        Some(_) => Some(state.session.lock().await),
        None => None,
    };
    let flags = engine::tokenize_chat_request(
        &mut req,
        &mut req_engine,
        &state.policy,
        state.detect.as_ref(),
        state.light_ner.as_deref(),
        session_guard.as_deref_mut(),
        state.session_options.as_ref(),
    )
    .await?;
    drop(session_guard);
    if flags.quasi_id_flagged {
        state.metrics.quasi_id_flags.fetch_add(1, Ordering::Relaxed);
        tracing::warn!(
            request_id = %request_id,
            "jauge quasi-id : densité élevée signalée (compteur, jamais de texte)"
        );
    }
    let tokenized = serde_json::to_value(&req).map_err(|e| {
        ProxyError::new(ErrorKind::Internal, "failed to serialize request")
            .with_field("request_id", &request_id)
            .with_field("detail", e.to_string())
    })?;

    if req.stream {
        let req_engine = Arc::new(Mutex::new(req_engine));
        let upstream = match state
            .upstream
            .chat_completions_stream(&key.upstream_key, tokenized)
            .await
        {
            Ok(u) => u,
            Err(e) => {
                state
                    .metrics
                    .upstream_errors
                    .fetch_add(1, Ordering::Relaxed);
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
            return Err(ProxyError::new(
                ErrorKind::Upstream,
                "upstream did not respond with an event stream",
            )
            .with_field("request_id", &request_id));
        }
        Ok(stream::sse_response(upstream, state, request_id, req_engine).into_response())
    } else {
        let upstream = match state
            .upstream
            .chat_completions(&key.upstream_key, tokenized)
            .await
        {
            Ok(u) => u,
            Err(e) => {
                state
                    .metrics
                    .upstream_errors
                    .fetch_add(1, Ordering::Relaxed);
                return Err(e.with_field("request_id", &request_id));
            }
        };
        let restored = engine::restore_chat_response_value(
            upstream,
            &req_engine,
            &state.stream_cfg.neutral_marker,
            &state.metrics,
            &request_id,
        )?;
        Ok(Json(restored).into_response())
    }
}

/// POST /v1/completions (legacy) — non-stream uniquement. Le contrat SSE du
/// legacy est identique à chat/completions ; son implémentation est
/// explicitement hors périmètre, l'erreur est nette, jamais silencieuse.
pub async fn completions_legacy(
    State(state): State<Arc<AppState>>,
    Extension(key): Extension<CompositeKey>,
    Extension(tenant): Extension<AuthenticatedTenant>,
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

    // STACK-4 : mode audit observe-only (legacy, non-stream).
    if state.audit.is_some() {
        return audit_completions_legacy(state, key, tenant.0, req, request_id).await;
    }

    let mut req_engine = RequestEngine::new(&state.keys, &request_id, state.vault.clone())?;
    let mut session_guard = match &state.session_options {
        Some(_) => Some(state.session.lock().await),
        None => None,
    };
    let flags = engine::tokenize_completion_request(
        &mut req,
        &mut req_engine,
        &state.policy,
        state.detect.as_ref(),
        state.light_ner.as_deref(),
        session_guard.as_deref_mut(),
        state.session_options.as_ref(),
    )
    .await?;
    drop(session_guard);
    if flags.quasi_id_flagged {
        state.metrics.quasi_id_flags.fetch_add(1, Ordering::Relaxed);
        tracing::warn!(
            request_id = %request_id,
            "jauge quasi-id : densité élevée signalée (compteur, jamais de texte)"
        );
    }
    let tokenized = serde_json::to_value(&req).map_err(|e| {
        ProxyError::new(ErrorKind::Internal, "failed to serialize request")
            .with_field("request_id", &request_id)
            .with_field("detail", e.to_string())
    })?;

    let mut upstream = match state
        .upstream
        .completions(&key.upstream_key, tokenized)
        .await
    {
        Ok(u) => u,
        Err(e) => {
            state
                .metrics
                .upstream_errors
                .fetch_add(1, Ordering::Relaxed);
            return Err(e.with_field("request_id", &request_id));
        }
    };

    let agg = engine::restore_completion_response(
        &mut upstream,
        &req_engine,
        &state.stream_cfg.neutral_marker,
    )?;
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
            state
                .metrics
                .upstream_errors
                .fetch_add(1, Ordering::Relaxed);
            return Err(e);
        }
    };
    Ok(Json(upstream))
}

// ---------------------------------------------------------------------------
// STACK-4 — mode audit observe-only
// ---------------------------------------------------------------------------

/// Header posé sur la réponse auditée : `base64url(canonical_json(receipt))`.
const AUDIT_RECEIPT_HEADER: &str = "x-cloison-audit-receipt";

/// Compte les PII du corps aller **sans le modifier** (mode audit).
fn audit_count_request(
    req: &ChatCompletionRequest,
    audit: &AuditEngine,
    policy: &Policy,
    counters: &mut Counters,
) {
    for msg in &req.messages {
        if let Some(content) = &msg.content {
            match content {
                Content::Text(s) => audit.count_text(s, policy, counters),
                Content::Parts(parts) => {
                    for part in parts {
                        if part.type_ == "text" {
                            if let Some(text) = &part.text {
                                audit.count_text(text, policy, counters);
                            }
                        }
                    }
                }
            }
        }
        if let Some(calls) = &msg.tool_calls {
            for call in calls {
                audit.count_text(&call.function.arguments, policy, counters);
            }
        }
    }
}

/// Compte les sentinelles de **toutes** les chaînes d'une réponse JSON
/// (mode audit : aucune réécriture, aucun marqueur neutre).
fn audit_count_response(value: &Value, audit: &AuditEngine, counters: &mut Counters) {
    match value {
        Value::String(s) => audit.count_response(s, counters),
        Value::Array(arr) => {
            for v in arr {
                audit_count_response(v, audit, counters);
            }
        }
        Value::Object(map) => {
            for v in map.values() {
                audit_count_response(v, audit, counters);
            }
        }
        _ => {}
    }
}

/// Construit, signe et accumule le reçu de la requête auditée.
///
/// `session_ref` = le jeton d'accès `mn_…` de la clé composite (dette réglée :
/// le reçu référence la **session réelle** — stable entre les requêtes du même
/// client — et non plus le `request_id` éphémère). Il est haché (SHA-256) :
/// le reçu ne révèle ni la session ni sa clé (invariant I2).
fn audit_build_and_record(
    audit: &AuditEngine,
    policy: &Policy,
    tenant_id: &str,
    request_id: &str,
    session_ref: &str,
    counters: Counters,
) -> Receipt {
    // Tenant de la REQUÊTE (multi-tenant, charte §7.2) : le reçu est tagué
    // avec le locataire authentifié (header X-Cloison-Tenant ou défaut) —
    // l'ingest est groupé par tenant et le journal reflète chaque client.
    let session_ref_hashed = receipt::hash_session_ref(tenant_id, session_ref);
    let ts_unix = receipt::now_unix();
    let unsigned = audit.build_receipt(tenant_id.to_string(), session_ref_hashed, ts_unix, counters);
    let signed = audit.sign(&unsigned);
    // Fail-loud : une erreur de persistance est loguée (le reçu reste signé
    // et en mémoire ; seul l'historique disque est perdu pour ce reçu).
    if let Err(e) = audit.record(signed.clone()) {
        tracing::error!(
            request_id = %request_id,
            detail = %e,
            "audit receipt persist failed (in-memory only)"
        );
    }
    tracing::info!(
        request_id = %request_id,
        key_id = %audit.key_id(),
        "audit receipt signed and recorded (counters only, never text)"
    );
    signed
}

/// Pose le header `X-Cloison-Audit-Receipt` sur une réponse.
fn attach_receipt_header(resp: &mut Response, receipt: &Receipt) {
    let value = receipt.to_base64url_json();
    if let Ok(hv) = HeaderValue::from_str(&value) {
        resp.headers_mut().insert(AUDIT_RECEIPT_HEADER, hv);
    }
}

/// Flux observe-only non-stream pour `/v1/chat/completions`.
///
/// 1. compte le corps aller sans le modifier ;
/// 2. forward amont **tel quel** (non tokenisé) ;
/// 3. scanne la réponse (comptage des sentinelles, aucune réécriture) ;
/// 4. reçu signé + accumulation + header `X-Cloison-Audit-Receipt` ;
/// 5. réponse pass-through (texte non masqué).
async fn audit_chat_completions(
    state: Arc<AppState>,
    key: CompositeKey,
    tenant: String,
    req: ChatCompletionRequest,
    request_id: String,
) -> Result<Response, ProxyError> {
    // Stream SSE : forward count-only (corps émis strictement identique).
    if req.stream {
        return audit_chat_stream(state, key, tenant, req, request_id).await;
    }

    let audit = state
        .audit
        .as_ref()
        .expect("audit engine present in audit mode")
        .clone();
    let mut counters = Counters::default();

    // Phase aller : détection + comptage, AUCUNE modification du corps.
    audit_count_request(&req, &audit, &state.policy, &mut counters);

    // Amont : corps NON tokenisé, tel quel.
    let body = serde_json::to_value(&req).map_err(|e| {
        ProxyError::new(ErrorKind::Internal, "failed to serialize request")
            .with_field("request_id", &request_id)
            .with_field("detail", e.to_string())
    })?;
    let upstream = match state
        .upstream
        .chat_completions(&key.upstream_key, body)
        .await
    {
        Ok(u) => u,
        Err(e) => {
            state
                .metrics
                .upstream_errors
                .fetch_add(1, Ordering::Relaxed);
            return Err(e.with_field("request_id", &request_id));
        }
    };

    // Phase retour : scan sentinelles (aucune réécriture).
    audit_count_response(&upstream, &audit, &mut counters);

    let receipt = audit_build_and_record(
        &audit,
        &state.policy,
        &tenant,
        &request_id,
        &key.access_token,
        counters,
    );
    let mut resp = Json(upstream).into_response();
    attach_receipt_header(&mut resp, &receipt);
    Ok(resp)
}

/// Flux observe-only non-stream pour `/v1/completions` (legacy).
async fn audit_completions_legacy(
    state: Arc<AppState>,
    key: CompositeKey,
    tenant: String,
    req: CompletionRequest,
    request_id: String,
) -> Result<Response, ProxyError> {
    let audit = state
        .audit
        .as_ref()
        .expect("audit engine present in audit mode")
        .clone();
    let mut counters = Counters::default();

    match &req.prompt {
        Prompt::Single(s) => audit.count_text(s, &state.policy, &mut counters),
        Prompt::Batch(v) => {
            for s in v {
                audit.count_text(s, &state.policy, &mut counters);
            }
        }
    }

    let body = serde_json::to_value(&req).map_err(|e| {
        ProxyError::new(ErrorKind::Internal, "failed to serialize request")
            .with_field("request_id", &request_id)
            .with_field("detail", e.to_string())
    })?;
    let upstream = match state.upstream.completions(&key.upstream_key, body).await {
        Ok(u) => u,
        Err(e) => {
            state
                .metrics
                .upstream_errors
                .fetch_add(1, Ordering::Relaxed);
            return Err(e.with_field("request_id", &request_id));
        }
    };

    audit_count_response(&upstream, &audit, &mut counters);

    let receipt = audit_build_and_record(
        &audit,
        &state.policy,
        &tenant,
        &request_id,
        &key.access_token,
        counters,
    );
    let mut resp = Json(upstream).into_response();
    attach_receipt_header(&mut resp, &receipt);
    Ok(resp)
}

/// Flux observe-only pour `/v1/chat/completions` **stream** : forward SSE
/// **strictement identique** à l'amont (aucune réécriture), comptage des
/// sentinelles des deltas, reçu signé et accumulé à la clôture du flux.
async fn audit_chat_stream(
    state: Arc<AppState>,
    key: CompositeKey,
    tenant: String,
    req: ChatCompletionRequest,
    request_id: String,
) -> Result<Response, ProxyError> {
    let audit = state
        .audit
        .as_ref()
        .expect("audit engine present in audit mode")
        .clone();
    let mut counters = Counters::default();

    // Phase aller : comptage sans modification.
    audit_count_request(&req, &audit, &state.policy, &mut counters);

    let body = serde_json::to_value(&req).map_err(|e| {
        ProxyError::new(ErrorKind::Internal, "failed to serialize request")
            .with_field("request_id", &request_id)
            .with_field("detail", e.to_string())
    })?;
    let upstream = match state
        .upstream
        .chat_completions_stream(&key.upstream_key, body)
        .await
    {
        Ok(u) => u,
        Err(e) => {
            state
                .metrics
                .upstream_errors
                .fetch_add(1, Ordering::Relaxed);
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
        return Err(ProxyError::new(
            ErrorKind::Upstream,
            "upstream did not respond with an event stream",
        )
        .with_field("request_id", &request_id));
    }

    Ok(audit_count_only_sse(
        upstream,
        state,
        audit,
        counters,
        request_id,
        key.access_token.as_str().to_string(),
        tenant,
    ))
}

/// Forward SSE count-only : les événements amont sont émis **à l'identique**,
/// les sentinelles des deltas sont comptées, le reçu est signé à la clôture.
///
/// (Le header `X-Cloison-Audit-Receipt` n'est posé que sur les réponses
/// non-stream : en stream, les compteurs ne sont complets qu'à la clôture ;
/// le reçu est néanmoins accumulé dans le journal.)
fn audit_count_only_sse(
    upstream: reqwest::Response,
    state: Arc<AppState>,
    audit: Arc<AuditEngine>,
    mut counters: Counters,
    request_id: String,
    session_ref: String,
    tenant: String,
) -> Response {
    let mut bytes = upstream.bytes_stream();
    let body = Body::from_stream(async_stream::stream! {
        let mut frame: Vec<u8> = Vec::new();
        loop {
            match bytes.next().await {
                Some(Ok(chunk)) => {
                    frame.extend_from_slice(&chunk);
                    while let Some((end, sep_len)) = find_event_end(&frame) {
                        let event: Vec<u8> = frame.drain(..end + sep_len).collect();
                        if let Some(payload) = event_data_payload(&event) {
                            if payload != "[DONE]" {
                                if let Ok(v) = serde_json::from_str::<Value>(&payload) {
                                    audit_count_response(&v, &audit, &mut counters);
                                }
                            }
                        }
                        yield Ok::<Bytes, std::convert::Infallible>(Bytes::from(event));
                    }
                }
                Some(Err(e)) => {
                    tracing::warn!(request_id = %request_id, error = %e, "audit stream: upstream read error");
                    break;
                }
                None => break,
            }
        }
        // Événement résiduel sans séparateur final.
        if !frame.is_empty() {
            if let Some(payload) = event_data_payload(&frame) {
                if payload != "[DONE]" {
                    if let Ok(v) = serde_json::from_str::<Value>(&payload) {
                        audit_count_response(&v, &audit, &mut counters);
                    }
                }
            }
            yield Ok::<Bytes, std::convert::Infallible>(Bytes::from(frame));
        }
        // Clôture : reçu signé et accumulé (compteurs uniquement).
        let _receipt = audit_build_and_record(
            &audit,
            &state.policy,
            &tenant,
            &request_id,
            &session_ref,
            counters,
        );
    });
    Response::builder()
        .header("content-type", "text/event-stream")
        .body(body)
        .expect("valid SSE response")
}

/// Trouve la fin du premier événement SSE (`\n\n` ou `\r\n\r\n`).
fn find_event_end(buf: &[u8]) -> Option<(usize, usize)> {
    if let Some(pos) = buf.windows(2).position(|w| w == b"\n\n") {
        return Some((pos, 2));
    }
    if let Some(pos) = buf.windows(4).position(|w| w == b"\r\n\r\n") {
        return Some((pos, 4));
    }
    None
}

/// Extrait la charge utile `data:` d'un événement SSE brut.
fn event_data_payload(event: &[u8]) -> Option<String> {
    let text = String::from_utf8_lossy(event);
    let mut parts = Vec::new();
    for line in text.lines() {
        let line = line.trim_end_matches('\r');
        if let Some(rest) = line.strip_prefix("data:") {
            parts.push(rest.trim_start().to_string());
        }
    }
    if parts.is_empty() {
        None
    } else {
        Some(parts.join("\n"))
    }
}

/// GET /v1/audit/report?period=hourly|daily|weekly|all — rapport de conformité
/// k-anonyme sur le journal accumulé.
///
/// Disponible **uniquement** en mode audit (`CLOISON_AUDIT_MODE=1`) : sinon
/// 404. `period` est validé **et filtrant** (dette STACK-4 réglée) : `hourly`
/// = dernière heure, `daily` = dernières 24 h, `weekly` = 7 jours, `all` =
/// tout le journal (persisté si `CLOISON_AUDIT_LEDGER_FILE` est configuré).
pub async fn audit_report(
    State(state): State<Arc<AppState>>,
    Query(params): Query<std::collections::HashMap<String, String>>,
) -> Result<Json<cloison_audit::report::ConformanceReport>, ProxyError> {
    let Some(audit) = &state.audit else {
        return Err(ProxyError::new(
            ErrorKind::NotFound,
            "audit mode is disabled; /v1/audit/report is not available",
        ));
    };
    let period = params.get("period").map(String::as_str).unwrap_or("all");
    if !matches!(period, "all" | "hourly" | "daily" | "weekly") {
        return Err(ProxyError::new(
            ErrorKind::BadRequest,
            "invalid period; expected hourly|daily|weekly|all",
        )
        .with_field("period", period));
    }
    let report = audit.report_for(period)?;
    tracing::info!(
        period = %period,
        total_requests = report.total_requests,
        publishable = report.publishable,
        "conformance report generated (k-anonymous, counters only)"
    );
    Ok(Json(report))
}
