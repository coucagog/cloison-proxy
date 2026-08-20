//! Tests de bout en bout : `cloison-proxy` contre un LLM **mock** (echo) axum
//! in-process. L'URL amont est pointée vers le mock (`CLOISON_UPSTREAM_BASE_URL`
//! équivalent via `Config`), aucune clé réelle n'est utilisée.
//!
//! Scénarios : roundtrip non-stream, roundtrip stream (sentinelles découpées),
//! sentinelle tronquée à la clôture → marqueur neutre, tool-calls, clé
//! malformée → 401 sans appel amont, jeton forgé → marqueur neutre, legacy,
//! models, erreur amont → 502.

use std::convert::Infallible;
use std::net::SocketAddr;
use std::sync::atomic::Ordering;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use axum::body::Body;
use axum::extract::Request;
use axum::http::{header, StatusCode};
use axum::response::sse::{Event, Sse};
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use axum::{Json, Router};
use futures_util::Stream;
use http_body_util::BodyExt;
use serde_json::{json, Value};
use tower::ServiceExt;
use url::Url;

use cloison_proxy::config::{Config, StreamConfig, UpstreamConfig};
use cloison_proxy::handlers::AppState;
use cloison_proxy::routes::router;

/// Clé locataire de test (déterministe).
const TEST_TENANT_KEY: [u8; 32] = [0x42; 32];
/// Sel de session de test.
const TEST_SESSION_SALT: [u8; 16] = [0x24; 16];
/// Jeton d'accès de test.
const TEST_ACCESS_TOKEN: &str = "mn_testtoken";
/// Clé amont de test — contient des points pour vérifier le découpage.
const TEST_UPSTREAM_KEY: &str = "sk-test.key.with.dots";

// ---------------------------------------------------------------------------
// Mock upstream
// ---------------------------------------------------------------------------

/// Modes de comportement du mock `chat/completions`.
#[derive(Clone)]
enum MockMode {
    /// Non-stream : écho du contenu (tokenisé) dans `choices[0].message.content`
    /// + écho des `tool_calls` de la première message qui en a.
    ChatEcho,
    /// Stream : écho du contenu en chunks de tailles données (découpe les sentinelles).
    ChatStream { chunk_lens: Vec<usize> },
    /// Stream : le dernier chunk se termine au milieu de la sentinelle finale.
    ChatStreamTruncated,
    ChatStreamToolCall,
    /// Non-stream : contenu contenant une sentinelle forgée (hors registre).
    ChatForgedSentinel,
    /// Toujours 500 (test fail-loud amont).
    AlwaysError,
}

/// Serveur mock axum + captures (header Authorization, corps reçus).
struct MockUpstream {
    addr: SocketAddr,
    auth_seen: Arc<Mutex<Vec<String>>>,
    bodies_seen: Arc<Mutex<Vec<Value>>>,
}

impl MockUpstream {
    async fn start(mode: MockMode) -> Self {
        let auth_seen = Arc::new(Mutex::new(Vec::new()));
        let bodies_seen = Arc::new(Mutex::new(Vec::new()));
        let router = mock_router(mode, auth_seen.clone(), bodies_seen.clone());
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        tokio::spawn(async move {
            axum::serve(listener, router).await.unwrap();
        });
        Self {
            addr,
            auth_seen,
            bodies_seen,
        }
    }

    fn url(&self) -> String {
        format!("http://{}", self.addr)
    }

    fn auth_count(&self) -> usize {
        self.auth_seen.lock().unwrap().len()
    }
    fn last_auth(&self) -> String {
        self.auth_seen.lock().unwrap().last().cloned().unwrap_or_default()
    }
    fn body_count(&self) -> usize {
        self.bodies_seen.lock().unwrap().len()
    }
    fn last_body(&self) -> Value {
        self.bodies_seen.lock().unwrap().last().cloned().unwrap_or(Value::Null)
    }
}

fn mock_router(mode: MockMode, auth_seen: Arc<Mutex<Vec<String>>>, bodies_seen: Arc<Mutex<Vec<Value>>>) -> Router {
    Router::new()
        .route(
            "/v1/chat/completions",
            post(move |req: Request| mock_chat(req, mode.clone(), auth_seen.clone(), bodies_seen.clone())),
        )
        .route("/v1/completions", post(mock_completions))
        .route("/v1/models", get(mock_models))
}

async fn mock_chat(
    req: Request,
    mode: MockMode,
    auth_seen: Arc<Mutex<Vec<String>>>,
    bodies_seen: Arc<Mutex<Vec<Value>>>,
) -> Response {
    let auth = req
        .headers()
        .get(header::AUTHORIZATION)
        .and_then(|v| v.to_str().ok())
        .unwrap_or("")
        .to_string();
    auth_seen.lock().unwrap().push(auth);
    let body_bytes = axum::body::to_bytes(req.into_body(), 1024 * 1024).await.unwrap();
    let body: Value = serde_json::from_slice(&body_bytes).unwrap_or(Value::Null);
    bodies_seen.lock().unwrap().push(body.clone());

    match mode {
        MockMode::ChatEcho => chat_echo_with(&body, None).into_response(),
        MockMode::ChatStream { chunk_lens } => chat_stream(&body, &chunk_lens, false).into_response(),
        MockMode::ChatStreamTruncated => chat_stream(&body, &[], true).into_response(),
        MockMode::ChatForgedSentinel => chat_echo_with(&body, Some(&forged_content())).into_response(),
        MockMode::ChatStreamToolCall => chat_stream_toolcall(&body).into_response(),
        MockMode::AlwaysError => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({"error": {"message": "mock failure", "type": "server_error", "code": "mock_error"}})),
        )
            .into_response(),
    }
}

/// Extrait le contenu (chaîne) de la DERNIÈRE message qui en possède un.
fn first_content(body: &Value) -> Option<String> {
    body.get("messages")
        .and_then(|m| m.as_array())
        .and_then(|arr| {
            arr.iter().rev().find_map(|m| {
                m.get("content").and_then(|c| c.as_str()).map(str::to_string)
            })
        })
}

/// Extrait les `tool_calls` de la première message qui en possède.
fn first_tool_calls(body: &Value) -> Option<Value> {
    body.get("messages")
        .and_then(|m| m.as_array())
        .and_then(|arr| {
            arr.iter().find_map(|m| {
                m.get("tool_calls")
                    .and_then(|t| t.as_array())
                    .filter(|a| !a.is_empty())
                    .map(|a| Value::Array(a.clone()))
            })
        })
}

fn chat_echo_with(body: &Value, forced_content: Option<&str>) -> Json<Value> {
    let model = body.get("model").cloned().unwrap_or(json!("mock-echo"));
    let content = forced_content.map(str::to_string).or_else(|| first_content(body));
    let tool_calls = first_tool_calls(body);
    Json(json!({
        "id": "chatcmpl-mock-1",
        "object": "chat.completion",
        "created": 1700000000,
        "model": model,
        "choices": [{
            "index": 0,
            "message": {
                "role": "assistant",
                "content": content,
                "tool_calls": tool_calls,
            },
            "finish_reason": "stop",
        }],
        "usage": {"prompt_tokens": 7, "completion_tokens": 5, "total_tokens": 12},
    }))
}

/// Découpe `content` en chunks des tailles données (ajustées aux frontières de
/// caractères) ; le reste est un dernier chunk.
fn split_chunks(content: &str, lens: &[usize]) -> Vec<String> {
    let mut chunks = Vec::new();
    let mut start = 0;
    for &l in lens {
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
    chunks
}

fn chat_stream(body: &Value, chunk_lens: &[usize], truncated: bool) -> Sse<impl Stream<Item = Result<Event, Infallible>>> {
    let model = body.get("model").cloned().unwrap_or(json!("mock-echo"));
    let content = first_content(body).unwrap_or_default();

    let chunks: Vec<String> = if truncated {
        // Coupe les 3 derniers octets (la `⟧` de la sentinelle finale) : le
        // flux se termine au milieu de la sentinelle → fail-loud à la clôture.
        let mut cut = content.len().saturating_sub(3);
        while !content.is_char_boundary(cut) {
            cut -= 1;
        }
        vec![content[..cut].to_string()]
    } else if chunk_lens.is_empty() {
        vec![content.clone()]
    } else {
        split_chunks(&content, chunk_lens)
    };

    let created = 1700000000u64;
    let stream = async_stream::stream! {
        for chunk in chunks {
            let payload = json!({
                "id": "chatcmpl-mock-stream",
                "object": "chat.completion.chunk",
                "created": created,
                "model": model,
                "choices": [{"index": 0, "delta": {"content": chunk}, "finish_reason": null}],
            });
            yield Ok::<Event, Infallible>(Event::default().data(payload.to_string()));
        }
        let final_payload = json!({
            "id": "chatcmpl-mock-stream",
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

/// Mock : tool_call en streaming — premier chunk id/name avec arguments:"",
/// puis deux chunks d'arguments (l'un contenant une sentinelle restauree).
fn chat_stream_toolcall(body: &Value) -> Sse<impl Stream<Item = Result<Event, Infallible>>> {
    let model = body.get("model").cloned().unwrap_or(json!("mock-echo"));
    let created = 1700000000u64;
    let stream = async_stream::stream! {
        // Chunk 1 : id + name de l'outil, arguments vides (NE doit PAS etre perdu).
        let c1 = json!({
            "id": "chatcmpl-mock-tool",
            "object": "chat.completion.chunk",
            "created": created,
            "model": model,
            "choices": [{"index": 0, "delta": {
                "tool_calls": [{"index": 0, "id": "call_abc123", "type": "function",
                                "function": {"name": "lookup_user", "arguments": ""}}]
            }, "finish_reason": null}],
        });
        yield Ok::<Event, Infallible>(Event::default().data(c1.to_string()));
        // Chunk 2 : debut des arguments (contient une sentinelle a restaurer).
        let c2 = json!({
            "id": "chatcmpl-mock-tool",
            "object": "chat.completion.chunk",
            "created": created,
            "model": model,
            "choices": [{"index": 0, "delta": {
                "tool_calls": [{"index": 0, "function": {"arguments": "{\"user_id\": "}}]
            }, "finish_reason": null}],
        });
        yield Ok::<Event, Infallible>(Event::default().data(c2.to_string()));
        // Chunk 3 : fin des arguments.
        let c3 = json!({
            "id": "chatcmpl-mock-tool",
            "object": "chat.completion.chunk",
            "created": created,
            "model": model,
            "choices": [{"index": 0, "delta": {
                "tool_calls": [{"index": 0, "function": {"arguments": "12345}"}}]
            }, "finish_reason": null}],
        });
        yield Ok::<Event, Infallible>(Event::default().data(c3.to_string()));
        let final_payload = json!({
            "id": "chatcmpl-mock-tool",
            "object": "chat.completion.chunk",
            "created": created,
            "model": model,
            "choices": [{"index": 0, "delta": {}, "finish_reason": "tool_calls"}],
        });
        yield Ok::<Event, Infallible>(Event::default().data(final_payload.to_string()));
        yield Ok::<Event, Infallible>(Event::default().data("[DONE]"));
    };
    Sse::new(stream)
}

/// Sentinelle forgee : shape plausible (⟦ + 26 × 'a' + ·EM⟧) mais hors registre.
fn forged_content() -> String {
    format!("{}aaaaaaaaaaaaaaaaaaaaaaaaaa{}EM{}", '\u{27E6}', '\u{00B7}', '\u{27E7}')
}

async fn mock_completions(req: Request) -> Response {
    let body_bytes = axum::body::to_bytes(req.into_body(), 1024 * 1024).await.unwrap();
    let body: Value = serde_json::from_slice(&body_bytes).unwrap_or(Value::Null);
    let prompt = body.get("prompt").and_then(|p| p.as_str()).map(str::to_string);
    let model = body.get("model").cloned().unwrap_or(json!("mock-echo"));
    Json(json!({
        "id": "cmpl-mock-1",
        "object": "text_completion",
        "created": 1700000000,
        "model": model,
        "choices": [{"index": 0, "text": prompt, "finish_reason": "stop"}],
        "usage": {"prompt_tokens": 3, "completion_tokens": 3, "total_tokens": 6},
    }))
    .into_response()
}

async fn mock_models() -> Json<Value> {
    Json(json!({
        "object": "list",
        "data": [{"id": "mock-echo", "object": "model", "created": 1700000000, "owned_by": "cloison"}],
    }))
}

// ---------------------------------------------------------------------------
// Helpers proxy
// ---------------------------------------------------------------------------

fn test_config(mock_url: &str) -> Config {
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
    }
}

async fn proxy_app(mock_url: &str) -> (Arc<AppState>, Router) {
    let config = test_config(mock_url);
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
) -> (StatusCode, String) {
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
    let bytes = resp.into_body().collect().await.unwrap().to_bytes();
    (status, String::from_utf8_lossy(&bytes).to_string())
}

fn good_auth() -> String {
    format!("{TEST_ACCESS_TOKEN}.{TEST_UPSTREAM_KEY}")
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

/// Non-stream : le mock reçoit un corps TOKENISÉ (aucune valeur claire), la
/// réponse client est restaurée ; la clé amont (avec points) est transmise
/// intacte.
#[tokio::test]
async fn non_stream_roundtrip_restores_pii() {
    let mock = MockUpstream::start(MockMode::ChatEcho).await;
    let (state, app) = proxy_app(&mock.url()).await;

    let body = json!({
        "model": "mock-echo",
        "messages": [
            {"role": "system", "content": "You are a helpful assistant."},
            {"role": "user", "content": "Contact: user@example.com, tel +221 77 123 45 67, ip 192.168.1.10"}
        ]
    });
    let (status, resp_body) = send_json(&app, "POST", "/v1/chat/completions", Some(&good_auth()), Some(body)).await;
    assert_eq!(status, StatusCode::OK);

    let resp: Value = serde_json::from_str(&resp_body).unwrap();
    let content = resp["choices"][0]["message"]["content"].as_str().unwrap();
    assert!(content.contains("user@example.com"), "email restored: {content}");
    assert!(content.contains("+221 77 123 45 67"), "phone restored: {content}");
    assert!(!content.contains('\u{27E6}'), "no sentinel leaked: {content}");
    // L'IP est généralisée par cloison-core (règle basse cardinalité `[IP]`),
    // pas tokenisée : elle ne doit ni fuir en clair ni être restaurée.
    assert!(!content.contains("192.168.1.10"), "clear ip must not leak: {content}");
    assert!(content.contains("[IP]"), "ip generalized upstream: {content}");

    // Clé amont transmise intacte (points conservés).
    assert_eq!(mock.last_auth(), format!("Bearer {TEST_UPSTREAM_KEY}"));

    // Le mock a reçu un corps transformé (sentinelles + généralisation).
    let upstream_text = mock.last_body().to_string();
    assert!(!upstream_text.contains("user@example.com"), "clear email reached upstream");
    assert!(!upstream_text.contains("+221 77 123 45 67"), "clear phone reached upstream");
    assert!(!upstream_text.contains("192.168.1.10"), "clear ip reached upstream");
    assert!(upstream_text.contains('\u{27E6}'), "sentinel present upstream");
    assert_eq!(state.metrics.unresolved_tokens.load(Ordering::Relaxed), 0);
}

/// Tool-calls : `function.arguments` tokenisé à l'aller, restauré au retour,
/// et le JSON des arguments reste valide.
#[tokio::test]
async fn tool_calls_arguments_restored() {
    let mock = MockUpstream::start(MockMode::ChatEcho).await;
    let (_state, app) = proxy_app(&mock.url()).await;

    let body = json!({
        "model": "mock-echo",
        "messages": [
            {"role": "system", "content": "You process contact data."},
            {"role": "assistant", "content": null, "tool_calls": [
                {"id": "call_1", "type": "function", "function": {
                    "name": "lookup_contact",
                    "arguments": "{\"email\": \"user@example.com\", \"tel\": \"+221 77 123 45 67\"}"
                }}
            ]},
            {"role": "tool", "tool_call_id": "call_1", "content": "ok"}
        ]
    });
    let (status, resp_body) = send_json(&app, "POST", "/v1/chat/completions", Some(&good_auth()), Some(body)).await;
    assert_eq!(status, StatusCode::OK);

    let resp: Value = serde_json::from_str(&resp_body).unwrap();
    let args = resp["choices"][0]["message"]["tool_calls"][0]["function"]["arguments"].as_str().unwrap();
    assert!(args.contains("user@example.com"), "email restored in arguments: {args}");
    assert!(args.contains("+221 77 123 45 67"), "phone restored in arguments: {args}");
    assert!(!args.contains('\u{27E6}'), "no sentinel in arguments: {args}");
    // Le JSON des arguments reste syntaxiquement valide après restauration.
    let parsed: Value = serde_json::from_str(args).unwrap();
    assert_eq!(parsed["email"], "user@example.com");
}

/// Stream : le mock découpe volontairement les sentinelles en petits chunks ;
/// la sortie SSE = texte clair complet, aucune sentinelle (même partielle)
/// n'est émise, et le flux se termine par `[DONE]`.
#[tokio::test]
async fn stream_roundtrip_reassembles_split_sentinels() {
    let lens = vec![5, 4, 6, 5, 4, 6, 5, 4, 6, 5, 4, 6, 5, 4, 6, 5, 4, 6, 5, 4, 6, 5, 4];
    let mock = MockUpstream::start(MockMode::ChatStream { chunk_lens: lens }).await;
    let (state, app) = proxy_app(&mock.url()).await;

    let original = "Bonjour, je vous confirme mon email user@example.com et mon telephone +221 77 123 45 67. Merci, cordialement.";
    let body = json!({
        "model": "mock-echo",
        "stream": true,
        "messages": [{"role": "user", "content": original}]
    });
    let (status, resp_body) = send_json(&app, "POST", "/v1/chat/completions", Some(&good_auth()), Some(body)).await;
    assert_eq!(status, StatusCode::OK);
    assert!(resp_body.contains("data: "), "SSE body expected");

    let (content, done) = sse_content_deltas(&resp_body);
    assert!(done, "stream must end with data: [DONE]");
    assert!(!content.contains('\u{27E6}'), "no sentinel leaked to client: {content}");
    assert!(!content.contains('\u{27E7}'), "no sentinel close leaked: {content}");
    assert_eq!(content, original, "reassembled stream must equal the original text");
    assert_eq!(state.metrics.unresolved_tokens.load(Ordering::Relaxed), 0);
}

/// Stream : sentinelle tronquée à la clôture → marqueur neutre `[REDACTED]`,
/// compteur `unresolved` = 1, aucune valeur claire ne fuit.
#[tokio::test]
async fn stream_truncated_sentinel_redacts_at_closure() {
    let mock = MockUpstream::start(MockMode::ChatStreamTruncated).await;
    let (state, app) = proxy_app(&mock.url()).await;

    let original = "Mon email est user@example.com et mon tel +221 77 123 45 67";
    let body = json!({
        "model": "mock-echo",
        "stream": true,
        "messages": [{"role": "user", "content": original}]
    });
    let (status, resp_body) = send_json(&app, "POST", "/v1/chat/completions", Some(&good_auth()), Some(body)).await;
    assert_eq!(status, StatusCode::OK);

    let (content, _done) = sse_content_deltas(&resp_body);
    assert!(content.contains("[REDACTED]"), "neutral marker expected: {content}");
    assert!(!content.contains('\u{27E6}'), "no partial sentinel leaked: {content}");
    assert!(!content.contains("+221 77 123 45 67"), "truncated phone must not leak: {content}");
    assert_eq!(state.metrics.unresolved_tokens.load(Ordering::Relaxed), 1);
}

/// Tool-call en streaming : le premier chunk (id/name, arguments:"") ne doit
/// pas etre perdu, et les arguments restaures doivent arriver au client.
#[tokio::test]
async fn stream_tool_call_first_chunk_preserved_and_args_restored() {
    let mock = MockUpstream::start(MockMode::ChatStreamToolCall).await;
    let (_state, app) = proxy_app(&mock.url()).await;

    let original = "Mon email est user@example.com";
    let body = json!({
        "model": "mock-echo",
        "stream": true,
        "messages": [{"role": "user", "content": original}]
    });
    let (status, resp_body) = send_json(&app, "POST", "/v1/chat/completions", Some(&good_auth()), Some(body)).await;
    assert_eq!(status, StatusCode::OK);
    assert!(resp_body.contains("call_abc123"), "tool call id must reach client: {resp_body}");
    assert!(resp_body.contains("lookup_user"), "tool name must reach client: {resp_body}");
    assert!(resp_body.contains("user_id"), "tool args must reach client: {resp_body}");
    assert!(resp_body.contains("[DONE]"), "stream must end with [DONE]");
}

/// Clés malformées → 401 `invalid_api_key`, AUCUN appel amont.
#[tokio::test]
async fn malformed_keys_rejected_without_upstream_call() {
    let mock = MockUpstream::start(MockMode::ChatEcho).await;
    let (_state, app) = proxy_app(&mock.url()).await;
    let body = json!({"model": "m", "messages": [{"role": "user", "content": "hi"}]});

    let cases: Vec<Option<&str>> = vec![
        None,                  // header absent
        Some("foo"),           // pas de schéma
        Some("Bearer foo"),    // pas de point
        Some("Bearer mn_x"),   // pas de point
        Some("Bearer mn_"),    // jeton vide au-delà du préfixe
        Some("Bearer mn_x."),  // clé amont vide
        Some("Basic mn_x.sk"), // mauvais schéma
        Some("Bearer .sk"),    // jeton vide
    ];
    for case in cases {
        let (status, resp_body) = send_json(&app, "POST", "/v1/chat/completions", case, Some(body.clone())).await;
        assert_eq!(status, StatusCode::UNAUTHORIZED, "case: {case:?}");
        let resp: Value = serde_json::from_str(&resp_body).unwrap_or(Value::Null);
        assert_eq!(resp["error"]["code"], "invalid_api_key", "case: {case:?}");
    }

    assert_eq!(mock.body_count(), 0, "no upstream call must be made");
    assert_eq!(mock.auth_count(), 0, "no upstream auth header must be seen");
}

/// Jeton d'accès attendu configuré : rejet à temps constant du mauvais jeton.
#[tokio::test]
async fn expected_access_token_enforced() {
    let mock = MockUpstream::start(MockMode::ChatEcho).await;
    let mut config = test_config(&mock.url());
    config.expected_access_token = Some(zeroize::Zeroizing::new("mn_goodtoken".to_string()));
    let state = Arc::new(AppState::new(&config).unwrap());
    let app = router(state.clone());
    let body = json!({"model": "m", "messages": [{"role": "user", "content": "hi"}]});

    let (status, _) = send_json(&app, "POST", "/v1/chat/completions", Some("mn_badtoken.sk-x"), Some(body.clone())).await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
    assert_eq!(mock.body_count(), 0);
    assert_eq!(state.metrics.auth_failures.load(Ordering::Relaxed), 1);

    let (status, resp_body) = send_json(&app, "POST", "/v1/chat/completions", Some("mn_goodtoken.sk-x"), Some(body)).await;
    assert_eq!(status, StatusCode::OK);
    let resp: Value = serde_json::from_str(&resp_body).unwrap();
    assert_eq!(resp["choices"][0]["message"]["content"], "hi");
    assert_eq!(mock.body_count(), 1);
}

/// Jeton forgé (shape valide, hors registre) → marqueur neutre `[REDACTED]`
/// + compteur `unresolved` = 1.
#[tokio::test]
async fn forged_sentinel_yields_neutral_marker() {
    let mock = MockUpstream::start(MockMode::ChatForgedSentinel).await;
    let (state, app) = proxy_app(&mock.url()).await;

    let body = json!({
        "model": "mock-echo",
        "messages": [{"role": "user", "content": "some request"}]
    });
    let (status, resp_body) = send_json(&app, "POST", "/v1/chat/completions", Some(&good_auth()), Some(body)).await;
    assert_eq!(status, StatusCode::OK);

    let resp: Value = serde_json::from_str(&resp_body).unwrap();
    let content = resp["choices"][0]["message"]["content"].as_str().unwrap();
    assert_eq!(content, "[REDACTED]", "forged sentinel must be redacted");
    assert!(!content.contains('\u{27E6}'));
    assert_eq!(state.metrics.unresolved_tokens.load(Ordering::Relaxed), 1);
}

/// Legacy `/v1/completions` : prompt tokenisé à l'aller, `choices[].text`
/// restauré au retour.
#[tokio::test]
async fn legacy_completions_roundtrip() {
    let mock = MockUpstream::start(MockMode::ChatEcho).await;
    let (_state, app) = proxy_app(&mock.url()).await;

    let body = json!({"model": "mock-echo", "prompt": "Contact: user@example.com"});
    let (status, resp_body) = send_json(&app, "POST", "/v1/completions", Some(&good_auth()), Some(body)).await;
    assert_eq!(status, StatusCode::OK);

    let resp: Value = serde_json::from_str(&resp_body).unwrap();
    let text = resp["choices"][0]["text"].as_str().unwrap();
    assert!(text.contains("user@example.com"), "restored: {text}");
    assert!(!text.contains('\u{27E6}'));
}

/// `GET /v1/models` : pass-through du mock.
#[tokio::test]
async fn models_pass_through() {
    let mock = MockUpstream::start(MockMode::ChatEcho).await;
    let (_state, app) = proxy_app(&mock.url()).await;

    let (status, resp_body) = send_json(&app, "GET", "/v1/models", Some(&good_auth()), None).await;
    assert_eq!(status, StatusCode::OK);
    let resp: Value = serde_json::from_str(&resp_body).unwrap();
    assert_eq!(resp["data"][0]["id"], "mock-echo");
}

/// Erreur amont (non-stream et stream) → 502 avec shape OpenAI, avant tout SSE.
#[tokio::test]
async fn upstream_error_yields_502() {
    let mock = MockUpstream::start(MockMode::AlwaysError).await;
    let (_state, app) = proxy_app(&mock.url()).await;
    let body = json!({"model": "m", "messages": [{"role": "user", "content": "hi"}]});

    let (status, resp_body) = send_json(&app, "POST", "/v1/chat/completions", Some(&good_auth()), Some(body.clone())).await;
    assert_eq!(status, StatusCode::BAD_GATEWAY);
    let resp: Value = serde_json::from_str(&resp_body).unwrap();
    assert_eq!(resp["error"]["code"], "upstream_error");

    let stream_body = json!({"model": "m", "stream": true, "messages": [{"role": "user", "content": "hi"}]});
    let (status, resp_body) = send_json(&app, "POST", "/v1/chat/completions", Some(&good_auth()), Some(stream_body)).await;
    assert_eq!(status, StatusCode::BAD_GATEWAY);
    assert!(resp_body.contains("upstream_error"), "{resp_body}");
}
