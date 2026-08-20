//! Erreurs unifiées du proxy : catégorie → statut HTTP + shape d'erreur OpenAI.
//!
//! Le `message` exposé au client ne contient **jamais** de secret : les
//! constructeurs n'acceptent que des messages publics. Les détails techniques
//! (jamais la clé amont) vont dans `log_fields`, consommés par le log structuré.

use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::Json;
use serde_json::json;

use cloison_core::CloisonError;

/// Catégorie d'erreur → statut HTTP et shape OpenAI.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ErrorKind {
    /// Clé composite absente ou malformée → 401.
    Auth,
    /// Accès interdit → 403.
    Forbidden,
    /// Corps invalide → 400.
    BadRequest,
    /// Corps trop volumineux → 413.
    PayloadTooLarge,
    /// Ressource indisponible (ex. rapport d'audit hors mode audit) → 404.
    NotFound,
    /// Quota / débit → 429.
    RateLimited,
    /// Erreur du fournisseur → 502.
    Upstream,
    /// Timeout du fournisseur → 504.
    UpstreamTimeout,
    /// Erreur interne → 500.
    Internal,
}

impl ErrorKind {
    /// Statut HTTP associé.
    pub fn status(self) -> StatusCode {
        match self {
            ErrorKind::Auth => StatusCode::UNAUTHORIZED,
            ErrorKind::Forbidden => StatusCode::FORBIDDEN,
            ErrorKind::BadRequest => StatusCode::BAD_REQUEST,
            ErrorKind::PayloadTooLarge => StatusCode::PAYLOAD_TOO_LARGE,
            ErrorKind::NotFound => StatusCode::NOT_FOUND,
            ErrorKind::RateLimited => StatusCode::TOO_MANY_REQUESTS,
            ErrorKind::Upstream => StatusCode::BAD_GATEWAY,
            ErrorKind::UpstreamTimeout => StatusCode::GATEWAY_TIMEOUT,
            ErrorKind::Internal => StatusCode::INTERNAL_SERVER_ERROR,
        }
    }

    /// Type d'erreur OpenAI.
    pub fn openai_type(self) -> &'static str {
        match self {
            ErrorKind::Auth => "authentication_error",
            ErrorKind::Forbidden => "permission_error",
            ErrorKind::BadRequest | ErrorKind::PayloadTooLarge | ErrorKind::NotFound => "invalid_request_error",
            ErrorKind::RateLimited => "rate_limit_error",
            ErrorKind::Upstream | ErrorKind::UpstreamTimeout | ErrorKind::Internal => "server_error",
        }
    }

    /// Code d'erreur OpenAI.
    pub fn openai_code(self) -> &'static str {
        match self {
            ErrorKind::Auth => "invalid_api_key",
            ErrorKind::Forbidden => "permission_denied",
            ErrorKind::BadRequest => "invalid_request_error",
            ErrorKind::PayloadTooLarge => "request_too_large",
            ErrorKind::NotFound => "not_found",
            ErrorKind::RateLimited => "rate_limit_exceeded",
            ErrorKind::Upstream => "upstream_error",
            ErrorKind::UpstreamTimeout => "upstream_timeout",
            ErrorKind::Internal => "internal_error",
        }
    }
}

/// Erreur de bout en bout. `message` = message public (aucun secret).
/// `log_fields` = paires clé/valeur pour le log structuré (request_id, statut, compteurs…).
#[derive(Debug, thiserror::Error)]
#[error("{message}")]
pub struct ProxyError {
    pub kind: ErrorKind,
    pub message: String,
    pub log_fields: Vec<(String, String)>,
}

impl ProxyError {
    /// Construit une erreur avec un message public (statique, sans secret).
    pub fn new(kind: ErrorKind, message: impl Into<String>) -> Self {
        Self {
            kind,
            message: message.into(),
            log_fields: Vec::new(),
        }
    }

    /// Ajoute un champ de log structuré (jamais un secret).
    pub fn with_field(mut self, key: &str, value: impl Into<String>) -> Self {
        self.log_fields.push((key.to_string(), value.into()));
        self
    }

    /// `request_id` associé, si présent dans les champs de log.
    pub fn request_id(&self) -> &str {
        self.log_fields
            .iter()
            .find(|(k, _)| k == "request_id")
            .map(|(_, v)| v.as_str())
            .unwrap_or("")
    }
}

impl From<reqwest::Error> for ProxyError {
    /// Toute erreur réseau amont → 502 (ou 504 si timeout). Le détail (éventuellement
    /// l'URL) ne va que dans le log, jamais au client.
    fn from(e: reqwest::Error) -> Self {
        let (kind, message) = if e.is_timeout() {
            (ErrorKind::UpstreamTimeout, "upstream request timed out")
        } else {
            (ErrorKind::Upstream, "upstream request failed")
        };
        ProxyError::new(kind, message).with_field("detail", truncate(&e.to_string(), 512))
    }
}

impl From<serde_json::Error> for ProxyError {
    fn from(e: serde_json::Error) -> Self {
        ProxyError::new(ErrorKind::BadRequest, "invalid JSON").with_field("detail", truncate(&e.to_string(), 512))
    }
}

impl From<url::ParseError> for ProxyError {
    fn from(e: url::ParseError) -> Self {
        ProxyError::new(ErrorKind::Internal, "invalid URL").with_field("detail", e.to_string())
    }
}

impl From<CloisonError> for ProxyError {
    /// Toute erreur interne de `cloison-core` remonte en 500 et est loggée
    /// avec le `request_id` (invariant I8 : échec = échec, jamais silencieux).
    fn from(e: CloisonError) -> Self {
        ProxyError::new(ErrorKind::Internal, "internal tokenization error").with_field("detail", truncate(&e.to_string(), 512))
    }
}

impl IntoResponse for ProxyError {
    fn into_response(self) -> Response {
        let request_id = self.request_id();
        tracing::warn!(
            target: "cloison_proxy::api",
            request_id = %request_id,
            kind = ?self.kind,
            log_fields = ?self.log_fields,
            "api error: {}",
            self.message,
        );
        let body = json!({
            "error": {
                "message": self.message,
                "type": self.kind.openai_type(),
                "code": self.kind.openai_code(),
            }
        });
        (self.kind.status(), Json(body)).into_response()
    }
}

/// Tronque une chaîne au niveau d'un caractère (pour les logs).
pub(crate) fn truncate(s: &str, max: usize) -> String {
    if s.len() <= max {
        return s.to_string();
    }
    s.chars().take(max).collect()
}
