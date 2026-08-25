//! Generalization module for low-cardinality entities.
//!
//! Low-cardinality PII (age, date, etc.) is never tokenized.
//! Instead, it is generalized (bucketed into ranges) or suppressed entirely.
//! This prevents trivial re-identification from small value spaces.

use serde::{Deserialize, Serialize};

use crate::detection::DetectorKind;

/// Rule for generalizing a specific PII type.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum GeneralizeRule {
    /// Partial mask: keep N characters at start and/or end.
    Mask {
        /// Number of characters to keep at the start.
        keep_start: usize,
        /// Number of characters to keep at the end.
        keep_end: usize,
        /// Character used to mask the middle portion.
        mask_char: char,
    },
    /// Range prefix: generalize to a prefix pattern.
    Range {
        /// Prefix pattern to generalize to.
        prefix: String,
    },
    /// Replace with a constant category label.
    Replace {
        /// Constant category label used as replacement.
        label: String,
    },
    /// Age bucket: generalize to 5-year ranges.
    AgeBucket,
    /// Date bucket: generalize to month+year.
    DateBucket,
    /// Suppress entirely.
    Suppress,
    /// No generalization (keep sentinel as-is).
    None,
}

/// Default generalization rules.
pub struct Generalizer {
    /// Rules per DetectorKind.
    rules: std::collections::HashMap<DetectorKind, GeneralizeRule>,
}

impl Generalizer {
    /// Create a generalizer with default rules.
    pub fn new() -> Self {
        let mut rules = std::collections::HashMap::new();

        // Age: bucket into 5-year ranges
        rules.insert(DetectorKind::Date, GeneralizeRule::DateBucket);

        // IP: range prefix (first 2 octets)
        rules.insert(
            DetectorKind::Ip,
            GeneralizeRule::Range {
                prefix: "[IP]".to_string(),
            },
        );

        // Credit card: mask most digits
        rules.insert(
            DetectorKind::CreditCard,
            GeneralizeRule::Mask {
                keep_start: 0,
                keep_end: 4,
                mask_char: '•',
            },
        );

        Self { rules }
    }

    /// Add or replace a rule for a specific PII type.
    pub fn add_rule(&mut self, kind: DetectorKind, rule: GeneralizeRule) {
        self.rules.insert(kind, rule);
    }

    /// Applique une règle donnée à une valeur (sans l'ajouter au registre).
    ///
    /// Utilisé par `Engine::process_spans` quand la **politique** porte la
    /// règle (ex. `Policy::n0_for` : ville_sn → `[VILLE_SN]`) — le
    /// `Generalizer` par défaut ne connaît que ses règles intégrées (Date,
    /// IP, carte) et rendrait la valeur brute sinon (bug corrigé : la ville
    /// restait en clair en mode N0).
    pub fn apply_rule(&self, rule: &GeneralizeRule, _kind: &DetectorKind, value: &str) -> String {
        match rule {
            GeneralizeRule::Mask {
                keep_start,
                keep_end,
                mask_char,
            } => {
                let chars: Vec<char> = value.chars().collect();
                let len = chars.len();
                if *keep_start + *keep_end >= len {
                    return value.to_string();
                }
                let mut result = String::with_capacity(len);
                for (i, &c) in chars.iter().enumerate() {
                    if i < *keep_start || i >= len - *keep_end {
                        result.push(c);
                    } else {
                        result.push(*mask_char);
                    }
                }
                result
            }
            GeneralizeRule::Range { prefix } => prefix.clone(),
            GeneralizeRule::Replace { label } => label.clone(),
            GeneralizeRule::AgeBucket => generalize_age_value(value),
            GeneralizeRule::DateBucket => generalize_date_value(value),
            GeneralizeRule::Suppress => suppress(),
            GeneralizeRule::None => value.to_string(),
        }
    }

    /// Check if a rule exists for the given kind.
    pub fn has_rule(&self, kind: &DetectorKind) -> bool {
        self.rules.contains_key(kind)
    }

    /// Apply generalization to a detected value.
    /// Returns the generalized string replacement.
    pub fn generalize(&self, kind: &DetectorKind, value: &str) -> String {
        match self.rules.get(kind) {
            Some(rule) => match rule {
                GeneralizeRule::Mask {
                    keep_start,
                    keep_end,
                    mask_char,
                } => {
                    let chars: Vec<char> = value.chars().collect();
                    let len = chars.len();
                    if *keep_start + *keep_end >= len {
                        return value.to_string();
                    }
                    let mut result = String::with_capacity(len);
                    for (i, &c) in chars.iter().enumerate() {
                        if i < *keep_start || i >= len - *keep_end {
                            result.push(c);
                        } else {
                            result.push(*mask_char);
                        }
                    }
                    result
                }
                GeneralizeRule::Range { prefix } => prefix.clone(),
                GeneralizeRule::Replace { label } => label.clone(),
                GeneralizeRule::AgeBucket => generalize_age_value(value),
                GeneralizeRule::DateBucket => generalize_date_value(value),
                GeneralizeRule::Suppress => suppress(),
                GeneralizeRule::None => value.to_string(),
            },
            None => value.to_string(),
        }
    }
}

impl Default for Generalizer {
    fn default() -> Self {
        Self::new()
    }
}

/// Generalize an age value into a 5-year range bucket.
///
/// Example: 27 → "25-29", 32 → "30-34"
pub fn generalize_age(age: u32) -> String {
    let lower = (age / 5) * 5;
    let upper = lower + 4;
    format!("{}-{}", lower, upper)
}

/// Try to generalize an age string value.
fn generalize_age_value(value: &str) -> String {
    match value.trim().parse::<u32>() {
        Ok(age) => generalize_age(age),
        Err(_) => suppress(),
    }
}

/// Generalize a date to month+year.
///
/// Example: "2024-03-15" → "2024-03", "15/03/2024" → "2024-03"
pub fn generalize_date(date: &str) -> String {
    let date = date.trim();
    // Try ISO format: YYYY-MM-DD
    if date.len() >= 7 {
        if let Some(pos) = date.find('-') {
            if pos == 4 {
                return date[..7].to_string(); // YYYY-MM
            }
        }
    }
    // Try DD/MM/YYYY
    let parts: Vec<&str> = date.split('/').collect();
    if parts.len() == 3 && parts[2].len() == 4 {
        return format!("{}-{}", parts[2], parts[1]); // YYYY-MM
    }
    // Fallback: suppress
    suppress()
}

fn generalize_date_value(value: &str) -> String {
    generalize_date(value)
}

/// Suppression marker: replaces the value entirely.
pub fn suppress() -> String {
    "[REDACTED]".to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_generalize_age() {
        assert_eq!(generalize_age(0), "0-4");
        assert_eq!(generalize_age(4), "0-4");
        assert_eq!(generalize_age(5), "5-9");
        assert_eq!(generalize_age(27), "25-29");
        assert_eq!(generalize_age(32), "30-34");
        assert_eq!(generalize_age(99), "95-99");
    }

    #[test]
    fn test_generalize_date_iso() {
        assert_eq!(generalize_date("2024-03-15"), "2024-03");
    }

    #[test]
    fn test_generalize_date_eu() {
        assert_eq!(generalize_date("15/03/2024"), "2024-03");
    }

    #[test]
    fn test_suppress() {
        assert_eq!(suppress(), "[REDACTED]");
    }

    #[test]
    fn test_generalizer_mask() {
        let mut gen = Generalizer::new();
        gen.add_rule(
            DetectorKind::CniSn,
            GeneralizeRule::Mask {
                keep_start: 0,
                keep_end: 4,
                mask_char: '•',
            },
        );
        let result = gen.generalize(&DetectorKind::CniSn, "1234567890123");
        // 13 caracteres, keep_end=4 : 9 masques + 4 gardes = 13 (longueur preservee)
        assert_eq!(result, "•••••••••0123");
        assert_eq!(result.chars().count(), 13);
    }

    #[test]
    fn test_generalizer_replace() {
        let mut gen = Generalizer::new();
        gen.add_rule(
            DetectorKind::Gazetteer("ville_sn".to_string()),
            GeneralizeRule::Replace {
                label: "[VILLE_SN]".to_string(),
            },
        );
        let result = gen.generalize(&DetectorKind::Gazetteer("ville_sn".to_string()), "Dakar");
        assert_eq!(result, "[VILLE_SN]");
    }

    #[test]
    fn test_generalizer_suppress() {
        let mut gen = Generalizer::new();
        gen.add_rule(DetectorKind::Ip, GeneralizeRule::Suppress);
        let result = gen.generalize(&DetectorKind::Ip, "192.168.1.1");
        assert_eq!(result, "[REDACTED]");
    }

    #[test]
    fn test_generalizer_no_rule() {
        let gen = Generalizer::new();
        let result = gen.generalize(&DetectorKind::Email, "user@example.com");
        assert_eq!(result, "user@example.com");
    }
}
