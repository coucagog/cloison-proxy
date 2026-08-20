//! Error types for cloison-core.
//!
//! Unified error handling using `thiserror` for ergonomic error propagation.

use thiserror::Error;

/// Unified error type for cloison-core operations.
#[derive(Debug, Error)]
pub enum CloisonError {
    /// Detection-related errors (regex compilation, gazetteer).
    #[error("detection error: {0}")]
    Detection(String),

    /// Token HMAC mismatch - forged or out-of-registry token.
    #[error("token HMAC mismatch: forged or invalid token")]
    TokenMacMismatch,

    /// Token body not found in emission registry.
    #[error("token not in emission registry")]
    TokenNotInRegistry,

    /// Invalid sentinel format.
    #[error("invalid sentinel format: {0}")]
    SentinelFormat(String),

    /// Vault-related errors.
    #[error("vault error: {0}")]
    Vault(String),

    /// Vault TTL expired for token.
    #[error("vault TTL expired for token: {0}")]
    VaultTtlExpired(String),

    /// Cardinality too low for tokenization.
    #[error("cardinality too low ({0}) for tokenization")]
    GeneralizeLowCardinality(usize),

    /// Unknown detector in policy.
    #[error("unknown detector in policy: {0}")]
    PolicyUnknownDetector(String),

    /// HKDF derivation error.
    #[error("HKDF error: {0}")]
    Hkdf(String),

    /// AES-GCM encryption/decryption error.
    #[error("AES-GCM error: {0}")]
    AesGcm(String),

    /// redb database error.
    #[error("redb error: {0}")]
    Redb(String),

    /// I/O error.
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),

    /// JSON serialization/deserialization error.
    #[error("JSON error: {0}")]
    Json(#[from] serde_json::Error),

    /// Base32 encoding/decoding error.
    #[error("base32 error: {0}")]
    Base32(String),

    /// Invalid key length.
    #[error("invalid key length: expected {expected}, got {actual}")]
    InvalidKeyLength {
        /// Expected key length in bytes.
        expected: usize,
        /// Actual key length in bytes.
        actual: usize,
    },

    /// Session not found.
    #[error("session not found: {0}")]
    SessionNotFound(u32),

    /// Invalid session state.
    #[error("invalid session state: {0}")]
    InvalidSessionState(String),

    /// Missing credential.
    #[error("missing credential: {0}")]
    MissingCredential(String),

    /// Policy violation.
    #[error("policy violation: {0}")]
    PolicyViolation(String),
}

/// Result type alias for cloison-core operations.
pub type CloisonResult<T> = Result<T, CloisonError>;

#[cfg(feature = "native")]
impl From<redb::Error> for CloisonError {
    fn from(err: redb::Error) -> Self {
        CloisonError::Redb(err.to_string())
    }
}

#[cfg(feature = "native")]
impl From<redb::TransactionError> for CloisonError {
    fn from(err: redb::TransactionError) -> Self {
        CloisonError::Redb(err.to_string())
    }
}

#[cfg(feature = "native")]
impl From<redb::StorageError> for CloisonError {
    fn from(err: redb::StorageError) -> Self {
        CloisonError::Redb(err.to_string())
    }
}
