//! Engine module: orchestrates detection, tokenization, generalization, and restoration.
//!
//! The Engine is the main entry point for CLOISON operations:
//! - `tokenize`: detect PII, generalize low-cardinality, tokenize the rest, replace in text
//! - `restore`: scan for sentinels, verify registry + MAC, restore from vault

use crate::detection::{Detector, Span};
use crate::error::{CloisonError, CloisonResult};
use crate::generalize::Generalizer;
use crate::policy::Policy;
use crate::registry::IssuanceRegistry;
use crate::token::{Sentinel, SessionKeys, Token, TokenBody};
use crate::vault::Vault;

/// Reference to an emitted token.
#[derive(Debug, Clone)]
pub struct TokenRef {
    /// Token body base32.
    pub body_b32: String,
    /// Kind tag.
    pub kind_tag: String,
    /// Original clear value.
    pub plain_value: String,
    /// Sentinel string.
    pub sentinel: String,
}

/// Result of a tokenize operation.
#[derive(Debug, Clone)]
pub struct TokenizeResult {
    /// Text with PII replaced by sentinels or generalizations.
    pub text_out: String,
    /// Emitted token references.
    pub emitted: Vec<TokenRef>,
}

/// Counters for restoration operations.
#[derive(Debug, Clone, Default)]
pub struct RestoreCounters {
    /// Successfully restored tokens.
    pub restored: usize,
    /// Incomplete restorations (sentinel found but couldn't restore).
    pub incomplete: usize,
    /// Blocked restorations (MAC mismatch or not in registry).
    pub blocked: usize,
}

/// Result of a restore operation.
#[derive(Debug, Clone)]
pub struct RestoreResult {
    /// Restored text with clear values.
    pub text_out: String,
    /// Restoration counters.
    pub counters: RestoreCounters,
}

/// The main CLOISON engine.
pub struct Engine {
    /// PII detector.
    detector: Detector,
    /// Encryption vault (optional: may be absent in WASM).
    vault: Option<Vault>,
    /// Session keys.
    keys: SessionKeys,
    /// Generalizer for low-cardinality PII.
    generalizer: Generalizer,
    /// Per-request emission registry.
    registry: IssuanceRegistry,
}

impl Engine {
    /// Create a new engine with the given session keys.
    pub fn new(keys: SessionKeys) -> CloisonResult<Self> {
        let detector = Detector::new()?;
        Ok(Self {
            detector,
            vault: None,
            keys,
            generalizer: Generalizer::new(),
            registry: IssuanceRegistry::new(),
        })
    }

    /// Create a new engine with a vault.
    pub fn with_vault(keys: SessionKeys, vault: Vault) -> CloisonResult<Self> {
        let detector = Detector::new()?;
        Ok(Self {
            detector,
            vault: Some(vault),
            keys,
            generalizer: Generalizer::new(),
            registry: IssuanceRegistry::new(),
        })
    }

    /// Set a custom generalizer.
    pub fn set_generalizer(&mut self, generalizer: Generalizer) {
        self.generalizer = generalizer;
    }

    /// Tokenize text according to the given policy.
    ///
    /// Steps:
    ///   1. Detect all PII matches (filtered by policy)
    ///   2. For each match:
    ///      a. If generalization rule exists → generalize (never tokenize)
    ///      b. Otherwise → emit token, register, store in vault, replace with sentinel
    ///   3. Return tokenized text and emitted token references
    pub fn tokenize(&mut self, text: &str, policy: &Policy, _request_id: &str) -> CloisonResult<TokenizeResult> {
        // Step 1: Detect
        let spans = self.detector.detect_with_policy(text, &policy.detection);

        // Sort by descending position so replacement doesn't break offsets
        let mut spans = spans;
        spans.sort_by_key(|b| std::cmp::Reverse(b.start));

        let mut text_out = text.to_string();
        let mut emitted = Vec::new();

        for span in spans {
            // Step 2a: Check generalization
            if policy.should_generalize(&span.entity_type) || self.generalizer.has_rule(&span.entity_type) {
                let replacement = self.generalizer.generalize(&span.entity_type, &span.value);
                text_out = replace_span(&text_out, span.start, span.end, &replacement);
                continue;
            }

            // Step 2b: Emit token
            let token = Token::emit(&span.value, &span.entity_type, &self.keys)?;

            // Register in emission registry
            self.registry.insert(&token.body, &span.value, &span.entity_type);

            // Store in vault (if available)
            if let Some(ref vault) = self.vault {
                let kind_tag = Sentinel::tag_from_kind(&span.entity_type);
                vault.put(&token.body.to_base32(), &span.value, kind_tag)?;
            }

            let sentinel_str = token.sentinel.format();

            emitted.push(TokenRef {
                body_b32: token.body.to_base32(),
                kind_tag: Sentinel::tag_from_kind(&span.entity_type).to_string(),
                plain_value: span.value.clone(),
                sentinel: sentinel_str.clone(),
            });

            // Replace in text
            text_out = replace_span(&text_out, span.start, span.end, &sentinel_str);
        }

        Ok(TokenizeResult { text_out, emitted })
    }

    /// Restore a tokenized text to its clear form.
    ///
    /// Steps:
    ///   1. Scan for sentinel patterns in the text
    ///   2. For each sentinel (reverse order):
    ///      a. Parse the sentinel
    ///      b. Verify the token body is in the emission registry
    ///      c. Retrieve the clear value (registry first, vault fallback)
    ///      d. Verify MAC integrity
    ///      e. Replace sentinel with clear value
    ///   3. Return restored text and counters
    pub fn restore(&self, text: &str, _request_id: &str) -> CloisonResult<RestoreResult> {
        let mut text_out = text.to_string();
        let mut counters = RestoreCounters::default();

        // Step 1: Extract all sentinel positions (scan forward)
        let sentinel_positions = extract_sentinel_positions(&text_out);

        // Step 2: Process in reverse order to preserve offsets
        for (sentinel_str, start, end) in sentinel_positions.into_iter().rev() {
            let parsed = match Sentinel::parse(&sentinel_str) {
                Some(s) => s,
                None => {
                    counters.blocked += 1;
                    continue;
                }
            };

            let body = match TokenBody::from_base32(&parsed.token_body_b32) {
                Ok(b) => b,
                Err(_) => {
                    counters.blocked += 1;
                    continue;
                }
            };

            let kind = match Sentinel::kind_from_tag(&parsed.kind_tag) {
                Ok(k) => k,
                Err(_) => {
                    counters.blocked += 1;
                    continue;
                }
            };

            // Step 2b: Verify emission registry
            if !self.registry.contains(&body) {
                counters.blocked += 1;
                continue;
            }

            // Step 2c: Retrieve clear value
            let plain_value = if let Some((val, _kind)) = self.registry.get(&body) {
                val.clone()
            } else if let Some(ref vault) = self.vault {
                match vault.get(&parsed.token_body_b32) {
                    Ok(Some((val, _tag))) => val,
                    Ok(None) => {
                        counters.incomplete += 1;
                        continue;
                    }
                    Err(CloisonError::VaultTtlExpired(_)) => {
                        counters.incomplete += 1;
                        continue;
                    }
                    Err(_) => {
                        counters.incomplete += 1;
                        continue;
                    }
                }
            } else {
                counters.incomplete += 1;
                continue;
            };

            // Step 2d: Verify MAC integrity
            if !Token::verify_body(&body, &plain_value, &kind, &self.keys) {
                counters.blocked += 1;
                continue;
            }

            // Step 2e: Replace sentinel with clear value
            text_out = replace_span(&text_out, start, end, &plain_value);
            counters.restored += 1;
        }

        Ok(RestoreResult { text_out, counters })
    }

    /// Clear the emission registry (call at end of request).
    pub fn clear_registry(&mut self) {
        self.registry.clear();
    }

    /// Get the current emission registry.
    pub fn registry(&self) -> &IssuanceRegistry {
        &self.registry
    }

    /// Get the session keys.
    pub fn keys(&self) -> &SessionKeys {
        &self.keys
    }

    /// Get the detector.
    pub fn detector(&self) -> &Detector {
        &self.detector
    }

    /// Run detection only (no tokenization).
    pub fn detect(&self, text: &str, policy: &Policy) -> Vec<Span> {
        self.detector.detect_with_policy(text, &policy.detection)
    }
}

/// Replace a span [start, end) in text with a replacement string.
fn replace_span(text: &str, start: usize, end: usize, replacement: &str) -> String {
    let mut result = String::with_capacity(text.len() + replacement.len());
    result.push_str(&text[..start]);
    result.push_str(replacement);
    result.push_str(&text[end..]);
    result
}

/// Extract all sentinel positions from text.
/// Returns (sentinel_string, start, end) tuples in forward order.
fn extract_sentinel_positions(text: &str) -> Vec<(String, usize, usize)> {
    let mut positions = Vec::new();
    let open = Sentinel::L_OPEN;
    let close = Sentinel::L_CLOSE;
    let open_str = open.to_string();
    let close_str = close.to_string();

    let mut search_start = 0;
    while search_start < text.len() {
        // Find opening delimiter
        let Some(open_pos) = text[search_start..].find(&open_str) else {
            break;
        };
        let abs_open = search_start + open_pos;

        // Find closing delimiter after the opening
        let after_open = abs_open + open.len_utf8();
        let Some(close_pos) = text[after_open..].find(&close_str) else {
            // Ouverture non fermee : sentinelle tronquee (ex. coupure max_tokens).
            // On signale le fragment : le moteur de restauration le remplacera
            // par le marqueur neutre (fail-loud) et incrementera `incomplete`.
            // Jamais de jeton brut transmis.
            positions.push((text[abs_open..].to_string(), abs_open, text.len()));
            break;
        };
        let abs_close = after_open + close_pos + close.len_utf8();

        let sentinel_str = text[abs_open..abs_close].to_string();

        // Validate that this looks like a sentinel
        if let Some(_parsed) = Sentinel::parse(&sentinel_str) {
            positions.push((sentinel_str, abs_open, abs_close));
        }

        search_start = abs_close;
    }

    positions
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::policy::Policy;
    use crate::token::SessionKeys;

    fn test_keys() -> SessionKeys {
        SessionKeys::derive([0xABu8; 32], [0xCDu8; 16]).unwrap()
    }

    #[test]
    fn test_tokenize_restore_roundtrip() {
        let keys = test_keys();
        let mut engine = Engine::new(keys).unwrap();
        let policy = Policy::default();

        let original = "Contact: user@example.com ou +221 77 123 45 67";
        let result = engine.tokenize(original, &policy, "req-1").unwrap();

        // Text should not contain the original PII values
        assert!(!result.text_out.contains("user@example.com"));

        // Restore
        let restored = engine.restore(&result.text_out, "req-1").unwrap();
        assert_eq!(restored.text_out, original);
        assert!(restored.counters.blocked == 0);
    }

    #[test]
    fn test_no_clear_leaving() {
        let keys = test_keys();
        let mut engine = Engine::new(keys).unwrap();
        let policy = Policy::default();

        let original = "Email: test@test.com";
        let result = engine.tokenize(original, &policy, "req-2").unwrap();
        assert!(!result.text_out.contains("test@test.com"));
    }

    #[test]
    fn test_blocked_fake_sentinel() {
        let keys = test_keys();
        let mut engine = Engine::new(keys).unwrap();
        let policy = Policy::default();

        // First tokenize something to set up the registry
        let _ = engine.tokenize("Contact: user@example.com", &policy, "req-3").unwrap();

        // Now try to restore text with a forged sentinel
        let fake_sentinel = format!(
            "{}{}{}{}{}",
            Sentinel::L_OPEN,
            "AAAAAAAAAAAAAAAAAAAAAAAAAA",
            Sentinel::L_SEP,
            "EM",
            Sentinel::L_CLOSE
        );

        let result = engine.restore(&fake_sentinel, "req-3").unwrap();
        assert!(result.counters.blocked > 0, "Forged sentinel should be blocked");
    }

    #[test]
    fn test_determinism_same_session() {
        let keys = test_keys();
        let mut engine1 = Engine::new(keys.clone()).unwrap();
        let mut engine2 = Engine::new(keys).unwrap();
        let policy = Policy::default();

        let r1 = engine1.tokenize("user@example.com", &policy, "req-a").unwrap();
        let r2 = engine2.tokenize("user@example.com", &policy, "req-b").unwrap();

        assert_eq!(r1.emitted[0].body_b32, r2.emitted[0].body_b32);
    }

    #[test]
    fn test_rotation_different_session() {
        let keys1 = SessionKeys::derive([0xABu8; 32], [0x01u8; 16]).unwrap();
        let keys2 = SessionKeys::derive([0xABu8; 32], [0x02u8; 16]).unwrap();
        let mut engine1 = Engine::new(keys1).unwrap();
        let mut engine2 = Engine::new(keys2).unwrap();
        let policy = Policy::default();

        let r1 = engine1.tokenize("user@example.com", &policy, "req-a").unwrap();
        let r2 = engine2.tokenize("user@example.com", &policy, "req-b").unwrap();

        assert_ne!(r1.emitted[0].body_b32, r2.emitted[0].body_b32);
    }
}
