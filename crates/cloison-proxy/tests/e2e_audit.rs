//! Tests e2e du mode audit (STACK-4) : `cloison-proxy` en `CLOISON_AUDIT_MODE=1`
//! contre un LLM **mock** (echo) axum in-process.
//!
//! Scénarios :
//! - I-A1 : le texte passe **non masqué** (aller vers l'amont et réponse au
//!   client, non-stream **et** stream) ;
//! - reçu : généré, signé (Ed25519 vérifiable) par requête, header
//!   `X-Cloison-Audit-Receipt`, **aucun texte** dans le reçu ;
//! - rapport : `GET /v1/audit/report` respecte le k-anonymat (cellules < k
//!   redactées, `publishable` cohérent) ;
//! - mode audit désactivé : la route rapport répond 404.

use std::convert::Infallible;
use std::net::SocketAddr;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use axum::body::Body;
use axum::extract::Request;
use axum::http::{header, StatusCode};
use axum::response::sse::{Event, Sse};
use axum::response::{IntoResponse, Response};
use axum::routing::post;
use axum::{Json, Router};
use futures_util::Stream;
use http_body_util::BodyExt;
use serde_json::{json, Value};
use tower::ServiceExt;
use url::Url;

use cloison_audit::ed25519_dalek::{SigningKey, VerifyingKey};
use cloison_audit::receipt::Receipt;
use cloison_audit::report::ConformanceReport;
use cloison_proxy::config::{Config, DetectConfig, StreamConfig, UpstreamConfig};
use cloison_proxy::handlers::AppState;
use cloison_proxy::routes::router;

/// Graine de test de la clé Ed25519 de l'agent (fichier écrit dans un tempdir).
const AGENT_SEED: [u8; 32] = [7u8; 32];
/// Clé locataire de test (déterministe).
const TEST_TENANT_KEY: [u8; 32] = [0x42; 32];
/// Sel de session de test.
const TEST_SESSION_SALT: [u8; 16] = [0x24; 16];
const TEST_ACCESS_TOKEN: &str = "mn_testtoken";
const TEST_UPSTREAM_KEY: &str = "sk-test.key.with.dots";

/// Contenu PII de test : 1 email + 1 téléphone + 1 IP (quasi-identifiant).
const PII_TEXT: &str = "Contactez user@example.com et +221 77 123 45 67 et IP 192.168.1.10";

// ---------------------------------------------------------------------------
// Mock upstream (echo)
// ---------------------------------------------------------------------------

struct MockUpstream {
    addr: SocketAddr,
    bodies: Arc<Mutex<Vec<Value>>>,
}

impl MockUpstream {
    async fn start() -> Self {
        let bodies = Arc::new(Mutex::new(Vec::new()));
        let router = mock_router(bodies.clone());
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        tokio::spawn(async move {
            axum::serve(listener, router).await.unwrap();
        });
        Self { addr, bodies }
    }

    fn url(&self) -> String {
        format!("http://{}", self.addr)
    }

    fn body_count(&self) -> usize {
        self.bodies.lock().unwrap().len()
    }

    fn last_body(&self) -> Value {
        self.bodies
            .lock()
            .unwrap()
            .last()
            .cloned()
            .unwrap_or(Value::Null)
    }
}

fn mock_router(bodies: Arc<Mutex<Vec<Value>>>) -> Router {
    let chat_bodies = bodies.clone();
    Router::new()
        .route(
            "/v1/chat/completions",
            post(move |req: Request| mock_chat(req, chat_bodies.clone())),
        )
        .route(
            "/v1/completions",
            post(move |req: Request| mock_completions(req, bodies.clone())),
        )
}

async fn mock_chat(req: Request, bodies: Arc<Mutex<Vec<Value>>>) -> Response {
    let body_bytes = axum::body::to_bytes(req.into_body(), 1024 * 1024)
        .await
        .unwrap();
    let body: Value = serde_json::from_slice(&body_bytes).unwrap_or(Value::Null);
    bodies.lock().unwrap().push(body.clone());

    let stream = body
        .get("stream")
        .and_then(|s| s.as_bool())
        .unwrap_or(false);
    if stream {
        mock_chat_stream(&body).into_response()
    } else {
        mock_chat_echo(&body).into_response()
    }
}

fn last_content(body: &Value) -> String {
    body.get("messages")
        .and_then(|m| m.as_array())
        .and_then(|arr| {
            arr.iter().rev().find_map(|m| {
                m.get("content")
                    .and_then(|c| c.as_str())
                    .map(str::to_string)
            })
        })
        .unwrap_or_default()
}

fn mock_chat_echo(body: &Value) -> Json<Value> {
    let model = body.get("model").cloned().unwrap_or(json!("mock-echo"));
    Json(json!({
        "id": "chatcmpl-audit-1",
        "object": "chat.completion",
        "created": 1700000000,
        "model": model,
        "choices": [{
            "index": 0,
            "message": {"role": "assistant", "content": last_content(body)},
            "finish_reason": "stop",
        }],
        "usage": {"prompt_tokens": 7, "completion_tokens": 5, "total_tokens": 12},
    }))
}

fn mock_chat_stream(body: &Value) -> Sse<impl Stream<Item = Result<Event, Infallible>>> {
    let model = body.get("model").cloned().unwrap_or(json!("mock-echo"));
    let content = last_content(body);
    // Découpe arbitraire : vérifie que le forward stream est byte-identique.
    let lens = [5, 4, 6, 5, 4, 6, 5, 4, 6, 5, 4, 6, 5, 4, 6, 5, 4, 6];
    let mut chunks = Vec::new();
    let mut start = 0;
    for &l in &lens {
        if start >= content.len() {
            break;
        }
        let mut end = (start + l).min(content.len());
        while !content.is_char_boundary(end) {
            end -= 1;
        }
        if end <= start {
            break;
        }
        chunks.push(content[start..end].to_string());
        start = end;
    }
    if start < content.len() {
        chunks.push(content[start..].to_string());
    }

    let created = 1700000000u64;
    let stream = async_stream::stream! {
        for chunk in chunks {
            let payload = json!({
                "id": "chatcmpl-audit-stream",
                "object": "chat.completion.chunk",
                "created": created,
                "model": model,
                "choices": [{"index": 0, "delta": {"content": chunk}, "finish_reason": null}],
            });
            yield Ok::<Event, Infallible>(Event::default().data(payload.to_string()));
        }
        let final_payload = json!({
            "id": "chatcmpl-audit-stream",
            "object": "chat.completion.chunk",
            "created": created,
            "model": model,
            "choices": [{"index": 0, "delta": {}, "finish_reason": "stop"}],
        });
        yield Ok::<Event, Infallible>(Event::default().data(final_payload.to_string()));
        yield Ok::<Event, Infallible>(Event::default().data("[DONE]"));
    };
    Sse::new(stream)
}

async fn mock_completions(req: Request, bodies: Arc<Mutex<Vec<Value>>>) -> Response {
    let body_bytes = axum::body::to_bytes(req.into_body(), 1024 * 1024)
        .await
        .unwrap();
    let body: Value = serde_json::from_slice(&body_bytes).unwrap_or(Value::Null);
    bodies.lock().unwrap().push(body.clone());
    let prompt = body
        .get("prompt")
        .and_then(|p| p.as_str())
        .map(str::to_string)
        .unwrap_or_default();
    let model = body.get("model").cloned().unwrap_or(json!("mock-echo"));
    Json(json!({
        "id": "cmpl-audit-1",
        "object": "text_completion",
        "created": 1700000000,
        "model": model,
        "choices": [{"index": 0, "text": prompt, "finish_reason": "stop"}],
        "usage": {"prompt_tokens": 3, "completion_tokens": 3, "total_tokens": 6},
    }))
    .into_response()
}

// ---------------------------------------------------------------------------
// Helpers proxy
// ---------------------------------------------------------------------------

fn audit_config(mock_url: &str, seed_path: Option<&std::path::Path>) -> Config {
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
        audit_mode: true,
        audit_keys: seed_path.map(|p| p.to_path_buf()),
        audit_k: 5,
        audit_ledger_file: None,
        // B.1 : pas de sidecar detect dans ces tests — détection embarquée seule.
        detect: DetectConfig::default(),
        // C : pas de wiring contrôle dans ces tests — auth locale + audit local.
        control: cloison_proxy::config::ControlConfig::default(),
    }
}

async fn proxy_app(mock_url: &str, seed_path: Option<&std::path::Path>) -> (Arc<AppState>, Router) {
    let config = audit_config(mock_url, seed_path);
    let state = Arc::new(AppState::new(&config).expect("AppState::new"));
    let app = router(state.clone());
    (state, app)
}

async fn send_json(
    app: &Router,
    method: &str,
    uri: &str,
    auth: Option<&str>,
    body: Option<Value>,
) -> (StatusCode, axum::http::HeaderMap, String) {
    let mut builder = Request::builder()
        .method(method)
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

fn good_auth() -> String {
    format!("{TEST_ACCESS_TOKEN}.{TEST_UPSTREAM_KEY}")
}

/// Décode le header reçu et vérifie la signature.
fn assert_receipt_verified(headers: &axum::http::HeaderMap, vk: &VerifyingKey) -> Receipt {
    let encoded = headers
        .get("x-cloison-audit-receipt")
        .expect("X-Cloison-Audit-Receipt header present")
        .to_str()
        .unwrap();
    let receipt = Receipt::from_base64url_json(encoded).expect("receipt decodes from base64url");
    assert!(
        receipt.verify(vk),
        "receipt signature must verify with the agent public key"
    );
    receipt
}

/// Concatène les `delta.content` d'un corps SSE et vérifie la terminaison.
fn sse_content_deltas(resp_body: &str) -> (String, bool) {
    let mut content = String::new();
    let mut done = false;
    for event in resp_body.split("\n\n") {
        for line in event.lines() {
            if let Some(data) = line.strip_prefix("data: ") {
                if data == "[DONE]" {
                    done = true;
                } else if let Ok(v) = serde_json::from_str::<Value>(data) {
                    if let Some(d) = v["choices"][0]["delta"]["content"].as_str() {
                        content.push_str(d);
                    }
                }
            }
        }
    }
    (content, done)
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

/// I-A1 non-stream : texte non masqué partout (aller amont + réponse client),
/// reçu signé par requête, compteurs corrects, aucun texte dans le reçu ;
/// le rapport final respecte le k-anonymat (k=5, toutes les cellules ≥ k).
#[tokio::test]
async fn audit_mode_passes_text_unmasked_and_signs_receipts() {
    let dir = tempfile::tempdir().unwrap();
    let seed_path = dir.path().join("audit_agent.seed");
    std::fs::write(&seed_path, AGENT_SEED).unwrap();
    let vk: VerifyingKey = SigningKey::from_bytes(&AGENT_SEED).verifying_key();

    let mock = MockUpstream::start().await;
    let (state, app) = proxy_app(&mock.url(), Some(&seed_path)).await;
    assert!(
        state.audit.is_some(),
        "audit engine must be present in audit mode"
    );

    let body = json!({
        "model": "mock-echo",
        "messages": [{"role": "user", "content": PII_TEXT}]
    });

    for i in 0..6 {
        let (status, headers, resp_body) = send_json(
            &app,
            "POST",
            "/v1/chat/completions",
            Some(&good_auth()),
            Some(body.clone()),
        )
        .await;
        assert_eq!(status, StatusCode::OK, "request {i}");

        // Réponse au client : texte NON masqué, aucune sentinelle.
        let resp: Value = serde_json::from_str(&resp_body).unwrap();
        let content = resp["choices"][0]["message"]["content"].as_str().unwrap();
        assert_eq!(
            content, PII_TEXT,
            "observe-only: text must reach the client unchanged"
        );
        assert!(
            !content.contains('\u{27E6}'),
            "no sentinel may appear in audit mode"
        );
        assert!(!content.contains('\u{27E7}'));

        // Reçu : présent, signé, compteurs non nuls, aucun texte.
        let receipt = assert_receipt_verified(&headers, &vk);
        assert_eq!(receipt.tenant_id, "default");
        assert_eq!(receipt.engine_version, env!("CARGO_PKG_VERSION"));
        assert_eq!(receipt.sig_agent.len(), 64);
        assert!(
            receipt
                .counters
                .masked_by_type
                .get("Email")
                .copied()
                .unwrap_or(0)
                >= 1,
            "email must be counted"
        );
        assert!(
            receipt
                .counters
                .masked_by_type
                .get("PhoneSn")
                .copied()
                .unwrap_or(0)
                >= 1,
            "phone must be counted"
        );
        assert!(
            receipt.counters.quasi_id_flags >= 1,
            "IP (quasi-identifier) must be flagged"
        );
        let receipt_json = serde_json::to_string(&receipt).unwrap();
        assert!(
            !receipt_json.contains("user@example.com"),
            "receipt must never contain PII text"
        );
        assert!(!receipt_json.contains("+221 77 123 45 67"));
        assert!(!receipt_json.contains("192.168.1.10"));
    }

    // L'amont a reçu le corps **tel quel** (texte clair, aucune sentinelle).
    assert_eq!(mock.body_count(), 6);
    let upstream = mock.last_body().to_string();
    assert!(
        upstream.contains("user@example.com"),
        "observe-only: clear text forwarded upstream"
    );
    assert!(upstream.contains("+221 77 123 45 67"));
    assert!(
        !upstream.contains('\u{27E6}'),
        "no tokenization in audit mode"
    );

    // Rapport de conformité : agrégats k-anonymes.
    let (status, _headers, resp_body) = send_json(
        &app,
        "GET",
        "/v1/audit/report?period=daily",
        Some(&good_auth()),
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    let report: ConformanceReport = serde_json::from_str(&resp_body).unwrap();
    assert_eq!(report.total_requests, 6);
    assert!(report.publishable, "all cells >= k=5 -> publishable");
    // P0-1 : le rapport servi n'expose QUE `redacted` (+ métadonnées) —
    // jamais les compteurs bruts `aggregated` (le champ désérialisé est vide).
    assert!(
        report.aggregated.masked_by_type.is_empty(),
        "aggregated is internal: the served JSON must not carry it"
    );
    assert_eq!(report.redacted.get("Email"), Some(&6));
    assert_eq!(report.redacted.get("PhoneSn"), Some(&6));
    assert!(report.period_start <= report.period_end);
    let report_json = serde_json::to_string(&report).unwrap();
    assert!(
        !report_json.contains("user@example.com"),
        "report must never contain PII text"
    );
    assert!(
        !report_json.contains("aggregated"),
        "P0-1: raw aggregated counters must never be serialized"
    );
    // P0-3 : le rapport servi est signé par la clé de l'agent au bord.
    assert!(
        report.verify_signature(&vk),
        "served report must carry a verifiable Ed25519 signature"
    );

    // Période invalide → 400.
    let (status, _, _) = send_json(
        &app,
        "GET",
        "/v1/audit/report?period=monthly",
        Some(&good_auth()),
        None,
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
}

/// K-anonymat réel : une cellule < k est redactée à zéro, le rapport n'est
/// pas publiable globalement.
#[tokio::test]
async fn audit_report_redacts_cells_below_k() {
    let dir = tempfile::tempdir().unwrap();
    let seed_path = dir.path().join("audit_agent.seed");
    std::fs::write(&seed_path, AGENT_SEED).unwrap();
    let mock = MockUpstream::start().await;
    let (_state, app) = proxy_app(&mock.url(), Some(&seed_path)).await;

    // CNI Luhn-valide 1234567890128 (13 chiffres commençant par 1, placé en fin de contenu
    // pour que le span CniSn ne soit pas déplacé par le span carte bancaire)
    // → 1 occurrence < k=5.
    let body = json!({
        "model": "mock-echo",
        "messages": [{"role": "user", "content": "email user@example.com et CNI: 1234567890128"}]
    });
    let (status, headers, _) = send_json(
        &app,
        "POST",
        "/v1/chat/completions",
        Some(&good_auth()),
        Some(body),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    let receipt = assert_receipt_verified(
        &headers,
        &SigningKey::from_bytes(&AGENT_SEED).verifying_key(),
    );
    assert_eq!(receipt.counters.masked_by_type.get("CniSn"), Some(&1));
    assert_eq!(receipt.counters.masked_by_type.get("Email"), Some(&1));

    let (status, _, resp_body) =
        send_json(&app, "GET", "/v1/audit/report", Some(&good_auth()), None).await;
    assert_eq!(status, StatusCode::OK);
    let report: ConformanceReport = serde_json::from_str(&resp_body).unwrap();
    assert_eq!(report.total_requests, 1);
    assert!(
        !report.publishable,
        "cells < k must block global publication"
    );
    assert_eq!(
        report.redacted.get("CniSn"),
        Some(&0),
        "CniSn=1 < k=5 must be redacted"
    );
    assert_eq!(
        report.redacted.get("Email"),
        Some(&0),
        "Email=1 < k=5 must be redacted"
    );
}

/// I-A1 stream : le flux SSE est transmis **à l'identique** (contenu non
/// masqué, aucune sentinelle) et un reçu signé est accumulé à la clôture.
#[tokio::test]
async fn audit_stream_forward_is_identical_and_records_receipt() {
    let dir = tempfile::tempdir().unwrap();
    let seed_path = dir.path().join("audit_agent.seed");
    std::fs::write(&seed_path, AGENT_SEED).unwrap();
    let mock = MockUpstream::start().await;
    let (state, app) = proxy_app(&mock.url(), Some(&seed_path)).await;

    let body = json!({
        "model": "mock-echo",
        "stream": true,
        "messages": [{"role": "user", "content": PII_TEXT}]
    });
    let (status, _headers, resp_body) = send_json(
        &app,
        "POST",
        "/v1/chat/completions",
        Some(&good_auth()),
        Some(body),
    )
    .await;
    assert_eq!(
        status,
        StatusCode::OK,
        "stream request should succeed, got body: {resp_body}"
    );
    assert!(resp_body.contains("data: "), "SSE body expected");

    let (content, done) = sse_content_deltas(&resp_body);
    assert!(done, "stream must end with data: [DONE]");
    assert_eq!(
        content, PII_TEXT,
        "streamed content must be byte-identical (unmasked)"
    );
    assert!(!content.contains('\u{27E6}'));
    assert!(!content.contains('\u{27E7}'));

    // Le reçu de la requête stream est signé et accumulé à la clôture.
    let receipts = state.audit.as_ref().unwrap().receipts();
    assert_eq!(receipts.len(), 1, "one receipt per audited request");
    let receipt = &receipts[0];
    assert!(receipt.verify(&SigningKey::from_bytes(&AGENT_SEED).verifying_key()));
    assert!(
        receipt
            .counters
            .masked_by_type
            .get("Email")
            .copied()
            .unwrap_or(0)
            >= 1
    );
}

/// Legacy `/v1/completions` en mode audit : prompt non masqué, reçu signé.
#[tokio::test]
async fn audit_legacy_completions_counts_and_signs() {
    let dir = tempfile::tempdir().unwrap();
    let seed_path = dir.path().join("audit_agent.seed");
    std::fs::write(&seed_path, AGENT_SEED).unwrap();
    let mock = MockUpstream::start().await;
    let (_state, app) = proxy_app(&mock.url(), Some(&seed_path)).await;

    let body = json!({"model": "mock-echo", "prompt": "Contact: user@example.com"});
    let (status, headers, resp_body) = send_json(
        &app,
        "POST",
        "/v1/completions",
        Some(&good_auth()),
        Some(body),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    let resp: Value = serde_json::from_str(&resp_body).unwrap();
    let text = resp["choices"][0]["text"].as_str().unwrap();
    assert!(
        text.contains("user@example.com"),
        "observe-only: prompt echoed unmasked: {text}"
    );
    assert!(!text.contains('\u{27E6}'));

    let receipt = assert_receipt_verified(
        &headers,
        &SigningKey::from_bytes(&AGENT_SEED).verifying_key(),
    );
    assert!(
        receipt
            .counters
            .masked_by_type
            .get("Email")
            .copied()
            .unwrap_or(0)
            >= 1
    );
}

/// Mode audit désactivé : la route `/v1/audit/report` répond 404 et le
/// comportement STACK-3 reste inchangé (masquage actif).
#[tokio::test]
async fn audit_disabled_report_route_returns_404() {
    let mock = MockUpstream::start().await;
    let config = audit_config(&mock.url(), None);
    // Config sans audit → le proxy construit l'état sans moteur d'audit.
    let mut cfg = config;
    cfg.audit_mode = false;
    let state = Arc::new(AppState::new(&cfg).unwrap());
    assert!(state.audit.is_none());
    let app = router(state);

    let (status, _headers, _body) =
        send_json(&app, "GET", "/v1/audit/report", Some(&good_auth()), None).await;
    assert_eq!(
        status,
        StatusCode::NOT_FOUND,
        "report route must be unavailable without audit mode"
    );

    // Et le masquage STACK-3 fonctionne toujours : le texte clair n'atteint
    // ni l'amont ni le client (sentinelle restaurée en bout de chaîne).
    let body = json!({
        "model": "mock-echo",
        "messages": [{"role": "user", "content": "Contact: user@example.com"}]
    });
    let (status, _headers, resp_body) = send_json(
        &app,
        "POST",
        "/v1/chat/completions",
        Some(&good_auth()),
        Some(body),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    let resp: Value = serde_json::from_str(&resp_body).unwrap();
    let content = resp["choices"][0]["message"]["content"].as_str().unwrap();
    assert!(
        content.contains("user@example.com"),
        "STACK-3 restores the original value"
    );
    assert!(!content.contains('\u{27E6}'));
    // L'amont, lui, n'a jamais vu le clair.
    assert!(
        !mock.last_body().to_string().contains("user@example.com"),
        "masking still active upstream"
    );
    assert!(
        mock.last_body().to_string().contains('\u{27E6}'),
        "sentinel still sent upstream"
    );
}

// ---------------------------------------------------------------------------
// Dette STACK-4 réglée : persistance JSONL 0600 + `period` filtrant.
// ---------------------------------------------------------------------------

/// La persistance des reçus (JSONL 0600) survit au restart et reste
/// vérifiable hors-ligne (signature Ed25519 intacte après rechargement).
#[tokio::test]
async fn audit_ledger_persists_across_restart() {
    use cloison_audit::receipt::Counters;
    use cloison_core::Policy;
    use cloison_proxy::engine::AuditEngine;

    let dir = tempfile::tempdir().unwrap();
    let ledger_path = dir.path().join("audit-ledger.jsonl");
    let policy = Policy::default();
    let verify_key: VerifyingKey;

    {
        let engine = AuditEngine::new(&policy, None, 5, Some(&ledger_path)).unwrap();
        for i in 0..2u64 {
            let mut c = Counters::default();
            *c.masked_by_type.entry("MAIL".to_string()).or_insert(0) += 1;
            let unsigned = engine.build_receipt(
                policy.tenant_id.clone(),
                format!("session-{i}"),
                1_700_000_000 + i,
                c,
            );
            engine.record(engine.sign(&unsigned)).unwrap();
        }
        verify_key = engine.verifying_key();
        assert_eq!(engine.receipts_len(), 2);
    }

    // Fichier append-only en 0600 (jamais de monde lisible).
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mode = std::fs::metadata(&ledger_path)
            .unwrap()
            .permissions()
            .mode()
            & 0o777;
        assert_eq!(mode, 0o600, "audit ledger file must be 0600");
    }

    // « Redémarrage » : nouveau moteur, même fichier → rechargé + vérifiable.
    let engine2 = AuditEngine::new(&policy, None, 5, Some(&ledger_path)).unwrap();
    assert_eq!(engine2.receipts_len(), 2, "receipts must survive a restart");
    for r in engine2.receipts() {
        assert!(r.verify(&verify_key), "reloaded receipt must still verify");
    }

    // L'append continue après rechargement.
    let mut c = Counters::default();
    *c.masked_by_type.entry("TEL".to_string()).or_insert(0) += 2;
    let unsigned = engine2.build_receipt(
        policy.tenant_id.clone(),
        "session-2".to_string(),
        1_700_000_010,
        c,
    );
    engine2.record(engine2.sign(&unsigned)).unwrap();
    assert_eq!(engine2.receipts_len(), 3);

    // Une ligne corrompue est ignorée (warn), pas un crash.
    use std::io::Write;
    std::fs::OpenOptions::new()
        .append(true)
        .open(&ledger_path)
        .unwrap()
        .write_all(b"{corrompu}\n")
        .unwrap();
    let engine3 = AuditEngine::new(&policy, None, 5, Some(&ledger_path)).unwrap();
    assert_eq!(
        engine3.receipts_len(),
        3,
        "corrupt line skipped, valid receipts kept"
    );
}

/// `period` est désormais filtrant : hourly/daily/weekly/all bornent la
/// fenêtre du rapport k-anonyme (dette STACK-4).
#[tokio::test]
async fn audit_report_period_filters_receipts() {
    use cloison_audit::receipt::{now_unix, Counters};
    use cloison_core::Policy;
    use cloison_proxy::engine::AuditEngine;

    let policy = Policy::default();
    let engine = AuditEngine::new(&policy, None, 5, None).unwrap();
    let now = now_unix();
    let mut c = Counters::default();
    *c.masked_by_type.entry("MAIL".to_string()).or_insert(0) += 1;

    // 1 reçu ancien (il y a 2 h) + 1 reçu récent (il y a 1 min).
    let old = engine.sign(&engine.build_receipt(
        policy.tenant_id.clone(),
        "old".to_string(),
        now - 7200,
        c.clone(),
    ));
    let fresh = engine.sign(&engine.build_receipt(
        policy.tenant_id.clone(),
        "fresh".to_string(),
        now - 60,
        c,
    ));
    engine.record(old).unwrap();
    engine.record(fresh).unwrap();

    let hourly = engine.report_for("hourly").unwrap();
    assert_eq!(
        hourly.total_requests, 1,
        "hourly keeps only receipts of the last hour"
    );
    assert!(hourly.sig_report.is_some(), "report stays signed");
    assert!(hourly.period_start <= now.saturating_sub(3600));

    let daily = engine.report_for("daily").unwrap();
    assert_eq!(daily.total_requests, 2, "daily keeps both receipts");
    let weekly = engine.report_for("weekly").unwrap();
    assert_eq!(weekly.total_requests, 2);
    let all = engine.report_for("all").unwrap();
    assert_eq!(all.total_requests, 2);

    // Période inconnue → erreur explicite (fail-loud, jamais un rapport vide).
    assert!(engine.report_for("decade").is_err());
}
