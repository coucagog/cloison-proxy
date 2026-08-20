//! Erreurs unifiées du crate `cloison-audit`.

/// Erreur du module audit.
///
/// Les messages d'erreur ne contiennent jamais de texte PII : le crate ne
/// reçoit que des compteurs en sortie, les erreurs ne font pas exception.
#[derive(Debug, thiserror::Error)]
pub enum AuditError {
    /// Clé Ed25519 invalide (longueur, décodage hex…).
    #[error("invalid ed25519 key: {0}")]
    InvalidKey(String),

    /// Échec de signature ou de vérification Ed25519.
    #[error("ed25519 signature error: {0}")]
    Signature(#[from] ed25519_dalek::SignatureError),

    /// Hex malformé (policy_hash, session_ref…).
    #[error("invalid hex encoding: {0}")]
    InvalidHex(String),

    /// Base64url malformé (header `X-Cloison-Audit-Receipt`).
    #[error("invalid base64url: {0}")]
    InvalidBase64(String),

    /// Seuil k-anonyme invalide (k doit être ≥ 2).
    #[error("k-anonymity threshold must be >= 2, got {0}")]
    InvalidK(usize),

    /// Échec JSON (sérialisation canonique, parse).
    #[error("json error: {0}")]
    Json(#[from] serde_json::Error),

    /// Échec d'E/S (journal des reçus, clé de l'agent…).
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),
}

/// Résultat typé du module audit.
pub type AuditResult<T> = Result<T, AuditError>;
