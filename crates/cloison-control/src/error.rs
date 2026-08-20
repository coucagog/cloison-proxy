//! Erreurs du plan de contrôle.
//!
//! Invariant : les messages d'erreur ne contiennent **jamais** de texte utilisateur ni
//! de jeton — uniquement des identifiants opérateur et des hash.

use thiserror::Error;

#[derive(Debug, Error)]
pub enum ControlError {
    #[error("tenant not found: {0}")]
    TenantNotFound(String),

    #[error("tenant already exists: {0}")]
    TenantConflict(String),

    #[error("token not found: {0}")]
    TokenNotFound(String),

    #[error("token already exists")]
    TokenConflict,

    #[error("token invalid or revoked")]
    TokenInvalid,

    #[error("license not found for tenant {0}")]
    LicenseNotFound(String),

    #[error("license expired")]
    LicenseExpired,

    #[error("policy not found for tenant {0}")]
    PolicyNotFound(String),

    #[error("invalid policy: {0}")]
    InvalidPolicy(String),

    /// `sig_agent` d'un reçu STACK-4 invalide (message signé = `signing_bytes()`).
    #[error("invalid agent signature on audit receipt")]
    InvalidAgentSignature,

    /// Ingest refusé (reçu d'un autre tenant, k invalide, aucune entrée…).
    #[error("ingest rejected: {0}")]
    IngestRejected(String),

    #[error("invalid signature: {0}")]
    Signature(#[from] ed25519_dalek::SignatureError),

    #[error("ledger error: {0}")]
    Ledger(#[from] cloison_ledger::LedgerError),

    #[error("audit error: {0}")]
    Audit(#[from] cloison_audit::AuditError),

    #[error("io error: {0}")]
    Io(#[from] std::io::Error),

    #[error("json error: {0}")]
    Json(#[from] serde_json::Error),

    #[error("internal error: {0}")]
    Internal(String),
}

/// Alias de résultat du plan de contrôle.
pub type ControlResult<T> = Result<T, ControlError>;
