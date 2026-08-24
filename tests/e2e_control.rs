//! Tests e2e du wiring edge → plan de contrôle (chantier C).
//!
//! Scénarios :
//! - **auth par hash** : jeton valide → 200 ; inconnu → 401 ; contrôle
//!   injoignable + cache froid → 401 (fail-closed) ; décision fraîche en cache
//!   → 200 pendant la panne (TTL) ;
//! - **purge sur rotation** : `poll_version` (long-poll) détecte la montée de
//!   `tokens_version` et purge le cache — le jeton révoqué redevient 401 ;
//! - **ingest automatique** : les reçus d'audit (compteurs uniquement) sont
//!   flusher vers `POST /v1/control/ingest` ; le curseur avance ; un second
//!   flush ne renvoie rien ; aucune PII dans le corps transmis ;
//! - **session_ref haché stable par jeton** (dette réglée) : même clé
//!   composite → même `session_ref_hashed` dans les reçus ; clé différente →
//!   hash différent ; jamais le clair du jeton dans le reçu.

use std::collections::HashSet;
use std::net::SocketAddr;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use axum::body::Body;
use axum::extract::{Query, Request, State};
use axum::http::{header, StatusCode};
use axum::routing::{get, post};
use axum::{Json, Router};
use http_body_util::BodyExt;
use serde_json::{json, Value};
use tower::ServiceExt;
use url::Url;

use cloison_audit::receipt::Receipt;
use cloison_proxy::config::{Config, ControlConfig, DetectConfig, StreamConfig, UpstreamConfig};
use cloison_proxy::control::{flush_pending_audit, token_hash, ControlClient};
use cloison_proxy::handlers::AppState;
use cloison_proxy::routes::router;

const TEST_TENANT_KEY: [u8; 32] = [0x42; 32];
const TEST_SESSION_SALT: [u8; 16] = [0x24; 16];
const TEST_TENANT: &str = "tenant-a";
const VALID_TOKEN: &str = "mn_validtoken";
const REVOKED_TOKEN: &str = "mn_revokedtoken";
const TEST_UPSTREAM_KEY: &str = "sk-test.key.with.dots";
/// Contenu PII de test (synthétique) : 1 email + 1 téléphone + 1 IP.
const PII_TEXT: &str = "Contactez user@example.com et +221 77 123 45 67 et IP 192.168.1.10";

// ---------------------------------------------------------------------------
// Mock du plan de contrôle (verify / version / ingest, pilotable)
// ---------------------------------------------------------------------------

#[derive(Default)]
struct MockControlState {
    valid_hashes: Mutex<HashSet<String>>,
    version: Mutex<u64>,
    ingests: Mutex<Vec<Value>>,
    /// `true` = le contrôle répond 500 (panne simulée).
    fail_all: Mutex<bool>,
}

struct MockControl {
    addr: SocketAddr,
    state: Arc<MockControlState>,
}

impl MockControl {
    async fn start() -> Self {
        let state = Arc::new(MockControlState::default());
        let router = mock_control_router(state.clone());
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        tokio::spawn(async move {
            axum::serve(listener, router).await.unwrap();
        });
        Self { addr, state }
    }

    fn url(&self) -> String {
        format!("http://{}", self.addr)
    }

    fn allow(&self, token: &str) {
        self.state
            .valid_hashes
            .lock()
            .unwrap()
            .insert(token_hash(token));
    }

    fn revoke(&self, token: &str) {
        self.state
            .valid_hashes
            .lock()
            .unwrap()
            .remove(&token_hash(token));
        *self.state.version.lock().unwrap() += 1;
    }

    fn set_fail_all(&self, fail: bool) {
        *self.state.fail_all.lock().unwrap() = fail;
    }

    fn ingest_count(&self) -> usize {
        self.state.ingests.lock().unwrap().len()
    }

    fn last_ingest(&self) -> Value {
        self.state
            .ingests
            .lock()
            .unwrap()
            .last()
            .cloned()
            .unwrap_or(Value::Null)
    }
}

fn mock_control_router(state: Arc<MockControlState>) -> Router {
    Router::new()
        .route(
            "/v1/control/verify",
            post(move |State(s): State<Arc<MockControlState>>, Json(req): Json<Value>| async move {
                if *s.fail_all.lock().unwrap() {
                    return (
                        StatusCode::INTERNAL_SERVER_ERROR,
                        Json(json!({ "error": "mock control down" })),
                    );
                }
                let hash = req["token_hash"].as_str().unwrap_or_default().to_string();
                let valid = s.valid_hashes.lock().unwrap().contains(&hash);
                let version = *s.version.lock().unwrap();
                (
                    StatusCode::OK,
                    Json(json!({ "tenant_id": req["tenant_id"], "valid": valid, "version": version })),
                )
            }),
        )
        .route(
            "/v1/control/version",
            get(move |State(s): State<Arc<MockControlState>>, Query(q): Query<Value>| async move {
                if *s.fail_all.lock().unwrap() {
                    return (
                        StatusCode::INTERNAL_SERVER_ERROR,
                        Json(json!({ "error": "mock control down" })),
                    );
                }
                let version = *s.version.lock().unwrap();
                (
                    StatusCode::OK,
                    Json(json!({ "tenant_id": q["tenant_id"], "version": version })),
                )
            }),
        )
        .route(
            "/v1/control/ingest",
            post(move |State(s): State<Arc<MockControlState>>, Json(req): Json<Value>| async move {
                if *s.fail_all.lock().unwrap() {
                    return (
                        StatusCode::INTERNAL_SERVER_ERROR,
                        Json(json!({ "error": "mock control down" })),
                    );
                }
                let mut ingests = s.ingests.lock().unwrap();
                ingests.push(req.clone());
                let seq = ingests.len() as u64;
                (
                    StatusCode::OK,
                    Json(json!({ "seq": seq, "root_hash": "mock-root" })),
                )
            }),
        )
        .with_state(state)
}

// ---------------------------------------------------------------------------
// Mock upstream (echo) — minimal
// ---------------------------------------------------------------------------

async fn mock_upstream() -> String {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move {
        let app = Router::new().route(
            "/v1/chat/completions",
            post(move |req: Request| async move {
                let body_bytes = axum::body::to_bytes(req.into_body(), 1024 * 1024)
                    .await
                    .unwrap();
                let body: Value = serde_json::from_slice(&body_bytes).unwrap_or(Value::Null);
                let content = body["messages"]
                    .as_array()
                    .and_then(|arr| arr.last())
                    .and_then(|m| m["content"].as_str())
                    .unwrap_or_default()
                    .to_string();
                Json(json!({
                    "id": "chatcmpl-c-1",
                    "object": "chat.completion",
                    "created": 1700000000,
                    "model": "mock-echo",
                    "choices": [{
                        "index": 0,
                        "message": {"role": "assistant", "content": content},
                        "finish_reason": "stop",
                    }],
                    "usage": {"prompt_tokens": 3, "completion_tokens": 3, "total_tokens": 6},
                }))
            }),
        );
        axum::serve(listener, app).await.unwrap();
    });
    format!("http://{addr}")
}

// ---------------------------------------------------------------------------
// Helpers proxy
// ---------------------------------------------------------------------------

fn proxy_config(mock_url: &str, control_url: &str, audit_mode: bool) -> Config {
    Config {
        listen_addr: "127.0.0.1:0".parse().unwrap(),
        upstream: UpstreamConfig {
            base_url: Url::parse(mock_url).unwrap(),
            chat_completions_path: "/v1/chat/completions".to_string(),
            completions_path: "/v1/completions".to_string(),
            models_path: "/v1/models".to_string(),
            connect_timeout: Duration::from_secs(2),
            request_timeout: Duration::from_secs(10),
            max_body_bytes: 1024 * 1024,
        },
        stream: StreamConfig {
            max_token_len: 64,
            neutral_marker: "[REDACTED]".to_string(),
            keep_alive: Duration::from_secs(30),
        },
        expected_access_token: None,
        tenant_key: TEST_TENANT_KEY,
        session_salt: TEST_SESSION_SALT,
        mock_mode: true,
        audit_mode,
        audit_keys: None,
        audit_k: 5,
        audit_ledger_file: None,
        detect: DetectConfig::default(),
        control: ControlConfig {
            url: Some(Url::parse(control_url).unwrap()),
            ingest_interval: Duration::from_secs(60),
            poll_interval: Duration::from_secs(30),
            verify_cache_ttl: Duration::from_secs(300),
            tenant_id: TEST_TENANT.to_string(),
        },
        vault: cloison_proxy::config::N0VaultConfig::default(),
    }
}

async fn proxy_app(mock_url: &str, control_url: &str, audit_mode: bool) -> (Arc<AppState>, Router) {
    let config = proxy_config(mock_url, control_url, audit_mode);
    let state = Arc::new(AppState::new(&config).expect("AppState::new"));
    let app = router(state.clone());
    (state, app)
}

async fn send_json(
    app: &Router,
    uri: &str,
    auth: Option<&str>,
    body: Option<Value>,
) -> (StatusCode, axum::http::HeaderMap, String) {
    let mut builder = Request::builder()
        .method("POST")
        .uri(uri)
        .header(header::CONTENT_TYPE, "application/json");
    if let Some(a) = auth {
        builder = builder.header(header::AUTHORIZATION, format!("Bearer {a}"));
    }
    let req = builder
        .body(Body::from(body.map(|v| v.to_string()).unwrap_or_default()))
        .unwrap();
    let resp = app.clone().oneshot(req).await.unwrap();
    let status = resp.status();
    let headers = resp.headers().clone();
    let bytes = resp.into_body().collect().await.unwrap().to_bytes();
    (status, headers, String::from_utf8_lossy(&bytes).to_string())
}

fn good_auth(token: &str) -> String {
    format!("{token}.{TEST_UPSTREAM_KEY}")
}

fn chat_body() -> Value {
    json!({
        "model": "mock-echo",
        "messages": [{"role": "user", "content": PII_TEXT}]
    })
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

/// Auth par hash (wiring C) : jeton émis par le contrôle → 200 ; inconnu → 401.
#[tokio::test]
async fn auth_resolves_tokens_via_control_hash() {
    let mock = MockControl::start().await;
    mock.allow(VALID_TOKEN);
    let upstream = mock_upstream().await;
    let (_state, app) = proxy_app(&upstream, &mock.url(), false).await;

    let (status, _, _) = send_json(
        &app,
        "/v1/chat/completions",
        Some(&good_auth(VALID_TOKEN)),
        Some(chat_body()),
    )
    .await;
    assert_eq!(
        status,
        StatusCode::OK,
        "jeton émis par le contrôle → autorisé"
    );

    let (status, _, _) = send_json(
        &app,
        "/v1/chat/completions",
        Some(&good_auth("mn_unknown")),
        Some(chat_body()),
    )
    .await;
    assert_eq!(status, StatusCode::UNAUTHORIZED, "jeton inconnu → 401");
}

/// Fail-closed : contrôle injoignable + cache froid → 401 ; décision fraîche en
/// cache (TTL) → la requête passe pendant la panne.
#[tokio::test]
async fn auth_fails_closed_when_control_down() {
    let mock = MockControl::start().await;
    mock.allow(VALID_TOKEN);
    let upstream = mock_upstream().await;
    let (_state, app) = proxy_app(&upstream, &mock.url(), false).await;

    // Cache froid + panne → 401 (jamais d'acceptation par défaut).
    mock.set_fail_all(true);
    let (status, _, _) = send_json(
        &app,
        "/v1/chat/completions",
        Some(&good_auth(VALID_TOKEN)),
        Some(chat_body()),
    )
    .await;
    assert_eq!(
        status,
        StatusCode::UNAUTHORIZED,
        "cache froid + contrôle down → fail-closed 401"
    );

    // Panne levée → vérification réussie, décision mise en cache.
    mock.set_fail_all(false);
    let (status, _, _) = send_json(
        &app,
        "/v1/chat/completions",
        Some(&good_auth(VALID_TOKEN)),
        Some(chat_body()),
    )
    .await;
    assert_eq!(status, StatusCode::OK);

    // Panne de nouveau → la décision fraîche (TTL 300 s) est honorée.
    mock.set_fail_all(true);
    let (status, _, _) = send_json(
        &app,
        "/v1/chat/completions",
        Some(&good_auth(VALID_TOKEN)),
        Some(chat_body()),
    )
    .await;
    assert_eq!(
        status,
        StatusCode::OK,
        "cache frais → tolérance de panne (TTL)"
    );
}

/// Long-poll des versions : une rotation/révocation incrémente `tokens_version`,
/// `poll_version` purge le cache et le jeton révoqué redevient 401.
#[tokio::test]
async fn version_bump_purges_token_cache() {
    let mock = MockControl::start().await;
    mock.allow(VALID_TOKEN);
    mock.allow(REVOKED_TOKEN);
    let upstream = mock_upstream().await;
    let (state, app) = proxy_app(&upstream, &mock.url(), false).await;
    let verifier = state
        .token_verifier
        .as_ref()
        .expect("verifier présent")
        .clone();

    // Les deux jetons sont valides → 200 (décisions mises en cache).
    let (s1, _, _) = send_json(
        &app,
        "/v1/chat/completions",
        Some(&good_auth(VALID_TOKEN)),
        Some(chat_body()),
    )
    .await;
    assert_eq!(s1, StatusCode::OK);
    let (s2, _, _) = send_json(
        &app,
        "/v1/chat/completions",
        Some(&good_auth(REVOKED_TOKEN)),
        Some(chat_body()),
    )
    .await;
    assert_eq!(s2, StatusCode::OK);
    assert!(verifier.cache_len() >= 2);

    // Révocation (côté contrôle) : la version monte.
    mock.revoke(REVOKED_TOKEN);

    // Long-poll : la montée de version purge le cache.
    verifier.poll_version().await.expect("poll_version ok");
    assert_eq!(verifier.cache_len(), 0, "cache purgé sur montée de version");

    // Le jeton révoqué → 401 ; le jeton intact → 200.
    let (s3, _, _) = send_json(
        &app,
        "/v1/chat/completions",
        Some(&good_auth(REVOKED_TOKEN)),
        Some(chat_body()),
    )
    .await;
    assert_eq!(s3, StatusCode::UNAUTHORIZED, "révoqué → 401 après purge");
    let (s4, _, _) = send_json(
        &app,
        "/v1/chat/completions",
        Some(&good_auth(VALID_TOKEN)),
        Some(chat_body()),
    )
    .await;
    assert_eq!(s4, StatusCode::OK);
}

/// Ingest automatique : les reçus d'audit (compteurs uniquement, jamais de
/// texte) sont flusher vers `POST /v1/control/ingest` ; le curseur avance ;
/// un second flush ne renvoie rien.
#[tokio::test]
async fn audit_receipts_are_flushed_to_control() {
    let mock = MockControl::start().await;
    mock.allow(VALID_TOKEN);
    let upstream = mock_upstream().await;
    let (state, app) = proxy_app(&upstream, &mock.url(), true).await;
    let audit = state.audit.as_ref().expect("audit engine présent").clone();
    let client = ControlClient::new(&state.control_cfg).expect("control client");

    // 2 requêtes auditées → 2 reçus signés (compteurs uniquement).
    for _ in 0..2 {
        let (status, _, _) = send_json(
            &app,
            "/v1/chat/completions",
            Some(&good_auth(VALID_TOKEN)),
            Some(chat_body()),
        )
        .await;
        assert_eq!(status, StatusCode::OK);
    }
    assert_eq!(audit.receipts_len(), 2);

    // Flush → le contrôle reçoit un lot de 2 reçus.
    let n = flush_pending_audit(&audit, &client, TEST_TENANT, 5)
        .await
        .expect("flush ok");
    assert_eq!(n, 2);
    assert_eq!(mock.ingest_count(), 1);
    let body = mock.last_ingest();
    assert_eq!(body["tenant_id"], TEST_TENANT);
    assert_eq!(body["k"], 5);
    assert!(body["period_start"].as_u64().unwrap() <= body["period_end"].as_u64().unwrap());
    let receipts = body["receipts"].as_array().expect("receipts array");
    assert_eq!(receipts.len(), 2);
    // Aucune PII dans le corps transmis (compteurs uniquement, invariant I9).
    let raw = body.to_string();
    assert!(!raw.contains("user@example.com"), "jamais de texte client");
    assert!(!raw.contains("+221 77 123 45 67"));

    // Curseur avancé : le second flush ne renvoie rien.
    let n2 = flush_pending_audit(&audit, &client, TEST_TENANT, 5)
        .await
        .expect("flush ok");
    assert_eq!(n2, 0);
    assert_eq!(mock.ingest_count(), 1, "aucun doublon");

    // Les reçus restent dans le journal (rapport de conformité intact).
    assert_eq!(audit.receipts_len(), 2);
}

/// session_ref_hashed renforcé : stable par jeton (session réelle), différent
/// entre jetons, jamais le clair (dette produit réglée).
#[tokio::test]
async fn receipt_session_ref_is_stable_per_token() {
    let mock = MockControl::start().await;
    mock.allow(VALID_TOKEN);
    mock.allow(REVOKED_TOKEN);
    let upstream = mock_upstream().await;
    let (state, app) = proxy_app(&upstream, &mock.url(), true).await;

    // Deux requêtes avec le MÊME jeton → même session_ref_hashed.
    for _ in 0..2 {
        let (status, headers, _) = send_json(
            &app,
            "/v1/chat/completions",
            Some(&good_auth(VALID_TOKEN)),
            Some(chat_body()),
        )
        .await;
        assert_eq!(status, StatusCode::OK);
        let encoded = headers
            .get("x-cloison-audit-receipt")
            .expect("receipt header")
            .to_str()
            .unwrap();
        let receipt = Receipt::from_base64url_json(encoded).expect("receipt");
        assert_ne!(
            receipt.session_ref_hashed, VALID_TOKEN,
            "le clair du jeton ne fuit jamais"
        );
        assert_eq!(receipt.session_ref_hashed.len(), 64, "SHA-256 hex");
    }
    let receipts = state.audit.as_ref().unwrap().receipts();
    assert_eq!(receipts.len(), 2);
    assert_eq!(
        receipts[0].session_ref_hashed, receipts[1].session_ref_hashed,
        "même jeton → même session (stabilité de coréférence)"
    );

    // Une requête avec un AUTRE jeton → session différente.
    let (status, _, _) = send_json(
        &app,
        "/v1/chat/completions",
        Some(&good_auth(REVOKED_TOKEN)),
        Some(chat_body()),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    let receipts = state.audit.as_ref().unwrap().receipts();
    assert_eq!(receipts.len(), 3);
    assert_ne!(
        receipts[0].session_ref_hashed, receipts[2].session_ref_hashed,
        "jeton différent → session différente"
    );
    // Les compteurs sont bien là (détection observe-only).
    assert!(
        receipts[0]
            .counters
            .masked_by_type
            .get("Email")
            .copied()
            .unwrap_or(0)
            >= 1
    );
}
