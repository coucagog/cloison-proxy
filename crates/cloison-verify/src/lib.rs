//! # cloison-verify — vérificateur public d'attestation
//!
//! Stateless et pur (aucune I/O) : il recompose la chaîne du journal depuis la genèse,
//! valide chaque signature Ed25519 du contrôle (`verify_strict`) et prouve l'inclusion
//! d'un payload par hash. Le cœur est **indépendant de wasm-bindgen** : testable nativement
//! et buildable WASM (`feature wasm`, cible `wasm32-unknown-unknown`).
//!
//! Le vérificateur n'a besoin d'aucun secret : uniquement les entrées publiques et la clé
//! publique du contrôle. Zéro PII : on ne manipule que des hash, des signatures et des compteurs.
//!
//! # Anti-troncature (checkpoints signés)
//!
//! Une chaîne **tronquée** (le miroir retire les entrées les plus récentes) reste
//! auto-cohérente : `verify_chain` ne peut pas la détecter seule. Avec un
//! [`Checkpoint`](cloison_ledger::Checkpoint) signé par le contrôle,
//! [`verify_chain_with_checkpoint`] refuse :
//! - toute chaîne dont la dernière entrée a `seq < checkpoint.seq` (troncature) ;
//! - toute chaîne dont l'entrée `checkpoint.seq` ne porte pas `checkpoint.entry_hash`
//!   (divergence) ;
//! - tout checkpoint invalide (signature ou chaîne de checkpoints cassée).

pub use cloison_ledger::{Checkpoint, LedgerEntry, LedgerPayload, GENESIS_PREV_HASH};

use ed25519_dalek::{Signature, VerifyingKey};
use serde::{Deserialize, Serialize};

/// Signature Ed25519 du contrôle : longueur fixe.
pub const SIGNATURE_LEN: usize = 64;

/// Raison de l'échec de vérification — statique, sans aucun contenu utilisateur.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum VerifyError {
    /// Aucune entrée fournie.
    #[error("empty chain")]
    EmptyChain,
    /// La première entrée n'est pas la genèse attendue (`seq == 0`, `prev_hash` nul).
    #[error("genesis mismatch at seq {seq}")]
    GenesisMismatch { seq: u64 },
    /// `seq` non consécutif (entrée supprimée ou doublée).
    #[error("seq gap: expected {expected}, got {got}")]
    SeqGap { expected: u64, got: u64 },
    /// `prev_hash` de l'entrée ≠ `entry_hash` de la précédente.
    #[error("prev hash mismatch at seq {seq}")]
    PrevHashMismatch { seq: u64 },
    /// `entry_hash` stocké ≠ `entry_hash` recomputé depuis les champs.
    #[error("entry hash mismatch at seq {seq}")]
    EntryHashMismatch { seq: u64 },
    /// Signature Ed25519 absente, malformée ou invalide.
    #[error("bad signature at seq {seq}")]
    BadSignature { seq: u64 },
    /// `ts_unix` non décroissant le long de la chaîne.
    #[error("timestamp regressed at seq {seq}")]
    TimestampRegressed { seq: u64 },
    /// La chaîne servie est **tronquée** : un checkpoint signé ancre `checkpoint_seq`,
    /// mais la dernière entrée fournie n'a que `head_seq`.
    #[error("truncated chain: checkpoint anchors seq {checkpoint_seq}, chain head is {head_seq}")]
    TruncatedChain { checkpoint_seq: u64, head_seq: u64 },
    /// Le checkpoint lui-même est invalide (signature ou lien `prev_cp_hash`).
    #[error("invalid checkpoint at seq {seq}")]
    CheckpointInvalid { seq: u64 },
    /// L'entrée ancrée par le checkpoint ne porte pas `checkpoint.entry_hash`
    /// (réécriture / divergence).
    #[error("checkpoint entry hash mismatch at seq {seq}")]
    CheckpointMismatch { seq: u64 },
}

/// Verdict structuré d'une vérification de chaîne (retourné par [`verify_chain_v`]).
/// Non sérialisable par serde : `VerifyError` enveloppe `SignatureError`, qui n'est pas
/// `Serialize`. Les exports WASM construisent leur JSON à la main (voir `wasm.rs`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ChainVerdict {
    pub ok: bool,
    /// Nombre d'entrées validées avant l'échec éventuel (0 = genèse non vérifiée).
    pub entries_checked: u64,
    pub head_seq: u64,
    pub head_entry_hash: Option<[u8; 32]>,
    pub failure: Option<VerifyError>,
}

/// Recompose la chaîne depuis la genèse et la valide intégralement.
///
/// - `entries[0]` doit être la genèse (`seq == 0`, `prev_hash == [0u8; 32]`) ;
/// - pour chaque entrée suivante : `seq` consécutif, `prev_hash == entry_hash` précédent,
///   `entry_hash` recomputé == stocké, `ts_unix` non décroissant ;
/// - chaque signature (entrées réelles, `seq ≥ 1`) est validée en `verify_strict`
///   contre `control_key` (la signature couvre `entry_hash`).
pub fn verify_chain(
    entries: &[LedgerEntry],
    control_key: &VerifyingKey,
) -> Result<(), VerifyError> {
    let Some(first) = entries.first() else {
        return Err(VerifyError::EmptyChain);
    };
    if first.seq != 0 {
        return Err(VerifyError::GenesisMismatch { seq: first.seq });
    }
    if first.prev_hash != GENESIS_PREV_HASH {
        return Err(VerifyError::GenesisMismatch { seq: 0 });
    }
    if LedgerEntry::compute_entry_hash(first.seq, &first.prev_hash, &first.payload_hash, first.ts_unix)
        != first.entry_hash
    {
        return Err(VerifyError::EntryHashMismatch { seq: 0 });
    }
    // La genèse n'a pas de signature ; la chaîne commence à la première entrée réelle.
    let mut prev_hash = first.entry_hash;
    let mut prev_ts = first.ts_unix;
    for (expected_seq, entry) in (1u64..).zip(entries.iter().skip(1)) {
        let seq = entry.seq;
        if seq != expected_seq {
            return Err(VerifyError::SeqGap {
                expected: expected_seq,
                got: seq,
            });
        }
        if entry.prev_hash != prev_hash {
            return Err(VerifyError::PrevHashMismatch { seq });
        }
        if LedgerEntry::compute_entry_hash(seq, &entry.prev_hash, &entry.payload_hash, entry.ts_unix)
            != entry.entry_hash
        {
            return Err(VerifyError::EntryHashMismatch { seq });
        }
        if entry.ts_unix < prev_ts {
            return Err(VerifyError::TimestampRegressed { seq });
        }
        verify_entry_signature(entry, control_key)?;
        prev_hash = entry.entry_hash;
        prev_ts = entry.ts_unix;
    }
    Ok(())
}

/// Variante retournant un [`ChainVerdict`] structuré (API publique du design).
pub fn verify_chain_v(
    entries: &[LedgerEntry],
    control_key: &VerifyingKey,
) -> ChainVerdict {
    let head = entries.last();
    match verify_chain(entries, control_key) {
        Ok(()) => ChainVerdict {
            ok: true,
            entries_checked: entries.len() as u64,
            head_seq: head.map(|e| e.seq).unwrap_or(0),
            head_entry_hash: head.map(|e| e.entry_hash),
            failure: None,
        },
        Err(failure) => {
            let failing_seq = failure_seq(&failure);
            let entries_checked = entries.iter().take_while(|e| e.seq < failing_seq).count() as u64;
            ChainVerdict {
                ok: false,
                entries_checked,
                head_seq: head.map(|e| e.seq).unwrap_or(0),
                head_entry_hash: None,
                failure: Some(failure),
            }
        }
    }
}

/// Vérifie une entrée isolée contre le `prev_hash` attendu (vérification incrémentale).
pub fn verify_entry(
    entry: &LedgerEntry,
    prev_hash: &[u8; 32],
    control_key: &VerifyingKey,
) -> Result<(), VerifyError> {
    if entry.prev_hash != *prev_hash {
        return Err(VerifyError::PrevHashMismatch { seq: entry.seq });
    }
    if LedgerEntry::compute_entry_hash(entry.seq, &entry.prev_hash, &entry.payload_hash, entry.ts_unix)
        != entry.entry_hash
    {
        return Err(VerifyError::EntryHashMismatch { seq: entry.seq });
    }
    if entry.seq > 0 {
        verify_entry_signature(entry, control_key)?;
    }
    Ok(())
}

/// Preuve d'inclusion d'un payload par hash : vrai si une entrée réelle (`seq ≥ 1`)
/// porte ce `payload_hash`. La preuve n'a de sens que sur une chaîne déjà validée par
/// [`verify_chain`].
pub fn prove_inclusion(entries: &[LedgerEntry], payload_hash: &[u8; 32]) -> bool {
    entries.iter().any(|e| e.seq > 0 && &e.payload_hash == payload_hash)
}

/// Preuve d'inclusion structurée : localise le `target_seq` et les `entry_hash` du
/// préfixe `1..=target_seq`. La **vérification** de la preuve se fait en re-validant les
/// entrées elles-mêmes avec [`verify_chain`] (les `entry_hash` seuls ne transportent pas
/// les signatures).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct InclusionProof {
    pub target_seq: u64,
    pub target_payload_hash: [u8; 32],
    /// `entry_hash` de chaque entrée de 1..=target_seq (préfixe de la chaîne).
    pub prefix_hashes: Vec<[u8; 32]>,
    pub head_seq: u64,
    pub head_entry_hash: [u8; 32],
}

/// Cherche le premier payload du hash demandé et construit la preuve d'inclusion
/// (préfixe de la chaîne jusqu'au target). Retourne `None` si absent.
pub fn find_inclusion(entries: &[LedgerEntry], payload_hash: &[u8; 32]) -> Option<InclusionProof> {
    let target = entries.iter().find(|e| e.seq > 0 && &e.payload_hash == payload_hash)?;
    let prefix_hashes = entries
        .iter()
        .filter(|e| e.seq >= 1 && e.seq <= target.seq)
        .map(|e| e.entry_hash)
        .collect::<Vec<_>>();
    let head = entries.last()?;
    Some(InclusionProof {
        target_seq: target.seq,
        target_payload_hash: *payload_hash,
        prefix_hashes,
        head_seq: head.seq,
        head_entry_hash: head.entry_hash,
    })
}

/// Vérifie la chaîne **puis** l'ancrage anti-troncature d'un checkpoint signé
/// (P1-2 — la troncature seule n'est pas détectable par [`verify_chain`]) :
///
/// 1. la chaîne elle-même est valide (`control_key`) ;
/// 2. le checkpoint est valide — signature + lien `prev_cp_hash` (contre la genèse des
///    checkpoints) avec `checkpoint_key` ;
/// 3. `checkpoint.seq ≤` dernière entrée servie, sinon [`VerifyError::TruncatedChain`] ;
/// 4. l'entrée `checkpoint.seq` porte `checkpoint.entry_hash`, sinon
///    [`VerifyError::CheckpointMismatch`].
pub fn verify_chain_with_checkpoint(
    entries: &[LedgerEntry],
    checkpoint: &Checkpoint,
    control_key: &VerifyingKey,
    checkpoint_key: &VerifyingKey,
) -> Result<(), VerifyError> {
    verify_chain(entries, control_key)?;
    if !checkpoint.verify(&Checkpoint::genesis(), checkpoint_key) {
        return Err(VerifyError::CheckpointInvalid { seq: checkpoint.seq });
    }
    let head_seq = entries.last().map(|e| e.seq).unwrap_or(0);
    if checkpoint.seq > head_seq {
        return Err(VerifyError::TruncatedChain {
            checkpoint_seq: checkpoint.seq,
            head_seq,
        });
    }
    let anchored = entries
        .iter()
        .find(|e| e.seq == checkpoint.seq)
        .ok_or(VerifyError::CheckpointMismatch { seq: checkpoint.seq })?;
    if anchored.entry_hash != checkpoint.entry_hash {
        return Err(VerifyError::CheckpointMismatch { seq: checkpoint.seq });
    }
    Ok(())
}

/// Vérifie la signature Ed25519 d'une entrée réelle (`verify_strict`).
fn verify_entry_signature(entry: &LedgerEntry, control_key: &VerifyingKey) -> Result<(), VerifyError> {
    let sig_bytes: [u8; SIGNATURE_LEN] = entry
        .sig
        .as_slice()
        .try_into()
        .map_err(|_| VerifyError::BadSignature { seq: entry.seq })?;
    let signature = Signature::from_bytes(&sig_bytes);
    control_key
        .verify_strict(&entry.entry_hash, &signature)
        .map_err(|_| VerifyError::BadSignature { seq: entry.seq })
}

/// Séquence de la première entrée concernée par une erreur (pour `entries_checked`).
fn failure_seq(failure: &VerifyError) -> u64 {
    match failure {
        VerifyError::EmptyChain | VerifyError::GenesisMismatch { .. } => 0,
        VerifyError::SeqGap { expected, .. }
        | VerifyError::PrevHashMismatch { seq: expected }
        | VerifyError::EntryHashMismatch { seq: expected }
        | VerifyError::BadSignature { seq: expected }
        | VerifyError::TimestampRegressed { seq: expected } => *expected,
        VerifyError::TruncatedChain { checkpoint_seq, .. }
        | VerifyError::CheckpointInvalid { seq: checkpoint_seq }
        | VerifyError::CheckpointMismatch { seq: checkpoint_seq } => *checkpoint_seq,
    }
}

#[cfg(all(feature = "wasm", target_arch = "wasm32"))]
pub mod wasm;
