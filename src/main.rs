//! Bootstrap : config → état → routeur → serveur.
//!
//! Port par défaut : 8787 (`CLOISON_PROXY_PORT` ou `CLOISON_LISTEN_ADDR`).

use std::process::ExitCode;
use std::sync::Arc;

use cloison_proxy::{config, handlers::AppState, routes};

#[tokio::main]
async fn main() -> ExitCode {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("cloison_proxy=info")),
        )
        .init();

    let config = match config::load() {
        Ok(c) => c,
        Err(e) => {
            tracing::error!(error = %e, "configuration error");
            return ExitCode::FAILURE;
        }
    };

    let state = match AppState::new(&config) {
        Ok(s) => Arc::new(s),
        Err(e) => {
            tracing::error!(error = %e, "failed to initialize application state");
            return ExitCode::FAILURE;
        }
    };

    // Wiring C — tâches de fond (ingest des reçus d'audit, long-poll des
    // versions) : lancées uniquement quand le contrôle est configuré.
    state.start_background_tasks();

    let listener = match tokio::net::TcpListener::bind(config.listen_addr).await {
        Ok(l) => l,
        Err(e) => {
            tracing::error!(addr = %config.listen_addr, error = %e, "failed to bind listener");
            return ExitCode::FAILURE;
        }
    };

    tracing::info!(addr = %config.listen_addr, mock_mode = config.mock_mode, "cloison-proxy listening");

    if let Err(e) = axum::serve(listener, routes::router(state)).await {
        tracing::error!(error = %e, "server terminated with error");
        return ExitCode::FAILURE;
    }
    ExitCode::SUCCESS
}
