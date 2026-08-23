//! Persistance du journal : trait [`LedgerStore`] + [`MemLedger`] + [`AppendOnlyFileLedger`].
//!
//! Le [`crate::ledger::Ledger`] est la couche de **vérification** (chaîne, hashes,
//! signatures) ; un `LedgerStore` est la couche de **durabilité**. Le `Ledger` enrobe un
//! store optionnel : chaque append validé est d'abord persisté (fsync) puis ajouté à la
//! chaîne en mémoire.
//!
//! # AppendOnlyFileLedger
//!
//! Fichier **JSONL append-only** (une entrée JSON par ligne) :
//! - ouvert en append (`OpenOptions::append`), **jamais réécrit ni tronqué** ;
//! - permissions `0600` sur Unix — le journal contient des engagements, pas de PII, mais
//!   reste un artefact de contrôle sensible ;
//! - chaque append : ligne JSON + `flush` + `sync_all` (fsync) avant retour ;
//! - au boot, les lignes existantes sont rechargées dans le cache — la chaîne se
//!   reconstruit à l'identique (append-only ⇒ pas de trou possible sans corruption).
//!
//! Le trait est **synchrone** (le ledger est pur, sans tokio ; le contrôle l'enrobe).

use crate::entry::LedgerEntry;
use crate::error::{LedgerError, LedgerResult};
use std::fs::{File, OpenOptions};
use std::io::{BufRead, BufReader, Write};
use std::path::Path;
use std::sync::{Mutex, RwLock};

/// Contrat de persistance du journal — append terminal vérifié, lecture par seq.
pub trait LedgerStore: Send + Sync {
    /// Append terminal : refuse `seq ≤ head` (ré-append) et `seq > head + 1` (trou),
    /// refuse `prev_hash ≠ head.entry_hash` (chaîne cassée). Persiste avant de valider.
    fn append(&self, entry: &LedgerEntry) -> LedgerResult<()>;

    /// Tête du journal (dernière entrée), ou `None` si le store est vide.
    fn head(&self) -> LedgerResult<Option<LedgerEntry>>;

    /// Entrée `seq` donnée, ou `None`.
    fn get(&self, seq: u64) -> LedgerResult<Option<LedgerEntry>>;

    /// Plage inclusive `[from, to]` d'entrées (ordre croissant).
    fn range(&self, from: u64, to: u64) -> LedgerResult<Vec<LedgerEntry>>;

    /// Nombre d'entrées (défaut : `head.seq + 1`).
    fn len(&self) -> LedgerResult<usize> {
        Ok(self
            .head()?
            .map(|h| h.seq as usize + 1)
            .unwrap_or(0))
    }

    /// Vrai si le store ne contient aucune entrée.
    fn is_empty(&self) -> LedgerResult<bool> {
        Ok(self.len()? == 0)
    }
}

/// Vérifications terminales partagées par les stores (seq et chaîne).
///
/// Store vide : seul l'append de la **genèse** (`seq = 0`, `prev_hash` nul) est accepté —
/// la genèse ancre la chaîne et n'a pas de prédécesseur.
fn check_terminal(entries: &[LedgerEntry], entry: &LedgerEntry) -> LedgerResult<()> {
    if entries.is_empty() {
        if entry.seq == 0 && entry.prev_hash == crate::entry::GENESIS_PREV_HASH {
            return Ok(());
        }
        return Err(LedgerError::SeqMismatch {
            expected: 0,
            got: entry.seq,
        });
    }
    let head_seq = entries.last().map(|e| e.seq).unwrap_or(0);
    if entry.seq <= head_seq {
        return Err(LedgerError::SeqAlreadyAppended(entry.seq, head_seq));
    }
    if entry.seq != head_seq + 1 {
        return Err(LedgerError::SeqMismatch {
            expected: head_seq + 1,
            got: entry.seq,
        });
    }
    if let Some(head) = entries.last() {
        if entry.prev_hash != head.entry_hash {
            return Err(LedgerError::BrokenChain(entry.seq));
        }
    }
    Ok(())
}

/// Journal en mémoire (tests, mode sans fichier) — `RwLock<Vec<LedgerEntry>>`.
#[derive(Debug, Default)]
pub struct MemLedger {
    entries: RwLock<Vec<LedgerEntry>>,
}

impl MemLedger {
    pub fn new() -> MemLedger {
        MemLedger::default()
    }
}

impl LedgerStore for MemLedger {
    fn append(&self, entry: &LedgerEntry) -> LedgerResult<()> {
        let mut entries = self.entries.write().expect("mem ledger lock poisoned");
        check_terminal(&entries, entry)?;
        entries.push(entry.clone());
        Ok(())
    }

    fn head(&self) -> LedgerResult<Option<LedgerEntry>> {
        Ok(self
            .entries
            .read()
            .expect("mem ledger lock poisoned")
            .last()
            .cloned())
    }

    fn get(&self, seq: u64) -> LedgerResult<Option<LedgerEntry>> {
        Ok(self
            .entries
            .read()
            .expect("mem ledger lock poisoned")
            .get(seq as usize)
            .cloned())
    }

    fn range(&self, from: u64, to: u64) -> LedgerResult<Vec<LedgerEntry>> {
        Ok(self
            .entries
            .read()
            .expect("mem ledger lock poisoned")
            .iter()
            .filter(|e| e.seq >= from && e.seq <= to)
            .cloned()
            .collect())
    }
}

/// Journal **fichier JSONL append-only** — voir la doc du module.
pub struct AppendOnlyFileLedger {
    /// Fichier ouvert en append (jamais réécrit). `Mutex` : un seul writer.
    file: Mutex<File>,
    /// Cache en mémoire des entrées rechargées au boot + appends du process courant.
    entries: RwLock<Vec<LedgerEntry>>,
}

impl AppendOnlyFileLedger {
    /// Ouvre (ou crée, mode `0600` sur Unix) le fichier JSONL, recharge les lignes
    /// existantes dans le cache. Le fichier n'est **jamais** tronqué ni réécrit.
    pub fn open(path: impl AsRef<Path>) -> LedgerResult<Self> {
        let path = path.as_ref();

        // 1. Recharge les entrées déjà persistées (append-only ⇒ lecture linéaire).
        let mut entries = Vec::new();
        if path.exists() {
            let file = File::open(path)?;
            for (idx, line) in BufReader::new(file).lines().enumerate() {
                let line = line?;
                if line.trim().is_empty() {
                    continue;
                }
                let entry: LedgerEntry = serde_json::from_str(&line)
                    .map_err(|_| LedgerError::CorruptEntry(idx as u64))?;
                entries.push(entry);
            }
        }

        // 2. Ouvre en append (create si absent) — jamais de write(false) ni de truncate.
        let mut opts = OpenOptions::new();
        opts.create(true).append(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            // 0644 : le ledger de TRANSPARENCE est PUBLIC par design (compteurs
            // k-anonymisés, jamais de texte — invariant I9). Le conteneur
            // journal (nginx uid 101) doit pouvoir le servir en lecture
            // (étape C, journal.wonkom.ai) sans courir en uid du control.
            // NB : le journal d'AUDIT (proxy, JSONL 0600) reste strictement
            // privé — ce changement ne concerne que le ledger du contrôle.
            opts.mode(0o644);
        }
        let file = opts.open(path)?;

        Ok(AppendOnlyFileLedger {
            file: Mutex::new(file),
            entries: RwLock::new(entries),
        })
    }
}

impl LedgerStore for AppendOnlyFileLedger {
    fn append(&self, entry: &LedgerEntry) -> LedgerResult<()> {
        let mut entries = self.entries.write().expect("file ledger cache lock poisoned");
        check_terminal(&entries, entry)?;
        let line = serde_json::to_string(entry)?;
        let mut file = self.file.lock().expect("file ledger lock poisoned");
        writeln!(file, "{line}")?;
        file.flush()?;
        // fsync : l'entrée est durable AVANT d'être considérée acceptée.
        file.sync_all()?;
        entries.push(entry.clone());
        Ok(())
    }

    fn head(&self) -> LedgerResult<Option<LedgerEntry>> {
        Ok(self
            .entries
            .read()
            .expect("file ledger cache lock poisoned")
            .last()
            .cloned())
    }

    fn get(&self, seq: u64) -> LedgerResult<Option<LedgerEntry>> {
        Ok(self
            .entries
            .read()
            .expect("file ledger cache lock poisoned")
            .get(seq as usize)
            .cloned())
    }

    fn range(&self, from: u64, to: u64) -> LedgerResult<Vec<LedgerEntry>> {
        Ok(self
            .entries
            .read()
            .expect("file ledger cache lock poisoned")
            .iter()
            .filter(|e| e.seq >= from && e.seq <= to)
            .cloned()
            .collect())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::entry::{payload_hash, LedgerPayload};
    use ed25519_dalek::SigningKey;

    fn control_key() -> SigningKey {
        SigningKey::from_bytes(&[7u8; 32])
    }

    fn sample_payload(n: u64) -> LedgerPayload {
        let mut payload = LedgerPayload::empty("t");
        payload.total_requests = n;
        payload.counters.insert("requests".to_string(), n);
        payload
    }

    fn signed_chain(count: u64) -> Vec<LedgerEntry> {
        let key = control_key();
        let mut entries = vec![LedgerEntry::genesis()];
        for i in 1..=count {
            let prev = entries.last().unwrap();
            entries.push(prev.next(payload_hash(&sample_payload(i)), 1_700_000_000 + i, &key));
        }
        entries
    }

    #[test]
    fn mem_ledger_append_get_range_head() {
        let store = MemLedger::new();
        assert_eq!(store.head().unwrap(), None);
        let entries = signed_chain(3);
        for e in &entries {
            store.append(e).unwrap();
        }
        assert_eq!(store.head().unwrap(), entries.last().cloned());
        assert_eq!(store.get(2).unwrap(), Some(entries[2].clone()));
        assert_eq!(store.get(99).unwrap(), None);
        assert_eq!(store.range(1, 2).unwrap(), entries[1..=2].to_vec());
        assert_eq!(store.len().unwrap(), 4);
    }

    #[test]
    fn mem_ledger_refuses_rewrite_and_gap() {
        let store = MemLedger::new();
        let entries = signed_chain(2);
        for e in &entries {
            store.append(e).unwrap();
        }
        // Ré-append de la genèse (seq 0 ≤ head 2) → refusé.
        assert!(matches!(
            store.append(&entries[0]),
            Err(LedgerError::SeqAlreadyAppended(0, 2))
        ));
        // Trou : seq 5 alors que la tête est 2 → refusé.
        let key = control_key();
        let bogus = entries[2].next(payload_hash(&sample_payload(9)), 1_700_000_009, &key);
        let bogus = LedgerEntry { seq: 5, ..bogus };
        assert!(matches!(
            store.append(&bogus),
            Err(LedgerError::SeqMismatch { expected: 3, got: 5 })
        ));
    }

    #[test]
    fn append_only_file_roundtrip_and_reopen() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("ledger.jsonl");

        let entries = signed_chain(4);
        {
            let store = AppendOnlyFileLedger::open(&path).unwrap();
            assert_eq!(store.len().unwrap(), 0);
            for e in &entries {
                store.append(e).unwrap();
            }
        }
        // Réouverture : les 5 entrées (genèse + 4) sont rechargées.
        let reopened = AppendOnlyFileLedger::open(&path).unwrap();
        assert_eq!(reopened.len().unwrap(), 5);
        assert_eq!(reopened.head().unwrap().unwrap(), entries[4].clone());
        assert_eq!(reopened.range(0, 4).unwrap(), entries);
    }

    #[test]
    fn append_only_file_refuses_rewrite_after_reopen() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("ledger.jsonl");
        let entries = signed_chain(2);
        {
            let store = AppendOnlyFileLedger::open(&path).unwrap();
            for e in &entries {
                store.append(e).unwrap();
            }
        }
        let reopened = AppendOnlyFileLedger::open(&path).unwrap();
        // Ré-append d'une entrée déjà persistée → refusé (append-only).
        assert!(matches!(
            reopened.append(&entries[1]),
            Err(LedgerError::SeqAlreadyAppended(1, 2))
        ));
        // Le fichier n'a pas été réécrit : toujours 3 lignes.
        let raw = std::fs::read_to_string(&path).unwrap();
        assert_eq!(raw.lines().count(), 3);
    }

    #[test]
    fn append_only_file_mode_is_0600_on_unix() {
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let dir = tempfile::tempdir().unwrap();
            let path = dir.path().join("ledger.jsonl");
            let _store = AppendOnlyFileLedger::open(&path).unwrap();
            let mode = std::fs::metadata(&path).unwrap().permissions().mode() & 0o777;
            assert_eq!(mode, 0o644, "ledger file must be created 0644 (public transparency)");
        }
        #[cfg(not(unix))]
        {
            // Non-Unix : le mode n'est pas garanti — le test n'a pas de sens ici.
        }
    }
}
