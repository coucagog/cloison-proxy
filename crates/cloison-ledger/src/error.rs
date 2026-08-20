//! Erreurs du journal de transparence.

use thiserror::Error;

/// Erreurs de la chaîne de hachage / de l'append / de la persistance.
///
/// Toutes les erreurs sont statiques (aucun contenu utilisateur) : le journal
/// ne transporte jamais de texte, y compris dans ses messages d'erreur.
///
/// Pas de `PartialEq`/`Eq` : les variantes `Io`/`Json` enveloppent des erreurs
/// externes non comparables — les tests comparent avec `matches!`.
#[derive(Debug, Error)]
pub enum LedgerError {
    /// Le `seq` proposé ne correspond pas à la position terminale attendue
    /// (append non séquentiel, ou ré-append d'un `seq` déjà présent).
    #[error("append refused: seq {got} != expected {expected}")]
    SeqMismatch { expected: u64, got: u64 },

    /// Ré-append : `seq ≤ head` (append-only violé — tentative de réécriture).
    #[error("append refused: seq {0} <= head {1}")]
    SeqAlreadyAppended(u64, u64),

    /// `prev_hash` de l'entrée ≠ `entry_hash` de la tête : chaîne cassée.
    #[error("broken chain: prev_hash mismatch at seq {0}")]
    BrokenChain(u64),

    /// `entry_hash` stocké ≠ `entry_hash` recomputé depuis les champs.
    #[error("entry hash mismatch at seq {0}")]
    EntryHashMismatch(u64),

    /// Signature Ed25519 absente/invalide (vérifiée en `verify_strict`).
    #[error("bad signature at seq {0}")]
    BadSignature(u64),

    /// Ligne JSONL illisible (fichier de journal corrompu) à la relecture.
    #[error("corrupt ledger file: line at index {0} is not a valid entry")]
    CorruptEntry(u64),

    /// Le journal est vide (ne peut arriver qu'en cas d'initialisation manuelle).
    #[error("ledger is empty")]
    EmptyLedger,

    /// Échec d'entrée/sortie (fichier JSONL, fsync…).
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),

    /// Échec de sérialisation/désérialisation JSON.
    #[error("json error: {0}")]
    Json(#[from] serde_json::Error),
}

/// Alias de résultat du journal.
pub type LedgerResult<T> = Result<T, LedgerError>;
