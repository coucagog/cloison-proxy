//! Encodage/décodage hexadécimal local (le crate `hex` n'est pas disponible dans le cache
//! hors-ligne ; équivalent fonctionnel minimal, déterministe).

use thiserror::Error;

/// Erreur de décodage hexadécimal.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
#[error("invalid hex string")]
pub struct HexError;

/// Encode des octets en hexadécimal minuscule.
pub fn encode(bytes: &[u8]) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut out = String::with_capacity(bytes.len() * 2);
    for b in bytes {
        out.push(HEX[(b >> 4) as usize] as char);
        out.push(HEX[(b & 0x0f) as usize] as char);
    }
    out
}

/// Décode une chaîne hexadécimale (longueur paire, caractères `0-9a-fA-F`).
pub fn decode(s: &str) -> Result<Vec<u8>, HexError> {
    let bytes = s.as_bytes();
    if bytes.len() & 1 != 0 {
        return Err(HexError);
    }
    let mut out = Vec::with_capacity(bytes.len() / 2);
    // La longueur est paire (vérifiée ci-dessus) : `as_chunks::<2>()` ne laisse
    // aucun reste — équivalent à `chunks_exact`, sans le lint clippy 1.98.
    for pair in bytes.as_chunks::<2>().0 {
        let hi = nibble(pair[0])?;
        let lo = nibble(pair[1])?;
        out.push((hi << 4) | lo);
    }
    Ok(out)
}

/// Décode une chaîne hexadécimale de longueur exacte `N` (ex. 32 octets pour un hash).
pub fn decode_array<const N: usize>(s: &str) -> Result<[u8; N], HexError> {
    let bytes = decode(s)?;
    bytes.try_into().map_err(|_| HexError)
}

#[inline]
fn nibble(b: u8) -> Result<u8, HexError> {
    match b {
        b'0'..=b'9' => Ok(b - b'0'),
        b'a'..=b'f' => Ok(b - b'a' + 10),
        b'A'..=b'F' => Ok(b - b'A' + 10),
        _ => Err(HexError),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trip() {
        let bytes = [0u8, 1, 15, 16, 255, 0xab, 0xcd];
        let encoded = encode(&bytes);
        assert_eq!(encoded, "00010f10ffabcd");
        assert_eq!(decode(&encoded).unwrap(), bytes);
    }

    #[test]
    fn rejects_odd_length_and_bad_chars() {
        assert_eq!(decode("abc"), Err(HexError));
        assert_eq!(decode("zz"), Err(HexError));
        assert_eq!(decode_array::<32>("ff"), Err(HexError));
    }
}
