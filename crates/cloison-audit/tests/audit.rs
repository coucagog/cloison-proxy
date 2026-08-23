//! Tests unitaires `cloison-audit` (I-A2..I-A6) :
//! - I-A2  reçu sans texte : compteurs = entiers uniquement ;
//! - I-A3  signature : `verify` vrai sur reçu signé, faux sur reçu altéré
//!   (chaque champ modifié) et faux sur clé différente ;
//! - I-A4  canonicalité : `signing_bytes()` déterministe ;
//! - I-A6  k-anonymat : cellule `< k` redactée, agrégation, rapport.

use std::collections::BTreeMap;

use cloison_audit::ed25519_dalek::{SigningKey, VerifyingKey};
use cloison_audit::k_anonymity::KAnonymity;
use cloison_audit::receipt::{self, Counters, Receipt, ReceiptMessage, AUDIT_SCHEMA_VERSION};
use cloison_audit::report::ConformanceReport;

/// Paire de test déterministe.
fn keys() -> (SigningKey, VerifyingKey) {
    let sk = SigningKey::from_bytes(&[7u8; 32]);
    let vk = sk.verifying_key();
    (sk, vk)
}

fn counters() -> Counters {
    let mut masked = BTreeMap::new();
    masked.insert("Email".to_string(), 2);
    masked.insert("PhoneSn".to_string(), 1);
    Counters {
        masked_by_type: masked,
        incomplete_restorations: 0,
        blocked_outputs: 1,
        quasi_id_flags: 3,
    }
}

fn message() -> ReceiptMessage {
    ReceiptMessage {
        tenant_id: "tenant-42".to_string(),
        session_ref_hashed: receipt::hash_session_ref("tenant-42", "session-ref-1"),
        ts_unix: 1_710_000_000,
        engine_version: "0.1.0".to_string(),
        policy_hash: "a1b2c3d4".to_string(),
        counters: counters(),
    }
}

// ---------------------------------------------------------------------------
// I-A3 : signature
// ---------------------------------------------------------------------------

#[test]
fn sign_then_verify() {
    let (sk, vk) = keys();
    let unsigned = Receipt::build(message());
    assert!(unsigned.sig_agent.is_empty());
    let signed = unsigned.sign(&sk);
    assert_eq!(signed.sig_agent.len(), 64);
    assert!(signed.verify(&vk), "signed receipt must verify");
}

#[test]
fn verify_fails_on_tampered_counters() {
    let (sk, vk) = keys();
    let mut signed = Receipt::build(message()).sign(&sk);
    // Altération d'un compteur → signature invalide.
    signed
        .counters
        .masked_by_type
        .insert("Email".to_string(), 999);
    assert!(
        !signed.verify(&vk),
        "tampered counters must fail verification"
    );
}

#[test]
fn verify_fails_on_tampered_metadata() {
    let (sk, vk) = keys();
    let mut signed = Receipt::build(message()).sign(&sk);
    signed.tenant_id = "other-tenant".to_string();
    assert!(!signed.verify(&vk));
    let mut signed = Receipt::build(message()).sign(&sk);
    signed.ts_unix += 1;
    assert!(!signed.verify(&vk));
    let mut signed = Receipt::build(message()).sign(&sk);
    signed.session_ref_hashed.push('0');
    assert!(!signed.verify(&vk));
    let mut signed = Receipt::build(message()).sign(&sk);
    signed.policy_hash = "deadbeef".to_string();
    assert!(!signed.verify(&vk));
    let mut signed = Receipt::build(message()).sign(&sk);
    signed.engine_version = "9.9.9".to_string();
    assert!(!signed.verify(&vk));
}

#[test]
fn verify_fails_on_wrong_key() {
    let (sk, _vk) = keys();
    let other_sk = SigningKey::from_bytes(&[9u8; 32]);
    let other_vk = other_sk.verifying_key();
    let signed = Receipt::build(message()).sign(&sk);
    assert!(
        !signed.verify(&other_vk),
        "wrong key must fail verification"
    );
}

#[test]
fn verify_fails_on_unsigned() {
    let (_sk, vk) = keys();
    let unsigned = Receipt::build(message());
    assert!(!unsigned.verify(&vk), "unsigned receipt must not verify");
}

// ---------------------------------------------------------------------------
// I-A4 : canonicalité
// ---------------------------------------------------------------------------

#[test]
fn signing_bytes_are_deterministic() {
    let (sk, _vk) = keys();
    let a = Receipt::build(message());
    let b = Receipt::build(message());
    assert_eq!(a.signing_bytes(), b.signing_bytes());
    // Deux constructions → même signature.
    assert_eq!(a.sign(&sk).sig_agent, b.sign(&sk).sig_agent);
    // L'ordre des clés de masked_by_type ne change rien (BTreeMap trié).
    let mut reversed = BTreeMap::new();
    reversed.insert("PhoneSn".to_string(), 1);
    reversed.insert("Email".to_string(), 2);
    let mut m2 = message();
    m2.counters = Counters {
        masked_by_type: reversed,
        ..counters()
    };
    let b2 = Receipt::build(m2);
    assert_eq!(a.signing_bytes(), b2.signing_bytes());
}

#[test]
fn base64url_json_roundtrip_preserves_receipt() {
    let (sk, vk) = keys();
    let signed = Receipt::build(message()).sign(&sk);
    let encoded = signed.to_base64url_json();
    let decoded = Receipt::from_base64url_json(&encoded).unwrap();
    assert_eq!(decoded, signed);
    assert!(decoded.verify(&vk));
    assert!(Receipt::from_base64url_json("not-base64!!").is_err());
}

// ---------------------------------------------------------------------------
// I-A2 : reçu sans texte
// ---------------------------------------------------------------------------

#[test]
fn counters_serialize_without_text_values() {
    let (sk, _vk) = keys();
    let signed = Receipt::build(message()).sign(&sk);
    let json = serde_json::to_string(&signed).unwrap();
    // Le texte PII d'origine n'apparaît nulle part.
    assert!(!json.contains("user@example.com"));
    assert!(!json.contains("+221"));
    // masked_by_type : clés = types (bornés), valeurs = entiers.
    let parsed: serde_json::Value = serde_json::from_str(&json).unwrap();
    let masked = &parsed["counters"]["masked_by_type"];
    assert!(masked.is_object());
    for (k, v) in masked.as_object().unwrap() {
        assert!(v.is_u64(), "counter {k} must be an integer, got {v}");
    }
    assert_eq!(parsed["counters"]["blocked_outputs"], 1);
    assert_eq!(parsed["counters"]["quasi_id_flags"], 3);
    // Le reçu expose la version de schéma via son absence dans le JSON ? Non —
    // la version est une constante documentée, pas un champ.
    assert_eq!(AUDIT_SCHEMA_VERSION, 1);
}

#[test]
fn receipt_message_fields_are_only_identifiers_and_integers() {
    let m = message();
    assert_eq!(m.tenant_id, "tenant-42");
    assert_eq!(m.session_ref_hashed.len(), 64); // sha256 hex, pas la session
    assert!(!m.session_ref_hashed.contains("session-ref-1"));
}

// ---------------------------------------------------------------------------
// I-A6 : k-anonymat
// ---------------------------------------------------------------------------

#[test]
fn k_anonymity_publishable_threshold() {
    let k = KAnonymity::new(5).unwrap();
    let mut ok = BTreeMap::new();
    ok.insert("Email".to_string(), 5);
    ok.insert("PhoneSn".to_string(), 12);
    assert!(k.is_publishable(5, &ok));

    let mut low = BTreeMap::new();
    low.insert("Email".to_string(), 5);
    low.insert("CniSn".to_string(), 1); // < k
    assert!(
        !k.is_publishable(5, &low),
        "cell < k must block publication"
    );

    let mut low = BTreeMap::new();
    low.insert("CniSn".to_string(), 4);
    assert!(
        !k.is_publishable(5, &low),
        "cell == k-1 must block publication"
    );

    // P0-2 : 1 requête x 6 emails → jamais publiable (requêtes < k), même
    // si la cellule Email=6 >= k.
    let mut single = BTreeMap::new();
    single.insert("Email".to_string(), 6);
    assert!(
        !k.is_publishable(1, &single),
        "1 requête < k=5 must block publication"
    );
}

#[test]
fn k_anonymity_redacts_below_k() {
    let k = KAnonymity::new(5).unwrap();
    let mut counts = BTreeMap::new();
    counts.insert("Email".to_string(), 9);
    counts.insert("PhoneSn".to_string(), 5);
    counts.insert("CniSn".to_string(), 4);
    counts.insert("Ip".to_string(), 0);
    let redacted = k.redact_below_k(&counts);
    assert_eq!(redacted.get("Email"), Some(&9));
    assert_eq!(redacted.get("PhoneSn"), Some(&5));
    assert_eq!(redacted.get("CniSn"), Some(&0), "4 < 5 must be zeroed");
    assert_eq!(redacted.get("Ip"), Some(&0));
}

#[test]
fn k_anonymity_aggregates_periods() {
    let k = KAnonymity::new(5).unwrap();
    let mut c1 = counters();
    let mut c2 = counters();
    c1.masked_by_type.insert("CniSn".to_string(), 4);
    c2.masked_by_type.insert("CniSn".to_string(), 4);
    let total = k.aggregate(vec![c1, c2]);
    assert_eq!(total.masked_by_type.get("Email"), Some(&4));
    assert_eq!(total.masked_by_type.get("PhoneSn"), Some(&2));
    // 4 + 4 = 8 ≥ k → publiable après agrégation (c'est le but du seuil).
    assert_eq!(total.masked_by_type.get("CniSn"), Some(&8));
    assert_eq!(total.blocked_outputs, 2);
    assert_eq!(total.quasi_id_flags, 6);
}

#[test]
fn k_anonymity_rejects_threshold_below_2() {
    assert!(KAnonymity::new(1).is_err());
    assert!(KAnonymity::new(0).is_err());
}

// ---------------------------------------------------------------------------
// Rapport de conformité
// ---------------------------------------------------------------------------

#[test]
fn report_from_receipts_aggregates_and_redacts() {
    let mut receipts = Vec::new();
    for i in 0..5 {
        let mut c = counters();
        c.masked_by_type.insert("Email".to_string(), 1);
        c.masked_by_type.insert("CniSn".to_string(), 1);
        let m = ReceiptMessage {
            tenant_id: "tenant-42".to_string(),
            session_ref_hashed: receipt::hash_session_ref("tenant-42", &format!("sess-{i}")),
            ts_unix: 1_710_000_000 + i,
            engine_version: "0.1.0".to_string(),
            policy_hash: "abc".to_string(),
            counters: c,
        };
        receipts.push(Receipt::build(m));
    }
    let report =
        ConformanceReport::from_receipts(&receipts, 1_710_000_000, 1_710_000_005, 5).unwrap();
    assert_eq!(report.total_requests, 5);
    assert_eq!(report.aggregated.masked_by_type.get("Email"), Some(&5));
    // PhoneSn: 1 par requête × 5 = 5 ; CniSn: 1 × 5 = 5.
    assert_eq!(report.aggregated.masked_by_type.get("PhoneSn"), Some(&5));
    assert_eq!(report.aggregated.masked_by_type.get("CniSn"), Some(&5));
    assert!(report.publishable, "all cells >= k=5");
    assert_eq!(report.redacted.get("CniSn"), Some(&5));
    assert_eq!(report.aggregated.quasi_id_flags, 15);
}

#[test]
fn report_redacts_low_cell() {
    let mut c = counters();
    c.masked_by_type.insert("CniSn".to_string(), 1); // < k
    let receipts = vec![Receipt::build(ReceiptMessage {
        tenant_id: "tenant-42".to_string(),
        session_ref_hashed: receipt::hash_session_ref("tenant-42", "sess-1"),
        ts_unix: 1_710_000_000,
        engine_version: "0.1.0".to_string(),
        policy_hash: "abc".to_string(),
        counters: c,
    })];
    let report =
        ConformanceReport::from_receipts(&receipts, 1_710_000_000, 1_710_000_001, 5).unwrap();
    assert_eq!(report.total_requests, 1);
    assert!(!report.publishable, "cell CniSn=1 < k=5");
    assert_eq!(report.redacted.get("CniSn"), Some(&0));
    assert_eq!(report.redacted.get("Email"), Some(&0), "2 < 5 redacted too");
    assert_eq!(
        report.redacted.get("PhoneSn"),
        Some(&0),
        "1 < 5 redacted too"
    );
}

#[test]
fn report_json_has_no_pii_text() {
    let mut receipts = Vec::new();
    for i in 0..6 {
        let mut c = counters();
        c.masked_by_type.insert("Email".to_string(), 1);
        let m = ReceiptMessage {
            tenant_id: "tenant-42".to_string(),
            session_ref_hashed: receipt::hash_session_ref("tenant-42", &format!("sess-{i}")),
            ts_unix: 1_710_000_000 + i,
            engine_version: "0.1.0".to_string(),
            policy_hash: "abc".to_string(),
            counters: c,
        };
        receipts.push(Receipt::build(m));
    }
    let report =
        ConformanceReport::from_receipts(&receipts, 1_710_000_000, 1_710_000_006, 5).unwrap();
    let json = serde_json::to_string(&report).unwrap();
    assert!(!json.contains("user@example.com"));
    assert!(!json.contains("+221"));
    assert!(json.contains("\"publishable\":true"));
}

#[test]
fn policy_hash_is_deterministic() {
    let p1 = cloison_core::Policy::default_for("tenant-42");
    let p2 = cloison_core::Policy::default_for("tenant-42");
    let h1 = receipt::policy_hash(&p1).unwrap();
    let h2 = receipt::policy_hash(&p2).unwrap();
    assert_eq!(h1, h2, "same policy -> same hash");
    assert_eq!(h1.len(), 64);
    let p3 = cloison_core::Policy::default_for("other");
    assert_ne!(h1, receipt::policy_hash(&p3).unwrap());
}
