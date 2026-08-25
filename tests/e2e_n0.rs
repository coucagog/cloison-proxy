//! Tests de bout en bout **N0** : daemon desktop avec coffre persistant local.
//!
//! Preuves :
//! - mode N0 actif (`CLOISON_VAULT_PATH` + passphrase) : roundtrip complet
//!   (sentinelles amont, PII restaurée client), **aucun clair dans le fichier
//!   coffre** (chiffré AES-256-GCM) ;
//! - **persistance** : fermeture + réouverture du coffre avec la même
//!   passphrase → les entrées restent lisibles (roundtrip après redémarrage) ;
//! - **fail-loud** : mauvaise passphrase ou passphrase absente → refus de
//!   démarrer (jamais de recréation silencieuse, N0-PREP §4.2) ;
//! - **sel de session persistant** : deux démarrages successifs émettent les
//!   mêmes jetons (déterminisme à travers les redémarrages du daemon) ;
//! - `/v1/embeddings` : **bloqué** (cas sensible charte §7.1) → 404.

use std::net::SocketAddr;
use std::path::Path;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use axum::body::Body;
use axum::extract::Request;
use axum::http::{header, StatusCode};
use axum::response::IntoResponse;
use axum::routing::{get, post};
use axum::{Json, Router};
use http_body_util::BodyExt;
use serde_json::{json, Value};
use tower::ServiceExt;
use url::Url;
use zeroize::Zeroizing;

use cloison_proxy::config::{
    Config, DetectConfig, N0VaultConfig, SessionConfig, StreamConfig, UpstreamConfig,
};
use cloison_proxy::handlers::AppState;
use cloison_proxy::routes::router;

/// Clé locataire de test.
const TEST_TENANT_KEY: [u8; 32] = [0x42; 32];
/// Jeton d'accès local de test.
const TEST_ACCESS_TOKEN: &str = "mn_testtoken";
/// Clé amont de test.
const TEST_UPSTREAM_KEY: &str = "sk-test.key.with.dots";
/// Passphrase du coffre N0 de test.
const TEST_PASSPHRASE: &str = "passphrase-n0-locale-de-test";

/// Sel de session persistant (fichier `<vault>.salt`) : 16 octets fixes.
fn test_salt() -> [u8; 16] {
    [0x24; 16]
}

// ---------------------------------------------------------------------------
// Mock upstream (écho non-stream)
// ---------------------------------------------------------------------------

struct MockUpstream {
    addr: SocketAddr,
    bodies_seen: Arc<Mutex<Vec<Value>>>,
}

impl MockUpstream {
    async fn start() -> Self {
        let bodies_seen = Arc::new(Mutex::new(Vec::new()));
        let bodies = bodies_seen.clone();
        let router = Router::new()
            .route(
                "/v1/chat/completions",
                post(move |req: Request| async move {
                    let body_bytes = axum::body::to_bytes(req.into_body(), 1024 * 1024)
                        .await
                        .unwrap();
                    let body: Value = serde_json::from_slice(&body_bytes).unwrap_or(Value::Null);
                    bodies.lock().unwrap().push(body.clone());
                    // Écho du dernier content (sentinelles) → le proxy restaure.
                    let content = body["messages"]
                        .as_array()
                        .and_then(|arr| {
                            arr.iter().rev().find_map(|m| {
                                m.get("content")
                                    .and_then(|c| c.as_str())
                                    .map(str::to_string)
                            })
                        })
                        .unwrap_or_default();
                    Json(json!({
                        "id": "mock-0",
                        "object": "chat.completion",
                        "model": "mock-echo",
                        "choices": [{
                            "index": 0,
                            "message": {"role": "assistant", "content": content},
                            "finish_reason": "stop"
                        }]
                    }))
                    .into_response()
                }),
            )
            .route(
                "/v1/models",
                get(|| async { Json(json!({"object": "list", "data": []})) }),
            );
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        tokio::spawn(async move {
            axum::serve(listener, router).await.unwrap();
        });
        Self { addr, bodies_seen }
    }

    fn url(&self) -> String {
        format!("http://{}", self.addr)
    }

    fn last_body(&self) -> Value {
        self.bodies_seen
            .lock()
            .unwrap()
            .last()
            .cloned()
            .unwrap_or(Value::Null)
    }
}

// ---------------------------------------------------------------------------
// Config N0 + helpers
// ---------------------------------------------------------------------------

/// Config N0 : coffre persistant dans `dir`, sel persistant à côté, mock amont.
/// Le fichier sel est écrit avec la même valeur que `Config.session_salt`
/// (le daemon réel le lirait via `load_session_salt` — testé unitairement).
fn n0_config(mock_url: &str, dir: &Path, passphrase: &str) -> Config {
    let salt = test_salt();
    let vault_path = dir.join("vault.redb");
    let salt_file = vault_path.with_extension("salt");
    std::fs::write(&salt_file, salt).unwrap();
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
        expected_access_token: Some(Zeroizing::new(TEST_ACCESS_TOKEN.to_string())),
        tenant_key: TEST_TENANT_KEY,
        session_salt: salt,
        mock_mode: true,
        audit_mode: false,
        audit_keys: None,
        audit_k: 5,
        audit_ledger_file: None,
        detect: DetectConfig::default(),
        control: cloison_proxy::config::ControlConfig::default(),
        vault: N0VaultConfig {
            path: Some(vault_path),
            passphrase: Some(Zeroizing::new(passphrase.to_string())),
            ttl_secs: 3600,
            session_salt_file: Some(salt_file),
            keychain_service: None,
            keychain_user: "default".to_string(),
        },
        session: SessionConfig::default(),
        // N0 v1.2 : pas de NER léger dans ces tests (modèle absent → N0 v1).
        light_ner: None,
    }
}

fn good_auth() -> String {
    format!("{TEST_ACCESS_TOKEN}.{TEST_UPSTREAM_KEY}")
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

fn chat_body() -> Value {
    json!({
        "model": "mock-echo",
        "messages": [
            {"role": "system", "content": "You are a helpful assistant."},
            {"role": "user", "content":
                "Contact: Aminata Diop, user@example.com, tel +221 77 123 45 67"}
        ]
    })
}

/// Une requête chat complète : renvoie la réponse client (Status, corps).
async fn one_chat(app: &Router) -> (StatusCode, String) {
    send_json(
        app,
        "POST",
        "/v1/chat/completions",
        Some(&good_auth()),
        Some(chat_body()),
    )
    .await
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

/// N0 : roundtrip complet avec coffre persistant + aucun clair dans le fichier
/// coffre (chiffré) + persistance à travers la réouverture.
#[tokio::test]
async fn n0_roundtrip_with_persistent_vault() {
    let mock = MockUpstream::start().await;
    let dir = tempfile::tempdir().unwrap();

    let config = n0_config(&mock.url(), dir.path(), TEST_PASSPHRASE);
    let vault_path = config.vault.path.clone().unwrap();

    // Boot #1 : ouverture (keycheck semé), requête, roundtrip.
    {
        let state = Arc::new(AppState::new(&config).expect("AppState N0 #1"));
        let app = router(state.clone());

        let (status, resp_body) = one_chat(&app).await;
        assert_eq!(status, StatusCode::OK);
        let resp: Value = serde_json::from_str(&resp_body).unwrap();
        let content = resp["choices"][0]["message"]["content"]
            .as_str()
            .unwrap()
            .to_string();
        assert!(content.contains("Aminata Diop"), "nom restauré: {content}");
        assert!(
            content.contains("user@example.com"),
            "email restauré: {content}"
        );
        assert!(
            content.contains("+221 77 123 45 67"),
            "téléphone restauré: {content}"
        );
        assert!(
            !content.contains('\u{27E6}'),
            "aucune sentinelle résiduelle: {content}"
        );

        // Le mock a reçu des sentinelles, jamais le clair.
        let upstream_text = mock.last_body().to_string();
        assert!(upstream_text.contains('\u{27E6}'), "sentinelles amont");
        assert!(
            !upstream_text.contains("Aminata Diop"),
            "clair amont interdit"
        );
        assert!(
            !upstream_text.contains("user@example.com"),
            "clair amont interdit"
        );

        // Le fichier coffre est chiffré : aucun clair en clair.
        let vault_bytes = std::fs::read(&vault_path).unwrap();
        let vault_str = String::from_utf8_lossy(&vault_bytes).to_lowercase();
        assert!(
            !vault_str.contains("aminata") && !vault_str.contains("user@example.com"),
            "le fichier coffre ne contient aucune valeur en clair"
        );
    } // drop → coffre fermé (redb)

    // Boot #2 : même passphrase → keycheck OK, les entrées persistées sont
    // récupérées et le roundtrip tient (le coffre est la source persistante).
    {
        let state = Arc::new(AppState::new(&config).expect("AppState N0 #2 (réouverture)"));
        let app = router(state.clone());
        let (status, resp_body) = one_chat(&app).await;
        assert_eq!(status, StatusCode::OK);
        let content: Value = serde_json::from_str(&resp_body).unwrap();
        let text = content["choices"][0]["message"]["content"]
            .as_str()
            .unwrap();
        assert!(
            text.contains("Aminata Diop"),
            "roundtrip après réouverture: {text}"
        );
        assert!(!text.contains('\u{27E6}'), "aucune sentinelle: {text}");
    }
}

/// N0 : une mauvaise passphrase sur un coffre existant est une erreur fatale
/// au boot (fail-loud) — jamais une recréation silencieuse.
#[tokio::test]
async fn n0_wrong_passphrase_fails_loud() {
    let mock = MockUpstream::start().await;
    let dir = tempfile::tempdir().unwrap();

    let good = n0_config(&mock.url(), dir.path(), TEST_PASSPHRASE);
    let vault_path = good.vault.path.clone().unwrap();
    {
        let state = Arc::new(AppState::new(&good).expect("boot avec la bonne passphrase"));
        let app = router(state.clone());
        let (status, _) = one_chat(&app).await;
        assert_eq!(status, StatusCode::OK);
    }

    let bad = n0_config(&mock.url(), dir.path(), "mauvaise-passphrase");
    assert_eq!(vault_path, bad.vault.path.clone().unwrap(), "même coffre");
    let err = match AppState::new(&bad) {
        Ok(_) => panic!("mauvaise passphrase doit échouer au boot (fail-loud)"),
        Err(e) => e,
    };
    let msg = err.to_string();
    assert!(
        msg.contains("fail-loud") || msg.contains("key mismatch") || msg.contains("corrupted"),
        "fail-loud attendu, obtenu: {msg}"
    );
}

/// N0 : coffre posé sans passphrase → refus de démarrer (config fail-loud).
#[tokio::test]
async fn n0_missing_passphrase_fails_loud() {
    let mock = MockUpstream::start().await;
    let dir = tempfile::tempdir().unwrap();
    let mut config = n0_config(&mock.url(), dir.path(), TEST_PASSPHRASE);
    config.vault.passphrase = None;
    let err = match AppState::new(&config) {
        Ok(_) => panic!("passphrase absente doit échouer (fail-loud)"),
        Err(e) => e,
    };
    assert!(err.to_string().contains("PASSPHRASE"), "message explicite");
}

/// N0 : sel de session persistant → deux démarrages successifs émettent les
/// MÊMES jetons (déterminisme à travers les redémarrages du daemon).
#[tokio::test]
async fn n0_persistent_salt_deterministic_tokens() {
    let mock = MockUpstream::start().await;
    let dir = tempfile::tempdir().unwrap();
    let config = n0_config(&mock.url(), dir.path(), TEST_PASSPHRASE);

    // Boot #1
    let tokens_1 = {
        let state = Arc::new(AppState::new(&config).unwrap());
        let app = router(state.clone());
        let (status, _) = one_chat(&app).await;
        assert_eq!(status, StatusCode::OK);
        mock.last_body().to_string()
    };
    // Boot #2 (même config, même fichier sel)
    let tokens_2 = {
        let state = Arc::new(AppState::new(&config).unwrap());
        let app = router(state.clone());
        let (status, _) = one_chat(&app).await;
        assert_eq!(status, StatusCode::OK);
        mock.last_body().to_string()
    };
    assert_eq!(
        tokens_1, tokens_2,
        "jetons identiques à travers les redémarrages"
    );
}

/// N0 : `/v1/embeddings` est **bloqué** (cas sensible charte §7.1) — aucune
/// route, 404 explicite, jamais un embedding de PII.
#[tokio::test]
async fn n0_embeddings_blocked() {
    let mock = MockUpstream::start().await;
    let dir = tempfile::tempdir().unwrap();
    let config = n0_config(&mock.url(), dir.path(), TEST_PASSPHRASE);
    let state = Arc::new(AppState::new(&config).unwrap());
    let app = router(state.clone());

    let (status, _) = send_json(
        &app,
        "POST",
        "/v1/embeddings",
        Some(&good_auth()),
        Some(json!({"model": "mock-echo", "input": "Aminata Diop user@example.com"})),
    )
    .await;
    assert_eq!(
        status,
        StatusCode::NOT_FOUND,
        "embeddings bloqué par défaut (404)"
    );
}

// ---------------------------------------------------------------------------
// N0 v1.1 — alias intra-session + jauge quasi-id in-core
// ---------------------------------------------------------------------------

/// Une requête chat avec un contenu utilisateur arbitraire.
async fn chat_with(app: &Router, content: &str) -> (StatusCode, Value) {
    let (status, body) = send_json(
        app,
        "POST",
        "/v1/chat/completions",
        Some(&good_auth()),
        Some(json!({
            "model": "mock-echo",
            "messages": [{"role": "user", "content": content}]
        })),
    )
    .await;
    let value: Value = serde_json::from_str(&body).unwrap_or(Value::Null);
    (status, value)
}

/// N0 v1.1 : l'alias intra-session masque une forme dérivée (R5 — diminutif)
/// dans une requête POSTÉRIEURE à la mention canonique. La session du daemon
/// persiste entre les requêtes : « Mamadou » (msg 1, gazetteer) → « Momo »
/// (msg 2) masqué par alias. Roundtrip : le client récupère « Momo ».
#[tokio::test]
async fn n0_alias_across_requests_masks_derived_form() {
    let mock = MockUpstream::start().await;
    let dir = tempfile::tempdir().unwrap();
    let config = n0_config(&mock.url(), dir.path(), TEST_PASSPHRASE);
    let state = Arc::new(AppState::new(&config).unwrap());
    let app = router(state.clone());

    // Message 1 : « Mamadou » (gazetteer nom_sn) → mention canonique en session.
    let (s1, r1) = chat_with(&app, "Mamadou est arrivé.").await;
    assert_eq!(s1, StatusCode::OK);
    let text1 = r1["choices"][0]["message"]["content"]
        .as_str()
        .unwrap()
        .to_string();
    assert!(text1.contains("Mamadou"), "roundtrip msg 1: {text1}");
    let upstream1 = mock.last_body().to_string();
    assert!(upstream1.contains('\u{27E6}'), "sentinelle amont msg 1");
    assert!(!upstream1.contains("Mamadou"), "clair amont interdit msg 1");

    // Message 2 : « Momo » n'est dans aucun gazetteer — seul l'alias
    // intra-session (diminutif R5 de « Mamadou ») le masque.
    let (s2, r2) = chat_with(&app, "Momo aussi.").await;
    assert_eq!(s2, StatusCode::OK);
    let text2 = r2["choices"][0]["message"]["content"]
        .as_str()
        .unwrap()
        .to_string();
    assert!(text2.contains("Momo"), "roundtrip msg 2: {text2}");
    let upstream2 = mock.last_body().to_string();
    assert!(
        upstream2.contains('\u{27E6}'),
        "« Momo » masqué par alias (sentinelle amont msg 2): {upstream2}"
    );
    assert!(
        !upstream2.contains("Momo"),
        "« Momo » jamais en clair amont"
    );
}

/// N0 v1.1 : la jauge quasi-id signale une densité élevée (age + acte + date
/// + lieu) — compteur incrémenté, jamais de texte. Opt-in (`CLOISON_QUASI_ID_GAUGE`).
#[tokio::test]
async fn n0_quasi_id_gauge_flags_dense_text() {
    let mock = MockUpstream::start().await;
    let dir = tempfile::tempdir().unwrap();
    let mut config = n0_config(&mock.url(), dir.path(), TEST_PASSPHRASE);
    config.session.enable_quasiid_gauge = true;
    config.session.quasiid_threshold = 0.5;
    let state = Arc::new(AppState::new(&config).unwrap());
    let app = router(state.clone());

    assert_eq!(
        state
            .metrics
            .quasi_id_flags
            .load(std::sync::atomic::Ordering::Relaxed),
        0
    );
    let (status, _r) = chat_with(
        &app,
        "Il a 42 ans, acte n° 1847, enregistré le 12/03/2021 à Dakar.",
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(
        state
            .metrics
            .quasi_id_flags
            .load(std::sync::atomic::Ordering::Relaxed),
        1,
        "densité age+acte+date+lieu → drapeau jauge (compteur, jamais de texte)"
    );

    // Jauge désactivée → aucun drapeau (répertoire distinct : redb verrouille
    // le fichier — deux AppState simultanés sur le même coffre échoueraient).
    let dir_off = tempfile::tempdir().unwrap();
    let mut config_off = n0_config(&mock.url(), dir_off.path(), TEST_PASSPHRASE);
    config_off.session.enable_quasiid_gauge = false;
    let state_off = Arc::new(AppState::new(&config_off).unwrap());
    let app_off = router(state_off.clone());
    let (s3, _) = chat_with(
        &app_off,
        "Il a 42 ans, acte n° 1847, enregistré le 12/03/2021 à Dakar.",
    )
    .await;
    assert_eq!(s3, StatusCode::OK);
    assert_eq!(
        state_off
            .metrics
            .quasi_id_flags
            .load(std::sync::atomic::Ordering::Relaxed),
        0,
        "jauge opt-in : désactivée → aucun drapeau"
    );
}

/// N0 v1.1 — chantier ② : `CLOISON_VAULT_KEYCHAIN_SERVICE` posé → le daemon
/// résout la passphrase via le fournisseur keychain (repli env au premier
/// démarrage : l'env est stockée dans le keychain si disponible, sinon
/// utilisée telle quelle — jamais persistée en clair par CLOISON). Boot +
/// roundtrip identiques au mode env.
#[tokio::test]
async fn n0_keychain_service_roundtrip() {
    let mock = MockUpstream::start().await;
    let dir = tempfile::tempdir().unwrap();
    let mut config = n0_config(&mock.url(), dir.path(), TEST_PASSPHRASE);
    config.vault.keychain_service = Some("cloison-e2e-n0".to_string());
    config.vault.keychain_user = "e2e".to_string();

    let state = Arc::new(AppState::new(&config).expect("boot avec keychain + env"));
    let app = router(state.clone());

    let (status, resp_body) = one_chat(&app).await;
    assert_eq!(status, StatusCode::OK);
    let resp: Value = serde_json::from_str(&resp_body).unwrap();
    let content = resp["choices"][0]["message"]["content"]
        .as_str()
        .unwrap()
        .to_string();
    assert!(
        content.contains("Aminata Diop"),
        "roundtrip keychain: {content}"
    );
    assert!(
        !content.contains('\u{27E6}'),
        "aucune sentinelle résiduelle: {content}"
    );
}
