//! Checkpoints signés — ancrage anti-troncature.
//!
//! Le journal est append-only, mais un **miroir compromis** peut retirer les entrées les
//! plus récentes : la réécriture est détectable, la **troncature ne l'est pas** sans un
//! ancrage externe. Le contrôle publie périodiquement un [`Checkpoint`] signé qui fige
//! `(seq, entry_hash)` de la tête du journal :
//!
//! - `sig = Ed25519(clé du contrôle, header canonique du checkpoint)` ;
//! - `prev_cp_hash` chaîne les checkpoints (genèse des checkpoints = `[0u8; 32]`) ;
//! - un vérificateur qui possède un checkpoint valide **refuse** toute chaîne dont la
//!   dernière entrée a un `seq < checkpoint.seq` (troncature) ou dont l'entrée
//!   `checkpoint.seq` ne porte pas `checkpoint.entry_hash` (divergence).
//!
//! Voir `API_DESIGN.md` (STACK-5) §3.4 (barrière 4) et §6.2.

use crate::entry::sha256;
use ed25519_dalek::{Signature, Signer, SigningKey, VerifyingKey};
use serde::{Deserialize, Serialize};

/// `prev_cp_hash` de la genèse des checkpoints (aucun checkpoint précédent) : `[0u8; 32]`.
pub const GENESIS_PREV_CP_HASH: [u8; 32] = [0u8; 32];

/// Longueur d'une signature Ed25519.
pub const SIGNATURE_LEN: usize = 64;

/// Checkpoint signé : ancre la chaîne du journal à un `seq` donné.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Checkpoint {
    /// `seq` de l'entrée ancrée (la tête du journal au moment de la signature).
    pub seq: u64,
    /// `entry_hash` de l'entrée ancrée — toute divergence ou troncature casse le lien.
    pub entry_hash: [u8; 32],
    /// Hash du checkpoint précédent ; genèse des checkpoints = `[0u8; 32]`.
    pub prev_cp_hash: [u8; 32],
    /// Horodatage Unix (secondes, UTC) de la signature.
    pub ts_unix: u64,
    /// `Ed25519(clé du contrôle, header canonique du checkpoint)` (64 octets). Vide pour la genèse.
    pub sig: Vec<u8>,
}

impl Checkpoint {
    /// Checkpoint de genèse : `seq = 0`, `entry_hash = [0u8; 32]`, aucun lien, aucune signature.
    pub fn genesis() -> Checkpoint {
        Checkpoint {
            seq: 0,
            entry_hash: [0u8; 32],
            prev_cp_hash: GENESIS_PREV_CP_HASH,
            ts_unix: 0,
            sig: Vec::new(),
        }
    }

    /// Header binaire canonique (80 octets) :
    /// `seq.to_le_bytes() ‖ entry_hash ‖ prev_cp_hash ‖ ts_unix.to_le_bytes()`.
    pub fn header_bytes(
        seq: u64,
        entry_hash: &[u8; 32],
        prev_cp_hash: &[u8; 32],
        ts_unix: u64,
    ) -> Vec<u8> {
        let mut header = Vec::with_capacity(8 + 32 + 32 + 8);
        header.extend_from_slice(&seq.to_le_bytes());
        header.extend_from_slice(entry_hash);
        header.extend_from_slice(prev_cp_hash);
        header.extend_from_slice(&ts_unix.to_le_bytes());
        header
    }

    /// `digest = SHA-256(header_bytes(...))` — le lien porté par `prev_cp_hash`.
    pub fn digest(&self) -> [u8; 32] {
        sha256(&Self::header_bytes(
            self.seq,
            &self.entry_hash,
            &self.prev_cp_hash,
            self.ts_unix,
        ))
    }

    /// `prev_cp_hash` attendu pour un checkpoint suivant `prev` : la genèse des checkpoints
    /// est `[0u8; 32]` (comme `GENESIS_PREV_HASH` pour le journal) ; sinon `prev.digest()`.
    pub fn prev_hash_of(prev: &Checkpoint) -> [u8; 32] {
        if prev.seq == 0 {
            GENESIS_PREV_CP_HASH
        } else {
            prev.digest()
        }
    }

    /// Construit un checkpoint signé par la clé du contrôle (`control_verify_key` dans le
    /// design STACK-5, distincte de la clé qui signe les entrées du journal).
    pub fn sign(
        seq: u64,
        entry_hash: [u8; 32],
        prev: &Checkpoint,
        ts_unix: u64,
        key: &SigningKey,
    ) -> Checkpoint {
        let prev_cp_hash = Self::prev_hash_of(prev);
        let header = Self::header_bytes(seq, &entry_hash, &prev_cp_hash, ts_unix);
        let sig = key.sign(&header).to_bytes().to_vec();
        Checkpoint {
            seq,
            entry_hash,
            prev_cp_hash,
            ts_unix,
            sig,
        }
    }

    /// Vérifie le checkpoint contre le précédent et la clé publique :
    /// 1. lien `prev_cp_hash` correct ;
    /// 2. signature Ed25519 valide (`verify_strict`) sur le header canonique ;
    /// 3. la genèse n'a ni signature ni lien.
    pub fn verify(&self, prev: &Checkpoint, key: &VerifyingKey) -> bool {
        if self.seq == 0 {
            return self.entry_hash == [0u8; 32]
                && self.prev_cp_hash == GENESIS_PREV_CP_HASH
                && self.sig.is_empty();
        }
        if self.prev_cp_hash != Self::prev_hash_of(prev) {
            return false;
        }
        let header =
            Self::header_bytes(self.seq, &self.entry_hash, &self.prev_cp_hash, self.ts_unix);
        let Ok(sig_bytes) = <[u8; SIGNATURE_LEN]>::try_from(self.sig.as_slice()) else {
            return false;
        };
        key.verify_strict(&header, &Signature::from_bytes(&sig_bytes))
            .is_ok()
    }

    /// Affiche le checkpoint sous forme hexadécimale compacte — aucun contenu sensible.
    pub fn summary(&self) -> String {
        format!(
            "seq={} entry={} prev_cp={} ts={}",
            self.seq,
            crate::hexutil::encode(&self.entry_hash),
            crate::hexutil::encode(&self.prev_cp_hash),
            self.ts_unix
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn control_key() -> SigningKey {
        SigningKey::from_bytes(&[21u8; 32])
    }

    #[test]
    fn genesis_checkpoint_is_well_formed() {
        let g = Checkpoint::genesis();
        assert_eq!(g.seq, 0);
        assert_eq!(g.entry_hash, [0u8; 32]);
        assert_eq!(g.prev_cp_hash, GENESIS_PREV_CP_HASH);
        assert!(g.sig.is_empty());
        assert!(g.verify(&g, &control_key().verifying_key()));
    }

    #[test]
    fn checkpoint_sign_verify_roundtrip() {
        let key = control_key();
        let g = Checkpoint::genesis();
        let cp = Checkpoint::sign(7, [0xAB; 32], &g, 1_700_000_000, &key);
        assert_eq!(cp.prev_cp_hash, GENESIS_PREV_CP_HASH);
        assert!(cp.verify(&g, &key.verifying_key()));

        // Mauvaise clé → invalide.
        let other = SigningKey::from_bytes(&[22u8; 32]);
        assert!(!cp.verify(&g, &other.verifying_key()));
        // Mauvais précédent → invalide.
        let other_prev = Checkpoint::sign(1, [0x01; 32], &g, 1, &key);
        assert!(!cp.verify(&other_prev, &key.verifying_key()));
        // Signature altérée → invalide.
        let mut tampered = cp.clone();
        tampered.sig[0] ^= 0x01;
        assert!(!tampered.verify(&g, &key.verifying_key()));
        // Message altéré (entry_hash) sans re-signature → invalide.
        let mut tampered = cp.clone();
        tampered.entry_hash = [0xCD; 32];
        assert!(!tampered.verify(&g, &key.verifying_key()));
    }

    #[test]
    fn checkpoint_chain_links() {
        let key = control_key();
        let g = Checkpoint::genesis();
        let cp1 = Checkpoint::sign(3, [0x11; 32], &g, 100, &key);
        let cp2 = Checkpoint::sign(8, [0x22; 32], &cp1, 200, &key);
        assert_eq!(cp2.prev_cp_hash, cp1.digest());
        assert!(cp2.verify(&cp1, &key.verifying_key()));
        assert!(!cp2.verify(&g, &key.verifying_key()));
    }
}
