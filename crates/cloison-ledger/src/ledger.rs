//! Journal append-only vérifiable.
//!
//! Le [`Ledger`] est la couche de **vérification** : il ne possède **aucune clé**
//! (la clé de signature appartient au contrôle) et refuse toute écriture non
//! terminale. Chaque [`LedgerEntry`](crate::entry::LedgerEntry) est vérifiée
//! avant d'être acceptée :
//!
//! 1. `seq == len` (append strictement terminal, jamais de trou ni de ré-écriture) ;
//! 2. `prev_hash == entry_hash` de la tête (chaîne de hachage ininterrompue) ;
//! 3. `entry_hash` recomputé == `entry_hash` stocké ;
//! 4. signature Ed25519 valide (`verify_strict`), quand une clé publique de vérification
//!    a été fournie.
//!
//! La genèse (`seq = 0`, non signée — les signatures commencent à `seq = 1`) est
//! ensemencée par [`Ledger::new`].
//!
//! # Durabilité
//!
//! Un store [`LedgerStore`](crate::store::LedgerStore) optionnel (ex.
//! [`AppendOnlyFileLedger`](crate::store::AppendOnlyFileLedger)) est enrobé : chaque
//! append validé est d'abord **persisté** (fsync) puis ajouté à la chaîne en mémoire.
//! [`Ledger::open_file`] recharge un fichier JSONL au boot (append-only).
//!
//! # Checkpoints (anti-troncature)
//!
//! [`Ledger::checkpoint`] produit un [`Checkpoint`](crate::checkpoint::Checkpoint) signé
//! qui ancre la tête ; [`Ledger::verify_chain_with_checkpoint`] refuse toute chaîne
//! tronquée (dernière entrée `seq < checkpoint.seq`) ou divergente.

use crate::checkpoint::Checkpoint;
use crate::entry::{LedgerEntry, GENESIS_PREV_HASH};
use crate::error::{LedgerError, LedgerResult};
use crate::store::{AppendOnlyFileLedger, LedgerStore};
use ed25519_dalek::{SigningKey, VerifyingKey};
use std::fmt;
use std::path::Path;

/// Journal de transparence, append-only vérifiable, avec persistance optionnelle.
pub struct Ledger {
    /// Clé publique du contrôle ; `None` = la vérification des signatures est désactivée
    /// (utile aux tests de chaîne purs). Avec clé, tout append non signé est refusé.
    verify_key: Option<VerifyingKey>,
    entries: Vec<LedgerEntry>,
    /// Persistance durable optionnelle (fichier JSONL append-only, etc.).
    store: Option<Box<dyn LedgerStore>>,
}

impl fmt::Debug for Ledger {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Ledger")
            .field("len", &self.entries.len())
            .field("root_hash", &crate::hexutil::encode(&self.root_hash()))
            .field("persisted", &self.store.is_some())
            .finish()
    }
}

impl Ledger {
    /// Nouveau journal avec la genèse déjà ensemencée (`seq = 0`) et **sans** vérification
    /// de signature (chaîne + hashes vérifiés uniquement).
    pub fn new() -> Ledger {
        Ledger {
            verify_key: None,
            entries: vec![LedgerEntry::genesis()],
            store: None,
        }
    }

    /// Nouveau journal avec la genèse ensemencée et la clé publique du contrôle :
    /// tout append est vérifié (chaîne + hash + signature).
    pub fn with_verify_key(control_verify_key: VerifyingKey) -> Ledger {
        Ledger {
            verify_key: Some(control_verify_key),
            entries: vec![LedgerEntry::genesis()],
            store: None,
        }
    }

    /// Journal **persistant** : enrobe un [`AppendOnlyFileLedger`] (JSONL append-only,
    /// mode 0600, rechargé au boot) et la clé publique du contrôle.
    ///
    /// Si le fichier est vide, la genèse y est ensemencée ; sinon les entrées existantes
    /// sont rechargées telles quelles (la validation se fait par `verify_chain`).
    pub fn open_file(path: impl AsRef<Path>, control_verify_key: VerifyingKey) -> LedgerResult<Ledger> {
        let store: Box<dyn LedgerStore> = Box::new(AppendOnlyFileLedger::open(path)?);
        let entries = store.range(0, u64::MAX)?;
        let mut ledger = Ledger {
            verify_key: Some(control_verify_key),
            entries,
            store: Some(store),
        };
        if ledger.entries.is_empty() {
            let genesis = LedgerEntry::genesis();
            ledger
                .store
                .as_ref()
                .expect("store present")
                .append(&genesis)?;
            ledger.entries.push(genesis);
        }
        Ok(ledger)
    }

    /// Reconstruit un journal à partir d'entrées fournies (ex. relecture d'un miroir ou
    /// d'un fichier JSONL). Utilisé par les tests de tampering et par les miroirs publics :
    /// aucune écriture n'est possible sans repasser par [`Ledger::append`].
    pub fn from_entries(entries: Vec<LedgerEntry>, verify_key: Option<VerifyingKey>) -> Ledger {
        Ledger {
            verify_key,
            entries,
            store: None,
        }
    }

    /// Entrée de genèse (`seq = 0`, `prev_hash = [0u8; 32]`, aucune signature).
    pub fn genesis() -> LedgerEntry {
        LedgerEntry::genesis()
    }

    /// Append terminal : vérifie `seq == len`, `prev_hash == head.entry_hash`,
    /// `entry_hash` recomputé, et (si clé fournie) la signature. Quand un store est
    /// configuré, l'entrée est d'abord **persistée** (fsync) puis ajoutée à la chaîne.
    pub fn append(&mut self, entry: LedgerEntry) -> LedgerResult<()> {
        let expected = self.entries.len() as u64;
        if entry.seq != expected {
            return Err(LedgerError::SeqMismatch {
                expected,
                got: entry.seq,
            });
        }
        let head = self
            .entries
            .last()
            .ok_or(LedgerError::EmptyLedger)?;
        if entry.prev_hash != head.entry_hash {
            return Err(LedgerError::BrokenChain(entry.seq));
        }
        if LedgerEntry::compute_entry_hash(
            entry.seq,
            &entry.prev_hash,
            &entry.payload_hash,
            entry.ts_unix,
        ) != entry.entry_hash
        {
            return Err(LedgerError::EntryHashMismatch(entry.seq));
        }
        if let Some(key) = &self.verify_key {
            // `verify` couvre prév_hash, entry_hash recomputé et signature (verify_strict).
            if !entry.verify(&head.entry_hash, key) {
                return Err(LedgerError::BadSignature(entry.seq));
            }
        }
        if let Some(store) = &self.store {
            store.append(&entry)?;
        }
        self.entries.push(entry);
        Ok(())
    }

    /// Recompose la chaîne depuis la genèse et la valide intégralement :
    /// seq consécutifs, `prev_hash` liés, `entry_hash` recomputés, `ts_unix` non
    /// décroissant, signatures valides (si clé fournie).
    pub fn verify_chain(&self) -> bool {
        let mut iter = self.entries.iter();
        let Some(first) = iter.next() else {
            return false;
        };
        // Genèse : seq 0, prev nul, hash recomputé, pas de signature.
        if first.seq != 0
            || first.prev_hash != GENESIS_PREV_HASH
            || LedgerEntry::compute_entry_hash(
                first.seq,
                &first.prev_hash,
                &first.payload_hash,
                first.ts_unix,
            ) != first.entry_hash
        {
            return false;
        }
        let mut prev_hash = first.entry_hash;
        let mut prev_ts = first.ts_unix;
        for (seq, entry) in (1u64..).zip(iter) {
            if entry.seq != seq || entry.prev_hash != prev_hash {
                return false;
            }
            if LedgerEntry::compute_entry_hash(
                entry.seq,
                &entry.prev_hash,
                &entry.payload_hash,
                entry.ts_unix,
            ) != entry.entry_hash
            {
                return false;
            }
            if entry.ts_unix < prev_ts {
                return false;
            }
            if let Some(key) = &self.verify_key {
                if !entry.verify(&prev_hash, key) {
                    return false;
                }
            }
            prev_hash = entry.entry_hash;
            prev_ts = entry.ts_unix;
        }
        true
    }

    /// Vérifie la chaîne **puis** l'ancrage anti-troncature d'un checkpoint signé :
    ///
    /// 1. la chaîne elle-même est valide ([`Ledger::verify_chain`]) ;
    /// 2. le checkpoint est valide (signature + lien `prev_cp_hash`) ;
    /// 3. la chaîne contient l'entrée `checkpoint.seq` et son `entry_hash` correspond.
    ///
    /// Si le checkpoint ancre un `seq` **au-delà** de la dernière entrée servie, la
    /// chaîne est **tronquée** → `false` (c'est le cas qu'une réécriture seule ne détecte pas).
    pub fn verify_chain_with_checkpoint(
        &self,
        checkpoint: &Checkpoint,
        checkpoint_key: &VerifyingKey,
    ) -> bool {
        if !self.verify_chain() {
            return false;
        }
        if !checkpoint.verify(&Checkpoint::genesis(), checkpoint_key) {
            return false;
        }
        match self.entries.get(checkpoint.seq as usize) {
            Some(entry) => entry.entry_hash == checkpoint.entry_hash,
            // checkpoint.seq > dernière entrée → troncature.
            None => false,
        }
    }

    /// Produit un [`Checkpoint`] signé ancré sur la tête actuelle (`seq`, `entry_hash`).
    /// La clé signe le header canonique du checkpoint (Ed25519, `verify_strict`).
    pub fn checkpoint(&self, ts_unix: u64, key: &SigningKey) -> Checkpoint {
        let head = self
            .entries
            .last()
            .expect("genesis is always present");
        Checkpoint::sign(head.seq, head.entry_hash, &Checkpoint::genesis(), ts_unix, key)
    }

    /// Preuve d'inclusion d'un payload par hash : vrai si une entrée réelle
    /// (`seq ≥ 1`) porte ce `payload_hash`.
    pub fn verify_inclusion(&self, payload_hash: &[u8; 32]) -> bool {
        self.entries
            .iter()
            .any(|e| e.seq > 0 && &e.payload_hash == payload_hash)
    }

    /// Hash de tête du journal : `entry_hash` de la dernière entrée.
    pub fn root_hash(&self) -> [u8; 32] {
        self.entries
            .last()
            .map(|e| e.entry_hash)
            .unwrap_or(GENESIS_PREV_HASH)
    }

    /// Dernière entrée (tête du journal).
    pub fn head(&self) -> Option<&LedgerEntry> {
        self.entries.last()
    }

    /// Vue en lecture seule des entrées (genèse comprise).
    pub fn entries(&self) -> &[LedgerEntry] {
        &self.entries
    }

    /// Nombre d'entrées (genèse comprise).
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// Le journal ne peut pas être vide (la genèse est toujours présente) mais le trait
    /// est fourni par symétrie.
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }
}

impl Default for Ledger {
    fn default() -> Self {
        Ledger::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::entry::{payload_hash, LedgerPayload};
    use ed25519_dalek::{Signer, SigningKey};

    fn control_key() -> SigningKey {
        SigningKey::from_bytes(&[7u8; 32])
    }

    fn sample_payload(tenant: &str, n: u64) -> LedgerPayload {
        let mut payload = LedgerPayload::empty(tenant);
        payload.total_requests = n;
        payload.counters.insert("requests".to_string(), n);
        payload
    }

    #[test]
    fn genesis_is_valid() {
        let ledger = Ledger::new();
        assert!(ledger.verify_chain());
        let g = Ledger::genesis();
        assert_eq!(g.seq, 0);
        assert_eq!(g.prev_hash, GENESIS_PREV_HASH);
        assert!(g.sig.is_empty());
        assert_eq!(
            ledger.root_hash(),
            LedgerEntry::compute_entry_hash(0, &GENESIS_PREV_HASH, &GENESIS_PREV_HASH, 0)
        );
    }

    #[test]
    fn append_accepts_sequential_signed_entries() {
        let key = control_key();
        let mut ledger = Ledger::with_verify_key(key.verifying_key());
        let mut head = ledger.head().cloned().unwrap();
        for i in 1..=5u64 {
            let entry = head.next(payload_hash(&sample_payload("t", i)), 1_700_000_000 + i, &key);
            ledger.append(entry.clone()).unwrap();
            head = entry;
        }
        assert_eq!(ledger.len(), 6); // genèse + 5
        assert!(ledger.verify_chain());
        assert_eq!(ledger.root_hash(), head.entry_hash);
    }

    #[test]
    fn append_rejects_wrong_seq() {
        let key = control_key();
        let mut ledger = Ledger::with_verify_key(key.verifying_key());
        let head = ledger.head().cloned().unwrap();
        let entry = head.next(payload_hash(&sample_payload("t", 1)), 1_700_000_001, &key);
        // On force seq = 3 alors que la position terminale attendue est 1.
        let bogus = LedgerEntry {
            seq: 3,
            ..entry
        };
        match ledger.append(bogus) {
            Err(LedgerError::SeqMismatch { expected, got }) => {
                assert_eq!(expected, 1);
                assert_eq!(got, 3);
            }
            other => panic!("unexpected: {other:?}"),
        }
    }

    #[test]
    fn append_rejects_broken_chain() {
        let key = control_key();
        let mut ledger = Ledger::with_verify_key(key.verifying_key());
        let head = ledger.head().cloned().unwrap();
        let mut entry = head.next(payload_hash(&sample_payload("t", 1)), 1_700_000_001, &key);
        entry.prev_hash = [0xAA; 32];
        assert!(matches!(ledger.append(entry), Err(LedgerError::BrokenChain(1))));
    }

    #[test]
    fn append_rejects_bad_signature() {
        let control = control_key();
        let attacker = SigningKey::from_bytes(&[9u8; 32]);
        let mut ledger = Ledger::with_verify_key(control.verifying_key());
        let head = ledger.head().cloned().unwrap();
        // Signé par une clé différente de celle du ledger.
        let entry = head.next(payload_hash(&sample_payload("t", 1)), 1_700_000_001, &attacker);
        assert!(matches!(ledger.append(entry), Err(LedgerError::BadSignature(1))));
    }

    #[test]
    fn tampering_breaks_chain() {
        let key = control_key();
        let mut ledger = Ledger::with_verify_key(key.verifying_key());
        let mut head = ledger.head().cloned().unwrap();
        for i in 1..=4u64 {
            let entry = head.next(payload_hash(&sample_payload("t", i)), 1_700_000_000 + i, &key);
            ledger.append(entry.clone()).unwrap();
            head = entry;
        }
        assert!(ledger.verify_chain());

        // Tampering : on retourne un octet du payload_hash de l'entrée 2.
        let mut entries = ledger.entries().to_vec();
        entries[2].payload_hash[0] ^= 0x01;
        let tampered = Ledger::from_entries(entries, Some(key.verifying_key()));
        assert!(!tampered.verify_chain());

        // Tampering : on retourne un octet de l'entry_hash de l'entrée 3 → la suivante casse aussi.
        let mut entries = ledger.entries().to_vec();
        entries[3].entry_hash[7] ^= 0x01;
        let tampered = Ledger::from_entries(entries, Some(key.verifying_key()));
        assert!(!tampered.verify_chain());
    }

    #[test]
    fn inclusion_is_detected_by_hash() {
        let key = control_key();
        let mut ledger = Ledger::with_verify_key(key.verifying_key());
        let mut head = ledger.head().cloned().unwrap();
        let p1 = payload_hash(&sample_payload("t", 1));
        let p2 = payload_hash(&sample_payload("t", 2));
        for (i, ph) in [p1, p2].iter().enumerate() {
            let entry = head.next(*ph, 1_700_000_000 + i as u64 + 1, &key);
            ledger.append(entry.clone()).unwrap();
            head = entry;
        }
        assert!(ledger.verify_inclusion(&p1));
        assert!(ledger.verify_inclusion(&p2));
        assert!(!ledger.verify_inclusion(&[0x42; 32]));
        // La genèse (payload nul) n'est pas une inclusion.
        assert!(!ledger.verify_inclusion(&GENESIS_PREV_HASH));
    }

    #[test]
    fn timestamp_must_not_regress() {
        let key = control_key();
        let mut ledger = Ledger::with_verify_key(key.verifying_key());
        let head = ledger.head().cloned().unwrap();
        let entry = head.next(payload_hash(&sample_payload("t", 1)), 1_700_000_000, &key);
        ledger.append(entry.clone()).unwrap();
        // ts antérieur au précédent → chaîne invalide.
        let next = entry.next(payload_hash(&sample_payload("t", 2)), 1_699_999_999, &key);
        ledger.append(next).unwrap(); // l'append accepte (vérifié à la relecture)
        assert!(!ledger.verify_chain());
    }

    // ---------------------------------------------------------------------------
    // Checkpoint — anti-troncature
    // ---------------------------------------------------------------------------

    fn build_chain(ledger: &mut Ledger, count: u64, key: &SigningKey) -> LedgerEntry {
        let mut head = ledger.head().cloned().expect("genesis seeded");
        for i in 1..=count {
            let entry = head.next(
                payload_hash(&sample_payload("t", i)),
                1_700_000_000 + i,
                key,
            );
            ledger.append(entry.clone()).unwrap();
            head = entry;
        }
        head
    }

    #[test]
    fn checkpoint_anchors_head_and_verifies() {
        let key = control_key();
        let mut ledger = Ledger::with_verify_key(key.verifying_key());
        let head = build_chain(&mut ledger, 5, &key);

        let cp = ledger.checkpoint(1_700_000_010, &key);
        assert_eq!(cp.seq, 5);
        assert_eq!(cp.entry_hash, head.entry_hash);
        assert!(ledger.verify_chain_with_checkpoint(&cp, &key.verifying_key()));
    }

    #[test]
    fn checkpoint_detects_truncation() {
        let key = control_key();
        let mut ledger = Ledger::with_verify_key(key.verifying_key());
        build_chain(&mut ledger, 5, &key);
        let cp = ledger.checkpoint(1_700_000_010, &key);

        // Un miroir tronque les 2 dernières entrées : la chaîne reste auto-cohérente
        // (verify_chain passe) mais le checkpoint révèle la troncature.
        let truncated = Ledger::from_entries(ledger.entries()[..4].to_vec(), Some(key.verifying_key()));
        assert!(truncated.verify_chain(), "la chaîne tronquée reste cohérente");
        assert!(
            !truncated.verify_chain_with_checkpoint(&cp, &key.verifying_key()),
            "checkpoint seq 5 > head 2 → troncature détectée"
        );
    }

    #[test]
    fn checkpoint_detects_divergence() {
        let key = control_key();
        let mut ledger = Ledger::with_verify_key(key.verifying_key());
        build_chain(&mut ledger, 5, &key);
        let cp = ledger.checkpoint(1_700_000_010, &key);

        // L'entrée ancrée a été réécrite (même seq, contenu différent) mais re-signée
        // avec la même clé : verify_chain passe, le checkpoint échoue.
        let mut entries = ledger.entries().to_vec();
        let mut new_entry = entries[5].clone();
        new_entry.payload_hash = [0xEE; 32];
        new_entry.entry_hash = LedgerEntry::compute_entry_hash(
            new_entry.seq,
            &new_entry.prev_hash,
            &new_entry.payload_hash,
            new_entry.ts_unix,
        );
        new_entry.sig = key.sign(&new_entry.entry_hash).to_bytes().to_vec();
        entries[5] = new_entry;
        let divergent = Ledger::from_entries(entries, Some(key.verifying_key()));
        assert!(divergent.verify_chain());
        assert!(!divergent.verify_chain_with_checkpoint(&cp, &key.verifying_key()));
    }

    #[test]
    fn checkpoint_bad_signature_is_rejected() {
        let key = control_key();
        let mut ledger = Ledger::with_verify_key(key.verifying_key());
        build_chain(&mut ledger, 3, &key);
        let mut cp = ledger.checkpoint(1_700_000_010, &key);
        cp.sig[0] ^= 0x01;
        assert!(!ledger.verify_chain_with_checkpoint(&cp, &key.verifying_key()));
    }

    // ---------------------------------------------------------------------------
    // Persistance fichier via Ledger::open_file
    // ---------------------------------------------------------------------------

    #[test]
    fn open_file_persists_and_reloads() {
        let key = control_key();
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("ledger.jsonl");

        {
            let mut ledger = Ledger::open_file(&path, key.verifying_key()).unwrap();
            build_chain(&mut ledger, 3, &key);
            assert!(ledger.verify_chain());
        }
        // Recharge au boot : genèse + 3 entrées, chaîne intacte.
        let reloaded = Ledger::open_file(&path, key.verifying_key()).unwrap();
        assert_eq!(reloaded.len(), 4);
        assert!(reloaded.verify_chain());
        assert!(reloaded.verify_inclusion(&payload_hash(&sample_payload("t", 2))));
    }

    #[test]
    fn open_file_append_is_terminal() {
        let key = control_key();
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("ledger.jsonl");
        let mut ledger = Ledger::open_file(&path, key.verifying_key()).unwrap();
        let head = ledger.head().cloned().unwrap();
        let entry = head.next(payload_hash(&sample_payload("t", 1)), 1_700_000_001, &key);
        ledger.append(entry).unwrap();
        // Ré-append de la même entrée → refusé.
        let head = ledger.head().cloned().unwrap();
        let dup = head.next(payload_hash(&sample_payload("t", 99)), 1_700_000_099, &key);
        let dup = LedgerEntry { seq: 1, ..dup };
        assert!(matches!(ledger.append(dup), Err(LedgerError::SeqMismatch { .. })));
    }
}
