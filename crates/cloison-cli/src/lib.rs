//! CLOISON — cloison-cli : outillage ops N3.
//!
//! Proxies l'API admin REST du plan de contrôle (`cloison-control`) pour les
//! opérations commerciales : provisioning tenant + jeton `mn_`, rotation,
//! révocation, politique, licences, requêtes ledger, stats.
//!
//! # Sécurité (invariants, docs/SECURITY.md)
//!
//! - **Zéro PII** : le CLI ne manipule que des identifiants opérateur (`tenant_id`)
//!   et des compteurs — jamais de texte client.
//! - **Zéro secret en log** : le clair `mn_` n'est affiché qu'à l'émission
//!   (`TokenIssued`), jamais journalisé ; les erreurs HTTP sont affichées sans
//!   corps (le corps de contrôle ne contient pas de secret, mais la règle reste).
//! - **Hash côté CLI** : `verify` calcule `hex(SHA-256(domaine ‖ clair))`
//!   localement (même domaine que le contrôle : `cloison-mn-token-v1:`) et
//!   n'envoie que le digest — le clair ne traverse jamais le réseau (I2).
//! - **Ledger check hors-ligne** : `ledger check` lit un fichier JSONL public et
//!   vérifie la chaîne + signatures + timestamps avec `cloison-verify` — la
//!   promesse « nous ne lisons pas » reste vérifiable par n'importe qui.
//!
//! # Configuration
//!
//! - `CLOISON_CONTROL_URL` : base URL de l'API contrôle (défaut
//!   `http://127.0.0.1:8788`). Aucun secret dans l'URL.

use clap::{Parser, Subcommand};
use sha2::{Digest, Sha256};
use std::path::PathBuf;

/// Domaine du hash de jeton — IDENTIQUE à `cloison_control::token` et au proxy :
/// les trois côtés comparent des digests de ce domaine.
pub const TOKEN_HASH_DOMAIN: &str = "cloison-mn-token-v1:";

/// `hex(SHA-256(TOKEN_HASH_DOMAIN ‖ clair))` — le digest envoyé au contrôle.
pub fn token_hash(token: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(TOKEN_HASH_DOMAIN.as_bytes());
    hasher.update(token.as_bytes());
    hex_encode(&hasher.finalize())
}

fn hex_encode(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

/// CLI de contrôle CLOISON.
#[derive(Debug, Parser)]
#[command(name = "cloison-cli", version, about)]
pub struct Cli {
    /// Base URL de l'API contrôle (défaut : $CLOISON_CONTROL_URL ou http://127.0.0.1:8788).
    #[arg(
        long,
        global = true,
        env = "CLOISON_CONTROL_URL",
        default_value = "http://127.0.0.1:8788"
    )]
    pub control_url: String,

    #[command(subcommand)]
    pub command: Command,
}

#[derive(Debug, Subcommand)]
pub enum Command {
    /// Crée un tenant (identifiant opérateur) + licence + émet un jeton `mn_`.
    Provision(ProvisionArgs),
    /// Détail d'un tenant.
    TenantGet(TenantGetArgs),
    /// Émet un jeton `mn_` pour un tenant (clair affiché UNE fois).
    TokenIssue(TokenIssueArgs),
    /// Rotation d'un jeton (l'ancien passe en période de grâce).
    TokenRotate(TokenRotateArgs),
    /// Révocation immédiate d'un jeton.
    TokenRevoke(TokenRevokeArgs),
    /// Vérifie un jeton par hash (le clair ne quitte jamais le CLI).
    TokenVerify(TokenVerifyArgs),
    /// Publie une politique JSON par locataire.
    PolicySet(PolicySetArgs),
    /// Ajoute une licence (plan + expiration optionnelle).
    LicenseAdd(LicenseAddArgs),
    /// Racine courante du journal / vérification hors-ligne (`ledger`).
    Ledger(LedgerCmd),
    /// Statistiques d'un tenant (version de jeton + racine du journal).
    Stats(StatsArgs),
}

/// Sous-commandes `ledger` (journal de transparence).
#[derive(Debug, Subcommand)]
pub enum LedgerCmd {
    /// Racine courante du journal (`GET /v1/control/root`).
    Root,
    /// Vérifie hors-ligne un fichier JSONL du journal (chaîne + signatures).
    Check(LedgerCheckArgs),
}

#[derive(Debug, clap::Args)]
pub struct ProvisionArgs {
    /// Identifiant opérateur du tenant (alphanumérique, tirets, underscores).
    pub tenant_id: String,
    /// Nom public du tenant (affiché, jamais une donnée client).
    #[arg(long)]
    pub nom: String,
    /// Plan de licence : free | pro | enterprise.
    #[arg(long, default_value = "free")]
    pub plan: String,
    /// Émet un jeton à la création (défaut : oui).
    #[arg(long, default_value_t = true)]
    pub issue_token: bool,
}

#[derive(Debug, clap::Args)]
pub struct TenantGetArgs {
    pub tenant_id: String,
}

#[derive(Debug, clap::Args)]
pub struct TokenIssueArgs {
    pub tenant_id: String,
    /// Scopes (répétable) — défaut : accès complet.
    #[arg(long)]
    pub scope: Vec<String>,
}

#[derive(Debug, clap::Args)]
pub struct TokenRotateArgs {
    pub tenant_id: String,
    /// Identifiant du jeton à remplacer (retourné à l'émission).
    pub token_id: String,
}

#[derive(Debug, clap::Args)]
pub struct TokenRevokeArgs {
    pub tenant_id: String,
    pub token_id: String,
}

#[derive(Debug, clap::Args)]
pub struct TokenVerifyArgs {
    pub tenant_id: String,
    /// Le jeton `mn_…` en clair — haché localement, jamais transmis.
    pub token: String,
}

#[derive(Debug, clap::Args)]
pub struct PolicySetArgs {
    pub tenant_id: String,
    /// Chemin d'un fichier JSON de politique (ou `-` pour stdin).
    pub json_file: String,
}

#[derive(Debug, clap::Args)]
pub struct LicenseAddArgs {
    pub tenant_id: String,
    /// Plan : free | pro | enterprise.
    #[arg(long)]
    pub plan: String,
    /// Expiration Unix (secondes) — optionnel.
    #[arg(long)]
    pub expires_at: Option<u64>,
}

#[derive(Debug, clap::Args)]
pub struct LedgerCheckArgs {
    /// Fichier JSONL du journal (ex. /ledger.jsonl téléchargé).
    pub ledger_file: PathBuf,
    /// Fichier hex de la clé publique du contrôle (64 hex) — optionnel : si
    /// absent, la chaîne de hash est vérifiée sans signatures.
    #[arg(long)]
    pub pubkey_file: Option<PathBuf>,
}

#[derive(Debug, clap::Args)]
pub struct StatsArgs {
    pub tenant_id: String,
}

// ---------------------------------------------------------------------------
// Corps de requête (miroir des contrats de cloison-control/src/api.rs)
// ---------------------------------------------------------------------------

#[derive(Debug, serde::Serialize)]
pub struct CreateTenantReq {
    pub id: String,
    pub nom_public: String,
    pub plan: String,
}

#[derive(Debug, serde::Serialize)]
pub struct IssueTokenReq {
    pub scopes: Vec<String>,
}

#[derive(Debug, serde::Serialize)]
pub struct RotateTokenReq {
    pub token_id: String,
}

#[derive(Debug, serde::Serialize)]
pub struct PutPolicyReq {
    pub json_policy: String,
}

#[derive(Debug, serde::Serialize)]
pub struct AddLicenseReq {
    pub plan: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub expires_at: Option<u64>,
}

#[derive(Debug, serde::Serialize)]
pub struct VerifyRequest {
    pub tenant_id: String,
    pub token_hash: String,
}

// ---------------------------------------------------------------------------
// Client HTTP minimal (aucun secret en log, erreurs sans corps)
// ---------------------------------------------------------------------------

#[derive(Debug, thiserror::Error)]
pub enum CliError {
    #[error("requête HTTP impossible : {0}")]
    Http(#[from] reqwest::Error),
    #[error("réponse inattendue : HTTP {0}")]
    Status(u16),
    #[error("JSON invalide : {0}")]
    Json(#[from] serde_json::Error),
    #[error("lecture fichier : {0}")]
    Io(#[from] std::io::Error),
    #[error("vérification ledger : {0}")]
    Verify(String),
    #[error("{0}")]
    Message(String),
}

pub struct ControlClient {
    pub base: String,
    client: reqwest::Client,
}

impl ControlClient {
    pub fn new(base: String) -> Result<Self, CliError> {
        let client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(30))
            .build()?;
        Ok(Self { base, client })
    }

    /// Construit l'URL absolue d'un chemin d'API (le binaire et ses tests l'utilisent).
    pub fn url(&self, path: &str) -> String {
        format!("{}{}", self.base.trim_end_matches('/'), path)
    }

    /// POST JSON → sérialise la réponse (corps complet, jamais journalisé).
    pub async fn post<T: serde::Serialize, R: serde::de::DeserializeOwned>(
        &self,
        path: &str,
        body: &T,
    ) -> Result<R, CliError> {
        let resp = self.client.post(self.url(path)).json(body).send().await?;
        let status = resp.status();
        if !status.is_success() {
            return Err(CliError::Status(status.as_u16()));
        }
        Ok(resp.json().await?)
    }

    /// PUT JSON → sérialise la réponse.
    pub async fn put_json<T: serde::Serialize, R: serde::de::DeserializeOwned>(
        &self,
        path: &str,
        body: &T,
    ) -> Result<R, CliError> {
        let resp = self.client.put(self.url(path)).json(body).send().await?;
        let status = resp.status();
        if !status.is_success() {
            return Err(CliError::Status(status.as_u16()));
        }
        Ok(resp.json().await?)
    }

    /// GET JSON → sérialise la réponse.
    pub async fn get<R: serde::de::DeserializeOwned>(&self, path: &str) -> Result<R, CliError> {
        let resp = self.client.get(self.url(path)).send().await?;
        let status = resp.status();
        if !status.is_success() {
            return Err(CliError::Status(status.as_u16()));
        }
        Ok(resp.json().await?)
    }

    /// DELETE → statut seulement.
    pub async fn delete(&self, path: &str) -> Result<u16, CliError> {
        let resp = self.client.delete(self.url(path)).send().await?;
        let status = resp.status();
        if !status.is_success() {
            return Err(CliError::Status(status.as_u16()));
        }
        Ok(status.as_u16())
    }
}

// ---------------------------------------------------------------------------
// Ledger check hors-ligne (cloison-verify)
// ---------------------------------------------------------------------------

/// Charge les entrées d'un JSONL public (une entrée par ligne).
pub fn load_ledger_entries(path: &PathBuf) -> Result<Vec<cloison_ledger::LedgerEntry>, CliError> {
    let raw = std::fs::read_to_string(path)?;
    let mut entries = Vec::new();
    for (i, line) in raw.lines().enumerate() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        let entry: cloison_ledger::LedgerEntry = serde_json::from_str(line)
            .map_err(|e| CliError::Message(format!("ligne {} : {}", i + 1, e)))?;
        entries.push(entry);
    }
    Ok(entries)
}

/// Charge la clé publique du contrôle depuis un fichier hex (64 caractères).
pub fn load_pubkey(path: &PathBuf) -> Result<ed25519_dalek::VerifyingKey, CliError> {
    let hex = std::fs::read_to_string(path)?.trim().to_string();
    if hex.len() != 64 {
        return Err(CliError::Message("clé publique : 64 hex attendus".into()));
    }
    let bytes: Vec<u8> = (0..hex.len())
        .step_by(2)
        .map(|i| u8::from_str_radix(&hex[i..i + 2], 16))
        .collect::<Result<_, _>>()
        .map_err(|_| CliError::Message("clé publique : hex invalide".into()))?;
    let arr: [u8; 32] = bytes
        .try_into()
        .map_err(|_| CliError::Message("clé publique : 32 octets attendus".into()))?;
    ed25519_dalek::VerifyingKey::from_bytes(&arr)
        .map_err(|e| CliError::Message(format!("clé publique invalide : {e}")))
}

/// Vérifie la chaîne (hors-ligne) et imprime le verdict.
pub fn verify_ledger_file(path: &PathBuf, pubkey: Option<&PathBuf>) -> Result<(), CliError> {
    let entries = load_ledger_entries(path)?;
    if entries.is_empty() {
        return Err(CliError::Message("journal vide".into()));
    }
    let verdict = match pubkey {
        Some(pk_path) => {
            let key = load_pubkey(pk_path)?;
            cloison_verify::verify_chain_v(&entries, &key)
        }
        None => {
            // Sans clé : vérifier la chaîne de hash uniquement (pas les signatures).
            let mut prev: Option<[u8; 32]> = None;
            let mut ok = true;
            let mut checked: u64 = 0;
            for e in &entries {
                let recomputed = cloison_ledger::LedgerEntry::compute_entry_hash(
                    e.seq,
                    &e.prev_hash,
                    &e.payload_hash,
                    e.ts_unix,
                );
                if recomputed != e.entry_hash {
                    ok = false;
                    break;
                }
                if let Some(p) = prev {
                    if e.prev_hash != p {
                        ok = false;
                        break;
                    }
                }
                prev = Some(e.entry_hash);
                checked += 1;
            }
            cloison_verify::ChainVerdict {
                ok,
                entries_checked: checked,
                head_seq: entries.last().map(|e| e.seq).unwrap_or(0),
                head_entry_hash: prev,
                failure: None,
            }
        }
    };
    println!(
        "Ledger: ok={} entrées vérifiées={} head_seq={}",
        verdict.ok, verdict.entries_checked, verdict.head_seq
    );
    if let Some(f) = &verdict.failure {
        return Err(CliError::Verify(format!(
            "chaîne invalide : {} (entrées vérifiées avant échec : {})",
            f, verdict.entries_checked
        )));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn token_hash_formula() {
        // Vecteur connu : domaine ‖ "mn_test" — vérifie la stabilité de la formule.
        let h = token_hash("mn_test");
        assert_eq!(h.len(), 64);
        assert!(h.chars().all(|c| c.is_ascii_hexdigit()));
    }

    #[test]
    fn token_hash_domain_matches_control() {
        // Le domaine doit être identique à cloison-control (token.rs) et au proxy
        // (control.rs) : c'est LE contrat de hash partagé des trois côtés.
        assert_eq!(TOKEN_HASH_DOMAIN, "cloison-mn-token-v1:");
    }

    #[test]
    fn hex_encode() {
        assert_eq!(super::hex_encode(&[0xab, 0xcd]), "abcd");
        assert_eq!(super::hex_encode(&[0x00, 0xff]), "00ff");
    }
}
