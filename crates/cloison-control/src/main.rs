//! Binaire du plan de contrôle CLOISON : sert l'API admin REST.
//! Variables d'environnement :
//!   CLOISON_CONTROL_PORT  (défaut 8788)
//!   CLOISON_LEDGER_FILE   (chemin du journal append-only, optionnel)
//!   CLOISON_ROTATION_GRACE_SECONDS (défaut 300)
//!   CLOISON_AGENT_VERIFY_KEY (clé publique Ed25519 de l'agent, hex 64 — optionnelle)
//!   CLOISON_CONTROL_SIGNING_KEY (clé privée Ed25519 du control, hex 64 — générée si absente)
//!   CLOISON_DATABASE_URL  (feature `pg` — PostgreSQL ; absent = InMemoryStore)

use axum::serve;
use cloison_control::api::{router, AppState};
use cloison_control::error::{ControlError, ControlResult};
use cloison_control::store::InMemoryStore;
use ed25519_dalek::{SigningKey, VerifyingKey};
use std::net::SocketAddr;
use std::sync::Arc;
use tokio::net::TcpListener;
use tracing_subscriber::EnvFilter;

fn hex_bytes(hex: &str, _what: &str) -> ControlResult<Vec<u8>> {
    if !hex.len().is_multiple_of(2) {
        return Err(ControlError::TokenInvalid); // placeholder : erreur générique
    }
    (0..hex.len())
        .step_by(2)
        .map(|i| u8::from_str_radix(&hex[i..i + 2], 16))
        .collect::<Result<_, _>>()
        .map_err(|_| ControlError::TokenInvalid)
}

/// Encodage hexadécimal (clé publique du contrôle → fichier de vérification).
fn to_hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

#[tokio::main]
async fn main() -> ControlResult<()> {
    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::from_default_env())
        .init();

    // Dispatch de rôle (dette STACK-9 « CLOISON_ROLE non lu ») : ce binaire
    // EST le rôle `control`. `CLOISON_ROLE` (posé par le compose) est vérifié
    // — une valeur autre que `control` (ex. `edge`) échoue BRUYAMMENT au boot.
    // Absent = défaut `control` (compatibilité dev). Voir cloison-proxy pour
    // la justification des deux binaires distincts (feature `pg`).
    match std::env::var("CLOISON_ROLE").ok().as_deref() {
        None | Some("control") => {}
        Some(other) => {
            tracing::error!(
                role = %other,
                "CLOISON_ROLE attend `control` pour ce binaire (cloison-control) — \
                 rôle incompatible, refus de démarrer"
            );
            return Err(ControlError::TokenInvalid); // échec bruyant au boot
        }
    }

    let port: u16 = std::env::var("CLOISON_CONTROL_PORT")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(8788);
    let addr: SocketAddr = format!("0.0.0.0:{port}")
        .parse()
        .map_err(|_| ControlError::TokenInvalid)?;

    let agent_verify_key = match std::env::var("CLOISON_AGENT_VERIFY_KEY").ok() {
        Some(h) => {
            let bytes = hex_bytes(&h, "agent verify key")?;
            let arr: [u8; 32] = bytes.try_into().map_err(|_| ControlError::TokenInvalid)?;
            VerifyingKey::from_bytes(&arr).map_err(|_| ControlError::TokenInvalid)?
        }
        None => {
            tracing::warn!(
                "CLOISON_AGENT_VERIFY_KEY absent : génération d'une paire éphémère (dev)"
            );
            SigningKey::generate(&mut rand::rngs::OsRng).verifying_key()
        }
    };
    let control_signing_key = match std::env::var("CLOISON_CONTROL_SIGNING_KEY").ok() {
        Some(h) => {
            let bytes = hex_bytes(&h, "control signing key")?;
            let arr: [u8; 32] = bytes.try_into().map_err(|_| ControlError::TokenInvalid)?;
            SigningKey::from_bytes(&arr)
        }
        None => {
            tracing::warn!("CLOISON_CONTROL_SIGNING_KEY absent : génération éphémère (dev)");
            SigningKey::generate(&mut rand::rngs::OsRng)
        }
    };

    // C — surface journal public (journal.wonkom.ai) : la clé publique du
    // contrôle est écrite à côté du ledger (volume partagé, monté en lecture
    // seule par le conteneur public) pour que le vérificateur valide la chaîne
    // SANS exposer l'API admin (THREAT-MODEL §3.1). Écriture atomique
    // (tmp + rename) pour ne jamais servir un fichier partiellement écrit.
    if let Ok(ledger_path) = std::env::var("CLOISON_LEDGER_FILE") {
        if let Some(dir) = std::path::Path::new(&ledger_path).parent() {
            let tmp = dir.join("control_pubkey.hex.tmp");
            let final_path = dir.join("control_pubkey.hex");
            let pub_hex = to_hex(control_signing_key.verifying_key().to_bytes().as_slice());
            match std::fs::write(&tmp, pub_hex.as_bytes())
                .and_then(|_| std::fs::rename(&tmp, &final_path))
            {
                Ok(()) => tracing::info!(
                    path = %final_path.display(),
                    "clé publique du contrôle écrite (vérification publique du journal)"
                ),
                Err(e) => tracing::warn!(
                    path = %final_path.display(),
                    detail = %e,
                    "écriture de la clé publique du contrôle impossible"
                ),
            }
        }
    }

    // Store : PostgreSQL si CLOISON_DATABASE_URL (feature `pg`), sinon mémoire.
    let store: Arc<dyn cloison_control::store::Store> = match std::env::var("CLOISON_DATABASE_URL")
    {
        #[cfg(feature = "pg")]
        Ok(url) => {
            let pool = cloison_control::postgres::PostgresStore::connect(&url, 5).await?;
            tracing::info!("cloison-control : store PostgreSQL connecté (0 PII, jetons hachés)");
            Arc::new(pool)
        }
        #[cfg(not(feature = "pg"))]
        Ok(_) => {
            tracing::warn!(
                "CLOISON_DATABASE_URL défini mais binaire compilé sans feature `pg` \
                 (cargo build -p cloison-control --features pg) — repli InMemoryStore"
            );
            Arc::new(InMemoryStore::new())
        }
        Err(_) => {
            tracing::warn!("CLOISON_DATABASE_URL absent : InMemoryStore (perte au restart, dev)");
            Arc::new(InMemoryStore::new())
        }
    };

    let state = AppState::from_env(store, agent_verify_key, control_signing_key)?;
    let app = router(state);

    let listener = TcpListener::bind(addr)
        .await
        .map_err(|_| ControlError::TokenInvalid)?;
    tracing::info!(%addr, "cloison-control prêt");
    serve(listener, app)
        .await
        .map_err(|_| ControlError::TokenInvalid)?;
    Ok(())
}
