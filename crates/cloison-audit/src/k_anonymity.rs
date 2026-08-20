//! Seuils d'agrégation **k-anonymes**.
//!
//! Les reçus sont des artefacts privés par requête : leur granularité
//! ré-identifierait un individu (une seule occurrence d'un type sur une
//! période trahit sa présence). Seul le **rapport agrégé** traverse le seuil,
//! cellule par cellule :
//!
//! - une cellule `(pii_type, valeur)` n'est publiable que si **aucun compteur
//!   non nul n'est strictement inférieur à `k`** ;
//! - les compteurs `< k` sont **redactés** (mis à zéro) avant publication.
//!
//! `k ≥ 2` (k = 1 ne protège rien).

use std::collections::BTreeMap;

use crate::error::{AuditError, AuditResult};
use crate::receipt::Counters;

/// Seuil k-anonyme d'agrégation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct KAnonymity {
    /// Nombre minimal d'occurrences pour qu'une cellule soit publiable.
    pub k: usize,
}

impl KAnonymity {
    /// Construit un seuil ; erreur si `k < 2` (k = 1 est re-identifiant).
    pub fn new(k: usize) -> AuditResult<Self> {
        if k < 2 {
            return Err(AuditError::InvalidK(k));
        }
        Ok(Self { k })
    }

    /// `true` si et seulement si les **deux** dimensions sont satisfaites :
    ///
    /// 1. assez de **requêtes distinctes** : `request_count >= k` ;
    /// 2. **aucun compteur non nul n'est < k**.
    ///
    /// Avec `request_count >= k`, un jeu de compteurs vide ou tout à zéro est
    /// publiable (vacuously) ; sous le seuil de requêtes, même des cellules
    /// `>= k` ne sont pas publiables (une seule requête trahit l'individu).
    pub fn is_publishable(&self, request_count: u64, counts: &BTreeMap<String, u64>) -> bool {
        // Deux dimensions : il faut assez de REQUÊTES distinctes (>= k) ET
        // chaque compteur non nul >= k. Sinon 1 requete x 6 emails -> "Email: 6"
        // serait publiee alors que request_count=1 < k (re-identification).
        if request_count < self.k as u64 {
            return false;
        }
        counts
            .values()
            .all(|&v| v == 0 || v >= self.k as u64)
    }

    /// Agrège (somme champ à champ) plusieurs jeux de compteurs — par exemple
    /// les compteurs de toutes les requêtes d'une période.
    pub fn aggregate(&self, periods: Vec<Counters>) -> Counters {
        let mut total = Counters::default();
        for period in periods {
            total.add(&period);
        }
        total
    }

    /// Redacte les compteurs `< k` (mis à zéro) pour la publication.
    ///
    /// Les compteurs `≥ k` sont conservés tels quels ; les clés à zéro sont
    /// conservées (le type est connu, sa masse ne l'est pas).
    pub fn redact_below_k(&self, counts: &BTreeMap<String, u64>) -> BTreeMap<String, u64> {
        counts
            .iter()
            .map(|(kind, &v)| (kind.clone(), if v < self.k as u64 { 0 } else { v }))
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn counts(entries: &[(&str, u64)]) -> BTreeMap<String, u64> {
        entries
            .iter()
            .map(|(k, v)| (k.to_string(), *v))
            .collect()
    }

    #[test]
    fn rejects_k_below_2() {
        assert!(KAnonymity::new(0).is_err());
        assert!(KAnonymity::new(1).is_err());
        assert!(KAnonymity::new(2).is_ok());
        assert!(KAnonymity::new(5).is_ok());
    }

    #[test]
    fn publishable_requires_enough_requests_and_every_nonzero_cell_at_least_k() {
        let k = KAnonymity::new(5).unwrap();
        // Requêtes >= k ET tous les compteurs non nuls >= k → publiable.
        assert!(k.is_publishable(5, &counts(&[("Email", 5), ("PhoneSn", 9)])));
        // Avec assez de requêtes, un jeu vide ou tout à zéro est publiable
        // (vacuously).
        assert!(k.is_publishable(5, &counts(&[])));
        assert!(k.is_publishable(5, &counts(&[("Email", 0)])));
        // Un compteur 1..k-1 → PAS publiable (granularité re-identifiante).
        assert!(!k.is_publishable(5, &counts(&[("Email", 5), ("PhoneSn", 4)])));
        assert!(!k.is_publishable(5, &counts(&[("Email", 1)])));
        assert!(!k.is_publishable(5, &counts(&[("Email", 0), ("CniSn", 3)])));
        // Pas assez de requêtes distinctes → jamais publiable, même avec des
        // cellules >= k (la dimension "requêtes" manque).
        assert!(!k.is_publishable(4, &counts(&[("Email", 5), ("PhoneSn", 9)])));
        assert!(!k.is_publishable(0, &counts(&[])));
    }

    #[test]
    fn single_request_is_not_publishable_even_with_high_cells() {
        let k = KAnonymity::new(5).unwrap();
        // P0-2 : 1 requête x 6 emails → request_count=1 < k=5 → jamais
        // publiable, même si la cellule Email=6 >= k (une seule requête
        // trahit la présence de l'individu).
        assert!(!k.is_publishable(1, &counts(&[("Email", 6)])));
    }

    #[test]
    fn redacts_below_k_only() {
        let k = KAnonymity::new(5).unwrap();
        let out = k.redact_below_k(&counts(&[("Email", 7), ("PhoneSn", 5), ("CniSn", 1), ("Ip", 0)]));
        assert_eq!(out.get("Email"), Some(&7));
        assert_eq!(out.get("PhoneSn"), Some(&5));
        assert_eq!(out.get("CniSn"), Some(&0), "counter < k must be zeroed");
        assert_eq!(out.get("Ip"), Some(&0));
    }

    #[test]
    fn aggregate_sums_all_fields() {
        let k = KAnonymity::new(5).unwrap();
        let mut c1 = Counters::default();
        c1.masked_by_type.insert("Email".to_string(), 2);
        c1.incomplete_restorations = 1;
        let mut c2 = Counters::default();
        c2.masked_by_type.insert("Email".to_string(), 3);
        c2.masked_by_type.insert("PhoneSn".to_string(), 5);
        c2.blocked_outputs = 2;
        c2.quasi_id_flags = 4;
        let total = k.aggregate(vec![c1, c2]);
        assert_eq!(total.masked_by_type.get("Email"), Some(&5));
        assert_eq!(total.masked_by_type.get("PhoneSn"), Some(&5));
        assert_eq!(total.incomplete_restorations, 1);
        assert_eq!(total.blocked_outputs, 2);
        assert_eq!(total.quasi_id_flags, 4);
    }
}
