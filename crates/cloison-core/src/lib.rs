//! CLOISON STACK-2: cloison-core
//!
//! Deterministic portable core (native + WASM) for PII privacy proxy.
//!
//! # Modules
//!
//! - `error`: Unified error types
//! - `fake`: Réalistic-fake substitution (déterministe, irréversible)
//! - `detection`: PII detection (regex, Aho-Corasick gazetteers, Luhn)
//! - `token`: Tokenization (HMAC-BLAKE3, sentinel format, session key derivation)
//! - `registry`: Per-request emission registry
//! - `vault`: Encrypted vault (redb + AES-256-GCM)
//! - `generalize`: Low-cardinality generalization
//! - `policy`: Per-tenant policy configuration
//! - `engine`: Orchestration (tokenize + restore)
//! - `wasm`: WASM bindings (gated behind target_arch = "wasm32")

#![warn(missing_docs)]

pub mod alias;
pub mod detection;
pub mod engine;
pub mod error;
pub mod fake;
pub mod generalize;
pub mod policy;
pub mod quasi_id;
pub mod registry;
pub mod token;
pub mod vault;

#[cfg(target_arch = "wasm32")]
pub mod wasm;

/// Arrondi à 4 décimales (miroir de `round(score, 4)` du sidecar — scores
/// d'alias et de la jauge, déterministes).
pub(crate) fn round4(x: f64) -> f64 {
    (x * 10_000.0).round() / 10_000.0
}

// Public re-exports
pub use alias::{
    insensitive_pattern, normalize_text, AliasConfig, AliasExpander, CanonicalMention,
    SessionContext,
};
pub use detection::{validate_luhn, Detector, DetectorKind, Gazetteer, Span};
pub use engine::{
    Engine, RestoreCounters, RestoreResult, SessionOptions, TokenRef, TokenizeResult,
};
pub use error::{CloisonError, CloisonResult};
pub use generalize::{generalize_age, generalize_date, suppress, GeneralizeRule, Generalizer};
pub use policy::{DetectorPolicy, Policy, SubstitutionMode};
pub use quasi_id::{category_for, GaugeConfig, QuasiIdCategory, QuasiIdGauge, QuasiIdReport};
pub use registry::{IssuanceRegistry, RegistrySnapshot};
pub use token::{
    canonicalize, compute_mac, token_body, verify_body, Sentinel, SessionKeys, Token, TokenBody,
};
pub use vault::{derive_key_from_passphrase, Vault, VaultConfig};
