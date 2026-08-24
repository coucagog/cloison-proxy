//! Jauge de quasi-identifiants (charte §6.1 couche 6, §11) — IN-CORE (N0 v1.1).
//!
//! Densité de catégories (âge + acte + date + lieu) dans une fenêtre
//! glissante. **SIGNAL, jamais de résolution** : la sortie ne contient ni
//! valeurs, ni identité reconstituée, ni chaînage — elle signale une densité
//! que l'appelant peut interpréter (avertissement, compteur, k-anonymat
//! renforcé). Miroir de `services/cloison-detect/src/quasi_id.py`
//! (référence de règles — zéro réécriture de logique).
//!
//! Invariants :
//! - liste fermée de 4 catégories (PERSON/ORG/ID ne sont PAS des
//!   quasi-identifiants pris en compte ici) ;
//! - `flagged = score > seuil` (strict — jamais `>=`, correctif F-27 du
//!   sidecar) ; seuil 1.0 = jauge désactivée de fait ;
//! - déterministe : mêmes entrées → mêmes sorties.

use std::collections::HashMap;

use regex::Regex;

use crate::detection::{DetectorKind, GAZETTEER_VILLE_SN};
use crate::round4;

/// Catégorie de signal de la jauge (liste fermée — 4 catégories).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum QuasiIdCategory {
    /// Âge (regex interne — le core n'a pas de détecteur AGE).
    Age,
    /// Acte d'état civil (regex interne — pas de détecteur ACT).
    Act,
    /// Date (spans core `Date` + regex interne).
    Date,
    /// Lieu (spans `Location` / gazetteer `ville_sn`).
    Loc,
}

impl QuasiIdCategory {
    /// Les 4 catégories, dans l'ordre stable de la jauge.
    pub const ALL: [QuasiIdCategory; 4] = [
        QuasiIdCategory::Age,
        QuasiIdCategory::Act,
        QuasiIdCategory::Date,
        QuasiIdCategory::Loc,
    ];

    /// Nom wire de la catégorie (miroir du sidecar).
    pub fn as_str(&self) -> &'static str {
        match self {
            QuasiIdCategory::Age => "age",
            QuasiIdCategory::Act => "act",
            QuasiIdCategory::Date => "date",
            QuasiIdCategory::Loc => "loc",
        }
    }
}

/// Type de span core → catégorie de signal. PERSON/ORG/ID ne sont PAS des
/// quasi-identifiants ici (liste fermée). `Date`/`Location`/`ville_sn`
/// seulement ; AGE/ACT passent par les regex internes.
pub fn category_for(kind: &DetectorKind) -> Option<QuasiIdCategory> {
    match kind {
        DetectorKind::Date => Some(QuasiIdCategory::Date),
        DetectorKind::Location => Some(QuasiIdCategory::Loc),
        DetectorKind::Gazetteer(n) if n == GAZETTEER_VILLE_SN => Some(QuasiIdCategory::Loc),
        _ => None,
    }
}

/// Configuration de la jauge (miroir de `GaugeConfig` du sidecar).
#[derive(Debug, Clone, Copy)]
pub struct GaugeConfig {
    /// Taille de la fenêtre glissante (caractères).
    pub window: usize,
    /// Pas de la fenêtre glissante.
    pub step: usize,
    /// Plafond du bonus « plus de 4 mentions ».
    pub max_bonus: f64,
}

impl Default for GaugeConfig {
    fn default() -> Self {
        Self {
            window: 160,
            step: 40,
            max_bonus: 0.20,
        }
    }
}

/// Rapport de la jauge : densité normalisée + flag + catégories.
/// Jamais de valeurs, jamais d'identité reconstituée.
#[derive(Debug, Clone, PartialEq)]
pub struct QuasiIdReport {
    /// Densité normalisée dans [0, 1].
    pub score: f64,
    /// `score > seuil` (strict) — signal, jamais une résolution.
    pub flagged: bool,
    /// Catégories présentes dans la meilleure fenêtre (ordre stable).
    pub signals: Vec<QuasiIdCategory>,
}

/// Jauge de densité de quasi-identifiants (fenêtre glissante).
pub struct QuasiIdGauge {
    config: GaugeConfig,
    re_age: Regex,
    re_act: Regex,
    re_date: Regex,
}

impl QuasiIdGauge {
    /// Construit la jauge (regex compilées — miroir du sidecar).
    pub fn new(config: GaugeConfig) -> Self {
        let re_age = Regex::new(r"(?i)\b\d{1,3}\s*(?:ans?|an)\b|\b(?:née?|né)\s+en\s+\d{4}\b")
            .expect("age regex is static");
        let re_act = Regex::new(r"(?i)\bacte\s*(?:n[°o]?|n\.)?\s*\d+(?:\s*/\s*\d{2,4})?\b")
            .expect("act regex is static");
        let re_date = Regex::new(
            r"(?i)\b\d{1,2}[/-]\d{1,2}[/-]\d{2,4}\b|\b\d{4}[/-]\d{1,2}[/-]\d{1,2}\b|\b\d{1,2}\s+(?:janvier|février|mars|avril|mai|juin|juillet|août|septembre|octobre|novembre|décembre)\s+\d{4}\b",
        )
        .expect("date regex is static");
        Self {
            config,
            re_age,
            re_act,
            re_date,
        }
    }

    /// Calcule le rapport de densité sur `text` + `spans`.
    ///
    /// `threshold` : seuil de flag (`score > threshold` — strict). Les
    /// intervalles viennent des spans core (date/lieu) ET des regex internes
    /// (âge/acte/date — couvrent les cas sans spans, miroir du sidecar).
    pub fn evaluate(
        &self,
        text: &str,
        spans: &[crate::detection::Span],
        threshold: f64,
    ) -> QuasiIdReport {
        let mut intervals: HashMap<QuasiIdCategory, Vec<(usize, usize)>> = HashMap::new();
        for s in spans {
            if let Some(cat) = category_for(&s.entity_type) {
                intervals.entry(cat).or_default().push((s.start, s.end));
            }
        }
        for m in self.re_age.find_iter(text) {
            intervals
                .entry(QuasiIdCategory::Age)
                .or_default()
                .push((m.start(), m.end()));
        }
        for m in self.re_act.find_iter(text) {
            intervals
                .entry(QuasiIdCategory::Act)
                .or_default()
                .push((m.start(), m.end()));
        }
        for m in self.re_date.find_iter(text) {
            intervals
                .entry(QuasiIdCategory::Date)
                .or_default()
                .push((m.start(), m.end()));
        }

        let windows = self.windows(text.len());
        let mut best_score = 0.0_f64;
        let mut best_signals: Vec<QuasiIdCategory> = Vec::new();
        for (w_start, w_end) in windows {
            let present: Vec<QuasiIdCategory> = QuasiIdCategory::ALL
                .iter()
                .copied()
                .filter(|cat| {
                    intervals
                        .get(cat)
                        .map(|ivs| ivs.iter().any(|&(s, e)| s < w_end && e > w_start))
                        .unwrap_or(false)
                })
                .collect();
            let count: usize = intervals
                .values()
                .flatten()
                .filter(|&&(s, e)| s < w_end && e > w_start)
                .count();
            let density = present.len() as f64 / QuasiIdCategory::ALL.len() as f64;
            let bonus = if count > 4 {
                (0.1 * (count - 4) as f64).min(self.config.max_bonus)
            } else {
                0.0
            };
            let score = (density + bonus).clamp(0.0, 1.0);
            if score > best_score {
                best_score = score;
                best_signals = present;
            }
        }
        QuasiIdReport {
            score: round4(best_score),
            flagged: best_score > threshold,
            signals: best_signals,
        }
    }

    /// Fenêtres glissantes `[i, i+window)`, la dernière couvre la fin
    /// (miroir de `_windows` du sidecar).
    fn windows(&self, length: usize) -> Vec<(usize, usize)> {
        if length == 0 {
            return vec![(0, 0)];
        }
        let mut windows = Vec::new();
        let mut i = 0;
        while i < length {
            windows.push((i, (i + self.config.window).min(length)));
            i += self.config.step;
        }
        if windows.last().map(|&(_, e)| e).unwrap_or(0) < length {
            windows.push((length.saturating_sub(self.config.window), length));
        }
        windows
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::detection::Span;

    fn loc_span(text: &str, name: &str) -> Span {
        let idx = text.find(name).expect("nom présent");
        Span {
            entity_type: DetectorKind::Location,
            start: idx,
            end: idx + name.len(),
            score: 1.0,
            value: name.to_string(),
        }
    }

    fn make_gauge(window: usize, step: usize) -> QuasiIdGauge {
        QuasiIdGauge::new(GaugeConfig {
            window,
            step,
            max_bonus: 0.20,
        })
    }

    #[test]
    fn dense_text_flagged() {
        let g = make_gauge(160, 40);
        let text = "Mamadou, 42 ans, acte n° 1847, enregistré le 12/03/2021 à Ouagadougou.";
        let core = vec![loc_span(text, "Ouagadougou")];
        let report = g.evaluate(text, &core, 0.5);
        assert!(report.score >= 0.5);
        assert!(report.flagged, "densité 4 catégories → flag");
        let cats: Vec<&str> = report.signals.iter().map(|c| c.as_str()).collect();
        assert_eq!(cats, vec!["age", "act", "date", "loc"]);
    }

    #[test]
    fn sparse_text_not_flagged() {
        let g = make_gauge(160, 40);
        let report = g.evaluate("Bonjour, comment allez-vous ?", &[], 0.5);
        assert_eq!(report.score, 0.0);
        assert!(!report.flagged);
        assert!(report.signals.is_empty());
    }

    #[test]
    fn threshold_1_disables_flag() {
        let g = make_gauge(160, 40);
        let text = "42 ans, acte n° 5, le 12/03/2021 à Ouagadougou.";
        let core = vec![loc_span(text, "Ouagadougou")];
        let report = g.evaluate(text, &core, 1.0);
        assert!(report.score >= 0.5);
        assert!(!report.flagged, "seuil 1.0 = jauge désactivée de fait");
    }

    #[test]
    fn windowing_max_over_windows() {
        let g = make_gauge(40, 10);
        let mut text = String::new();
        for _ in 0..20 {
            text.push_str("Bonjour. ");
        }
        text.push_str("Il a 42 ans, acte n° 7, le 12/03/2021 à Ouagadougou.");
        let core = vec![loc_span(&text, "Ouagadougou")];
        let report = g.evaluate(&text, &core, 0.5);
        assert!(report.flagged, "fenêtre couvrant la densité");
        assert!(report.score >= 0.5);
    }

    #[test]
    fn no_resolution_no_values() {
        let g = make_gauge(160, 40);
        let text = "Awa, 42 ans, acte n° 1847, le 12/03/2021, Ouagadougou.";
        let report = g.evaluate(text, &[loc_span(text, "Ouagadougou")], 0.5);
        assert!((0.0..=1.0).contains(&report.score));
        assert!(report.signals.len() <= QuasiIdCategory::ALL.len());
        // ordre stable = ordre canonique des catégories (Age, Act, Date, Loc)
        let cats: Vec<&str> = report.signals.iter().map(|c| c.as_str()).collect();
        let expected: Vec<&str> = QuasiIdCategory::ALL
            .iter()
            .filter(|c| report.signals.contains(c))
            .map(|c| c.as_str())
            .collect();
        assert_eq!(cats, expected, "signaux dans l'ordre des catégories");
    }

    #[test]
    fn empty_text() {
        let g = make_gauge(160, 40);
        let report = g.evaluate("", &[], 0.5);
        assert_eq!(report.score, 0.0);
        assert!(!report.flagged);
        assert!(report.signals.is_empty());
    }

    #[test]
    fn signals_from_spans() {
        let g = make_gauge(160, 40);
        let text = "Rendez-vous le 3 mars 2021 dans la ville.";
        let date_start = text.find("3 mars 2021").unwrap();
        let date = Span {
            entity_type: DetectorKind::Date,
            start: date_start,
            end: date_start + "3 mars 2021".len(),
            score: 0.9,
            value: "3 mars 2021".to_string(),
        };
        let loc = Span {
            entity_type: DetectorKind::Gazetteer(GAZETTEER_VILLE_SN.to_string()),
            start: text.find("ville").unwrap(),
            end: text.find("ville").unwrap() + 5,
            score: 0.8,
            value: "ville".to_string(),
        };
        let report = g.evaluate(text, &[date, loc], 0.5);
        let cats: Vec<&str> = report.signals.iter().map(|c| c.as_str()).collect();
        assert!(
            cats.contains(&"date") && cats.contains(&"loc"),
            "signaux: {cats:?}"
        );
    }
}
