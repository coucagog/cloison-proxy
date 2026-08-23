//! WASM bindings module.
//!
//! Exports CLOISON functionality to WebAssembly for browser/embedded use.
//! Uses wasm-bindgen for JavaScript interoperability.
//! Vault is replaced by in-memory storage (WasmMemVault).

#![cfg(target_arch = "wasm32")]

use wasm_bindgen::prelude::*;

use std::collections::HashMap;

use crate::engine::Engine;
use crate::policy::Policy;
use crate::token::SessionKeys;
use base64::Engine as _; // trait pour encode/decode

/// Global session store (simplified: single-process WASM).
static mut SESSIONS: Option<HashMap<u32, WasmSession>> = None;
static mut NEXT_SESSION_ID: u32 = 1;

/// WASM session state.
struct WasmSession {
    engine: Engine,
}

/// Initialize a new tokenization session.
/// Returns a session ID for use in subsequent calls.
#[wasm_bindgen(js_name = "cloisonInitSession")]
#[allow(unused_unsafe)]
pub fn init_session(tenant_key_b64: &str) -> Result<u32, JsValue> {
    let tenant_key = base64::engine::general_purpose::STANDARD
        .decode(tenant_key_b64)
        .map_err(|e| JsValue::from_str(&format!("base64 decode error: {}", e)))?;
    if tenant_key.len() != 32 {
        return Err(JsValue::from_str("tenant_key must be 32 bytes"));
    }
    let mut key_arr = [0u8; 32];
    key_arr.copy_from_slice(&tenant_key);

    // Generate random session salt using getrandom (js feature provides web crypto)
    let mut session_salt = [0u8; 16];
    getrandom::getrandom(&mut session_salt)
        .map_err(|e| JsValue::from_str(&format!("random error: {}", e)))?;

    let keys = SessionKeys::derive(key_arr, session_salt)
        .map_err(|e| JsValue::from_str(&format!("key derivation failed: {}", e)))?;

    let engine = Engine::new(keys).map_err(|e| JsValue::from_str(&e.to_string()))?;

    // SAFETY: WASM is single-threaded; mutable access to global statics is safe.
    #[allow(unused_unsafe)]
    unsafe {
        if SESSIONS.is_none() {
            SESSIONS = Some(HashMap::new());
        }
        let session_id = NEXT_SESSION_ID;
        NEXT_SESSION_ID += 1;
        SESSIONS
            .as_mut()
            .unwrap()
            .insert(session_id, WasmSession { engine });
        Ok(session_id)
    }
}

/// Tokenize text with the default policy.
/// Returns JSON: { text, tokens: [{body_b32, kind_tag}] }
#[wasm_bindgen(js_name = "cloisonTokenize")]
pub fn tokenize(session_id: u32, text: &str) -> Result<String, JsValue> {
    let policy = Policy::default();
    tokenize_with_policy(session_id, text, &serde_json::to_string(&policy).unwrap())
}

/// Tokenize text with a custom policy (JSON).
#[wasm_bindgen(js_name = "cloisonTokenizeWithPolicy")]
pub fn tokenize_with_policy(
    session_id: u32,
    text: &str,
    policy_json: &str,
) -> Result<String, JsValue> {
    let policy: Policy = serde_json::from_str(policy_json)
        .map_err(|e| JsValue::from_str(&format!("invalid policy JSON: {}", e)))?;

    // SAFETY: WASM is single-threaded.
    #[allow(unused_unsafe)]
    unsafe {
        let sessions = SESSIONS
            .as_mut()
            .ok_or_else(|| JsValue::from_str("no sessions"))?;
        let session = sessions
            .get_mut(&session_id)
            .ok_or_else(|| JsValue::from_str(&format!("session {} not found", session_id)))?;

        let result = session
            .engine
            .tokenize(text, &policy, "wasm-req")
            .map_err(|e| JsValue::from_str(&e.to_string()))?;

        let wasm_result = WasmTokenizeResult {
            text: result.text_out,
            tokens: result
                .emitted
                .into_iter()
                .map(|t| WasmToken {
                    body_b32: t.body_b32,
                    kind_tag: t.kind_tag,
                })
                .collect(),
        };

        serde_json::to_string(&wasm_result).map_err(|e| JsValue::from_str(&e.to_string()))
    }
}

/// Restore a tokenized text to clear form.
/// Returns the clear text if all tokens are valid.
#[wasm_bindgen(js_name = "cloisonRestore")]
pub fn restore(session_id: u32, text: &str) -> Result<String, JsValue> {
    // SAFETY: WASM is single-threaded.
    #[allow(unused_unsafe)]
    unsafe {
        let sessions = SESSIONS
            .as_mut()
            .ok_or_else(|| JsValue::from_str("no sessions"))?;
        let session = sessions
            .get(&session_id)
            .ok_or_else(|| JsValue::from_str(&format!("session {} not found", session_id)))?;

        let result = session
            .engine
            .restore(text, "wasm-req")
            .map_err(|e| JsValue::from_str(&e.to_string()))?;

        Ok(result.text_out)
    }
}

/// Destroy a session (clears registry, zeroizes keys).
#[wasm_bindgen(js_name = "cloisonDestroySession")]
pub fn destroy_session(session_id: u32) -> Result<(), JsValue> {
    // SAFETY: WASM is single-threaded.
    #[allow(unused_unsafe)]
    unsafe {
        let sessions = SESSIONS
            .as_mut()
            .ok_or_else(|| JsValue::from_str("no sessions"))?;
        sessions.remove(&session_id);
        Ok(())
    }
}

/// Detect PII without tokenizing.
/// Returns JSON array of detected spans.
#[wasm_bindgen(js_name = "cloisonDetect")]
pub fn detect(text: &str) -> Result<String, JsValue> {
    let detector =
        crate::detection::Detector::new().map_err(|e| JsValue::from_str(&e.to_string()))?;
    let policy = Policy::default();
    let spans = detector.detect_with_policy(text, &policy.detection);
    serde_json::to_string(&spans).map_err(|e| JsValue::from_str(&e.to_string()))
}

/// Detect PII with a custom policy (JSON).
#[wasm_bindgen(js_name = "cloisonDetectWithPolicy")]
pub fn detect_with_policy(text: &str, policy_json: &str) -> Result<String, JsValue> {
    let detector =
        crate::detection::Detector::new().map_err(|e| JsValue::from_str(&e.to_string()))?;
    let policy: Policy = serde_json::from_str(policy_json)
        .map_err(|e| JsValue::from_str(&format!("invalid policy JSON: {}", e)))?;
    let spans = detector.detect_with_policy(text, &policy.detection);
    serde_json::to_string(&spans).map_err(|e| JsValue::from_str(&e.to_string()))
}

/// Validate a sentinel (format + registry).
#[wasm_bindgen(js_name = "cloisonValidateSentinel")]
pub fn validate_sentinel(session_id: u32, sentinel_str: &str) -> Result<bool, JsValue> {
    // SAFETY: WASM is single-threaded.
    #[allow(unused_unsafe)]
    unsafe {
        let sessions = SESSIONS
            .as_mut()
            .ok_or_else(|| JsValue::from_str("no sessions"))?;
        let session = sessions
            .get(&session_id)
            .ok_or_else(|| JsValue::from_str(&format!("session {} not found", session_id)))?;

        let parsed = match crate::token::Sentinel::parse(sentinel_str) {
            Some(s) => s,
            None => return Ok(false),
        };

        let body = match crate::token::TokenBody::from_base32(&parsed.token_body_b32) {
            Ok(b) => b,
            Err(_) => return Ok(false),
        };

        Ok(session.engine.registry().contains(&body))
    }
}

/// Derive session keys from tenant key and salt (for testing).
/// Returns JSON: { mac_key_b64, enc_key_b64 }
#[wasm_bindgen(js_name = "cloisonDeriveKeys")]
pub fn derive_keys(tenant_key_b64: &str, session_salt_b64: &str) -> Result<String, JsValue> {
    let tenant_key = base64::engine::general_purpose::STANDARD
        .decode(tenant_key_b64)
        .map_err(|e| JsValue::from_str(&format!("base64 decode error: {}", e)))?;
    if tenant_key.len() != 32 {
        return Err(JsValue::from_str("tenant_key must be 32 bytes"));
    }
    let mut key_arr = [0u8; 32];
    key_arr.copy_from_slice(&tenant_key);

    let session_salt = base64::engine::general_purpose::STANDARD
        .decode(session_salt_b64)
        .map_err(|e| JsValue::from_str(&format!("base64 decode error: {}", e)))?;
    if session_salt.len() != 16 {
        return Err(JsValue::from_str("session_salt must be 16 bytes"));
    }
    let mut salt_arr = [0u8; 16];
    salt_arr.copy_from_slice(&session_salt);

    let keys = SessionKeys::derive(key_arr, salt_arr)
        .map_err(|e| JsValue::from_str(&format!("key derivation failed: {}", e)))?;

    let result = WasmDerivedKeys {
        mac_key_b64: base64::engine::general_purpose::STANDARD.encode(keys.mac_key),
        enc_key_b64: base64::engine::general_purpose::STANDARD.encode(keys.enc_key),
    };

    serde_json::to_string(&result).map_err(|e| JsValue::from_str(&e.to_string()))
}

// ──── Helper types ────

#[derive(serde::Serialize, serde::Deserialize)]
struct WasmTokenizeResult {
    text: String,
    tokens: Vec<WasmToken>,
}

#[derive(serde::Serialize, serde::Deserialize)]
struct WasmToken {
    body_b32: String,
    kind_tag: String,
}

#[derive(serde::Serialize, serde::Deserialize)]
struct WasmDerivedKeys {
    mac_key_b64: String,
    enc_key_b64: String,
}
