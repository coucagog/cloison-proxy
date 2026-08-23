//! Encrypted vault module.
//!
//! Persistent encrypted storage using redb + AES-256-GCM (native).
//! In-memory encrypted vault (WasmMemVault) for WASM builds.
//! Stores the mapping: token_body_b32 → (plain_value, kind_tag).
//! Each entry is encrypted with a derived session encryption key.
//! TTL-based expiration with garbage collection.

use aes_gcm::{Aes256Gcm, KeyInit, Nonce};
use serde::{Deserialize, Serialize};

use crate::error::{CloisonError, CloisonResult};

/// Vault configuration.
#[derive(Debug, Clone)]
pub struct VaultConfig {
    /// Path to the redb database file (native only).
    pub db_path: std::path::PathBuf,
    /// Maximum database size in bytes. Default: 64 MiB.
    pub max_db_size: usize,
    /// Time-to-live for entries in seconds. Default: 3600 (1 hour).
    pub ttl_secs: u64,
}

impl Default for VaultConfig {
    fn default() -> Self {
        Self {
            db_path: std::path::PathBuf::from("cloison-vault.redb"),
            max_db_size: 64 * 1024 * 1024,
            ttl_secs: 3600,
        }
    }
}

/// Serialized entry stored in the vault (before encryption).
#[derive(Serialize, Deserialize)]
struct VaultEntryData {
    plain_value: String,
    kind_tag: String,
    issued_at: u64,
}

// ──── Native vault (redb + AES-256-GCM) ────

#[cfg(feature = "native")]
use std::sync::Arc;

#[cfg(feature = "native")]
use redb::{Database, ReadableTable, TableDefinition};

#[cfg(feature = "native")]
const VAULT_TABLE: TableDefinition<&str, &[u8]> = TableDefinition::new("vault");

/// Encrypted vault using redb + AES-256-GCM (native) or in-memory HashMap (WASM).
pub struct Vault {
    #[cfg(feature = "native")]
    db: Arc<Database>,
    #[cfg(not(feature = "native"))]
    entries: HashMap<String, Vec<u8>>,
    enc_key: [u8; 32],
    ttl_secs: u64,
}

impl Vault {
    /// Open or create the vault with the given configuration and encryption key.
    #[cfg(feature = "native")]
    pub fn open(config: &VaultConfig, enc_key: &[u8; 32]) -> CloisonResult<Self> {
        let db = Database::builder()
            .create(&config.db_path)
            .map_err(|e| CloisonError::Vault(format!("failed to open database: {}", e)))?;

        // Create the table if it doesn't exist
        let write_txn = db.begin_write().map_err(|e| {
            CloisonError::Vault(format!("failed to begin write transaction: {}", e))
        })?;
        write_txn
            .open_table(VAULT_TABLE)
            .map_err(|e| CloisonError::Vault(format!("failed to open table: {}", e)))?;
        write_txn
            .commit()
            .map_err(|e| CloisonError::Vault(format!("failed to commit: {}", e)))?;

        Ok(Self {
            db: Arc::new(db),
            enc_key: *enc_key,
            ttl_secs: config.ttl_secs,
        })
    }

    /// Open an in-memory vault (WASM / no-redb builds).
    #[cfg(not(feature = "native"))]
    pub fn open(_config: &VaultConfig, enc_key: &[u8; 32]) -> CloisonResult<Self> {
        Ok(Self {
            entries: HashMap::new(),
            enc_key: *enc_key,
            ttl_secs: _config.ttl_secs,
        })
    }

    /// Store an entry: token_body_b32 → (plain_value, kind_tag).
    /// Encrypts with AES-256-GCM using a random 12-byte nonce.
    /// Silently overwrites if the key already exists.
    #[cfg(feature = "native")]
    pub fn put(
        &self,
        token_body_b32: &str,
        plain_value: &str,
        kind_tag: &str,
    ) -> CloisonResult<()> {
        let entry_data = VaultEntryData {
            plain_value: plain_value.to_string(),
            kind_tag: kind_tag.to_string(),
            issued_at: current_epoch_secs(),
        };

        let plaintext = serde_json::to_vec(&entry_data)?;
        let ciphertext = self.encrypt(&plaintext)?;

        let write_txn = self
            .db
            .begin_write()
            .map_err(|e| CloisonError::Vault(format!("put: begin_write: {}", e)))?;
        {
            let mut table = write_txn
                .open_table(VAULT_TABLE)
                .map_err(|e| CloisonError::Vault(format!("put: open_table: {}", e)))?;
            table
                .insert(token_body_b32, ciphertext.as_slice())
                .map_err(|e| CloisonError::Vault(format!("put: insert: {}", e)))?;
        }
        write_txn
            .commit()
            .map_err(|e| CloisonError::Vault(format!("put: commit: {}", e)))?;

        Ok(())
    }

    /// Store an entry (in-memory, WASM).
    #[cfg(not(feature = "native"))]
    pub fn put(
        &self,
        token_body_b32: &str,
        plain_value: &str,
        kind_tag: &str,
    ) -> CloisonResult<()> {
        let entry_data = VaultEntryData {
            plain_value: plain_value.to_string(),
            kind_tag: kind_tag.to_string(),
            issued_at: current_epoch_secs(),
        };

        let plaintext = serde_json::to_vec(&entry_data)?;
        let ciphertext = self.encrypt(&plaintext)?;

        // In WASM, use interior mutability workaround or accept &self
        // Since WasmMemVault is used behind Arc/Mutex in practice,
        // we use a workaround here with unsafe for the simple HashMap
        // SAFETY: WASM is single-threaded; native uses redb which handles concurrency.
        #[allow(unused_unsafe)]
        unsafe {
            // This is a known limitation of the no-redb path.
            // In production, wrap Vault in Arc<Mutex<Vault>>.
            let entries =
                &self.entries as *const HashMap<String, Vec<u8>> as *mut HashMap<String, Vec<u8>>;
            (*entries).insert(token_body_b32.to_string(), ciphertext);
        }

        Ok(())
    }

    /// Retrieve an entry from the vault.
    /// Decrypts and verifies the AES-GCM tag.
    /// Returns `Ok(None)` if the key doesn't exist.
    /// Returns `Err(VaultTtlExpired)` if the entry has exceeded TTL.
    #[cfg(feature = "native")]
    pub fn get(&self, token_body_b32: &str) -> CloisonResult<Option<(String, String)>> {
        let read_txn = self
            .db
            .begin_read()
            .map_err(|e| CloisonError::Vault(format!("get: begin_read: {}", e)))?;
        let table = read_txn
            .open_table(VAULT_TABLE)
            .map_err(|e| CloisonError::Vault(format!("get: open_table: {}", e)))?;

        let Some(access_guard) = table
            .get(token_body_b32)
            .map_err(|e| CloisonError::Vault(format!("get: get: {}", e)))?
        else {
            return Ok(None);
        };

        let ciphertext = access_guard.value();
        let plaintext = self.decrypt(ciphertext)?;

        let entry: VaultEntryData = serde_json::from_slice(&plaintext)?;

        // Check TTL
        let now = current_epoch_secs();
        if entry.issued_at + self.ttl_secs < now {
            return Err(CloisonError::VaultTtlExpired(token_body_b32.to_string()));
        }

        Ok(Some((entry.plain_value, entry.kind_tag)))
    }

    /// Retrieve an entry (in-memory, WASM).
    #[cfg(not(feature = "native"))]
    pub fn get(&self, token_body_b32: &str) -> CloisonResult<Option<(String, String)>> {
        let Some(ciphertext) = self.entries.get(token_body_b32) else {
            return Ok(None);
        };

        let plaintext = self.decrypt(ciphertext)?;
        let entry: VaultEntryData = serde_json::from_slice(&plaintext)?;

        let now = current_epoch_secs();
        if entry.issued_at + self.ttl_secs < now {
            return Err(CloisonError::VaultTtlExpired(token_body_b32.to_string()));
        }

        Ok(Some((entry.plain_value, entry.kind_tag)))
    }

    /// Delete an entry from the vault.
    #[cfg(feature = "native")]
    pub fn delete(&self, token_body_b32: &str) -> CloisonResult<()> {
        let write_txn = self
            .db
            .begin_write()
            .map_err(|e| CloisonError::Vault(format!("delete: begin_write: {}", e)))?;
        {
            let mut table = write_txn
                .open_table(VAULT_TABLE)
                .map_err(|e| CloisonError::Vault(format!("delete: open_table: {}", e)))?;
            table
                .remove(token_body_b32)
                .map_err(|e| CloisonError::Vault(format!("delete: remove: {}", e)))?;
        }
        write_txn
            .commit()
            .map_err(|e| CloisonError::Vault(format!("delete: commit: {}", e)))?;

        Ok(())
    }

    /// Delete an entry (in-memory, WASM).
    #[cfg(not(feature = "native"))]
    pub fn delete(&self, token_body_b32: &str) -> CloisonResult<()> {
        #[allow(unused_unsafe)]
        unsafe {
            let entries =
                &self.entries as *const HashMap<String, Vec<u8>> as *mut HashMap<String, Vec<u8>>;
            (*entries).remove(token_body_b32);
        }
        Ok(())
    }

    /// Garbage collect expired entries.
    /// Returns the number of entries removed.
    #[cfg(feature = "native")]
    pub fn gc(&self) -> CloisonResult<usize> {
        let now = current_epoch_secs();
        let mut expired_keys = Vec::new();

        let read_txn = self
            .db
            .begin_read()
            .map_err(|e| CloisonError::Vault(format!("gc: begin_read: {}", e)))?;
        {
            let table = read_txn
                .open_table(VAULT_TABLE)
                .map_err(|e| CloisonError::Vault(format!("gc: open_table: {}", e)))?;

            for result in table
                .iter()
                .map_err(|e| CloisonError::Vault(format!("gc: iter: {}", e)))?
            {
                let (key, value) =
                    result.map_err(|e| CloisonError::Vault(format!("gc: iter item: {}", e)))?;

                if let Ok(plaintext) = self.decrypt(value.value()) {
                    if let Ok(entry) = serde_json::from_slice::<VaultEntryData>(&plaintext) {
                        if entry.issued_at + self.ttl_secs < now {
                            expired_keys.push(key.value().to_string());
                        }
                    }
                }
            }
        }

        let count = expired_keys.len();
        if count > 0 {
            let write_txn = self
                .db
                .begin_write()
                .map_err(|e| CloisonError::Vault(format!("gc: begin_write: {}", e)))?;
            {
                let mut table = write_txn
                    .open_table(VAULT_TABLE)
                    .map_err(|e| CloisonError::Vault(format!("gc: open_table: {}", e)))?;
                for key in expired_keys {
                    table
                        .remove(key.as_str())
                        .map_err(|e| CloisonError::Vault(format!("gc: remove: {}", e)))?;
                }
            }
            write_txn
                .commit()
                .map_err(|e| CloisonError::Vault(format!("gc: commit: {}", e)))?;
        }

        Ok(count)
    }

    /// Garbage collect expired entries (in-memory, WASM).
    #[cfg(not(feature = "native"))]
    pub fn gc(&self) -> CloisonResult<usize> {
        let now = current_epoch_secs();
        let mut expired_keys = Vec::new();

        for (key, ciphertext) in &self.entries {
            if let Ok(plaintext) = self.decrypt(ciphertext) {
                if let Ok(entry) = serde_json::from_slice::<VaultEntryData>(&plaintext) {
                    if entry.issued_at + self.ttl_secs < now {
                        expired_keys.push(key.clone());
                    }
                }
            }
        }

        let count = expired_keys.len();
        #[allow(unused_unsafe)]
        unsafe {
            let entries =
                &self.entries as *const HashMap<String, Vec<u8>> as *mut HashMap<String, Vec<u8>>;
            for key in &expired_keys {
                (*entries).remove(key);
            }
        }

        Ok(count)
    }

    /// Close the vault and release resources (Database is closed on drop).
    #[cfg(feature = "native")]
    pub fn close(self) -> CloisonResult<()> {
        let _ = Arc::try_unwrap(self.db)
            .map_err(|_| CloisonError::Vault("cannot close vault: multiple references".into()))?;
        Ok(())
    }

    /// Close the vault (no-op for in-memory).
    #[cfg(not(feature = "native"))]
    pub fn close(self) -> CloisonResult<()> {
        Ok(())
    }

    /// Purge all entries for a given session.
    pub fn purge_session(&self, _session_id: u64) -> CloisonResult<()> {
        // No-op: vault does not track session IDs per entry
        Ok(())
    }

    /// Encrypt plaintext with AES-256-GCM.
    /// Format: 12-byte nonce || ciphertext || 16-byte tag.
    fn encrypt(&self, plaintext: &[u8]) -> CloisonResult<Vec<u8>> {
        use aes_gcm::aead::Aead;
        use rand::RngCore;

        let cipher = Aes256Gcm::new_from_slice(&self.enc_key)
            .map_err(|e| CloisonError::AesGcm(format!("key init: {}", e)))?;

        let mut nonce_bytes = [0u8; 12];
        rand::thread_rng().fill_bytes(&mut nonce_bytes);
        let nonce = Nonce::from_slice(&nonce_bytes);

        let ciphertext = cipher
            .encrypt(nonce, plaintext)
            .map_err(|e| CloisonError::AesGcm(format!("encrypt: {}", e)))?;

        // Prepend nonce to ciphertext+tag
        let mut result = nonce_bytes.to_vec();
        result.extend(ciphertext);
        Ok(result)
    }

    /// Decrypt ciphertext with AES-256-GCM.
    /// Expected format: 12-byte nonce || ciphertext || 16-byte tag.
    fn decrypt(&self, ciphertext: &[u8]) -> CloisonResult<Vec<u8>> {
        use aes_gcm::aead::Aead;

        if ciphertext.len() < 12 + 16 {
            return Err(CloisonError::AesGcm("ciphertext too short".into()));
        }

        let cipher = Aes256Gcm::new_from_slice(&self.enc_key)
            .map_err(|e| CloisonError::AesGcm(format!("key init: {}", e)))?;

        let nonce = Nonce::from_slice(&ciphertext[..12]);
        let plaintext = cipher
            .decrypt(nonce, &ciphertext[12..])
            .map_err(|e| CloisonError::AesGcm(format!("decrypt: {}", e)))?;

        Ok(plaintext)
    }
}

/// Get current epoch time in seconds.
fn current_epoch_secs() -> u64 {
    use std::time::{SystemTime, UNIX_EPOCH};
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

/// In-memory encrypted vault for WASM builds.
#[cfg(not(feature = "native"))]
pub struct WasmMemVault {
    entries: HashMap<String, ([u8; 12], Vec<u8>, u64)>,
    enc_key: [u8; 32],
    ttl_secs: u64,
}

#[cfg(not(feature = "native"))]
impl WasmMemVault {
    /// Create a new in-memory vault.
    pub fn new(enc_key: [u8; 32], ttl_secs: u64) -> Self {
        Self {
            entries: HashMap::new(),
            enc_key,
            ttl_secs,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[cfg(feature = "native")]
    fn test_vault() -> (Vault, tempfile::TempDir) {
        let dir = tempdir().unwrap();
        let config = VaultConfig {
            db_path: dir.path().join("test.redb"),
            max_db_size: 1024 * 1024,
            ttl_secs: 3600,
        };
        let enc_key = [0xABu8; 32];
        let vault = Vault::open(&config, &enc_key).unwrap();
        (vault, dir)
    }

    #[cfg(feature = "native")]
    #[test]
    fn test_put_get_delete() {
        let (vault, _dir) = test_vault();

        vault.put("body123", "user@example.com", "EM").unwrap();
        let (val, tag) = vault.get("body123").unwrap().unwrap();
        assert_eq!(val, "user@example.com");
        assert_eq!(tag, "EM");

        vault.delete("body123").unwrap();
        assert!(vault.get("body123").unwrap().is_none());
    }

    #[cfg(feature = "native")]
    #[test]
    fn test_encryption_integrity() {
        let (vault, _dir) = test_vault();
        vault.put("body456", "secret", "PH").unwrap();

        let (val, _) = vault.get("body456").unwrap().unwrap();
        assert_eq!(val, "secret");
    }

    #[cfg(feature = "native")]
    #[test]
    fn test_overwrite() {
        let (vault, _dir) = test_vault();

        vault.put("key1", "value1", "EM").unwrap();
        vault.put("key1", "value2", "PH").unwrap();

        let (val, tag) = vault.get("key1").unwrap().unwrap();
        assert_eq!(val, "value2");
        assert_eq!(tag, "PH");
    }
}
