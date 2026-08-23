//! CLOISON STACK-2: cloison-core
//!
//! Deterministic portable core (native + WASM) for PII privacy proxy.
//!
//! # Modules
//!
//! - `error`: Unified error types
//! - `detection`: PII detection (regex, Aho-Corasick gazetteers, Luhn)
//! - `token`: Tokenization (HMAC-BLAKE3, sentinel format, session key derivation)
//! - `registry`: Per-request emission registry
//! - `vault`: Encrypted vault (redb + AES-256-GCM)
//! - `generalize`: Low-cardinality generalization
//! - `policy`: Per-tenant policy configuration
//! - `engine`: Orchestration (tokenize + restore)
//! - `wasm`: WASM bindings (gated behind target_arch = "wasm32")

#![warn(missing_docs)]

pub mod detection;
pub mod engine;
pub mod error;
pub mod generalize;
pub mod policy;
pub mod registry;
pub mod token;
pub mod vault;

#[cfg(target_arch = "wasm32")]
pub mod wasm;

// Public re-exports
pub use detection::{validate_luhn, Detector, DetectorKind, Gazetteer, Span};
pub use engine::{Engine, RestoreCounters, RestoreResult, TokenRef, TokenizeResult};
pub use error::{CloisonError, CloisonResult};
pub use generalize::{generalize_age, generalize_date, suppress, GeneralizeRule, Generalizer};
pub use policy::{DetectorPolicy, Policy, SubstitutionMode};
pub use registry::{IssuanceRegistry, RegistrySnapshot};
pub use token::{
    canonicalize, compute_mac, token_body, verify_body, Sentinel, SessionKeys, Token, TokenBody,
};
pub use vault::{Vault, VaultConfig};
