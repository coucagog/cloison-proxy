//! Assemblage du routeur : 3 routes + middleware d'auth + limite de corps.

use std::sync::Arc;

use axum::routing::{get, post};
use axum::Router;
use tower_http::limit::RequestBodyLimitLayer;

use crate::auth;
use crate::handlers::{self, AppState};

/// Assemble le routeur : `POST /v1/chat/completions`, `POST /v1/completions`,
/// `GET /v1/models`, couche d'auth `from_fn_with_state` et limite de corps.
pub fn router(state: Arc<AppState>) -> Router {
    let max_body_bytes = state.upstream.config.max_body_bytes;
    Router::new()
        .route("/v1/chat/completions", post(handlers::chat_completions))
        .route("/v1/completions", post(handlers::completions_legacy))
        .route("/v1/models", get(handlers::models))
        .layer(axum::middleware::from_fn_with_state(state.clone(), auth::auth_middleware))
        .layer(RequestBodyLimitLayer::new(max_body_bytes))
        .with_state(state)
}
