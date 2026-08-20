//! Jetons `mn_` : génération, hash et comparaison en temps constant.
//!
//! - Format : `mn_` + base64url(32 octets aléatoires) → 46 caractères.
//! - `token_hash = hex(SHA-256("cloison-mn-token-v1:" ‖ clair))` — domaine séparé,
//!   cohérent avec STACK-4 (`hash_session_ref`).
//! - La comparaison se fait sur les **digests**, en temps constant (`subtle`).

use rand::rngs::OsRng;
use rand::RngCore;
use sha2::{Digest, Sha256};

/// Domaine du hash de jeton (séparé des autres hash CLOISON).
pub const TOKEN_HASH_DOMAIN: &str = "cloison-mn-token-v1:";

/// Préfixe de tout jeton `mn_`.
pub const TOKEN_PREFIX: &str = "mn_";

/// Nombre d'octets aléatoires du secret.
pub const TOKEN_SECRET_BYTES: usize = 32;

/// Génère un jeton `mn_` : 32 octets via `OsRng`, encodés base64url.
/// Le clair est renvoyé **une seule fois** ; seul son hash doit être persisté.
pub fn generate_token() -> String {
    let mut secret = [0u8; TOKEN_SECRET_BYTES];
    OsRng.fill_bytes(&mut secret);
    format!("{}{}", TOKEN_PREFIX, base64url(&secret))
}

/// Identifiant unique de jeton : `tok-{tenant_id}-{ts}-{aléa}` — aucune PII, aucun
/// risque de collision entre deux émissions dans la même seconde.
pub fn new_token_id(tenant_id: &str, now_unix: u64) -> String {
    let mut rng_bytes = [0u8; 4];
    OsRng.fill_bytes(&mut rng_bytes);
    format!("tok-{}-{}-{}", tenant_id, now_unix, hex_encode(&rng_bytes))
}

/// `hex(SHA-256("cloison-mn-token-v1:" ‖ token))` — la formule de stockage.
pub fn token_hash(token: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(TOKEN_HASH_DOMAIN.as_bytes());
    hasher.update(token.as_bytes());
    hex_encode(&hasher.finalize())
}

/// Compare le digest SHA-256 du jeton présenté au hash stocké, en temps constant
/// (jamais de comparaison sur le clair, jamais de fuite par timing sur la longueur).
pub fn verify_token_constant_time(presented: &str, stored_hash: &str) -> bool {
    let digest = token_hash(presented);
    constant_time_eq(digest.as_bytes(), stored_hash.as_bytes())
}

/// Encodage base64url sans padding (RFC 4648 §5) — implémentation locale, pas de
/// dépendance externe.
pub(crate) fn base64url(bytes: &[u8]) -> String {
    const ALPHABET: &[u8; 64] =
        b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789-_";
    let mut out = String::with_capacity(bytes.len().div_ceil(3) * 4);
    for chunk in bytes.chunks(3) {
        let b0 = chunk[0] as u32;
        let b1 = *chunk.get(1).unwrap_or(&0) as u32;
        let b2 = *chunk.get(2).unwrap_or(&0) as u32;
        let triple = (b0 << 16) | (b1 << 8) | b2;
        out.push(ALPHABET[((triple >> 18) & 0x3f) as usize] as char);
        out.push(ALPHABET[((triple >> 12) & 0x3f) as usize] as char);
        if chunk.len() > 1 {
            out.push(ALPHABET[((triple >> 6) & 0x3f) as usize] as char);
        }
        if chunk.len() > 2 {
            out.push(ALPHABET[(triple & 0x3f) as usize] as char);
        }
    }
    out
}

/// Encodage hexadécimal local (le crate `hex` n'est pas dans le cache hors-ligne).
pub(crate) fn hex_encode(bytes: &[u8]) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut out = String::with_capacity(bytes.len() * 2);
    for b in bytes {
        out.push(HEX[(b >> 4) as usize] as char);
        out.push(HEX[(b & 0x0f) as usize] as char);
    }
    out
}

/// Comparaison en temps constant (digests de même longueur).
pub(crate) fn constant_time_eq(a: &[u8], b: &[u8]) -> bool {
    use subtle::ConstantTimeEq;
    bool::from(a.ct_eq(b))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn token_shape_and_uniqueness() {
        let a = generate_token();
        let b = generate_token();
        assert!(a.starts_with("mn_"));
        assert_eq!(a.len(), 46);
        assert_ne!(a, b);
    }

    #[test]
    fn hash_is_hex_sha256_with_domain() {
        let token = "mn_test";
        let expected = hex_encode(&{
            let mut h = Sha256::new();
            h.update(TOKEN_HASH_DOMAIN.as_bytes());
            h.update(token.as_bytes());
            h.finalize()
        });
        assert_eq!(token_hash(token), expected);
        assert_eq!(token_hash(token).len(), 64);
    }

    #[test]
    fn constant_time_verify() {
        let token = generate_token();
        let stored = token_hash(&token);
        assert!(verify_token_constant_time(&token, &stored));
        assert!(!verify_token_constant_time("mn_other", &stored));
        // Le clair n'est jamais comparable directement au stockage.
        assert_ne!(token, stored);
    }

    #[test]
    fn base64url_is_padding_free() {
        // 0xfbffef → 111110 111111 111111 101111 → '-', '_', '_', 'v'
        let s = base64url(&[0xfb, 0xff, 0xef]);
        assert_eq!(s, "-__v");
    }
}
