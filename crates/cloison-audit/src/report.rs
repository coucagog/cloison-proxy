//! Rapport de conformité d'une période.
//!
//! Généré depuis une liste de `Receipt` : les compteurs sont déjà hachés /
//! agrégés — **jamais de texte** ne transite par le rapport. Le rapport est
//! publiable tel quel si `publishable` est vrai ; sinon les cellules
//! re-identifiantes (`< k`) sont **redactées** (zéro) dans `redacted`.

use std::collections::BTreeMap;

use ed25519_dalek::{Signature, Signer, SigningKey, VerifyingKey};
use serde::{Deserialize, Serialize};

use crate::error::AuditResult;
use crate::k_anonymity::KAnonymity;
use crate::receipt::{Counters, Receipt};

/// Rapport de conformité agrégé d'une période.
///
/// Compteurs entiers uniquement — aucun texte, aucun span, aucune valeur.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ConformanceReport {
    /// Début de période (Unix, UTC, inclusif).
    pub period_start: u64,
    /// Fin de période (Unix, UTC, exclusif).
    pub period_end: u64,
    /// Nombre de requêtes auditées sur la période.
    pub total_requests: u64,
    /// Compteurs agrégés bruts (somme de tous les reçus, pré-redaction).
    /// INTERNE — jamais sérialisé ni servi : granularité re-identifiante.
    /// Le rapport publie uniquement `redacted` (k-anonyme) + métadonnées.
    /// (`skip_serializing` : le JSON servi ne contient pas `aggregated` ;
    /// `default` : un JSON sans ce champ se désérialise quand même.)
    #[serde(skip_serializing, default)]
    pub aggregated: Counters,
    /// `true` si tous les compteurs non nuls respectent le seuil k
    /// (aucune cellule re-identifiante → rapport publiable).
    pub publishable: bool,
    /// Compteurs publiables : les cellules `< k` sont mises à zéro.
    pub redacted: BTreeMap<String, u64>,
    /// Seuil k-anonyme appliqué.
    pub k: usize,
    /// Signature Ed25519 du rapport (authenticite verifiable hors-ligne).
    /// Message signe = JSON canonique de (period_start, period_end,
    /// total_requests, redacted) — jamais les bruts.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sig_report: Option<Vec<u8>>,
}

impl ConformanceReport {
    /// Génère le rapport depuis une liste de reçus.
    ///
    /// - `total_requests` = nombre de reçus ;
    /// - `aggregated` = somme des compteurs de tous les reçus ;
    /// - `publishable` = `is_publishable(total_requests, aggregated.masked_by_type)` ;
    /// - `redacted` = `redact_below_k(aggregated.masked_by_type)`.
    pub fn from_receipts(
        receipts: &[Receipt],
        period_start: u64,
        period_end: u64,
        k: usize,
    ) -> AuditResult<Self> {
        let k_anon = KAnonymity::new(k)?;
        let counters: Vec<Counters> = receipts.iter().map(|r| r.counters.clone()).collect();
        let aggregated = k_anon.aggregate(counters);
        // P0-2 : la dimension "requêtes distinctes" (nb de reçus) est
        // transmise au seuil k-anonyme — 1 requête x 6 emails n'est JAMAIS
        // publiable.
        let total_requests = receipts.len() as u64;
        let publishable = k_anon.is_publishable(total_requests, &aggregated.masked_by_type);
        let redacted = k_anon.redact_below_k(&aggregated.masked_by_type);
        Ok(Self {
            period_start,
            period_end,
            total_requests,
            aggregated,
            publishable,
            redacted,
            k,
            // La signature dépend de la clé de l'agent : `from_receipts` ne
            // signe pas ; le proxy appelle `sign_report` (P0-3).
            sig_report: None,
        })
    }

    /// Octets du message signé (JSON canonique, sans espace) :
    /// `{period_start, period_end, total_requests, redacted}`.
    ///
    /// L'ordre des champs est fixe (déclaration) et `redacted` est un
    /// `BTreeMap` → clés triées : deux machines produisent exactement les
    /// mêmes octets (déterminisme, comme pour les reçus).
    fn signing_bytes(&self) -> Vec<u8> {
        let message = SignableReport {
            period_start: self.period_start,
            period_end: self.period_end,
            total_requests: self.total_requests,
            redacted: &self.redacted,
        };
        serde_json::to_vec(&message).expect("canonical report serialization is infallible")
    }

    /// Signe ce rapport (remplit `sig_report`).
    ///
    /// Signature Ed25519 déterministe du message canonique
    /// `{period_start, period_end, total_requests, redacted}` — **jamais**
    /// les compteurs bruts (`aggregated`).
    pub fn sign_report(&mut self, signing_key: &SigningKey) {
        let sig: Signature = signing_key.sign(&self.signing_bytes());
        self.sig_report = Some(sig.to_bytes().to_vec());
    }

    /// Vérifie `sig_report` contre une clé publique.
    ///
    /// Reconstruit `signing_bytes()` puis `verify_strict` (rejet de la
    /// malléabilité de signature). `false` si `sig_report` est vide ou
    /// invalide.
    pub fn verify_signature(&self, verifying_key: &VerifyingKey) -> bool {
        let Some(sig_bytes) = &self.sig_report else {
            return false;
        };
        if sig_bytes.len() != 64 {
            return false;
        }
        let Ok(sig) = Signature::from_slice(sig_bytes) else {
            return false;
        };
        verifying_key.verify_strict(&self.signing_bytes(), &sig).is_ok()
    }
}

/// Message exact qui est signé : JSON canonique (serde_json compact) de
/// `{period_start, period_end, total_requests, redacted}` — jamais les
/// compteurs bruts. L'ordre des champs est celui de la déclaration.
#[derive(Serialize)]
struct SignableReport<'a> {
    period_start: u64,
    period_end: u64,
    total_requests: u64,
    redacted: &'a BTreeMap<String, u64>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::receipt::{hash_session_ref, ReceiptMessage};

    fn receipt(counters: Counters, ts: u64) -> Receipt {
        Receipt::build(ReceiptMessage {
            tenant_id: "tenant-42".to_string(),
            session_ref_hashed: hash_session_ref("tenant-42", "sess"),
            ts_unix: ts,
            engine_version: "0.1.0".to_string(),
            policy_hash: "abc".to_string(),
            counters,
        })
    }

    fn counters_with(email: u64, phone: u64, cni: u64) -> Counters {
        let mut masked = BTreeMap::new();
        if email > 0 {
            masked.insert("Email".to_string(), email);
        }
        if phone > 0 {
            masked.insert("PhoneSn".to_string(), phone);
        }
        if cni > 0 {
            masked.insert("CniSn".to_string(), cni);
        }
        Counters {
            masked_by_type: masked,
            incomplete_restorations: 0,
            blocked_outputs: 0,
            quasi_id_flags: 0,
        }
    }

    #[test]
    fn report_aggregates_receipts() {
        let receipts = vec![
            receipt(counters_with(2, 3, 0), 100),
            receipt(counters_with(3, 2, 0), 200),
            receipt(counters_with(1, 0, 1), 300),
        ];
        let report = ConformanceReport::from_receipts(&receipts, 100, 301, 5).unwrap();
        assert_eq!(report.period_start, 100);
        assert_eq!(report.period_end, 301);
        assert_eq!(report.total_requests, 3);
        assert_eq!(report.aggregated.masked_by_type.get("Email"), Some(&6));
        assert_eq!(report.aggregated.masked_by_type.get("PhoneSn"), Some(&5));
        assert_eq!(report.aggregated.masked_by_type.get("CniSn"), Some(&1));
        // CniSn = 1 < k=5 → PAS publiable globalement, et redacté à zéro.
        assert!(!report.publishable);
        assert_eq!(report.redacted.get("Email"), Some(&6));
        assert_eq!(report.redacted.get("PhoneSn"), Some(&5));
        assert_eq!(report.redacted.get("CniSn"), Some(&0), "cell < k must be redacted");
    }

    #[test]
    fn report_publishable_when_enough_requests_and_all_cells_at_least_k() {
        // 5 requêtes (>= k) x 1 email + 1 téléphone chacune → cellules 5 et 5.
        let receipts = vec![
            receipt(counters_with(1, 1, 0), 100),
            receipt(counters_with(1, 1, 0), 200),
            receipt(counters_with(1, 1, 0), 300),
            receipt(counters_with(1, 1, 0), 400),
            receipt(counters_with(1, 1, 0), 500),
        ];
        let report = ConformanceReport::from_receipts(&receipts, 100, 501, 5).unwrap();
        assert!(report.publishable, "5 requêtes >= k=5 et cellules 5 >= k");
        assert_eq!(report.redacted.get("Email"), Some(&5));
        assert_eq!(report.redacted.get("PhoneSn"), Some(&5));
    }

    #[test]
    fn report_empty_period_is_not_publishable() {
        // 0 requête < k=5 : la dimension "requêtes distinctes" bloque la
        // publication, même avec un jeu de compteurs vide (P0-2).
        let report = ConformanceReport::from_receipts(&[], 0, 0, 5).unwrap();
        assert_eq!(report.total_requests, 0);
        assert!(!report.publishable, "0 requête < k=5 → jamais publiable");
        assert!(report.redacted.is_empty());
    }

    #[test]
    fn report_rejects_k_below_2() {
        assert!(ConformanceReport::from_receipts(&[], 0, 0, 1).is_err());
    }

    #[test]
    fn report_json_contains_no_pii_text() {
        let mut masked = BTreeMap::new();
        masked.insert("Email".to_string(), 12);
        let r = receipt(
            Counters {
                masked_by_type: masked,
                incomplete_restorations: 0,
                blocked_outputs: 1,
                quasi_id_flags: 3,
            },
            100,
        );
        let report = ConformanceReport::from_receipts(&[r], 100, 200, 5).unwrap();
        let json = serde_json::to_string(&report).unwrap();
        assert!(!json.contains("user@example.com"));
        assert!(!json.contains("+221"));
        // Seules des valeurs numériques : pas de champ texte arbitraire.
        assert!(json.contains("\"total_requests\":1"));
        assert!(json.contains("\"Email\":12"));
    }

    #[test]
    fn report_json_never_exposes_aggregated() {
        let receipts = vec![
            receipt(counters_with(2, 3, 0), 100),
            receipt(counters_with(3, 2, 0), 200),
            receipt(counters_with(1, 0, 1), 300),
            receipt(counters_with(2, 2, 2), 400),
            receipt(counters_with(2, 3, 1), 500),
        ];
        let report = ConformanceReport::from_receipts(&receipts, 100, 501, 5).unwrap();
        let json = serde_json::to_string(&report).unwrap();
        // P0-1 : le champ brut `aggregated` ne doit JAMAIS être sérialisé.
        assert!(!json.contains("aggregated"), "raw aggregated counters must never be serialized");
        let parsed: serde_json::Value = serde_json::from_str(&json).unwrap();
        assert!(parsed.get("aggregated").is_none(), "served JSON must not expose aggregated");
        // Seules les métadonnées + redacted sont publiques.
        assert_eq!(parsed["period_start"].as_u64(), Some(100));
        assert_eq!(parsed["period_end"].as_u64(), Some(501));
        assert_eq!(parsed["total_requests"].as_u64(), Some(5));
        assert_eq!(parsed["k"].as_u64(), Some(5));
        assert_eq!(parsed["publishable"].as_bool(), Some(false));
        assert_eq!(parsed["redacted"]["Email"].as_u64(), Some(10));
        assert_eq!(parsed["redacted"]["CniSn"].as_u64(), Some(0), "CniSn=4 < k=5 redacted");
    }

    #[test]
    fn report_signature_verifies_and_detects_tampering() {
        let signing_key = SigningKey::generate(&mut rand::rngs::OsRng);
        let receipts = vec![
            receipt(counters_with(2, 3, 0), 100),
            receipt(counters_with(3, 2, 0), 200),
            receipt(counters_with(1, 0, 1), 300),
            receipt(counters_with(2, 2, 2), 400),
            receipt(counters_with(2, 3, 1), 500),
        ];
        let mut report = ConformanceReport::from_receipts(&receipts, 100, 501, 5).unwrap();
        assert!(report.sig_report.is_none(), "unsigned by default");

        // P0-3 : signé → vérifiable avec la clé publique.
        report.sign_report(&signing_key);
        let verifying_key = signing_key.verifying_key();
        assert_eq!(report.sig_report.as_ref().unwrap().len(), 64);
        assert!(report.verify_signature(&verifying_key), "signed report must verify");

        // Altération d'une cellule publiée (redacted) → signature invalide.
        report.redacted.insert("Email".to_string(), 999);
        assert!(!report.verify_signature(&verifying_key), "tampered redacted must fail");

        // Clé différente → invalide.
        let other = SigningKey::generate(&mut rand::rngs::OsRng);
        assert!(!report.verify_signature(&other.verifying_key()));

        // Non signé → invalide.
        let unsigned = ConformanceReport::from_receipts(&receipts, 100, 501, 5).unwrap();
        assert!(!unsigned.verify_signature(&verifying_key));
    }
}
