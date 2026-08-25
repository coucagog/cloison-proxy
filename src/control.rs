//! Wiring edge → plan de contrôle (chantier C) : le chaînon manquant
//! audit → transparence.
//!
//! Trois responsabilités, toutes **optionnelles** (`CLOISON_CONTROL_URL` absent
//! = comportement N0/historique inchangé) :
//!
//! 1. **Ingest automatique** — les reçus d'audit signés (STACK-4) sont flusher
//!    périodiquement vers `POST /v1/control/ingest` ; le contrôle contresigne
//!    et append l'entrée au journal de transparence public (jamais de texte :
//!    compteurs k-anonymes uniquement, invariant I9).
//! 2. **Vérification des jetons par hash** — `POST /v1/control/verify` : le
//!    clair `mn_` ne quitte JAMAIS le bord, seul `hex(SHA-256(domaine ‖ clair))`
//!    circule (le stockage du contrôle n'est que hash — invariant I2).
//! 3. **Long-poll des versions** — `GET /v1/control/version` : toute montée de
//!    `tokens_version` (rotation/révocation) purge le cache local de
//!    vérification (propagation quasi-instantanée, design STACK-5 P1-4).
//!
//! Sûreté : panne du contrôle → **fail-closed** (401) sauf décision fraîche en
//! cache ; échec d'ingest → warn + retry au tick suivant (le reçu reste
//! persisté dans le JSONL 0600 — aucune perte de preuve).

use std::collections::HashMap;
use std::sync::Mutex;
use std::time::{Duration, Instant};

use cloison_audit::Receipt;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use url::Url;

use crate::config::ControlConfig;
use crate::engine::AuditEngine;
use crate::errors::{ErrorKind, ProxyError};

/// Domaine du hash de jeton — IDENTIQUE à `cloison_control::token::TOKEN_HASH_DOMAIN` :
/// le contrôle ne compare que des digests de ce domaine (les deux côtés du fil
/// partagent le même contrat, testé côté contrôle).
pub const TOKEN_HASH_DOMAIN: &str = "cloison-mn-token-v1:";

/// `hex(SHA-256(TOKEN_HASH_DOMAIN ‖ clair))` — le digest que le proxy envoie au
/// contrôle. Le clair n'existe que dans le header `Authorization` de la requête
/// entrante (jamais en log, jamais sur le fil vers le contrôle).
pub fn token_hash(token: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(TOKEN_HASH_DOMAIN.as_bytes());
    hasher.update(token.as_bytes());
    hex_encode(&hasher.finalize())
}

/// Nombre max de reçus par lot d'ingest (borne le corps de requête).
pub const MAX_INGEST_BATCH: usize = 512;

/// Encodage hexadécimal local (pas de dépendance au crate `hex`).
fn hex_encode(bytes: &[u8]) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut out = String::with_capacity(bytes.len() * 2);
    for b in bytes {
        out.push(HEX[(b >> 4) as usize] as char);
        out.push(HEX[(b & 0x0f) as usize] as char);
    }
    out
}

// ---------------------------------------------------------------------------
// Corps de requêtes / réponses — miroir de cloison_control::api
// ---------------------------------------------------------------------------

/// Corps de requête d'ingest — miroir de `IngestRequest` (contrôle).
#[derive(Serialize)]
struct IngestRequestBody<'a> {
    tenant_id: &'a str,
    period_start: u64,
    period_end: u64,
    k: usize,
    receipts: &'a [Receipt],
}

#[derive(Debug, Deserialize)]
struct IngestResponseBody {
    seq: u64,
    root_hash: String,
}

#[derive(Serialize)]
struct VerifyRequestBody<'a> {
    tenant_id: &'a str,
    token_hash: &'a str,
}

#[derive(Debug, Deserialize)]
struct VerifyResponseBody {
    valid: bool,
    version: u64,
}

#[derive(Debug, Deserialize)]
struct VersionResponseBody {
    version: u64,
}

// ---------------------------------------------------------------------------
// Client HTTP vers le contrôle
// ---------------------------------------------------------------------------

/// Client HTTP du plan de contrôle (stateless, réutilisable).
#[derive(Clone)]
pub struct ControlClient {
    http: reqwest::Client,
    base_url: Url,
    timeout: Duration,
}

impl std::fmt::Debug for ControlClient {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ControlClient")
            .field("base_url", &self.base_url)
            .field("timeout", &self.timeout)
            .finish()
    }
}

impl ControlClient {
    /// Construit le client depuis la configuration (URL obligatoire ici).
    pub fn new(config: &ControlConfig) -> Result<Self, ProxyError> {
        let base_url = config
            .url
            .clone()
            .ok_or_else(|| ProxyError::new(ErrorKind::Internal, "control client without url"))?;
        let http = reqwest::Client::builder()
            .connect_timeout(Duration::from_secs(2))
            .build()
            .map_err(|e| {
                ProxyError::new(ErrorKind::Internal, "failed to build control http client")
                    .with_field("detail", e.to_string())
            })?;
        Ok(Self {
            http,
            base_url,
            timeout: Duration::from_secs(10),
        })
    }

    /// Construit l'URL d'un chemin du contrôle (base + chemin, jamais de secret).
    fn url(&self, path: &str) -> Result<Url, ProxyError> {
        let base = self.base_url.as_str().trim_end_matches('/');
        Url::parse(&format!("{base}{path}")).map_err(|e| {
            ProxyError::new(ErrorKind::Internal, "invalid control URL")
                .with_field("detail", e.to_string())
        })
    }

    /// `POST /v1/control/ingest` — soumet un lot de reçus signés.
    /// Renvoie `(seq, root_hash)` de l'entrée contresignée.
    pub async fn ingest(
        &self,
        tenant_id: &str,
        period_start: u64,
        period_end: u64,
        k: usize,
        receipts: &[Receipt],
    ) -> Result<(u64, String), ProxyError> {
        let body = IngestRequestBody {
            tenant_id,
            period_start,
            period_end,
            k,
            receipts,
        };
        let resp = self
            .http
            .post(self.url("/v1/control/ingest")?)
            .json(&body)
            .timeout(self.timeout)
            .send()
            .await
            .map_err(|e| {
                ProxyError::new(ErrorKind::Upstream, "control ingest request failed")
                    .with_field("detail", e.to_string())
            })?;
        let status = resp.status();
        if !status.is_success() {
            let detail = resp.text().await.unwrap_or_default();
            return Err(
                ProxyError::new(ErrorKind::Upstream, "control ingest rejected")
                    .with_field("status", status.to_string())
                    .with_field("detail", crate::errors::truncate(&detail, 512)),
            );
        }
        let parsed: IngestResponseBody = resp.json().await.map_err(|e| {
            ProxyError::new(ErrorKind::Upstream, "invalid control ingest response")
                .with_field("detail", e.to_string())
        })?;
        Ok((parsed.seq, parsed.root_hash))
    }

    /// `GET /v1/control/version?tenant_id=…` — `tokens_version` du tenant
    /// (propagation des rotations/révocations).
    pub async fn tokens_version(&self, tenant_id: &str) -> Result<u64, ProxyError> {
        let mut url = self.url("/v1/control/version")?;
        url.query_pairs_mut().append_pair("tenant_id", tenant_id);
        let resp = self
            .http
            .get(url)
            .timeout(self.timeout)
            .send()
            .await
            .map_err(|e| {
                ProxyError::new(ErrorKind::Upstream, "control version request failed")
                    .with_field("detail", e.to_string())
            })?;
        let status = resp.status();
        if !status.is_success() {
            let detail = resp.text().await.unwrap_or_default();
            return Err(
                ProxyError::new(ErrorKind::Upstream, "control version error status")
                    .with_field("status", status.to_string())
                    .with_field("detail", crate::errors::truncate(&detail, 512)),
            );
        }
        let parsed: VersionResponseBody = resp.json().await.map_err(|e| {
            ProxyError::new(ErrorKind::Upstream, "invalid control version response")
                .with_field("detail", e.to_string())
        })?;
        Ok(parsed.version)
    }

    /// `POST /v1/control/verify` — vérifie un jeton **par son hash**.
    /// Renvoie `(valid, version)`.
    pub async fn verify(
        &self,
        tenant_id: &str,
        token_hash: &str,
    ) -> Result<(bool, u64), ProxyError> {
        let body = VerifyRequestBody {
            tenant_id,
            token_hash,
        };
        let resp = self
            .http
            .post(self.url("/v1/control/verify")?)
            .json(&body)
            .timeout(self.timeout)
            .send()
            .await
            .map_err(|e| {
                ProxyError::new(ErrorKind::Upstream, "control verify request failed")
                    .with_field("detail", e.to_string())
            })?;
        let status = resp.status();
        if !status.is_success() {
            let detail = resp.text().await.unwrap_or_default();
            return Err(
                ProxyError::new(ErrorKind::Upstream, "control verify error status")
                    .with_field("status", status.to_string())
                    .with_field("detail", crate::errors::truncate(&detail, 512)),
            );
        }
        let parsed: VerifyResponseBody = resp.json().await.map_err(|e| {
            ProxyError::new(ErrorKind::Upstream, "invalid control verify response")
                .with_field("detail", e.to_string())
        })?;
        Ok((parsed.valid, parsed.version))
    }
}

// ---------------------------------------------------------------------------
// Vérificateur de jetons (cache local + purge sur rotation)
// ---------------------------------------------------------------------------

/// Décision de vérification mise en cache (avec horodatage).
struct CachedDecision {
    valid: bool,
    cached_at: Instant,
}

/// Vérificateur de jetons `mn_` par hash auprès du contrôle.
///
/// Cache local par digest avec TTL (les appels réseau sont évités à chaud) ;
/// le long-poll de `GET /v1/control/version` purge le cache dès qu'une
/// rotation/révocation incrémente la version du tenant. Panne du contrôle :
/// une entrée fraîche en cache est honorée, sinon **fail-closed** (401) —
/// jamais d'acceptation par défaut (invariant I8 : échouer bruyamment).
///
/// **Multi-tenant (charte §7.2)** : la vérification se fait pour le tenant de
/// la REQUÊTE (header `X-Cloison-Tenant`, non secret — ou le tenant par
/// défaut si absent). Le cache est clé par `(tenant, digest)` ; le long-poll
/// surveille le tenant par défaut + les tenants vus récemment (borné).
pub struct TokenVerifier {
    client: ControlClient,
    default_tenant_id: String,
    ttl: Duration,
    cache: Mutex<HashMap<(String, String), CachedDecision>>,
    versions: Mutex<HashMap<String, u64>>,
    seen_tenants: Mutex<Vec<String>>,
}

/// Nombre maximal de tenants « vus » surveillés par le long-poll (borné —
/// chaque tenant vérifié est enregistré ; les plus anciens sortent).
const MAX_SEEN_TENANTS: usize = 64;

impl TokenVerifier {
    /// Construit le vérificateur pour le tenant par défaut.
    pub fn new(client: ControlClient, tenant_id: String, ttl: Duration) -> Self {
        Self {
            client,
            default_tenant_id: tenant_id,
            ttl,
            cache: Mutex::new(HashMap::new()),
            versions: Mutex::new(HashMap::new()),
            seen_tenants: Mutex::new(Vec::new()),
        }
    }

    /// Identifiant locataire par défaut (non sensible).
    pub fn tenant_id(&self) -> &str {
        &self.default_tenant_id
    }

    /// Enregistre un tenant vérifié (pour le long-poll de rotation).
    fn note_seen_tenant(&self, tenant_id: &str) {
        if tenant_id == self.default_tenant_id {
            return;
        }
        let mut seen = self.seen_tenants.lock().expect("seen tenants poisoned");
        if seen.iter().any(|t| t == tenant_id) {
            return;
        }
        seen.push(tenant_id.to_string());
        if seen.len() > MAX_SEEN_TENANTS {
            let excess = seen.len() - MAX_SEEN_TENANTS;
            seen.drain(..excess);
        }
    }

    /// Vérifie un jeton présenté pour le tenant de la requête (résolution par
    /// hash auprès du contrôle).
    ///
    /// `Ok(true)` = jeton actif (ou en grâce) ; `Ok(false)` = inconnu/révoqué ;
    /// `Err` = contrôle injoignable et aucune décision fraîche en cache — le
    /// middleware d'auth traduit en 401 (fail-closed).
    pub async fn verify(
        &self,
        tenant_id: &str,
        access_token: &str,
    ) -> Result<bool, ProxyError> {
        let digest = token_hash(access_token);
        let key = (tenant_id.to_string(), digest.clone());
        {
            let cache = self.cache.lock().expect("verify cache poisoned");
            if let Some(decision) = cache.get(&key) {
                if decision.cached_at.elapsed() < self.ttl {
                    return Ok(decision.valid);
                }
            }
        }
        match self.client.verify(tenant_id, &digest).await {
            Ok((valid, version)) => {
                self.cache.lock().expect("verify cache poisoned").insert(
                    key,
                    CachedDecision {
                        valid,
                        cached_at: Instant::now(),
                    },
                );
                self.versions
                    .lock()
                    .expect("version lock poisoned")
                    .insert(tenant_id.to_string(), version);
                self.note_seen_tenant(tenant_id);
                Ok(valid)
            }
            Err(e) => {
                // Fail-closed : seule une décision fraîche en cache est honorée.
                let cache = self.cache.lock().expect("verify cache poisoned");
                if let Some(decision) = cache.get(&key) {
                    if decision.cached_at.elapsed() < self.ttl {
                        return Ok(decision.valid);
                    }
                }
                tracing::warn!(
                    tenant_id,
                    error = %e,
                    "contrôle injoignable et aucune décision fraîche — fail-closed (401)"
                );
                Err(e)
            }
        }
    }

    /// Long-poll des versions : le tenant par défaut + les tenants vus (borné).
    /// Si la version d'un tenant monte (rotation/révocation), le cache des
    /// jetons de CE tenant est purgé — la prochaine requête re-vérifie.
    pub async fn poll_version(&self) -> Result<(), ProxyError> {
        let tenants: Vec<String> = {
            let mut t: Vec<String> = vec![self.default_tenant_id.clone()];
            t.extend(self.seen_tenants.lock().expect("seen tenants poisoned").clone());
            t
        };
        for tenant in tenants {
            match self.client.tokens_version(&tenant).await {
                Ok(version) => {
                    let mut versions = self.versions.lock().expect("version lock poisoned");
                    // Copie locale du courant pour éviter un emprunt chevauchant
                    // (E0502 : `get()` emprunte `versions` dans le garde).
                    let current = versions.get(&tenant).copied();
                    match current {
                        Some(prev) if prev != version => {
                            tracing::warn!(
                                tenant_id = %tenant,
                                prev_version = prev,
                                version,
                                "rotation/révocation détectée — purge du cache de jetons (tenant)"
                            );
                            self.cache
                                .lock()
                                .expect("verify cache poisoned")
                                .retain(|(t, _), _| t != &tenant);
                            versions.insert(tenant.clone(), version);
                        }
                        None => {
                            versions.insert(tenant.clone(), version);
                        }
                        _ => {}
                    }
                }
                Err(e) => {
                    tracing::warn!(
                        tenant_id = %tenant,
                        error = %e,
                        "long-poll version contrôle échoué (cache intact — fail-closed inchangé)"
                    );
                }
            }
        }
        Ok(())
    }

    /// Nombre d'entrées en cache (tests/diagnostic).
    pub fn cache_len(&self) -> usize {
        self.cache.lock().expect("verify cache poisoned").len()
    }
}

// ---------------------------------------------------------------------------
// Flush des reçus d'audit vers le contrôle (chaînon manquant audit → journal)
// ---------------------------------------------------------------------------

/// Flush un lot de reçus d'audit **pendants** vers le contrôle.
///
/// - rien à envoyer → `Ok(0)` ;
/// - les reçus sont **groupés par tenant** (le contrôle rejette un reçu dont
///   le tenant ne correspond pas au lot — `api.rs` `receipt.tenant_id !=
///   req.tenant_id` → chaque tenant est ingéré dans son propre lot, ordre
///   préservé) ;
/// - succès → le curseur d'ingest avance (persisté à côté du JSONL) du nombre
///   de reçus **contigus** ingérés et le total est renvoyé ;
/// - échec d'un groupe → on s'arrête (les groupes suivants restent pendants,
///   re-tentés au tick suivant — aucune perte de preuve).
pub async fn flush_pending_audit(
    audit: &AuditEngine,
    client: &ControlClient,
    _default_tenant: &str,
    k: usize,
) -> Result<usize, ProxyError> {
    let pending = audit.pending_receipts();
    if pending.is_empty() {
        return Ok(0);
    }
    let batch = &pending[..pending.len().min(MAX_INGEST_BATCH)];
    // Groupement par tenant (ordre du journal préservé — clones possédés).
    let mut groups: Vec<(String, Vec<Receipt>)> = Vec::new();
    for r in batch {
        if let Some((tenant, recs)) = groups.last_mut() {
            if tenant == &r.tenant_id {
                recs.push(r.clone());
                continue;
            }
        }
        groups.push((r.tenant_id.clone(), vec![r.clone()]));
    }
    let mut ingested = 0usize;
    for (tenant, recs) in &groups {
        let period_start = recs.iter().map(|r| r.ts_unix).min().unwrap_or(0);
        let period_end = recs
            .iter()
            .map(|r| r.ts_unix)
            .max()
            .unwrap_or(0)
            .saturating_add(1);
        match client.ingest(tenant, period_start, period_end, k, recs).await {
            Ok((seq, root_hash)) => {
                tracing::info!(
                    tenant_id = tenant,
                    receipts = recs.len(),
                    seq,
                    root_hash = %root_hash,
                    "reçus d'audit ingérés au contrôle (transparence, compteurs uniquement)"
                );
                ingested += recs.len();
            }
            Err(e) => {
                tracing::warn!(
                    tenant_id = tenant,
                    error = %e,
                    "ingest d'un groupe échoué — re-tenté au tick suivant (reçus persistés, aucune perte)"
                );
                break;
            }
        }
    }
    if ingested > 0 {
        audit.mark_ingested(ingested)?;
    }
    Ok(ingested)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn token_hash_is_sha256_hex_with_domain() {
        // Contrat partagé avec cloison-control::token (même domaine) — testé
        // aussi côté contrôle (verify_hash_matches_stored_hash).
        let h = token_hash("mn_testtoken");
        assert_eq!(h.len(), 64);
        assert!(h.chars().all(|c| c.is_ascii_hexdigit()));
        assert_ne!(h, "mn_testtoken");
        // Déterministe.
        assert_eq!(token_hash("mn_testtoken"), h);
        assert_ne!(token_hash("mn_other"), h);
    }
}
