//! CLOISON STACK-2 — Blocking invariant tests.
//!
//! These tests verify the security invariants that MUST hold:
//! 1. Roundtrip: tokenize(restore(x)) == x
//! 2. No clear leaving: tokenized text contains no original PII values
//! 3. Anti-collision: forged sentinel (wrong MAC) is never restored
//! 4. Determinism: same value + same session → same token
//! 5. Rotation: same value + different session → different token
//! 6. Luhn: valid CNI detected, invalid CNI rejected

use base32::{encode as b32_encode, Alphabet};
use cloison_core::*;

fn make_keys(salt_byte: u8) -> SessionKeys {
    let tenant_key = [0xABu8; 32];
    let session_salt = [salt_byte; 16];
    SessionKeys::derive(tenant_key, session_salt).unwrap()
}

fn make_policy() -> Policy {
    Policy::default_for("test-tenant")
}

/// Compute a Luhn check digit for a base digit string.
fn luhn_check_digit(base: &str) -> u32 {
    let digits: Vec<u32> = base.chars().filter_map(|c| c.to_digit(10)).collect();
    let mut sum = 0u32;
    let mut double = true; // parite alignee : la base de 12 est decalee
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

// ──── Invariant 1: Roundtrip ────

#[test]
fn invariant_roundtrip_email() {
    let keys = make_keys(0x01);
    let mut engine = Engine::new(keys).unwrap();
    let policy = make_policy();

    let original = "Contact: user@example.com pour details";
    let tokenized = engine.tokenize(original, &policy, "req-rt-1").unwrap();
    let restored = engine.restore(&tokenized.text_out, "req-rt-1").unwrap();

    assert_eq!(
        restored.text_out, original,
        "INVIOLABLE: restore(tokenize(x)) == x"
    );
}

#[test]
fn invariant_roundtrip_phone_sn() {
    let keys = make_keys(0x02);
    let mut engine = Engine::new(keys).unwrap();
    let policy = make_policy();

    let original = "Appeler +221 77 123 45 67 maintenant";
    let tokenized = engine.tokenize(original, &policy, "req-rt-2").unwrap();
    let restored = engine.restore(&tokenized.text_out, "req-rt-2").unwrap();

    assert_eq!(
        restored.text_out, original,
        "INVIOLABLE: restore(tokenize(x)) == x"
    );
}

#[test]
fn invariant_roundtrip_cni_sn() {
    let keys = make_keys(0x03);
    let mut engine = Engine::new(keys).unwrap();
    let policy = make_policy();

    // Build a valid 13-digit CNI starting with 1
    let base = "123456789012";
    let check = luhn_check_digit(base);
    let cni = format!("{}{}", base, check);
    let original = format!("CNI: {}", cni);

    let tokenized = engine.tokenize(&original, &policy, "req-rt-3").unwrap();
    let restored = engine.restore(&tokenized.text_out, "req-rt-3").unwrap();

    assert_eq!(
        restored.text_out, original,
        "INVIOLABLE: restore(tokenize(x)) == x"
    );
}

#[test]
fn invariant_roundtrip_multiple_pii() {
    let keys = make_keys(0x04);
    let mut engine = Engine::new(keys).unwrap();
    let policy = make_policy();

    let base = "123456789012";
    let check = luhn_check_digit(base);
    let cni = format!("{}{}", base, check);
    let original = format!("Email: user@test.com, Tel: +221 77 123 45 67, CNI: {}", cni);

    let tokenized = engine.tokenize(&original, &policy, "req-rt-4").unwrap();
    let restored = engine.restore(&tokenized.text_out, "req-rt-4").unwrap();

    assert_eq!(
        restored.text_out, original,
        "INVIOLABLE: restore(tokenize(x)) == x with multiple PII types"
    );
}

// ──── Invariant 2: No clear leaving ────

#[test]
fn invariant_no_clear_leaving_email() {
    let keys = make_keys(0x10);
    let mut engine = Engine::new(keys).unwrap();
    let policy = make_policy();

    let original = "Send to secret@private.org";
    let tokenized = engine.tokenize(original, &policy, "req-ncl-1").unwrap();

    assert!(
        !tokenized.text_out.contains("secret@private.org"),
        "INVIOLABLE: no clear value in tokenized text"
    );
}

#[test]
fn invariant_no_clear_leaving_phone() {
    let keys = make_keys(0x11);
    let mut engine = Engine::new(keys).unwrap();
    let policy = make_policy();

    let original = "Call +221 77 123 45 67 now";
    let tokenized = engine.tokenize(original, &policy, "req-ncl-2").unwrap();

    assert!(
        !tokenized.text_out.contains("+221 77 123 45 67"),
        "INVIOLABLE: no clear value in tokenized text"
    );
}

#[test]
fn invariant_no_clear_leaving_cni() {
    let keys = make_keys(0x12);
    let mut engine = Engine::new(keys).unwrap();
    let policy = make_policy();

    let base = "123456789012";
    let check = luhn_check_digit(base);
    let cni = format!("{}{}", base, check);
    let original = format!("CNI: {}", cni);

    let tokenized = engine.tokenize(&original, &policy, "req-ncl-3").unwrap();

    assert!(
        !tokenized.text_out.contains(&cni),
        "INVIOLABLE: no clear value in tokenized text"
    );
}

// ──── Invariant 3: Anti-collision ────

#[test]
fn invariant_anti_collision_forged_mac() {
    let keys = make_keys(0x20);
    let mut engine = Engine::new(keys).unwrap();
    let policy = make_policy();

    // Tokenize something to populate the registry
    let _ = engine
        .tokenize("user@example.com", &policy, "req-ac-1")
        .unwrap();

    // Forge a sentinel with a random body (wrong MAC)
    let forged_body = [0xFFu8; 16];
    let forged_body_b32 = b32_encode(Alphabet::Rfc4648Lower { padding: false }, &forged_body);
    let forged_sentinel = format!(
        "{}{}{}{}{}",
        Sentinel::L_OPEN,
        forged_body_b32,
        Sentinel::L_SEP,
        "EM",
        Sentinel::L_CLOSE,
    );

    let restored = engine.restore(&forged_sentinel, "req-ac-1").unwrap();

    assert!(
        restored.counters.blocked > 0 || restored.counters.incomplete > 0,
        "INVIOLABLE: forged sentinel must never be restored"
    );
    assert_eq!(
        restored.counters.restored, 0,
        "INVIOLABLE: forged sentinel must never be restored"
    );
}

#[test]
fn invariant_anti_collision_cross_session() {
    let keys1 = make_keys(0x21);
    let keys2 = make_keys(0x22);
    let mut engine1 = Engine::new(keys1).unwrap();
    let engine2 = Engine::new(keys2).unwrap();
    let policy = make_policy();

    // Tokenize with engine1
    let r1 = engine1
        .tokenize("user@example.com", &policy, "req-ac-2")
        .unwrap();

    // Try to restore the sentinel from engine1 using engine2 (different session)
    let restored = engine2.restore(&r1.text_out, "req-ac-2b").unwrap();

    assert_eq!(
        restored.counters.restored, 0,
        "INVIOLABLE: cross-session sentinel must never be restored"
    );
}

// ──── Invariant 4: Determinism ────

#[test]
fn invariant_determinism_same_session() {
    let keys = make_keys(0x30);
    let mut engine1 = Engine::new(keys.clone()).unwrap();
    let mut engine2 = Engine::new(keys).unwrap();
    let policy = make_policy();

    let r1 = engine1
        .tokenize("user@example.com", &policy, "req-det-1")
        .unwrap();
    let r2 = engine2
        .tokenize("user@example.com", &policy, "req-det-2")
        .unwrap();

    assert_eq!(
        r1.emitted[0].body_b32, r2.emitted[0].body_b32,
        "INVIOLABLE: same value + same session → same token"
    );
    assert_eq!(
        r1.emitted[0].sentinel, r2.emitted[0].sentinel,
        "INVIOLABLE: same value + same session → same sentinel"
    );
}

#[test]
fn invariant_determinism_phone() {
    let keys = make_keys(0x31);
    let mut engine1 = Engine::new(keys.clone()).unwrap();
    let mut engine2 = Engine::new(keys).unwrap();
    let policy = make_policy();

    let phone = "+221 77 123 45 67";
    let r1 = engine1.tokenize(phone, &policy, "req-det-3").unwrap();
    let r2 = engine2.tokenize(phone, &policy, "req-det-4").unwrap();

    assert_eq!(
        r1.emitted[0].body_b32, r2.emitted[0].body_b32,
        "INVIOLABLE: determinism for phone numbers"
    );
}

// ──── Invariant 5: Rotation ────

#[test]
fn invariant_rotation_different_salt() {
    let keys1 = make_keys(0x40);
    let keys2 = make_keys(0x41);
    let mut engine1 = Engine::new(keys1).unwrap();
    let mut engine2 = Engine::new(keys2).unwrap();
    let policy = make_policy();

    let r1 = engine1
        .tokenize("user@example.com", &policy, "req-rot-1")
        .unwrap();
    let r2 = engine2
        .tokenize("user@example.com", &policy, "req-rot-2")
        .unwrap();

    assert_ne!(
        r1.emitted[0].body_b32, r2.emitted[0].body_b32,
        "INVIOLABLE: same value + different session → different token"
    );
}

#[test]
fn invariant_rotation_different_tenant_key() {
    let keys1 = SessionKeys::derive([0xAAu8; 32], [0x01u8; 16]).unwrap();
    let keys2 = SessionKeys::derive([0xBBu8; 32], [0x01u8; 16]).unwrap();
    let mut engine1 = Engine::new(keys1).unwrap();
    let mut engine2 = Engine::new(keys2).unwrap();
    let policy = make_policy();

    let r1 = engine1
        .tokenize("user@example.com", &policy, "req-rot-3")
        .unwrap();
    let r2 = engine2
        .tokenize("user@example.com", &policy, "req-rot-4")
        .unwrap();

    assert_ne!(
        r1.emitted[0].body_b32, r2.emitted[0].body_b32,
        "INVIOLABLE: different tenant key → different token"
    );
}

// ──── Invariant 6: Luhn ────

#[test]
fn invariant_luhn_valid_cni_detected() {
    let base = "123456789012";
    let check = luhn_check_digit(base);
    let cni = format!("{}{}", base, check);

    assert!(
        validate_luhn(&cni),
        "INVIOLABLE: CNI with valid Luhn must pass validation"
    );

    let detector = Detector::new().unwrap();
    let policy = make_policy();
    let spans = detector.detect_with_policy(&format!("CNI: {}", cni), &policy.detection);

    let cni_spans: Vec<_> = spans
        .iter()
        .filter(|s| s.entity_type == DetectorKind::CniSn)
        .collect();
    assert_eq!(cni_spans.len(), 1, "INVIOLABLE: valid CNI must be detected");
}

#[test]
fn invariant_luhn_invalid_cni_rejected() {
    // 13 digits starting with 1 but failing Luhn
    let invalid_cni = "1234567890123"; // This is unlikely to pass Luhn

    assert!(
        !validate_luhn(invalid_cni),
        "INVIOLABLE: invalid CNI must fail Luhn check"
    );

    let detector = Detector::new().unwrap();
    let policy = make_policy();
    let spans = detector.detect_with_policy(&format!("CNI: {}", invalid_cni), &policy.detection);

    let cni_spans: Vec<_> = spans
        .iter()
        .filter(|s| s.entity_type == DetectorKind::CniSn)
        .collect();
    assert!(
        cni_spans.is_empty(),
        "INVIOLABLE: CNI with invalid Luhn must NOT be detected"
    );
}

#[test]
fn invariant_luhn_known_valid() {
    // Test with known valid Luhn numbers
    assert!(validate_luhn("79927398713"), "known valid Luhn number");
    assert!(validate_luhn("4242424242424242"), "Visa test number");
}

#[test]
fn invariant_luhn_known_invalid() {
    assert!(!validate_luhn("79927398714"), "known invalid Luhn number");
    assert!(
        !validate_luhn("4242424242424243"),
        "altered Visa test number"
    );
}
