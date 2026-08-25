//! Policy module.
//!
//! Per-tenant policy configuration: which detectors are active,
//! substitution modes, and generalization rules.

use std::collections::{HashMap, HashSet};

use serde::{Deserialize, Serialize};

use crate::detection::DetectorKind;
use crate::generalize::GeneralizeRule;

/// Substitution mode for detected PII.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum SubstitutionMode {
    /// Replace with the sentinel ⟦body·tag⟧.
    #[default]
    Sentinel,
    /// Replace with a realistic fake value (deterministic from token_body).
    RealisticFake,
}

/// Per-detector activation policy.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DetectorPolicy {
    /// Enabled detectors.
    pub enabled: HashSet<DetectorKind>,
    /// Substitution mode per detector (default: Sentinel).
    pub mode: HashMap<DetectorKind, SubstitutionMode>,
}

impl DetectorPolicy {
    /// Create a policy with all built-in detectors enabled.
    pub fn all_enabled() -> Self {
        let mut enabled = HashSet::new();
        enabled.insert(DetectorKind::Email);
        enabled.insert(DetectorKind::PhoneSn);
        enabled.insert(DetectorKind::CniSn);
        enabled.insert(DetectorKind::CreditCard);
        enabled.insert(DetectorKind::Ip);
        enabled.insert(DetectorKind::Date);
        // NER sidecar (wiring B.1) : activés par défaut — les détecteurs
        // embarqués n'émettent JAMAIS ces types, seuls des spans externes
        // validés par `Engine::tokenize_with_extra` les portent. Sans
        // sidecar configuré, aucun impact.
        enabled.insert(DetectorKind::Person);
        enabled.insert(DetectorKind::Location);
        // Identifiants contextuels sénégalais (N3) : passeport, permis de
        // conduire, matricules État/IPRES — masqués par défaut (une valeur
        // non détectée partirait en clair, invariant I1).
        enabled.insert(DetectorKind::Passport);
        enabled.insert(DetectorKind::DriverLicense);
        enabled.insert(DetectorKind::Matricule);
        // Gazetteers embarqués (noms/toponymes sénégalais) : masqués par
        // défaut — la doc (DATA-MODEL §1, THREAT-MODEL) annonce des
        // sentinelles GZ* ; sans eux, les noms passaient en clair.
        enabled.insert(DetectorKind::Gazetteer(
            crate::detection::GAZETTEER_NOM_SN.to_string(),
        ));
        enabled.insert(DetectorKind::Gazetteer(
            crate::detection::GAZETTEER_VILLE_SN.to_string(),
        ));

        Self {
            enabled,
            mode: HashMap::new(),
        }
    }

    /// Create a policy with no detectors enabled.
    pub fn none_enabled() -> Self {
        Self {
            enabled: HashSet::new(),
            mode: HashMap::new(),
        }
    }

    /// Check if a detector is enabled.
    pub fn is_enabled(&self, kind: &DetectorKind) -> bool {
        match kind {
            DetectorKind::Gazetteer(name) => {
                // Gazetteer is enabled if its specific name is in the set
                self.enabled.contains(kind)
                    || self
                        .enabled
                        .iter()
                        .any(|k| matches!(k, DetectorKind::Gazetteer(n) if n == name))
            }
            _ => self.enabled.contains(kind),
        }
    }

    /// Get the substitution mode for a detector.
    pub fn substitution_mode(&self, kind: &DetectorKind) -> SubstitutionMode {
        self.mode.get(kind).cloned().unwrap_or_default()
    }
}

impl Default for DetectorPolicy {
    fn default() -> Self {
        Self::all_enabled()
    }
}

/// Complete tenant policy.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Policy {
    /// Tenant identifier.
    pub tenant_id: String,
    /// Detection policy (which detectors, which mode).
    pub detection: DetectorPolicy,
    /// Generalization rules per PII type.
    pub generalization: HashMap<DetectorKind, GeneralizeRule>,
    /// Cardinality thresholds per PII type.
    pub cardinality_thresholds: HashMap<DetectorKind, usize>,
}

impl Policy {
    /// Load default policy for a tenant.
    pub fn default_for(tenant_id: &str) -> Self {
        Self {
            tenant_id: tenant_id.to_string(),
            detection: DetectorPolicy::all_enabled(),
            generalization: HashMap::new(),
            cardinality_thresholds: HashMap::new(),
        }
    }

    /// Politique par défaut **N0** (daemon desktop, moteur Rust seul) :
    /// comme `default_for`, plus la **généralisation des faibles cardinalités
    /// explicitement posée** (charte §6.1 couche 5, N0-PREP §4.6 : on
    /// généralise, jamais on tokenise ce que la fréquence trahit).
    ///
    /// Les règles par défaut du `Generalizer` (Date/Ip/CreditCard) couvrent
    /// déjà ces types ; les poser dans la politique rend le comportement N0
    /// autonome et documenté, et ajoute la **ville** (toponyme gazetteer,
    /// faible cardinalité) en généralisation `[VILLE_SN]` — jamais de jeton.
    pub fn n0_for(tenant_id: &str) -> Self {
        let mut p = Self::default_for(tenant_id);
        p.generalization
            .insert(DetectorKind::Date, GeneralizeRule::DateBucket);
        p.generalization.insert(
            DetectorKind::Ip,
            GeneralizeRule::Range {
                prefix: "[IP]".to_string(),
            },
        );
        p.generalization.insert(
            DetectorKind::CreditCard,
            GeneralizeRule::Mask {
                keep_start: 0,
                keep_end: 4,
                mask_char: '•',
            },
        );
        p.generalization.insert(
            DetectorKind::Gazetteer(crate::detection::GAZETTEER_VILLE_SN.to_string()),
            GeneralizeRule::Replace {
                label: "[VILLE_SN]".to_string(),
            },
        );
        p
    }

    /// Check if a detector is enabled.
    pub fn is_enabled(&self, kind: &DetectorKind) -> bool {
        self.detection.is_enabled(kind)
    }

    /// Get the substitution mode for a PII type.
    pub fn substitution_mode(&self, kind: &DetectorKind) -> SubstitutionMode {
        self.detection.substitution_mode(kind)
    }

    /// Check if a PII type should be generalized (low cardinality).
    pub fn should_generalize(&self, kind: &DetectorKind) -> bool {
        self.generalization.contains_key(kind)
    }

    /// Règle de généralisation de la politique pour un type (si posée).
    pub fn generalize_rule(&self, kind: &DetectorKind) -> Option<&GeneralizeRule> {
        self.generalization.get(kind)
    }
}

impl Default for Policy {
    fn default() -> Self {
        Self::default_for("default")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_policy_all_enabled() {
        let policy = Policy::default();
        assert!(policy.is_enabled(&DetectorKind::Email));
        assert!(policy.is_enabled(&DetectorKind::PhoneSn));
        assert!(policy.is_enabled(&DetectorKind::CniSn));
    }

    #[test]
    fn test_gazetteer_enabled() {
        let mut policy = DetectorPolicy::all_enabled();
        let ville = DetectorKind::Gazetteer("ville_sn".to_string());
        policy.enabled.insert(ville.clone());
        assert!(policy.is_enabled(&ville));
    }

    #[test]
    fn test_substitution_mode_default() {
        let policy = DetectorPolicy::all_enabled();
        assert_eq!(
            policy.substitution_mode(&DetectorKind::Email),
            SubstitutionMode::Sentinel
        );
    }

    #[test]
    fn test_policy_default_for() {
        let policy = Policy::default_for("tenant-42");
        assert_eq!(policy.tenant_id, "tenant-42");
    }

    #[test]
    fn test_policy_n0_for_generalizes_low_cardinality() {
        let policy = Policy::n0_for("n0-local");
        // Faibles cardinalités : généralisation posée dans la politique.
        assert!(policy.should_generalize(&DetectorKind::Date));
        assert!(policy.should_generalize(&DetectorKind::Ip));
        assert!(policy.should_generalize(&DetectorKind::CreditCard));
        let ville = DetectorKind::Gazetteer("ville_sn".to_string());
        assert!(policy.should_generalize(&ville));
        // Types à forte entropie : jamais généralisés par défaut.
        assert!(!policy.should_generalize(&DetectorKind::Email));
        assert!(!policy.should_generalize(&DetectorKind::PhoneSn));
        assert!(!policy.should_generalize(&DetectorKind::CniSn));
    }
}
