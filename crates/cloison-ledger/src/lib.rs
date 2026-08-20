//! # cloison-ledger — journal de transparence append-only vérifiable
//!
//! Noyau de STACK-5 : une chaîne de hachage d'entrées **signées par le contrôle** où
//! chaque entrée lie la précédente. Le journal ne contient **jamais de texte** :
//! les payloads sont des compteurs déjà k-anonymisés (STACK-4) et des engagements par hash.
//!
//! - [`entry::LedgerEntry`] : `entry_hash = SHA-256(seq ‖ prev_hash ‖ payload_hash ‖ ts)`
//!   (format public du design, §3.2), signature Ed25519 du contrôle sur `entry_hash`
//!   (64 octets). Genèse `seq = 0` **non signée** (les signatures commencent à `seq = 1`).
//! - [`entry::LedgerPayload`] : compteurs `BTreeMap<String, u64>` (clés triées → JSON
//!   canonique) + `receipt_hashes` ; `payload_hash = SHA-256(JSON canonique compact)`.
//! - [`ledger::Ledger`] : append terminal (refuse `seq ≠ len`, `prev_hash ≠ head`,
//!   hash non conforme, signature invalide), `verify_chain`, `verify_inclusion`,
//!   `root_hash`, genèse `seq = 0`. Peut enrober un store durable.
//! - [`store::LedgerStore`] : persistance — [`store::MemLedger`] (tests) et
//!   [`store::AppendOnlyFileLedger`] (JSONL append-only, mode 0600, rechargé au boot).
//! - [`checkpoint::Checkpoint`] : ancrage signé de la tête — [`ledger::Ledger::checkpoint`]
//!   le produit, [`ledger::Ledger::verify_chain_with_checkpoint`] détecte la **troncature**.
//!
//! Zéro dépendance externe lourde : pur Rust, sans tokio, buildable WASM.

pub mod checkpoint;
pub mod entry;
pub mod error;
pub mod hexutil;
pub mod ledger;
pub mod store;

pub use checkpoint::Checkpoint;
pub use entry::{
    payload_hash, payload_hash_from_json, sha256, LedgerEntry, LedgerPayload, GENESIS_PREV_HASH,
};
pub use error::{LedgerError, LedgerResult};
pub use ledger::Ledger;
pub use store::{AppendOnlyFileLedger, LedgerStore, MemLedger};
