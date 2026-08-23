//! Tokenization module.
//!
//! HMAC-BLAKE3 token body generation, sentinel formatting/parsing,
//! session key derivation via HKDF, and canonicalization.

use hkdf::Hkdf;
use hmac::Mac;
use serde::{Deserialize, Serialize};
use zeroize::{Zeroize, ZeroizeOnDrop};

use crate::detection::DetectorKind;
use crate::error::{CloisonError, CloisonResult};

/// Session keys derived from tenant key and session salt via HKDF.
///
/// Derivation:
///   ikm   = tenant_key (32 bytes)
///   salt  = session_salt (16 bytes)
///   hkdf  = HKDF-SHA256 (info = b"cloison-session-v1")
///   mac_key = hkdf.expand(b"mac", 32)
///   enc_key = hkdf.expand(b"enc", 32)
#[derive(Clone, Zeroize, ZeroizeOnDrop)]
pub struct SessionKeys {
    /// Raw tenant key (32 bytes).
    pub tenant_key: [u8; 32],
    /// Session salt (16 bytes). Different per session → token rotation.
    pub session_salt: [u8; 16],
    /// MAC key (32 bytes) for HMAC-BLAKE3 token body computation.
    pub mac_key: [u8; 32],
    /// AES-256-GCM encryption key (32 bytes) for the vault.
    pub enc_key: [u8; 32],
}

impl SessionKeys {
    /// Derive session keys from tenant key and session salt via HKDF-SHA256.
    ///
    /// ```text
    /// prk = HKDF-SHA256-Extract(salt=session_salt, ikm=tenant_key)
    /// mac_key = HKDF-SHA256-Expand(prk, info=b"cloison-session-v1/mac", L=32)
    /// enc_key = HKDF-SHA256-Expand(prk, info=b"cloison-session-v1/enc", L=32)
    /// ```
    pub fn derive(tenant_key: [u8; 32], session_salt: [u8; 16]) -> CloisonResult<Self> {
        let hkdf = Hkdf::<sha2::Sha256>::new(Some(&session_salt), &tenant_key);
        let mut mac_key = [0u8; 32];
        let mut enc_key = [0u8; 32];

        hkdf.expand(b"cloison-session-v1/mac", &mut mac_key)
            .map_err(|e| CloisonError::Hkdf(format!("mac key: {}", e)))?;
        hkdf.expand(b"cloison-session-v1/enc", &mut enc_key)
            .map_err(|e| CloisonError::Hkdf(format!("enc key: {}", e)))?;

        Ok(Self {
            tenant_key,
            session_salt,
            mac_key,
            enc_key,
        })
    }

    /// Return the encryption key reference (for vault).
    pub fn enc_key(&self) -> &[u8; 32] {
        &self.enc_key
    }
}

impl std::fmt::Debug for SessionKeys {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SessionKeys")
            .field("session_salt", &hex::encode(self.session_salt))
            .finish_non_exhaustive()
    }
}

/// Helper module for hex encoding without external dep.
mod hex {
    pub fn encode(bytes: impl AsRef<[u8]>) -> String {
        bytes
            .as_ref()
            .iter()
            .map(|b| format!("{:02x}", b))
            .collect()
    }
}

/// Token body: k bytes of HMAC truncation + m bytes of BLAKE3 value hash.
///
/// Construction:
///   mac_full  = HMAC-BLAKE3(mac_key, canonical_value || kind_tag)
///   mac_part  = mac_full[0..K]
///   value_id  = BLAKE3(canonical_value)
///   value_part = value_id[0..M]
///   body      = mac_part || value_part  (K + M = 16 bytes)
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct TokenBody(#[serde(with = "serde_base32")] pub Vec<u8>);

impl TokenBody {
    /// Construct a TokenBody from raw bytes.
    pub fn from_bytes(bytes: Vec<u8>) -> CloisonResult<Self> {
        if bytes.len() != Sentinel::BODY_LEN {
            return Err(CloisonError::SentinelFormat(format!(
                "expected {} bytes, got {}",
                Sentinel::BODY_LEN,
                bytes.len()
            )));
        }
        Ok(Self(bytes))
    }

    /// Construct from base32-encoded string.
    pub fn from_base32(s: &str) -> CloisonResult<Self> {
        let bytes = base32::decode(base32::Alphabet::Rfc4648Lower { padding: false }, s)
            .ok_or_else(|| CloisonError::Base32(format!("invalid base32: {}", s)))?;
        Self::from_bytes(bytes)
    }

    /// Encode to base32 string (RFC 4648 lower case, no padding).
    pub fn to_base32(&self) -> String {
        base32::encode(base32::Alphabet::Rfc4648Lower { padding: false }, &self.0)
    }

    /// Access raw bytes.
    pub fn as_bytes(&self) -> &[u8] {
        &self.0
    }
}

/// Serde helper for base32 encoding/decoding of TokenBody.
mod serde_base32 {
    use base32;
    use serde::{Deserialize, Deserializer, Serializer};

    pub fn serialize<S: Serializer>(bytes: &[u8], serializer: S) -> Result<S::Ok, S::Error> {
        let s = base32::encode(base32::Alphabet::Rfc4648Lower { padding: false }, bytes);
        serializer.serialize_str(&s)
    }

    pub fn deserialize<'de, D: Deserializer<'de>>(deserializer: D) -> Result<Vec<u8>, D::Error> {
        let s = String::deserialize(deserializer)?;
        base32::decode(base32::Alphabet::Rfc4648Lower { padding: false }, &s)
            .ok_or_else(|| serde::de::Error::custom(format!("invalid base32: {}", s)))
    }
}

/// Sentinel format: ⟦body_b32·kind_tag⟧
///
/// Delimiters: U+27E6 ⟦ and U+27E7 ⟧
/// Internal separator: U+00B7 ·
/// kind_tag: 2-4 uppercase ASCII letters identifying the PII type
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Sentinel {
    /// Token body encoded in base32 (RFC 4648, no padding).
    pub token_body_b32: String,
    /// PII type tag (2-4 uppercase ASCII letters).
    pub kind_tag: String,
}

impl Sentinel {
    /// Left delimiter: ⟦ (U+27E6, LEFT WHITE SQUARE BRACKET).
    pub const L_OPEN: char = '\u{27E6}';
    /// Right delimiter: ⟧ (U+27E7, RIGHT WHITE SQUARE BRACKET).
    pub const L_CLOSE: char = '\u{27E7}';
    /// Internal separator: · (U+00B7, MIDDLE DOT).
    pub const L_SEP: char = '\u{00B7}';
    /// MAC truncation length in bytes.
    pub const K: usize = 8;
    /// BLAKE3 value hash truncation length in bytes.
    pub const M: usize = 8;
    /// Total body length in bytes (K + M = 16).
    pub const BODY_LEN: usize = Self::K + Self::M;
    /// Base32 length for 16 bytes: ceil(16*8/5) = 26 characters.
    pub const B32_LEN: usize = 26;

    /// Kind tag mapping.
    pub const TAG_EMAIL: &'static str = "EM";
    /// Kind tag for phone serial numbers.
    pub const TAG_PHONE_SN: &'static str = "PH";
    /// Kind tag for CNI (national ID) serial numbers.
    pub const TAG_CNI_SN: &'static str = "CN";
    /// Kind tag for credit cards.
    pub const TAG_CREDIT_CARD: &'static str = "CC";
    /// Kind tag for IP addresses.
    pub const TAG_IP: &'static str = "IP";
    /// Kind tag for dates.
    pub const TAG_DATE: &'static str = "DT";
    /// Kind tag for NER sidecar PERSON (wiring B.1).
    pub const TAG_PERSON: &'static str = "PE";
    /// Kind tag for NER sidecar LOC (wiring B.1).
    pub const TAG_LOCATION: &'static str = "LO";
    /// Kind tag for gazetteer prefix matches.
    pub const TAG_GAZETTEER_PREFIX: &'static str = "GZ";

    /// Construct a sentinel from a TokenBody and a DetectorKind.
    pub fn new(body: &TokenBody, kind: &DetectorKind) -> CloisonResult<Self> {
        let token_body_b32 = body.to_base32();
        let kind_tag = Self::tag_from_kind(kind).to_string();
        Ok(Self {
            token_body_b32,
            kind_tag,
        })
    }

    /// Serialize the sentinel as a string: ⟦body_b32·kind_tag⟧
    pub fn format(&self) -> String {
        format!(
            "{}{}{}{}",
            Self::L_OPEN,
            self.token_body_b32,
            Self::L_SEP,
            self.kind_tag,
        ) + &Self::L_CLOSE.to_string()
    }

    /// Parse a sentinel from a string.
    ///
    /// Expected format: ⟦[base32]·[tag]⟧
    pub fn parse(s: &str) -> Option<Sentinel> {
        let s = s.trim();
        if !s.starts_with(Self::L_OPEN) || !s.ends_with(Self::L_CLOSE) {
            return None;
        }
        let inner = &s[Self::L_OPEN.len_utf8()..s.len() - Self::L_CLOSE.len_utf8()];
        let parts: Vec<&str> = inner.split(Self::L_SEP).collect();
        if parts.len() != 2 {
            return None;
        }
        let token_body_b32 = parts[0].to_string();
        let kind_tag = parts[1].to_string();

        // Validate kind_tag: 2-4 uppercase ASCII letters
        if kind_tag.is_empty()
            || kind_tag.len() > 4
            || !kind_tag.chars().all(|c| c.is_ascii_uppercase())
        {
            return None;
        }

        // Validate base32 length
        if token_body_b32.len() != Self::B32_LEN {
            return None;
        }

        Some(Sentinel {
            token_body_b32,
            kind_tag,
        })
    }

    /// Map a DetectorKind to its kind_tag.
    pub fn tag_from_kind(kind: &DetectorKind) -> &'static str {
        match kind {
            DetectorKind::Email => Self::TAG_EMAIL,
            DetectorKind::PhoneSn => Self::TAG_PHONE_SN,
            DetectorKind::CniSn => Self::TAG_CNI_SN,
            DetectorKind::CreditCard => Self::TAG_CREDIT_CARD,
            DetectorKind::Ip => Self::TAG_IP,
            DetectorKind::Date => Self::TAG_DATE,
            DetectorKind::Person => Self::TAG_PERSON,
            DetectorKind::Location => Self::TAG_LOCATION,
            DetectorKind::Gazetteer(name) => {
                // GZ + first letter of name uppercase
                match name.as_str() {
                    "nom_sn" => "GZA",
                    "ville_sn" => "GZV",
                    _ => Self::TAG_GAZETTEER_PREFIX,
                }
            }
        }
    }

    /// Map a kind_tag back to a DetectorKind.
    pub fn kind_from_tag(tag: &str) -> CloisonResult<DetectorKind> {
        match tag {
            "EM" => Ok(DetectorKind::Email),
            "PH" => Ok(DetectorKind::PhoneSn),
            "CN" => Ok(DetectorKind::CniSn),
            "CC" => Ok(DetectorKind::CreditCard),
            "IP" => Ok(DetectorKind::Ip),
            "DT" => Ok(DetectorKind::Date),
            "PE" => Ok(DetectorKind::Person),
            "LO" => Ok(DetectorKind::Location),
            "GZA" => Ok(DetectorKind::Gazetteer("nom_sn".to_string())),
            "GZV" => Ok(DetectorKind::Gazetteer("ville_sn".to_string())),
            other if other.starts_with("GZ") => Ok(DetectorKind::Gazetteer(format!(
                "gz_{}",
                other.to_lowercase()
            ))),
            _ => Err(CloisonError::SentinelFormat(format!(
                "unknown kind_tag: {}",
                tag
            ))),
        }
    }

    /// Extract the full sentinel string from text at a given position.
    /// Returns the sentinel string and its byte span (start, end).
    pub fn extract_from_text(text: &str, start: usize) -> Option<(String, usize, usize)> {
        let remaining = text.get(start..)?;
        if !remaining.starts_with(Self::L_OPEN) {
            return None;
        }
        // Find the closing delimiter
        let close_pos = remaining.find(Self::L_CLOSE)?;
        let sentinel_str = &remaining[..close_pos + Self::L_CLOSE.len_utf8()];
        let _parsed = Self::parse(sentinel_str)?;
        Some((sentinel_str.to_string(), start, start + sentinel_str.len()))
    }
}

/// Compute the token body for a canonical value and entity type.
///
/// Steps:
///   1. mac_full = HMAC-SHA256(mac_key, canonical_value || kind_tag)
///   2. mac_part = mac_full[0..K]
///   3. value_id = BLAKE3(canonical_value)
///   4. value_part = value_id[0..M]
///   5. body = mac_part || value_part
pub fn token_body(
    keys: &SessionKeys,
    canonical_value: &str,
    kind: &DetectorKind,
) -> CloisonResult<TokenBody> {
    let kind_tag = Sentinel::tag_from_kind(kind);
    let mac_input = [canonical_value.as_bytes(), kind_tag.as_bytes()].concat();

    // HMAC-SHA256
    let mut mac = hmac::Hmac::<sha2::Sha256>::new_from_slice(&keys.mac_key)
        .expect("HMAC accepts any key size");
    mac.update(&mac_input);
    let mac_result = mac.finalize().into_bytes();
    let mac_part = &mac_result[..Sentinel::K];

    // BLAKE3 value hash
    let value_hash = blake3::hash(canonical_value.as_bytes());
    let value_part = &value_hash.as_bytes()[..Sentinel::M];

    let body_bytes = [mac_part, value_part].concat();
    TokenBody::from_bytes(body_bytes)
}

/// Compute a MAC over entity type + token body using BLAKE3 keyed hash.
///
/// This provides an integrity check: mac = BLAKE3_keyed(mac_key, TYPE + body)[..M]
pub fn compute_mac(keys: &SessionKeys, kind: &DetectorKind, body_b32: &str) -> String {
    let kind_tag = Sentinel::tag_from_kind(kind);
    let input = [kind_tag.as_bytes(), body_b32.as_bytes()].concat();
    let keyed_hash = blake3::keyed_hash(&keys.mac_key, &input);
    let mac_bytes = &keyed_hash.as_bytes()[..12]; // m = 12 bytes
    base32::encode(base32::Alphabet::Rfc4648Lower { padding: false }, mac_bytes)
}

/// Verify that a TokenBody corresponds to a given clear value and entity type.
pub fn verify_body(
    body: &TokenBody,
    plain_value: &str,
    kind: &DetectorKind,
    keys: &SessionKeys,
) -> bool {
    // Le MAC est calculé sur la valeur CANONIQUE (canonicalize) à l'émission :
    // vérifier aussi sur la valeur canonique, sinon toute valeur dont la forme
    // canonique diffère (majuscules, espaces, NFC) échoue à la restauration.
    let canonical = canonicalize(plain_value);
    match token_body(keys, &canonical, kind) {
        Ok(expected) => expected == *body,
        Err(_) => false,
    }
}

/// Canonicalize a value: trim whitespace, lowercase, NFC normalization.
pub fn canonicalize(value: &str) -> String {
    use unicode_normalization::UnicodeNormalization;
    value.trim().to_lowercase().nfc().collect()
}

/// Complete token: body + kind + plain value + sentinel.
pub struct Token {
    /// Token body (K + M = 16 bytes).
    pub body: TokenBody,
    /// PII entity type.
    pub kind: DetectorKind,
    /// Original clear value (available only at emission time).
    pub plain_value: String,
    /// Formatted sentinel (⟦body·tag⟧).
    pub sentinel: Sentinel,
}

impl Token {
    /// Emit a new token from a clear value, entity type, and session keys.
    ///
    /// The token body is deterministic: same (value, kind, keys) → same body.
    pub fn emit(plain_value: &str, kind: &DetectorKind, keys: &SessionKeys) -> CloisonResult<Self> {
        let canonical = canonicalize(plain_value);
        let body = token_body(keys, &canonical, kind)?;
        let sentinel = Sentinel::new(&body, kind)?;

        // Self-verification invariant
        debug_assert!(
            verify_body(&body, &canonical, kind, keys),
            "Token self-verification failed"
        );

        Ok(Self {
            body,
            kind: kind.clone(),
            plain_value: plain_value.to_string(),
            sentinel,
        })
    }

    /// Verify that a TokenBody matches a given clear value under session keys.
    pub fn verify_body(
        body: &TokenBody,
        plain_value: &str,
        kind: &DetectorKind,
        keys: &SessionKeys,
    ) -> bool {
        verify_body(body, plain_value, kind, keys)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_keys() -> SessionKeys {
        let tenant_key = [0xABu8; 32];
        let session_salt = [0xCDu8; 16];
        SessionKeys::derive(tenant_key, session_salt).unwrap()
    }

    #[test]
    fn test_sentinel_format_parse_roundtrip() {
        let body = TokenBody::from_bytes(vec![0u8; 16]).unwrap();
        let sent = Sentinel::new(&body, &DetectorKind::Email).unwrap();
        let s = sent.format();
        let parsed = Sentinel::parse(&s).unwrap();
        assert_eq!(parsed.token_body_b32, sent.token_body_b32);
        assert_eq!(parsed.kind_tag, "EM");
    }

    #[test]
    fn test_sentinel_delimiters() {
        let body = TokenBody::from_bytes(vec![0u8; 16]).unwrap();
        let sent = Sentinel::new(&body, &DetectorKind::Email).unwrap();
        let s = sent.format();
        assert!(s.starts_with('\u{27E6}'));
        assert!(s.ends_with('\u{27E7}'));
        assert!(s.contains('\u{00B7}'));
    }

    #[test]
    fn test_token_determinism() {
        let keys = test_keys();
        let t1 = Token::emit("user@example.com", &DetectorKind::Email, &keys).unwrap();
        let t2 = Token::emit("user@example.com", &DetectorKind::Email, &keys).unwrap();
        assert_eq!(t1.body, t2.body);
        assert_eq!(t1.sentinel, t2.sentinel);
    }

    #[test]
    fn test_token_rotation() {
        let keys1 = test_keys();
        let keys2 = SessionKeys::derive([0xFFu8; 32], [0x00u8; 16]).unwrap();
        let t1 = Token::emit("user@example.com", &DetectorKind::Email, &keys1).unwrap();
        let t2 = Token::emit("user@example.com", &DetectorKind::Email, &keys2).unwrap();
        assert_ne!(
            t1.body, t2.body,
            "Different sessions must produce different tokens"
        );
    }

    #[test]
    fn test_verify_body() {
        let keys = test_keys();
        let token = Token::emit("user@example.com", &DetectorKind::Email, &keys).unwrap();
        assert!(Token::verify_body(
            &token.body,
            "user@example.com",
            &DetectorKind::Email,
            &keys
        ));
        assert!(!Token::verify_body(
            &token.body,
            "other@example.com",
            &DetectorKind::Email,
            &keys
        ));
    }

    #[test]
    fn test_canonicalize() {
        assert_eq!(canonicalize("  Hello World  "), "hello world");
        // NFC normalization
        assert_eq!(canonicalize("é"), "é"); // already NFC
    }
}
