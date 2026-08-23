//! API admin REST (Axum 0.8) du plan de contrôle.
//!
//! Les corps de requêtes/réponses ne transportent **jamais** de texte client.
//! Le clair `mn_` n'apparaît que dans les réponses d'émission/rotation (`TokenIssued`),
//! affiché une seule fois via [`TokenIssued::to_issued_json`], puis oublié.
//!
//! # Pipeline de service (STACK-5 §5) — routes `/v1/control/*`
//!
//! - `POST /v1/control/ingest` : reçoit des reçus STACK-4 (`IngestRequest`), vérifie
//!   `sig_agent` sur `Receipt::signing_bytes()`, applique le k-anonymat (`cloison-audit`),
//!   construit un payload de compteurs **redactés**, crée l'entrée contresignée par la
//!   clé du contrôle, l'append au ledger (persistant si `CLOISON_LEDGER_FILE` est posé) ;
//! - `GET  /v1/control/root` : racine courante du ledger ;
//! - `GET  /v1/control/version?tenant_id=…` : `tokens_version` du tenant (propagation
//!   des rotations/révocations vers les caches du proxy — voir §« Propagation »).
//!
//! # Propagation des rotations/révocations (P1-4)
//!
//! Le design (`API_DESIGN.md` §2.4/§2.5) fait cacher au **proxy** les vues de jetons
//! (TokenView) localement, avec un TTL. Pour une révocation quasi-instantanée, le proxy
//! long-polle `GET /v1/control/version` (ETag/If-None-Match) : le contrôle incrémente
//! `tokens_version` à chaque rotate/revoke (`store.rs`), et toute montée de version
//! purge les entrées de cache périmées. La rotation conserve une **période de grâce** :
//! l'ancien jeton reste valide `grace_period_secs` (env `CLOISON_ROTATION_GRACE_SECONDS`,
//! défaut 300) après la rotation — le trafic en vol n'est pas coupé net.
//!
//! # Routes
//!
//! - `POST   /admin/tenants`                 → crée un tenant + licence
//! - `GET    /admin/tenants/{id}`            → détail tenant
//! - `POST   /admin/tenants/{id}/tokens`     → émet un jeton `mn_` (clair affiché une fois)
//! - `POST   /admin/tenants/{id}/rotate`     → rotation (l'ancien passe en grâce)
//! - `DELETE /admin/tenants/{id}/tokens/{token_id}` → révocation immédiate
//! - `PUT    /admin/tenants/{id}/policy`     → publie une politique
//! - `POST   /admin/tenants/{id}/licenses`   → ajoute une licence
//! - `POST   /v1/control/ingest`             → reçus → entrée de journal
//! - `GET    /v1/control/root`               → racine du journal
//! - `GET    /v1/control/version`            → tokens_version du tenant
//! - `GET    /healthz`                       → liveness

use crate::contersign;
use crate::error::{ControlError, ControlResult};
use crate::model::{
    ApiToken, License, LicenseLimites, Plan, Policy, Tenant, TenantStatut, TokenIssued,
};
use crate::store::Store;
use crate::token;
use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::routing::{delete, get, post, put};
use axum::{Json, Router};
use cloison_audit::Receipt;
use cloison_ledger::{hexutil, Ledger};
use ed25519_dalek::{SigningKey, VerifyingKey};
use serde::{Deserialize, Serialize};
use std::sync::{Arc, Mutex};

/// État partagé des handlers.
#[derive(Clone)]
pub struct AppState {
    pub store: Arc<dyn Store>,
    /// Journal append-only vérifiable (cloison-ledger). `Mutex` : l'append est
    /// sérialisé (allocation de seq terminale) ; `Arc` pour que `State` soit `Clone`.
    pub ledger: Arc<Mutex<Ledger>>,
    /// Clé publique de l'agent au bord (STACK-4) — vérifie `sig_agent` sur
    /// `receipt.signing_bytes()` à l'ingest.
    pub agent_verify_key: VerifyingKey,
    /// Clé de contresignature du contrôle : signe les entrées du journal (`entry_hash`)
    /// et les reçus contresignés (`signing_bytes()`).
    pub control_signing_key: Arc<SigningKey>,
    /// Période de grâce de rotation (secondes), env `CLOISON_ROTATION_GRACE_SECONDS`
    /// (défaut 300).
    pub grace_period_secs: u64,
}

impl AppState {
    /// Construit l'état du plan de contrôle.
    pub fn new(
        store: Arc<dyn Store>,
        ledger: Arc<Mutex<Ledger>>,
        agent_verify_key: VerifyingKey,
        control_signing_key: SigningKey,
        grace_period_secs: u64,
    ) -> AppState {
        AppState {
            store,
            ledger,
            agent_verify_key,
            control_signing_key: Arc::new(control_signing_key),
            grace_period_secs,
        }
    }

    /// Construit l'état depuis l'environnement :
    /// - `CLOISON_LEDGER_FILE` : si posé, le journal est **persistant** (JSONL
    ///   append-only, rechargé au boot) ; sinon `MemLedger` (tests, mode sans fichier) ;
    /// - `CLOISON_ROTATION_GRACE_SECONDS` : grâce de rotation (défaut 300).
    pub fn from_env(
        store: Arc<dyn Store>,
        agent_verify_key: VerifyingKey,
        control_signing_key: SigningKey,
    ) -> ControlResult<AppState> {
        let grace_period_secs = std::env::var("CLOISON_ROTATION_GRACE_SECONDS")
            .ok()
            .and_then(|s| s.parse().ok())
            .unwrap_or(300);
        let ledger = match std::env::var("CLOISON_LEDGER_FILE") {
            Ok(path) if !path.trim().is_empty() => {
                Ledger::open_file(path, control_signing_key.verifying_key())?
            }
            _ => Ledger::with_verify_key(control_signing_key.verifying_key()),
        };
        tracing::info!(
            grace_period_secs,
            ledger_file = std::env::var("CLOISON_LEDGER_FILE").ok(),
            "control state initialized"
        );
        Ok(AppState::new(
            store,
            Arc::new(Mutex::new(ledger)),
            agent_verify_key,
            control_signing_key,
            grace_period_secs,
        ))
    }
}

/// Erreur HTTP normalisée (aucun détail interne, aucun texte client).
#[derive(Debug)]
pub struct ApiError {
    status: StatusCode,
    message: String,
}

impl From<ControlError> for ApiError {
    fn from(err: ControlError) -> ApiError {
        let (status, message) = match &err {
            ControlError::TenantNotFound(_) | ControlError::TokenNotFound(_) => {
                (StatusCode::NOT_FOUND, err.to_string())
            }
            ControlError::TenantConflict(_) | ControlError::TokenConflict => {
                (StatusCode::CONFLICT, err.to_string())
            }
            ControlError::TokenInvalid | ControlError::LicenseExpired => {
                (StatusCode::UNAUTHORIZED, err.to_string())
            }
            ControlError::LicenseNotFound(_) | ControlError::PolicyNotFound(_) => {
                (StatusCode::NOT_FOUND, err.to_string())
            }
            ControlError::InvalidPolicy(_)
            | ControlError::InvalidAgentSignature
            | ControlError::IngestRejected(_)
            | ControlError::Signature(_)
            | ControlError::Audit(_) => (StatusCode::BAD_REQUEST, err.to_string()),
            ControlError::Ledger(_) | ControlError::Io(_) | ControlError::Json(_) => (
                StatusCode::INTERNAL_SERVER_ERROR,
                "internal error".to_string(),
            ),
            ControlError::Store(_) => (StatusCode::INTERNAL_SERVER_ERROR, err.to_string()),
            ControlError::Internal(_) => (StatusCode::BAD_REQUEST, err.to_string()),
        };
        ApiError { status, message }
    }
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        (
            self.status,
            Json(serde_json::json!({ "error": self.message })),
        )
            .into_response()
    }
}

// ---------------------------------------------------------------------------
// Corps de requêtes / réponses
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Deserialize)]
pub struct CreateTenantReq {
    pub id: String,
    pub nom_public: String,
    pub plan: Plan,
}

#[derive(Debug, Clone, Deserialize)]
pub struct IssueTokenReq {
    #[serde(default)]
    pub scopes: Vec<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct RotateTokenReq {
    pub token_id: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct PutPolicyReq {
    pub json_policy: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct AddLicenseReq {
    pub plan: Plan,
    #[serde(default)]
    pub expires_at: Option<u64>,
}

/// Requête d'ingest (STACK-5 §5) : des reçus STACK-4 signés, agrégés en une entrée.
#[derive(Debug, Clone, Deserialize)]
pub struct IngestRequest {
    pub tenant_id: String,
    pub period_start: u64,
    pub period_end: u64,
    /// Seuil k-anonyme (≥ 2, défaut 5) appliqué avant écriture dans le journal.
    #[serde(default = "default_k")]
    pub k: usize,
    /// Reçus signés par l'agent — `sig_agent` est vérifiée ici sur `signing_bytes()`.
    pub receipts: Vec<Receipt>,
}

fn default_k() -> usize {
    5
}

/// Réponse d'ingest : l'entrée créée (`seq`) et la racine du journal après append.
#[derive(Debug, Clone, Serialize)]
pub struct IngestResponse {
    pub seq: u64,
    pub root_hash: String,
}

// ---------------------------------------------------------------------------
// Handlers
// ---------------------------------------------------------------------------

/// `POST /admin/tenants` — crée un tenant (identifiant opérateur non sensible) + licence.
pub async fn create_tenant(
    State(state): State<AppState>,
    Json(req): Json<CreateTenantReq>,
) -> Result<Json<Tenant>, ApiError> {
    let now = crate::now_unix();
    let tenant = Tenant {
        id: req.id.clone(),
        nom_public: req.nom_public,
        statut: TenantStatut::Actif,
        created_at: now,
        tokens_version: 0,
    };
    state.store.create_tenant(&tenant)?;
    let license = License {
        tenant_id: req.id,
        plan: req.plan,
        limites: LicenseLimites::default(),
        expires_at: None,
        created_at: now,
    };
    state.store.add_license(&license)?;
    tracing::info!(tenant_id = %tenant.id, "tenant created");
    Ok(Json(tenant))
}

/// `GET /admin/tenants/{id}` — détail d'un tenant.
pub async fn get_tenant(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<Json<Tenant>, ApiError> {
    let tenant = state
        .store
        .get_tenant(&id)?
        .ok_or_else(|| ControlError::TenantNotFound(id))?;
    Ok(Json(tenant))
}

/// `POST /admin/tenants/{id}/tokens` — émet un jeton `mn_`.
/// La réponse contient le clair **une seule fois** ; le store ne garde que le hash.
pub async fn issue_token(
    State(state): State<AppState>,
    Path(tenant_id): Path<String>,
    Json(req): Json<IssueTokenReq>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let now = crate::now_unix();
    state
        .store
        .get_tenant(&tenant_id)?
        .ok_or_else(|| ControlError::TenantNotFound(tenant_id.clone()))?;
    let clair = token::generate_token();
    let id = token::new_token_id(&tenant_id, now);
    let stored = ApiToken::issue(id.clone(), tenant_id.clone(), &clair, req.scopes, now);
    state.store.create_token(&stored)?;
    tracing::info!(tenant_id, token_id = %id, "token issued (hash only stored)");
    Ok(Json(
        TokenIssued {
            id,
            token: clair,
            expires_at: None,
        }
        .to_issued_json(),
    ))
}

/// `POST /admin/tenants/{id}/rotate` — rotation avec **grâce** : nouveau secret,
/// l'ancien reste valide `grace_period_secs` (env `CLOISON_ROTATION_GRACE_SECONDS`).
///
/// IDOR : le `token_id` doit appartenir au tenant du chemin, sinon 404 (aucune fuite
/// d'existence) — la vérification est rejouée au niveau du store (`store.rs`).
pub async fn rotate_token(
    State(state): State<AppState>,
    Path(tenant_id): Path<String>,
    Json(req): Json<RotateTokenReq>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let now = crate::now_unix();
    state
        .store
        .get_tenant(&tenant_id)?
        .ok_or_else(|| ControlError::TenantNotFound(tenant_id.clone()))?;
    // IDOR cross-tenant : le jeton doit appartenir au tenant du chemin.
    let existing = state
        .store
        .get_token(&req.token_id)?
        .ok_or_else(|| ControlError::TokenNotFound(req.token_id.clone()))?;
    if existing.tenant_id != tenant_id {
        return Err(ControlError::TokenNotFound(req.token_id).into());
    }
    let clair = token::generate_token();
    let id = token::new_token_id(&tenant_id, now);
    // Le nouveau jeton hérite des scopes de l'ancien (fait dans le store).
    let new_token = ApiToken::issue(id.clone(), tenant_id.clone(), &clair, Vec::new(), now);
    state.store.rotate_token(
        &tenant_id,
        &req.token_id,
        &new_token,
        state.grace_period_secs,
    )?;
    tracing::info!(
        tenant_id,
        old_token_id = %req.token_id,
        grace_secs = state.grace_period_secs,
        "token rotated (ancien en période de grâce)"
    );
    Ok(Json(
        TokenIssued {
            id,
            token: clair,
            expires_at: None,
        }
        .to_issued_json(),
    ))
}

/// `DELETE /admin/tenants/{id}/tokens/{token_id}` — révocation immédiate (204).
///
/// IDOR : le `token_id` doit appartenir au tenant du chemin, sinon 404 (la vérification
/// est rejouée au niveau du store).
pub async fn revoke_token(
    State(state): State<AppState>,
    Path((tenant_id, token_id)): Path<(String, String)>,
) -> Result<StatusCode, ApiError> {
    let existing = state
        .store
        .get_token(&token_id)?
        .ok_or_else(|| ControlError::TokenNotFound(token_id.clone()))?;
    if existing.tenant_id != tenant_id {
        return Err(ControlError::TokenNotFound(token_id).into());
    }
    state.store.revoke_token(&tenant_id, &token_id)?;
    tracing::info!(tenant_id, token_id, "token revoked");
    Ok(StatusCode::NO_CONTENT)
}

/// `PUT /admin/tenants/{id}/policy` — publie une politique (version incrémentée).
pub async fn put_policy(
    State(state): State<AppState>,
    Path(tenant_id): Path<String>,
    Json(req): Json<PutPolicyReq>,
) -> Result<Json<Policy>, ApiError> {
    let now = crate::now_unix();
    state
        .store
        .get_tenant(&tenant_id)?
        .ok_or_else(|| ControlError::TenantNotFound(tenant_id.clone()))?;
    // Version = précédente + 1.
    let version = match state.store.get_policy(&tenant_id)? {
        Some(prev) => prev.version + 1,
        None => 1,
    };
    let policy = Policy {
        tenant_id: tenant_id.clone(),
        json_policy: req.json_policy,
        version,
        updated_at: now,
    };
    state.store.set_policy(&policy)?;
    tracing::info!(tenant_id, version, "policy published");
    Ok(Json(policy))
}

/// `POST /admin/tenants/{id}/licenses` — ajoute une licence (quotas, jamais de données).
pub async fn add_license(
    State(state): State<AppState>,
    Path(tenant_id): Path<String>,
    Json(req): Json<AddLicenseReq>,
) -> Result<Json<License>, ApiError> {
    let now = crate::now_unix();
    state
        .store
        .get_tenant(&tenant_id)?
        .ok_or_else(|| ControlError::TenantNotFound(tenant_id.clone()))?;
    let license = License {
        tenant_id,
        plan: req.plan,
        limites: LicenseLimites::default(),
        expires_at: req.expires_at,
        created_at: now,
    };
    state.store.add_license(&license)?;
    tracing::info!(tenant_id = %license.tenant_id, "license added");
    Ok(Json(license))
}

/// `POST /v1/control/ingest` — pipeline control → ledger (P0-1/P0-2/P0-3).
///
/// 1. vérifie chaque reçu : `tenant_id` du corps, puis `sig_agent` sur
///    `receipt.signing_bytes()` ([`contersign::contresigner_reçu`]) ;
/// 2. agrège les compteurs et applique le k-anonymat (`cloison-audit`) : seuls les
///    compteurs **redactés** entrent dans le journal — un compteur `< k` n'existe
///    nulle part dans la chaîne ;
/// 3. construit le [`LedgerPayload`](cloison_ledger::LedgerPayload) (engagement sur
///    chaque reçu par `SHA-256(signing_bytes())`) et calcule `payload_hash` ;
/// 4. crée l'entrée suivante (contresignée par `control_signing_key` sur `entry_hash`),
///    l'append au ledger — persistant si `CLOISON_LEDGER_FILE` est configuré ;
/// 5. répond `{seq, root_hash}`.
pub async fn ingest(
    State(state): State<AppState>,
    Json(req): Json<IngestRequest>,
) -> Result<Json<IngestResponse>, ApiError> {
    if req.receipts.is_empty() {
        return Err(ControlError::IngestRejected("no receipts".to_string()).into());
    }
    let now = crate::now_unix();
    let k_anon = cloison_audit::KAnonymity::new(req.k).map_err(ControlError::Audit)?;

    // 1. Vérification des reçus (tenant + signature agent sur signing_bytes()).
    let mut receipt_hashes = Vec::with_capacity(req.receipts.len());
    let mut counters: Vec<cloison_audit::Counters> = Vec::with_capacity(req.receipts.len());
    for receipt in &req.receipts {
        if receipt.tenant_id != req.tenant_id {
            return Err(ControlError::IngestRejected(
                "receipt tenant does not match request tenant".to_string(),
            )
            .into());
        }
        // Vérifie sig_agent puis contresigne le reçu (même message : signing_bytes()).
        let cs = contersign::contresigner_reçu(
            receipt,
            &state.agent_verify_key,
            &state.control_signing_key,
        )?;
        tracing::debug!(
            tenant_id = %req.tenant_id,
            message_hash = %hexutil::encode(&cs.message_hash),
            "receipt verified and countersigned"
        );
        receipt_hashes.push(cloison_ledger::sha256(&receipt.signing_bytes()));
        counters.push(receipt.counters.clone());
    }

    // 2. Agrégation + redaction k-anonyme (P0-2 du design : jamais de compteur < k).
    let aggregated = k_anon.aggregate(counters);
    let redacted = k_anon.redact_below_k(&aggregated.masked_by_type);

    // 3. Payload de compteurs redactés + engagement sur les reçus.
    let payload = cloison_ledger::LedgerPayload {
        schema_version: 1,
        kind: "conformance-period".to_string(),
        tenant_id: req.tenant_id,
        period_start: req.period_start,
        period_end: req.period_end,
        total_requests: req.receipts.len() as u64,
        counters: redacted,
        receipt_hashes,
    };
    let payload_hash = cloison_ledger::payload_hash(&payload);

    // 4. Entrée contresignée par la clé control + append terminal (persistant si fichier).
    let mut ledger = state.ledger.lock().expect("ledger lock poisoned");
    let head = ledger
        .head()
        .cloned()
        .ok_or_else(|| ControlError::Ledger(cloison_ledger::LedgerError::EmptyLedger))?;
    let entry = head.next(payload_hash, now, &state.control_signing_key);
    let seq = entry.seq;
    ledger.append(entry).map_err(ControlError::from)?;
    let root_hash = hexutil::encode(&ledger.root_hash());

    tracing::info!(tenant_id = %payload.tenant_id, seq, k = req.k, "ledger entry appended");
    Ok(Json(IngestResponse { seq, root_hash }))
}

/// `GET /v1/control/root` — racine courante du journal (`{seq, root_hash}`).
pub async fn root(State(state): State<AppState>) -> Result<Json<serde_json::Value>, ApiError> {
    let ledger = state.ledger.lock().expect("ledger lock poisoned");
    let seq = ledger.head().map(|e| e.seq).unwrap_or(0);
    let root_hash = hexutil::encode(&ledger.root_hash());
    Ok(Json(
        serde_json::json!({ "seq": seq, "root_hash": root_hash }),
    ))
}

/// `GET /v1/control/version?tenant_id=…` — `tokens_version` du tenant.
///
/// Le proxy (design §2.4/§2.5) long-polle cette route (ETag/If-None-Match) pour purger
/// ses caches de TokenView dès qu'une rotation/révocation incrémente la version.
#[derive(Debug, Clone, Deserialize)]
pub struct VersionQuery {
    pub tenant_id: String,
}

pub async fn tokens_version(
    State(state): State<AppState>,
    Query(q): Query<VersionQuery>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let version = state.store.tokens_version(&q.tenant_id)?;
    Ok(Json(
        serde_json::json!({ "tenant_id": q.tenant_id, "version": version }),
    ))
}

/// `GET /healthz` — liveness du plan de contrôle.
pub async fn healthz() -> Json<serde_json::Value> {
    Json(serde_json::json!({
        "status": "ok",
        "service": "cloison-control"
    }))
}

// ---------------------------------------------------------------------------
// Routeur
// ---------------------------------------------------------------------------

/// Assemble le routeur Axum (préfixe `/v1` à poser au niveau de la passerelle pour les
/// routes admin ; les routes `/v1/control/*` sont montées telles quelles).
pub fn router(state: AppState) -> Router {
    Router::new()
        .route("/admin/tenants", post(create_tenant))
        .route("/admin/tenants/{id}", get(get_tenant))
        .route("/admin/tenants/{id}/tokens", post(issue_token))
        .route("/admin/tenants/{id}/rotate", post(rotate_token))
        .route(
            "/admin/tenants/{id}/tokens/{token_id}",
            delete(revoke_token),
        )
        .route("/admin/tenants/{id}/policy", put(put_policy))
        .route("/admin/tenants/{id}/licenses", post(add_license))
        .route("/v1/control/ingest", post(ingest))
        .route("/v1/control/root", get(root))
        .route("/v1/control/version", get(tokens_version))
        .route("/healthz", get(healthz))
        .with_state(state)
}
