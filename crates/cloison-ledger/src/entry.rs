//! Entrée du journal et payload (compteurs k-anonymisés uniquement).
//!
//! Règles d'or :
//! - `entry_hash = SHA-256(header binaire canonique)` où
//!   `header = seq.to_le_bytes() ‖ prev_hash ‖ payload_hash ‖ ts_unix.to_le_bytes()` (80 octets) ;
//! - `payload_hash = SHA-256(JSON canonique compact du LedgerPayload)` (BTreeMap → clés triées) ;
//! - la signature Ed25519 du contrôle couvre `entry_hash` (message qui lie seq, prev_hash,
//!   payload_hash et ts) ;
//! - **aucune PII** : le payload ne contient que des compteurs déjà k-anonymisés et des
//!   hash de reçus — jamais de texte, jamais de span, jamais de valeur brute.
//!
//! Format public verrouillé par `API_DESIGN.md` (STACK-5) : `entry_hash = SHA-256(header)`,
//! genèse `seq = 0` non signée (les signatures commencent à `seq = 1`), champ `ts_unix`.

use crate::hexutil;
use ed25519_dalek::{Signature, Signer, SigningKey, VerifyingKey};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;

/// `prev_hash` de la genèse (aucune entrée précédente) : `[0u8; 32]`.
pub const GENESIS_PREV_HASH: [u8; 32] = [0u8; 32];

/// Taille de l'`entry_hash` (32 octets, SHA-256).
pub const HASH_LEN: usize = 32;

/// Longueur d'une signature Ed25519.
pub const SIGNATURE_LEN: usize = 64;

/// Une entrée du journal de transparence.
///
/// La chaîne est liée par `prev_hash` (→ l'`entry_hash` de l'entrée précédente) et par
/// `entry_hash` (→ hash canonique de tous les autres champs). Toute modification d'un
/// seul octet casse `entry_hash` **et** le `prev_hash` de l'entrée suivante.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LedgerEntry {
    /// Numéro séquentiel ; la genèse vaut 0, les entrées réelles commencent à 1.
    pub seq: u64,
    /// `entry_hash` de l'entrée précédente ; genèse = `[0u8; 32]`.
    pub prev_hash: [u8; 32],
    /// `entry_hash` de cette entrée : `SHA-256(header canonique)`.
    pub entry_hash: [u8; 32],
    /// `SHA-256(JSON canonique compact du payload)` — le journal ne stocke jamais le payload
    /// en clair dans la chaîne, seulement son engagement.
    pub payload_hash: [u8; 32],
    /// Horodatage Unix (secondes, UTC) — non décroissant le long de la chaîne.
    pub ts_unix: u64,
    /// Signature Ed25519 du contrôle sur `entry_hash` (64 octets). Vide pour la genèse.
    pub sig: Vec<u8>,
}

impl LedgerEntry {
    /// Header binaire canonique (80 octets) :
    /// `seq.to_le_bytes() ‖ prev_hash ‖ payload_hash ‖ ts_unix.to_le_bytes()`.
    pub fn header_bytes(
        seq: u64,
        prev_hash: &[u8; 32],
        payload_hash: &[u8; 32],
        ts_unix: u64,
    ) -> Vec<u8> {
        let mut header = Vec::with_capacity(8 + 32 + 32 + 8);
        header.extend_from_slice(&seq.to_le_bytes());
        header.extend_from_slice(prev_hash);
        header.extend_from_slice(payload_hash);
        header.extend_from_slice(&ts_unix.to_le_bytes());
        header
    }

    /// `entry_hash = SHA-256(header_bytes(seq, prev_hash, payload_hash, ts_unix))`.
    ///
    /// SHA-256 (et non blake3) : format public du design STACK-5 (§3.2) — tout
    /// vérificateur tiers doit recomposer le même digest depuis ces champs.
    pub fn compute_entry_hash(
        seq: u64,
        prev_hash: &[u8; 32],
        payload_hash: &[u8; 32],
        ts_unix: u64,
    ) -> [u8; 32] {
        let header = Self::header_bytes(seq, prev_hash, payload_hash, ts_unix);
        sha256(&header)
    }

    /// Entrée de genèse : `seq = 0`, `prev_hash = [0u8; 32]`, aucun payload, **aucune signature**
    /// (les signatures commencent à `seq = 1` — voir `API_DESIGN.md` STACK-5).
    ///
    /// La genèse ancre la chaîne : sa présence et son intégrité sont vérifiées par
    /// [`crate::ledger::Ledger::verify_chain`] et par `cloison-verify`.
    pub fn genesis() -> LedgerEntry {
        let entry_hash = Self::compute_entry_hash(0, &GENESIS_PREV_HASH, &GENESIS_PREV_HASH, 0);
        LedgerEntry {
            seq: 0,
            prev_hash: GENESIS_PREV_HASH,
            entry_hash,
            payload_hash: GENESIS_PREV_HASH,
            ts_unix: 0,
            sig: Vec::new(),
        }
    }

    /// Entrée suivante : `seq = self.seq + 1`, `prev_hash = self.entry_hash`, signée par le
    /// contrôle. `ts_unix` doit être ≥ au ts de cette entrée (vérifié par la chaîne).
    pub fn next(
        &self,
        payload_hash: [u8; 32],
        ts_unix: u64,
        control_key: &SigningKey,
    ) -> LedgerEntry {
        let seq = self.seq + 1;
        let entry_hash = Self::compute_entry_hash(seq, &self.entry_hash, &payload_hash, ts_unix);
        let sig = control_key.sign(&entry_hash).to_bytes().to_vec();
        LedgerEntry {
            seq,
            prev_hash: self.entry_hash,
            entry_hash,
            payload_hash,
            ts_unix,
            sig,
        }
    }

    /// Vérifie l'entrée contre le `prev_hash` attendu :
    /// 1. `self.prev_hash == prev_hash` ;
    /// 2. `entry_hash` recomputé == `entry_hash` stocké ;
    /// 3. pour `seq ≥ 1` : signature Ed25519 valide sur `entry_hash` (`verify_strict`,
    ///    rejet de la malléabilité) ; la genèse (`seq == 0`) n'a pas de signature.
    pub fn verify(&self, prev_hash: &[u8; 32], control_key: &VerifyingKey) -> bool {
        if self.prev_hash != *prev_hash {
            return false;
        }
        if Self::compute_entry_hash(self.seq, &self.prev_hash, &self.payload_hash, self.ts_unix)
            != self.entry_hash
        {
            return false;
        }
        if self.seq == 0 {
            // Genèse : pas de signature, mais elle doit bien être la genèse.
            return self.prev_hash == GENESIS_PREV_HASH;
        }
        let Ok(sig_bytes) = <[u8; SIGNATURE_LEN]>::try_from(self.sig.as_slice()) else {
            return false;
        };
        control_key
            .verify_strict(&self.entry_hash, &Signature::from_bytes(&sig_bytes))
            .is_ok()
    }

    /// Affiche l'entrée sous forme hexadécimale compacte (logs, tests) — aucun texte client.
    pub fn summary(&self) -> String {
        format!(
            "seq={} prev={} entry={} payload={} ts={}",
            self.seq,
            hexutil::encode(&self.prev_hash),
            hexutil::encode(&self.entry_hash),
            hexutil::encode(&self.payload_hash),
            self.ts_unix
        )
    }
}

/// Payload du journal — **uniquement des compteurs k-anonymisés** (STACK-4, `redacted`) et
/// des engagements par hash. Jamais de texte, de span ou de valeur brute.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LedgerPayload {
    /// Version du schéma de payload.
    pub schema_version: u8,
    /// Type de période, ex. `"conformance-period"`.
    pub kind: String,
    /// Identifiant opérateur non sensible du locataire (jamais un texte utilisateur).
    pub tenant_id: String,
    pub period_start: u64,
    pub period_end: u64,
    /// Total des requêtes agrégées sur la période (compteur, pas de contenu).
    pub total_requests: u64,
    /// Cellules déjà redactées par le k-anonymat de cloison-audit — clés triées par
    /// `BTreeMap` pour un JSON canonique.
    pub counters: BTreeMap<String, u64>,
    /// Engagement sur les reçus STACK-4 : hash de chaque reçu (SHA-256 du JSON canonique
    /// du reçu). Les reçus eux-mêmes restent **hors** journal.
    pub receipt_hashes: Vec<[u8; 32]>,
}

impl LedgerPayload {
    /// Payload vide de démonstration (utile aux tests de chaîne).
    pub fn empty(tenant_id: impl Into<String>) -> LedgerPayload {
        LedgerPayload {
            schema_version: 1,
            kind: "conformance-period".to_string(),
            tenant_id: tenant_id.into(),
            period_start: 0,
            period_end: 0,
            total_requests: 0,
            counters: BTreeMap::new(),
            receipt_hashes: Vec::new(),
        }
    }
}

/// `payload_hash = SHA-256(JSON canonique compact du payload)`.
///
/// Canonique : sérialisation compacte de `serde_json` sur une structure à champs fixes
/// et `BTreeMap` (clés triées) — déterministe à travers les versions.
pub fn payload_hash(payload: &LedgerPayload) -> [u8; 32] {
    // La sérialisation d'un LedgerPayload bien formé ne peut pas échouer.
    let canonical = serde_json::to_vec(payload).expect("canonical LedgerPayload serialization");
    sha256(&canonical)
}

/// Hash du JSON canonique servi par un miroir (vérification hors-ligne d'un payload).
///
/// Le payload **servi** doit être exactement le JSON canonique produit par `payload_hash`
/// (compact, clés triées) ; un JSON ré-ordonné ne correspond pas et l'inclusion échoue —
/// c'est le comportement voulu.
pub fn payload_hash_from_json(payload_json: &str) -> [u8; 32] {
    sha256(payload_json.as_bytes())
}

/// `SHA-256(bytes)` — helper interne, partagé par `entry_hash` et `payload_hash`.
pub fn sha256(bytes: &[u8]) -> [u8; 32] {
    let mut hasher = Sha256::new();
    hasher.update(bytes);
    let out = hasher.finalize();
    let mut digest = [0u8; 32];
    digest.copy_from_slice(&out);
    digest
}
