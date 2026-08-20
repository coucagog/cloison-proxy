//! Binaire du plan de contrôle CLOISON : sert l'API admin REST.
//! Variables d'environnement :
//!   CLOISON_CONTROL_PORT  (défaut 8788)
//!   CLOISON_LEDGER_FILE   (chemin du journal append-only, optionnel)
//!   CLOISON_ROTATION_GRACE_SECONDS (défaut 300)
//!   CLOISON_AGENT_VERIFY_KEY (clé publique Ed25519 de l'agent, hex 64 — optionnelle)
//!   CLOISON_CONTROL_SIGNING_KEY (clé privée Ed25519 du control, hex 64 — générée si absente)

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

#[tokio::main]
async fn main() -> ControlResult<()> {
    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::from_default_env())
        .init();

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
            let arr: [u8; 32] = bytes
                .try_into()
                .map_err(|_| ControlError::TokenInvalid)?;
            VerifyingKey::from_bytes(&arr).map_err(|_| ControlError::TokenInvalid)?
        }
        None => {
            tracing::warn!("CLOISON_AGENT_VERIFY_KEY absent : génération d'une paire éphémère (dev)");
            SigningKey::generate(&mut rand::rngs::OsRng).verifying_key()
        }
    };
    let control_signing_key = match std::env::var("CLOISON_CONTROL_SIGNING_KEY").ok() {
        Some(h) => {
            let bytes = hex_bytes(&h, "control signing key")?;
            let arr: [u8; 32] = bytes
                .try_into()
                .map_err(|_| ControlError::TokenInvalid)?;
            SigningKey::from_bytes(&arr)
        }
        None => {
            tracing::warn!("CLOISON_CONTROL_SIGNING_KEY absent : génération éphémère (dev)");
            SigningKey::generate(&mut rand::rngs::OsRng)
        }
    };

    let state = AppState::from_env(
        Arc::new(InMemoryStore::new()),
        agent_verify_key,
        control_signing_key,
    )?;
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
