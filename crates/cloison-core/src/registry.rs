//! Emission registry module.
//!
//! Per-request registry of emitted token bodies.
//! Only tokens present in the registry are eligible for restoration.
//! This guarantees that a token not emitted by this session/request
//! can never be restored — even if the format is valid.

use std::collections::{HashMap, HashSet};

use serde::{Deserialize, Serialize};

use crate::detection::DetectorKind;
use crate::token::TokenBody;

/// Registry of emitted tokens for a single request.
///
/// Pure in-memory structure, created per-request and cleared after use.
pub struct IssuanceRegistry {
    /// Set of emitted token bodies.
    emitted: HashSet<TokenBody>,
    /// Reverse mapping: token_body → (plain_value, kind) for fast restoration.
    reverse: HashMap<TokenBody, (String, DetectorKind)>,
}

impl IssuanceRegistry {
    /// Create an empty registry.
    pub fn new() -> Self {
        Self {
            emitted: HashSet::new(),
            reverse: HashMap::new(),
        }
    }

    /// Register an emitted token body with its clear value and entity type.
    /// Returns `false` if already present (idempotent, no error).
    pub fn insert(&mut self, body: &TokenBody, plain_value: &str, kind: &DetectorKind) -> bool {
        if self.emitted.contains(body) {
            return false;
        }
        self.emitted.insert(body.clone());
        self.reverse.insert(body.clone(), (plain_value.to_string(), kind.clone()));
        true
    }

    /// Check if a token body has been emitted in this request.
    pub fn contains(&self, body: &TokenBody) -> bool {
        self.emitted.contains(body)
    }

    /// Retrieve the clear value and entity type for a token body.
    /// Returns `None` if the body is not in the registry.
    pub fn get(&self, body: &TokenBody) -> Option<&(String, DetectorKind)> {
        self.reverse.get(body)
    }

    /// Number of emitted tokens.
    pub fn len(&self) -> usize {
        self.emitted.len()
    }

    /// Whether the registry is empty.
    pub fn is_empty(&self) -> bool {
        self.emitted.is_empty()
    }

    /// Clear the registry (called at end of request).
    pub fn clear(&mut self) {
        self.emitted.clear();
        self.reverse.clear();
    }

    /// Create a serializable snapshot of the registry.
    pub fn snapshot(&self) -> RegistrySnapshot {
        let entries = self
            .emitted
            .iter()
            .map(|body| {
                let kind_tag = self
                    .reverse
                    .get(body)
                    .map(|(_, kind)| crate::token::Sentinel::tag_from_kind(kind).to_string())
                    .unwrap_or_default();
                (body.to_base32(), kind_tag)
            })
            .collect();
        RegistrySnapshot { entries }
    }
}

impl Default for IssuanceRegistry {
    fn default() -> Self {
        Self::new()
    }
}

/// Serializable snapshot of the registry (body_b32 + kind_tag pairs).
/// Clear values transit through the vault, not here.
#[derive(Serialize, Deserialize)]
pub struct RegistrySnapshot {
    /// Entries: (token_body_b32, kind_tag).
    pub entries: Vec<(String, String)>,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_body(seed: u8) -> TokenBody {
        TokenBody::from_bytes(vec![seed; 16]).unwrap()
    }

    #[test]
    fn test_insert_and_contains() {
        let mut reg = IssuanceRegistry::new();
        let body = make_body(0x42);
        assert!(reg.insert(&body, "user@example.com", &DetectorKind::Email));
        assert!(reg.contains(&body));
        assert!(!reg.insert(&body, "other@example.com", &DetectorKind::Email));
    }

    #[test]
    fn test_get() {
        let mut reg = IssuanceRegistry::new();
        let body = make_body(0x42);
        reg.insert(&body, "user@example.com", &DetectorKind::Email);
        let (val, kind) = reg.get(&body).unwrap();
        assert_eq!(val, "user@example.com");
        assert_eq!(*kind, DetectorKind::Email);
    }

    #[test]
    fn test_clear() {
        let mut reg = IssuanceRegistry::new();
        let body = make_body(0x42);
        reg.insert(&body, "user@example.com", &DetectorKind::Email);
        assert_eq!(reg.len(), 1);
        reg.clear();
        assert!(reg.is_empty());
        assert!(!reg.contains(&body));
    }

    #[test]
    fn test_snapshot() {
        let mut reg = IssuanceRegistry::new();
        let body = make_body(0x42);
        reg.insert(&body, "user@example.com", &DetectorKind::Email);
        let snap = reg.snapshot();
        assert_eq!(snap.entries.len(), 1);
        assert_eq!(snap.entries[0].1, "EM");
    }
}
