//! Tests d'intégration du journal : chaîne, tampering, inclusion, genèse, append-only,
//! checkpoints anti-troncature, persistance fichier JSONL.

use cloison_ledger::{
    payload_hash, AppendOnlyFileLedger, Checkpoint, Ledger, LedgerEntry, LedgerError,
    LedgerPayload, LedgerStore, MemLedger, GENESIS_PREV_HASH,
};
use ed25519_dalek::SigningKey;

/// Clé de contrôle déterministe (comme STACK-4) — Ed25519 est déterministe, aucun aléa.
fn control_key() -> SigningKey {
    SigningKey::from_bytes(&[7u8; 32])
}

fn sample_payload(tenant: &str, n: u64) -> LedgerPayload {
    let mut payload = LedgerPayload::empty(tenant);
    payload.total_requests = n;
    payload.counters.insert("requests".to_string(), n);
    payload
}

/// Construit un journal de `count` entrées réelles signées, et retourne (ledger, tête).
fn build_chain(count: u64) -> (Ledger, LedgerEntry) {
    let key = control_key();
    let mut ledger = Ledger::with_verify_key(key.verifying_key());
    let mut head = ledger.head().cloned().expect("genesis seeded");
    for i in 1..=count {
        let entry = head.next(
            payload_hash(&sample_payload("tenant-x", i)),
            1_700_000_000 + i,
            &key,
        );
        ledger.append(entry.clone()).expect("append ok");
        head = entry;
    }
    (ledger, head)
}

#[test]
fn genesis_is_well_formed() {
    let g = Ledger::genesis();
    assert_eq!(g.seq, 0);
    assert_eq!(g.prev_hash, GENESIS_PREV_HASH);
    assert_eq!(g.payload_hash, GENESIS_PREV_HASH);
    assert_eq!(g.ts_unix, 0);
    assert!(
        g.sig.is_empty(),
        "genèse non signée (signatures à partir de seq=1)"
    );
    assert_eq!(
        g.entry_hash,
        LedgerEntry::compute_entry_hash(0, &GENESIS_PREV_HASH, &GENESIS_PREV_HASH, 0)
    );
}

#[test]
fn entry_hash_is_sha256_of_canonical_header() {
    // Verrouille le format public : entry_hash = SHA-256(seq ‖ prev_hash ‖ payload_hash ‖ ts).
    // Vecteur de référence calculé indépendamment (sha256 du header binaire canonique).
    let seq = 1u64;
    let prev_hash = [0xAB; 32];
    let payload_hash_bytes = [0xCD; 32];
    let ts = 1_700_000_001u64;
    let digest = LedgerEntry::compute_entry_hash(seq, &prev_hash, &payload_hash_bytes, ts);
    assert_eq!(
        hex(&digest),
        "6f6d1ca79db36e4f81aa4259f4eb8068ef2ca90dd310550f7ca49eb4da3b67aa"
    );
    // Genèse verrouillée : seq=0, prev nul, payload nul, ts=0 → digest de référence.
    let g = LedgerEntry::genesis();
    assert_eq!(
        hex(&g.entry_hash),
        "5b6fb58e61fa475939767d68a446f97f1bff02c0e5935a3ea8bb51e6515783d8"
    );
}

/// hex minuscule (test local, indépendant du crate).
fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

#[test]
fn chain_of_ten_entries_verifies() {
    let (ledger, head) = build_chain(10);
    assert_eq!(ledger.len(), 11); // genèse + 10
    assert!(ledger.verify_chain());
    assert_eq!(ledger.root_hash(), head.entry_hash);
    // Chaque entrée réelle lie la précédente.
    for pair in ledger.entries().windows(2) {
        assert_eq!(pair[1].prev_hash, pair[0].entry_hash);
        assert_eq!(pair[1].seq, pair[0].seq + 1);
    }
}

#[test]
fn entry_signature_round_trip() {
    let key = control_key();
    let other = SigningKey::from_bytes(&[8u8; 32]);
    let g = Ledger::genesis();
    let e = g.next(payload_hash(&sample_payload("t", 1)), 1_700_000_001, &key);

    // Bonne clé + bon prev_hash → OK.
    assert!(e.verify(&g.entry_hash, &key.verifying_key()));
    // Mauvaise clé → refus.
    assert!(!e.verify(&g.entry_hash, &other.verifying_key()));
    // Mauvais prev_hash → refus.
    assert!(!e.verify(&[0x42; 32], &key.verifying_key()));
    // Signature tronquée → refus.
    let mut truncated = e.clone();
    truncated.sig = vec![0u8; 16];
    assert!(!truncated.verify(&g.entry_hash, &key.verifying_key()));
}

#[test]
fn append_verifies_chain_sequentially() {
    let key = control_key();
    let mut ledger = Ledger::with_verify_key(key.verifying_key());
    let mut head = ledger.head().cloned().unwrap();
    for i in 1..=3u64 {
        let entry = head.next(
            payload_hash(&sample_payload("t", i)),
            1_700_000_000 + i,
            &key,
        );
        ledger.append(entry.clone()).unwrap();
        head = entry;
    }
    assert!(ledger.verify_chain());

    // Ré-append de la même entrée → SeqMismatch (le seq a déjà été consommé).
    let dup = head.next(payload_hash(&sample_payload("t", 99)), 1_700_000_099, &key);
    let dup2 = LedgerEntry { seq: 2, ..dup };
    assert!(matches!(
        ledger.append(dup2),
        Err(LedgerError::SeqMismatch { .. })
    ));
}

#[test]
fn tampering_with_payload_hash_is_detected() {
    let (ledger, _) = build_chain(5);
    assert!(ledger.verify_chain());

    // On retourne UN octet du payload_hash de l'entrée 3 (index 3, genèse en 0).
    let mut entries = ledger.entries().to_vec();
    entries[3].payload_hash[0] ^= 0x01;
    let tampered = Ledger::from_entries(entries, Some(control_key().verifying_key()));
    assert!(!tampered.verify_chain(), "tampering must be detected");

    // L'entrée 3 est refusée à la relecture : recompute ≠ stocké.
    let head = &tampered.entries()[2];
    assert!(!tampered.entries()[3].verify(&head.entry_hash, &control_key().verifying_key()));
}

#[test]
fn tampering_with_entry_hash_breaks_next_prev() {
    let (ledger, _) = build_chain(5);
    let mut entries = ledger.entries().to_vec();
    entries[2].entry_hash[0] ^= 0x01;
    let tampered = Ledger::from_entries(entries, None);
    assert!(!tampered.verify_chain());
    // La chaîne casse dès l'entrée 2 (hash recomputé ≠ stocké) et l'entrée 3 a un
    // prev_hash qui ne correspond plus à l'entry_hash modifié.
}

#[test]
fn dropped_entry_breaks_chain() {
    let (ledger, _) = build_chain(5);
    let mut entries = ledger.entries().to_vec();
    entries.remove(3); // suppression d'une entrée réelle
    let ledger = Ledger::from_entries(entries, Some(control_key().verifying_key()));
    assert!(!ledger.verify_chain());
    // seq 3 attendu mais l'entrée suivante porte seq 4 → trou.
}

#[test]
fn swapped_entries_break_chain() {
    let (ledger, _) = build_chain(5);
    let mut entries = ledger.entries().to_vec();
    entries.swap(3, 4);
    let ledger = Ledger::from_entries(entries, Some(control_key().verifying_key()));
    assert!(!ledger.verify_chain());
}

#[test]
fn inclusion_proof_by_payload_hash() {
    let (ledger, _) = build_chain(4);
    let p2 = payload_hash(&sample_payload("tenant-x", 2));
    let p9 = payload_hash(&sample_payload("tenant-x", 9));
    assert!(ledger.verify_inclusion(&p2));
    assert!(!ledger.verify_inclusion(&p9));
}

#[test]
fn root_hash_is_head_entry_hash() {
    let (ledger, head) = build_chain(7);
    assert_eq!(ledger.root_hash(), head.entry_hash);
}

#[test]
fn payload_hash_is_canonical_json() {
    let mut a = LedgerPayload::empty("t1");
    a.counters.insert("b".to_string(), 2);
    a.counters.insert("a".to_string(), 1);
    let mut b = LedgerPayload::empty("t1");
    b.counters.insert("a".to_string(), 1);
    b.counters.insert("b".to_string(), 2);
    // Mêmes compteurs, ordre d'insertion différent → même hash canonique.
    assert_eq!(payload_hash(&a), payload_hash(&b));
    // Le hash est stable via le JSON canonique servi.
    let json = serde_json::to_string(&a).unwrap();
    assert_eq!(
        payload_hash(&a),
        cloison_ledger::payload_hash_from_json(&json)
    );
}

// ---------------------------------------------------------------------------
// Checkpoint — anti-troncature
// ---------------------------------------------------------------------------

#[test]
fn checkpoint_detects_truncation_of_served_chain() {
    let key = control_key();
    let (ledger, head) = build_chain(6);
    let cp = ledger.checkpoint(1_700_000_020, &key);
    assert_eq!(cp.seq, 6);
    assert_eq!(cp.entry_hash, head.entry_hash);

    // Chaîne complète + checkpoint → valide.
    let full = Ledger::from_entries(ledger.entries().to_vec(), Some(key.verifying_key()));
    assert!(full.verify_chain_with_checkpoint(&cp, &key.verifying_key()));

    // Miroir tronqué (on coupe les 3 dernières entrées) : la chaîne reste auto-cohérente
    // mais le checkpoint signé révèle la troncature.
    let truncated: Vec<LedgerEntry> = ledger.entries()[..4].to_vec();
    let truncated = Ledger::from_entries(truncated, Some(key.verifying_key()));
    assert!(truncated.verify_chain());
    assert!(
        !truncated.verify_chain_with_checkpoint(&cp, &key.verifying_key()),
        "checkpoint seq=6 > head seq=3 → troncature détectée"
    );
}

#[test]
fn checkpoint_signature_must_verify() {
    let key = control_key();
    let (ledger, _) = build_chain(4);
    let mut cp = ledger.checkpoint(1_700_000_010, &key);
    cp.sig[10] ^= 0xFF; // altération de la signature
    let ledger = Ledger::from_entries(ledger.entries().to_vec(), Some(key.verifying_key()));
    assert!(!ledger.verify_chain_with_checkpoint(&cp, &key.verifying_key()));
}

#[test]
fn checkpoint_chain_of_two_verifies_against_prev() {
    let key = control_key();
    let g = Checkpoint::genesis();
    let cp1 = Checkpoint::sign(3, [0x11; 32], &g, 100, &key);
    let cp2 = Checkpoint::sign(9, [0x22; 32], &cp1, 200, &key);
    assert!(cp1.verify(&g, &key.verifying_key()));
    assert!(cp2.verify(&cp1, &key.verifying_key()));
    // cp2 ne vérifie pas contre la genèse (mauvais prédecesseur).
    assert!(!cp2.verify(&g, &key.verifying_key()));
}

// ---------------------------------------------------------------------------
// LedgerStore : MemLedger + AppendOnlyFileLedger
// ---------------------------------------------------------------------------

#[test]
fn mem_ledger_store_append_and_read() {
    let store = MemLedger::new();
    let key = control_key();
    // Un store vide n'accepte que la genèse comme premier append.
    store.append(&Ledger::genesis()).unwrap();
    let mut prev = Ledger::genesis();
    for i in 1..=3u64 {
        let entry = prev.next(
            payload_hash(&sample_payload("t", i)),
            1_700_000_000 + i,
            &key,
        );
        store.append(&entry).unwrap();
        prev = entry;
    }
    assert_eq!(store.len().unwrap(), 4);
    assert_eq!(store.get(2).unwrap().unwrap().seq, 2);
    assert_eq!(store.range(1, 3).unwrap().len(), 3);
}

#[test]
fn append_only_file_ledger_roundtrip_and_append_only() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("ledger.jsonl");
    let key = control_key();

    // Premier process : genèse + 3 entrées.
    let mut entries = vec![Ledger::genesis()];
    for i in 1..=3u64 {
        let prev = entries.last().unwrap();
        entries.push(prev.next(
            payload_hash(&sample_payload("t", i)),
            1_700_000_000 + i,
            &key,
        ));
    }
    {
        let store = AppendOnlyFileLedger::open(&path).unwrap();
        for e in &entries {
            store.append(e).unwrap();
        }
    }
    // Réouverture : rechargé à l'identique.
    let reopened = AppendOnlyFileLedger::open(&path).unwrap();
    assert_eq!(reopened.range(0, 3).unwrap(), entries);
    assert_eq!(reopened.head().unwrap().unwrap(), entries[3]);

    // Ré-append d'un seq déjà présent → refusé (append-only).
    assert!(matches!(
        reopened.append(&entries[2]),
        Err(LedgerError::SeqAlreadyAppended(2, 3))
    ));
    // Le fichier n'est jamais réécrit : exactement 4 lignes.
    let raw = std::fs::read_to_string(&path).unwrap();
    assert_eq!(raw.lines().count(), 4);
}

#[test]
fn ledger_open_file_reloads_chain_and_verifies() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("ledger.jsonl");
    let key = control_key();

    let root_after;
    {
        let mut ledger = Ledger::open_file(&path, key.verifying_key()).unwrap();
        let mut head = ledger.head().cloned().unwrap();
        for i in 1..=5u64 {
            let entry = head.next(
                payload_hash(&sample_payload("t", i)),
                1_700_000_000 + i,
                &key,
            );
            ledger.append(entry.clone()).unwrap();
            head = entry;
        }
        assert!(ledger.verify_chain());
        root_after = ledger.root_hash();
    }
    // Boot suivant : la chaîne entière est rechargée depuis le fichier.
    let reloaded = Ledger::open_file(&path, key.verifying_key()).unwrap();
    assert_eq!(reloaded.len(), 6);
    assert!(reloaded.verify_chain());
    assert_eq!(reloaded.root_hash(), root_after);
    assert!(reloaded.verify_inclusion(&payload_hash(&sample_payload("t", 3))));
}
