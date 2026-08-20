//! Exports WASM (feature `wasm`, cible `wasm32-unknown-unknown` uniquement).
//!
//! Entrées/sorties en JSON string (pas de glue JS complexe) :
//! - `verify_chain_bytes(entries_json, control_key_hex)` → JSON `{"ok":bool,"error":?}` ;
//! - `prove_inclusion_bytes(entries_json, payload_hash_hex)` → `bool`.
//!
//! Le cœur (`verify_chain`, `prove_inclusion`) est indépendant de wasm-bindgen et reste
//! testable nativement : ce module n'est compilé que pour wasm32 avec la feature `wasm`.
//!
//! Build : `cargo build -p cloison-verify --target wasm32-unknown-unknown --features wasm`

use wasm_bindgen::prelude::*;

use cloison_ledger::hexutil;
use ed25519_dalek::VerifyingKey;

/// `entries_json` = tableau JSON de `LedgerEntry` ; `control_key_hex` = 32 octets hexadécimaux.
/// Retourne le JSON d'un verdict : `{"ok":true}` ou `{"ok":false,"error":"..."}`.
#[wasm_bindgen]
pub fn verify_chain_bytes(entries_json: &str, control_key_hex: &str) -> String {
    let entries: Vec<crate::LedgerEntry> = match serde_json::from_str(entries_json) {
        Ok(entries) => entries,
        Err(_) => return invalid_input(),
    };
    let key_bytes: [u8; 32] = match hexutil::decode_array(control_key_hex) {
        Ok(bytes) => bytes,
        Err(_) => return invalid_input(),
    };
    let control_key: VerifyingKey = match VerifyingKey::from_bytes(&key_bytes) {
        Ok(key) => key,
        Err(_) => return invalid_input(),
    };
    match crate::verify_chain(&entries, &control_key) {
        Ok(()) => r#"{"ok":true}"#.to_string(),
        Err(err) => format!(r#"{{"ok":false,"error":"{}"}}"#, err),
    }
}

/// `entries_json` = tableau JSON de `LedgerEntry` ; `payload_hash_hex` = 32 octets hexadécimaux.
/// Retourne `true` si un payload du hash demandé est inclus dans le journal.
#[wasm_bindgen]
pub fn prove_inclusion_bytes(entries_json: &str, payload_hash_hex: &str) -> bool {
    let entries: Vec<crate::LedgerEntry> = match serde_json::from_str(entries_json) {
        Ok(entries) => entries,
        Err(_) => return false,
    };
    let payload_hash: [u8; 32] = match hexutil::decode_array(payload_hash_hex) {
        Ok(bytes) => bytes,
        Err(_) => return false,
    };
    crate::prove_inclusion(&entries, &payload_hash)
}

fn invalid_input() -> String {
    r#"{"ok":false,"error":"invalid input"}"#.to_string()
}
