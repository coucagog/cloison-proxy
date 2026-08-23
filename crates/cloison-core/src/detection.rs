//! PII detection module.
//!
//! Deterministic detection of personally identifiable information:
//! - Email addresses (RFC-simplified regex)
//! - Senegalese phone numbers (+221)
//! - Senegalese CNI (13 digits with Luhn validation)
//! - Credit card numbers (Luhn validation)
//! - Gazetteers (Aho-Corasick): Senegalese names and toponyms

use std::collections::HashMap;

use aho_corasick::AhoCorasick;
use regex::Regex;
use serde::{Deserialize, Serialize};

use crate::error::{CloisonError, CloisonResult};
use crate::policy::DetectorPolicy;

/// Categories of detectable PII.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum DetectorKind {
    /// Email address.
    Email,
    /// Senegalese phone number: +221 XX XXX XX XX.
    PhoneSn,
    /// Senegalese CNI: 13 digits starting with 1, Luhn-validated.
    CniSn,
    /// Credit card number (Luhn-validated).
    CreditCard,
    /// IP address (v4).
    Ip,
    /// Date pattern.
    Date,
    /// NER sidecar PERSON (cloison-detect, wiring B.1) : le cœur n'émet
    /// jamais ce type lui-même — seuls des spans externes validés le portent.
    Person,
    /// NER sidecar LOC (cloison-detect, wiring B.1) : idem, jamais émis par
    /// les détecteurs embarqués.
    Location,
    /// Named gazetteer (e.g., "ville_sn", "nom_sn").
    Gazetteer(String),
}

impl std::fmt::Display for DetectorKind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            DetectorKind::Email => write!(f, "Email"),
            DetectorKind::PhoneSn => write!(f, "PhoneSn"),
            DetectorKind::CniSn => write!(f, "CniSn"),
            DetectorKind::CreditCard => write!(f, "CreditCard"),
            DetectorKind::Ip => write!(f, "Ip"),
            DetectorKind::Date => write!(f, "Date"),
            DetectorKind::Person => write!(f, "Person"),
            DetectorKind::Location => write!(f, "Location"),
            DetectorKind::Gazetteer(name) => write!(f, "Gazetteer({})", name),
        }
    }
}

/// Occurrence of PII in a text.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Span {
    /// Type of PII detected.
    pub entity_type: DetectorKind,
    /// Start offset (byte) in the source text.
    pub start: usize,
    /// End offset (byte, exclusive) in the source text.
    pub end: usize,
    /// Confidence score (0.0 - 1.0). Always 1.0 for deterministic detectors.
    pub score: f64,
    /// Matched clear value.
    pub value: String,
}

/// Identifiant du gazetteer des prénoms sénégalais (construit dans `Detector::new`).
/// Référencé par `DetectorPolicy::all_enabled` (cloison-core) : les noms
/// sont masqués par défaut (sentinelles `GZ*`, cf. docs/DATA-MODEL.md §1).
pub const GAZETTEER_NOM_SN: &str = "nom_sn";

/// Identifiant du gazetteer des toponymes sénégalais (construit dans `Detector::new`).
/// Référencé par `DetectorPolicy::all_enabled` (cloison-core).
pub const GAZETTEER_VILLE_SN: &str = "ville_sn";

/// A compiled gazetteer (Aho-Corasick) for a named set of terms.
pub struct Gazetteer {
    /// Gazetteer identifier (e.g., "ville_sn").
    pub name: String,
    /// Compiled Aho-Corasick automaton.
    ac: AhoCorasick,
    /// Mapping: pattern index → term.
    patterns: Vec<String>,
}

impl Gazetteer {
    /// Build a gazetteer from an ordered list of terms.
    pub fn new(name: String, terms: Vec<String>) -> CloisonResult<Self> {
        if terms.is_empty() {
            return Ok(Self {
                name,
                ac: AhoCorasick::new([""]).map_err(|e| CloisonError::Detection(e.to_string()))?,
                patterns: vec![],
            });
        }
        let ac = AhoCorasick::builder()
            .ascii_case_insensitive(true)
            .build(&terms)
            .map_err(|e| CloisonError::Detection(e.to_string()))?;
        Ok(Self {
            name,
            ac,
            patterns: terms,
        })
    }

    /// Find all occurrences in `text`, returns matching `Span`s.
    pub fn find(&self, text: &str) -> Vec<Span> {
        let mut results = Vec::new();
        for mat in self.ac.find_iter(text) {
            let pattern = &self.patterns[mat.pattern().as_usize()];
            results.push(Span {
                entity_type: DetectorKind::Gazetteer(self.name.clone()),
                start: mat.start(),
                end: mat.end(),
                score: 1.0,
                value: pattern.clone(),
            });
        }
        results
    }
}

/// Default Senegalese first names for the built-in gazetteer.
const DEFAULT_NAMES_SN: &[&str] = &[
    "Amadou",
    "Mariama",
    "Ousmane",
    "Fatou",
    "Moussa",
    "Aissatou",
    "Ibrahima",
    "Khady",
    "Saliou",
    "Aminata",
    "Cheikh",
    "Dieynaba",
    "Mamadou",
    "Coumba",
    "Abdoulaye",
    "Ndeye",
    "Modou",
    "Sokhna",
    "Boubacar",
    "Yacine",
];

/// Default Senegalese toponyms for the built-in gazetteer.
const DEFAULT_VILLES_SN: &[&str] = &[
    "Dakar",
    "Thiès",
    "Saint-Louis",
    "Ziguinchor",
    "Kaolack",
    "Tambacounda",
    "Kolda",
    "Louga",
    "Diourbel",
    "Matam",
    "Sédhiou",
    "Kédougou",
    "Rufisque",
    "Pikine",
    "Touba",
];

/// PII Detection engine — encapsulates compiled regexes, gazetteers, and Luhn.
pub struct Detector {
    /// Compiled regex for emails.
    email_re: Regex,
    /// Compiled regex for Senegalese phone numbers (+221).
    phone_sn_re: Regex,
    /// Compiled regex for Senegalese CNI (13 digits starting with 1).
    cni_sn_re: Regex,
    /// Compiled regex for credit card numbers (13-19 digits).
    credit_card_re: Regex,
    /// Compiled regex for IPv4 addresses.
    ip_re: Regex,
    /// Registered gazetteers (name → Gazetteer).
    gazetteers: HashMap<String, Gazetteer>,
}

impl Detector {
    /// Construct a detector with default patterns and built-in gazetteers.
    pub fn new() -> CloisonResult<Self> {
        let email_re = Regex::new(
            r"(?i)[\p{L}0-9.!#$%&'*+/=?^_`{|}~-]+@[\p{L}0-9](?:[\p{L}0-9-]{0,61}[\p{L}0-9])?(?:\.[\p{L}0-9](?:[\p{L}0-9-]{0,61}[\p{L}0-9])?)+"
        )
        .map_err(|e| CloisonError::Detection(format!("email regex: {}", e)))?;

        let phone_sn_re = Regex::new(
            r"(?:\+221|00221)\s?(?:7[0-9]|3[0-9])\s?[0-9]{3}\s?[0-9]{2}\s?[0-9]{2}|(?:70|75|76|77|78)(?:[0-9]{7}|\s?[0-9]{3}\s?[0-9]{2}\s?[0-9]{2})"
        )
        .map_err(|e| CloisonError::Detection(format!("phone_sn regex: {}", e)))?;

        // Frontière = début de chaîne OU caractère non-chiffre (pas de lookaround :
        // la crate `regex` standard ne les supporte pas). Le préfixe capturé est
        // retiré par detect_cni_sn (offset ajusté).
        let cni_sn_re = Regex::new(
            r"(?:^|[^0-9])(1\d{2}[ \u00A0]?\d{3}[ \u00A0]?\d{4}[ \u00A0]?\d{3}|1\d{12})",
        )
        .map_err(|e| CloisonError::Detection(format!("cni_sn regex: {}", e)))?;

        let credit_card_re = Regex::new(r"\b(?:\d[ -]?){13,19}\b")
            .map_err(|e| CloisonError::Detection(format!("credit_card regex: {}", e)))?;

        let ip_re = Regex::new(
            r"\b(?:(?:25[0-5]|2[0-4][0-9]|[01]?[0-9][0-9]?)\.){3}(?:25[0-5]|2[0-4][0-9]|[01]?[0-9][0-9]?)\b"
        )
        .map_err(|e| CloisonError::Detection(format!("ip regex: {}", e)))?;

        let mut detector = Self {
            email_re,
            phone_sn_re,
            cni_sn_re,
            credit_card_re,
            ip_re,
            gazetteers: HashMap::new(),
        };

        // Add built-in gazetteers
        let nom_gaz = Gazetteer::new(
            GAZETTEER_NOM_SN.to_string(),
            DEFAULT_NAMES_SN.iter().map(|s| s.to_string()).collect(),
        )?;
        detector.add_gazetteer(nom_gaz)?;

        let ville_gaz = Gazetteer::new(
            GAZETTEER_VILLE_SN.to_string(),
            DEFAULT_VILLES_SN.iter().map(|s| s.to_string()).collect(),
        )?;
        detector.add_gazetteer(ville_gaz)?;

        Ok(detector)
    }

    /// Add a gazetteer.
    pub fn add_gazetteer(&mut self, gaz: Gazetteer) -> CloisonResult<()> {
        self.gazetteers.insert(gaz.name.clone(), gaz);
        Ok(())
    }

    /// Execute all active detectors (filtered by policy) and return sorted matches.
    pub fn detect_with_policy(&self, text: &str, policy: &DetectorPolicy) -> Vec<Span> {
        let mut spans = Vec::new();

        if policy.is_enabled(&DetectorKind::Email) {
            spans.extend(self.detect_email(text));
        }
        if policy.is_enabled(&DetectorKind::PhoneSn) {
            spans.extend(self.detect_phone_sn(text));
        }
        if policy.is_enabled(&DetectorKind::CniSn) {
            spans.extend(self.detect_cni_sn(text));
        }
        if policy.is_enabled(&DetectorKind::CreditCard) {
            spans.extend(self.detect_credit_card(text));
        }
        if policy.is_enabled(&DetectorKind::Ip) {
            spans.extend(self.detect_ip(text));
        }

        // Gazetteers
        for (name, gaz) in &self.gazetteers {
            let kind = DetectorKind::Gazetteer(name.clone());
            if policy.is_enabled(&kind) {
                spans.extend(gaz.find(text));
            }
        }

        // Sort by start position
        spans.sort_by_key(|s| s.start);

        // Precedence du type specifique : une CNI (13 chiffres Luhn debutant
        // par 1) prime sur une carte bancaire qui la chevauche — le regex
        // CreditCard avale le separateur final, son span est donc plus long et
        // `dedup_overlaps` (longueur) jetterait la CNI (benchmark : 63/182).
        spans = drop_credit_card_over_cni(spans);

        // Remove overlapping spans (keep the first/longest)
        spans = dedup_overlaps(spans);

        spans
    }

    /// Execute all detectors without policy filtering.
    pub fn detect_all(&self, text: &str) -> Vec<Span> {
        let mut spans = Vec::new();
        spans.extend(self.detect_email(text));
        spans.extend(self.detect_phone_sn(text));
        spans.extend(self.detect_cni_sn(text));
        spans.extend(self.detect_credit_card(text));
        spans.extend(self.detect_ip(text));

        for gaz in self.gazetteers.values() {
            spans.extend(gaz.find(text));
        }

        spans.sort_by_key(|s| s.start);
        spans = drop_credit_card_over_cni(spans);
        spans = dedup_overlaps(spans);
        spans
    }

    /// Detect email addresses.
    fn detect_email(&self, text: &str) -> Vec<Span> {
        self.email_re
            .find_iter(text)
            .map(|m| Span {
                entity_type: DetectorKind::Email,
                start: m.start(),
                end: m.end(),
                score: 1.0,
                value: m.as_str().to_string(),
            })
            .collect()
    }

    /// Detect Senegalese phone numbers (+221).
    fn detect_phone_sn(&self, text: &str) -> Vec<Span> {
        self.phone_sn_re
            .find_iter(text)
            .map(|m| Span {
                entity_type: DetectorKind::PhoneSn,
                start: m.start(),
                end: m.end(),
                score: 1.0,
                value: m.as_str().to_string(),
            })
            .collect()
    }

    /// Detect Senegalese CNI numbers (13 digits starting with 1, Luhn-validated).
    fn detect_cni_sn(&self, text: &str) -> Vec<Span> {
        self.cni_sn_re
            .find_iter(text)
            .filter_map(|m| {
                let raw = m.as_str();
                // Le match inclut un préfixe non-chiffre (frontière) : le retirer.
                let (prefix_len, cni) = if raw.starts_with(|c: char| c.is_ascii_digit()) {
                    (0, raw)
                } else {
                    (1, &raw[1..])
                };
                let digits: String = cni.chars().filter(|c| c.is_ascii_digit()).collect();
                if !validate_luhn(&digits) {
                    return None;
                }
                let start = m.start() + prefix_len;
                Some(Span {
                    entity_type: DetectorKind::CniSn,
                    start,
                    end: start + cni.len(),
                    score: 1.0,
                    value: cni.to_string(),
                })
            })
            .collect()
    }

    /// Detect credit card numbers (Luhn-validated).
    fn detect_credit_card(&self, text: &str) -> Vec<Span> {
        self.credit_card_re
            .find_iter(text)
            .filter(|m| {
                let digits: String = m.as_str().chars().filter(|c| c.is_ascii_digit()).collect();
                digits.len() >= 13 && digits.len() <= 19 && validate_luhn(&digits)
            })
            .map(|m| Span {
                entity_type: DetectorKind::CreditCard,
                start: m.start(),
                end: m.end(),
                score: 1.0,
                value: m.as_str().to_string(),
            })
            .collect()
    }

    /// Detect IPv4 addresses.
    fn detect_ip(&self, text: &str) -> Vec<Span> {
        self.ip_re
            .find_iter(text)
            .map(|m| Span {
                entity_type: DetectorKind::Ip,
                start: m.start(),
                end: m.end(),
                score: 1.0,
                value: m.as_str().to_string(),
            })
            .collect()
    }
}

impl Default for Detector {
    fn default() -> Self {
        Self::new().expect("Detector::default() failed")
    }
}

/// Validate a numeric string using the Luhn algorithm.
///
/// Returns `true` if the digit sequence passes Luhn check.
/// An empty string or non-digit string returns `false`.
pub fn validate_luhn(digits: &str) -> bool {
    let digits: Vec<u32> = digits.chars().filter_map(|c| c.to_digit(10)).collect();
    if digits.is_empty() {
        return false;
    }
    let mut sum = 0u32;
    let mut double = false;
    for &d in digits.iter().rev() {
        if double {
            let doubled = d * 2;
            sum += if doubled > 9 { doubled - 9 } else { doubled };
        } else {
            sum += d;
        }
        double = !double;
    }
    sum.is_multiple_of(10)
}

/// Remove overlapping spans, keeping the first occurrence.
fn dedup_overlaps(spans: Vec<Span>) -> Vec<Span> {
    let mut result: Vec<Span> = Vec::with_capacity(spans.len());
    for span in spans {
        if let Some(last) = result.last() {
            if span.start < last.end {
                // Overlap: skip the shorter span
                if span.end - span.start > last.end - last.start {
                    result.pop();
                    result.push(span);
                }
                continue;
            }
        }
        result.push(span);
    }
    result
}

/// Precedence du type specifique : retire tout span `CreditCard` qui chevauche
/// un span `CniSn`. Un nombre de 13 chiffres Luhn-validé débutant par 1 est une
/// CNI (jamais une carte : IIN carte ∈ {2,3,4,5,6}) ; le regex CreditCard peut
/// avaler le séparateur final (span +1 caractère) et gagner le dedup de
/// longueur — la CNI serait perdue (63/182 au benchmark STACK-1).
fn drop_credit_card_over_cni(spans: Vec<Span>) -> Vec<Span> {
    let cni: Vec<(usize, usize)> = spans
        .iter()
        .filter(|s| s.entity_type == DetectorKind::CniSn)
        .map(|s| (s.start, s.end))
        .collect();
    if cni.is_empty() {
        return spans;
    }
    spans
        .into_iter()
        .filter(|s| {
            s.entity_type != DetectorKind::CreditCard
                || !cni.iter().any(|&(cs, ce)| s.start < ce && cs < s.end)
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_luhn_valid() {
        // Known valid Luhn numbers
        assert!(validate_luhn("79927398713"));
        assert!(validate_luhn("4242424242424242"));
    }

    #[test]
    fn test_luhn_invalid() {
        assert!(!validate_luhn("79927398714"));
        assert!(!validate_luhn("4242424242424243"));
    }

    #[test]
    fn test_luhn_empty() {
        assert!(!validate_luhn(""));
    }

    #[test]
    fn test_detect_email() {
        let det = Detector::new().unwrap();
        let spans = det.detect_email("Contact: user@example.com ou admin@test.org");
        assert_eq!(spans.len(), 2);
        assert_eq!(spans[0].entity_type, DetectorKind::Email);
    }

    #[test]
    fn test_detect_phone_sn() {
        let det = Detector::new().unwrap();
        let spans = det.detect_phone_sn("Appeler +221 77 123 45 67 maintenant");
        assert!(!spans.is_empty());
        assert_eq!(spans[0].entity_type, DetectorKind::PhoneSn);
    }

    #[test]
    fn test_detect_cni_sn_valid_luhn() {
        let det = Detector::new().unwrap();
        // Construct a 13-digit number starting with 1 that passes Luhn
        // 1234567890123 -> compute check digit
        let base = "123456789012";
        let check = compute_luhn_check(base);
        let cni = format!("{}{}", base, check);
        let spans = det.detect_cni_sn(&format!("CNI: {}", cni));
        assert!(!spans.is_empty(), "CNI {} should be detected", cni);
        assert_eq!(spans[0].entity_type, DetectorKind::CniSn);
    }

    #[test]
    fn test_detect_cni_sn_invalid_luhn() {
        let det = Detector::new().unwrap();
        // 13 digits starting with 1 but failing Luhn
        let spans = det.detect_cni_sn("CNI: 1234567890123");
        assert!(spans.is_empty(), "CNI with invalid Luhn should be rejected");
    }

    #[test]
    fn test_gazetteer_nom_sn() {
        let det = Detector::new().unwrap();
        let spans = det.detect_all("Amadou est arrivé à Dakar");
        assert!(spans
            .iter()
            .any(|s| matches!(s.entity_type, DetectorKind::Gazetteer(ref n) if n == "nom_sn")));
        assert!(spans
            .iter()
            .any(|s| matches!(s.entity_type, DetectorKind::Gazetteer(ref n) if n == "ville_sn")));
    }

    #[test]
    fn test_cni_wins_over_credit_card_when_followed_by_space() {
        // Régression benchmark STACK-1 (CNI 0.79) : le regex CreditCard avale
        // le séparateur final (« 1752345678017 et … ») → span plus long →
        // dedup de longueur jetait la CNI. La précédence CniSn doit primer.
        let det = Detector::new().unwrap();
        // 13 chiffres débutant par 1, Luhn valide, suivis d'un espace.
        let base = "175234567801";
        let check = compute_luhn_check(base);
        let cni = format!("{}{}", base, check);
        let text = format!("numéro {} et un autre texte", cni);
        let spans = det.detect_all(&text);
        let cni_spans: Vec<_> = spans
            .iter()
            .filter(|s| s.entity_type == DetectorKind::CniSn)
            .collect();
        assert_eq!(
            cni_spans.len(),
            1,
            "la CNI doit survivre au conflit CreditCard: {:?}",
            spans
        );
        assert!(
            !spans
                .iter()
                .any(|s| s.entity_type == DetectorKind::CreditCard),
            "aucun span CreditCard ne doit chevaucher la CNI: {:?}",
            spans
        );
        let s = cni_spans[0];
        assert_eq!(
            &text[s.start..s.end],
            &cni,
            "span CNI exact (sans séparateur)"
        );
    }

    #[test]
    fn test_credit_card_still_detected_when_no_cni() {
        // Une vraie carte (IIN 4, 16 chiffres Luhn) reste détectée : la
        // précédence CniSn ne touche que les chevauchements.
        let det = Detector::new().unwrap();
        let spans = det.detect_all("carte 4242424242424242 valide");
        assert!(
            spans
                .iter()
                .any(|s| s.entity_type == DetectorKind::CreditCard),
            "carte bancaire toujours détectée: {:?}",
            spans
        );
    }

    #[test]
    fn test_detect_email_with_accents() {
        // Régression benchmark STACK-1 (MAIL 0.91) : le regex était ASCII-only
        // et ratait les emails à local-part accentué (marèmesylla@…).
        let det = Detector::new().unwrap();
        let spans =
            det.detect_email("contact: marèmesylla@dakar.sn et amadou.bâ_diallo@entreprise.sn");
        assert_eq!(spans.len(), 2, "emails accentués détectés: {:?}", spans);
        assert!(spans.iter().all(|s| s.entity_type == DetectorKind::Email));
    }

    /// Helper: compute Luhn check digit for a digit string (to be appended).
    fn compute_luhn_check(base: &str) -> u32 {
        let digits: Vec<u32> = base.chars().filter_map(|c| c.to_digit(10)).collect();
        let mut sum = 0u32;
        // Parite alignee sur validate_luhn (13 chiffres) : la base de 12 est
        // decalee d un cran, donc on double les indices pairs de la base inversee.
        let mut double = true;
        for &d in digits.iter().rev() {
            if double {
                let doubled = d * 2;
                sum += if doubled > 9 { doubled - 9 } else { doubled };
            } else {
                sum += d;
            }
            double = !double;
        }
        (10 - (sum % 10)) % 10
    }
}
