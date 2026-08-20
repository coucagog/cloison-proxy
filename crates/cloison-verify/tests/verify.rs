//! Tests du vérificateur public : chaîne, signatures, tampering, inclusion,
//! checkpoints anti-troncature.

use cloison_ledger::{payload_hash, Ledger, LedgerEntry, LedgerPayload};
use cloison_verify::{
    find_inclusion, prove_inclusion, verify_chain, verify_chain_v, verify_chain_with_checkpoint,
    verify_entry, VerifyError,
};
use ed25519_dalek::{Signer, SigningKey, VerifyingKey};

fn control_key() -> SigningKey {
    SigningKey::from_bytes(&[7u8; 32])
}

fn attacker_key() -> SigningKey {
    SigningKey::from_bytes(&[11u8; 32])
}

fn sample_payload(tenant: &str, n: u64) -> LedgerPayload {
    let mut payload = LedgerPayload::empty(tenant);
    payload.total_requests = n;
    payload.counters.insert("requests".to_string(), n);
    payload
}

/// Construit un jeu d'entrées (genèse + `count` entrées réelles signées).
fn build_entries(count: u64) -> Vec<LedgerEntry> {
    let key = control_key();
    let mut entries = vec![Ledger::genesis()];
    for i in 1..=count {
        let prev = entries.last().unwrap();
        entries.push(prev.next(payload_hash(&sample_payload("t", i)), 1_700_000_000 + i, &key));
    }
    entries
}

#[test]
fn valid_chain_passes() {
    let entries = build_entries(10);
    let key: VerifyingKey = control_key().verifying_key();
    assert_eq!(verify_chain(&entries, &key), Ok(()));
    let verdict = verify_chain_v(&entries, &key);
    assert!(verdict.ok);
    assert_eq!(verdict.entries_checked, 11);
    assert_eq!(verdict.head_seq, 10);
    assert_eq!(verdict.head_entry_hash, entries.last().map(|e| e.entry_hash));
    assert_eq!(verdict.failure, None);
}

#[test]
fn genesis_only_chain_passes() {
    let entries = vec![Ledger::genesis()];
    let key = control_key().verifying_key();
    assert_eq!(verify_chain(&entries, &key), Ok(()));
}

#[test]
fn empty_chain_fails() {
    let key = control_key().verifying_key();
    assert_eq!(verify_chain(&[], &key), Err(VerifyError::EmptyChain));
}

#[test]
fn wrong_genesis_fails() {
    let mut entries = build_entries(3);
    entries[0].seq = 1; // la genèse doit porter seq 0
    let key = control_key().verifying_key();
    assert_eq!(
        verify_chain(&entries, &key),
        Err(VerifyError::GenesisMismatch { seq: 1 })
    );

    let mut entries = build_entries(3);
    entries[0].prev_hash[0] = 0x01;
    assert_eq!(
        verify_chain(&entries, &key),
        Err(VerifyError::GenesisMismatch { seq: 0 })
    );
}

#[test]
fn tampered_payload_hash_is_entry_hash_mismatch() {
    let mut entries = build_entries(5);
    entries[2].payload_hash[0] ^= 0x01; // retourner un octet
    let key = control_key().verifying_key();
    assert_eq!(
        verify_chain(&entries, &key),
        Err(VerifyError::EntryHashMismatch { seq: 2 })
    );
}

#[test]
fn tampered_entry_hash_breaks_next_prev() {
    let mut entries = build_entries(5);
    entries[3].entry_hash[0] ^= 0x01;
    let key = control_key().verifying_key();
    assert_eq!(
        verify_chain(&entries, &key),
        Err(VerifyError::EntryHashMismatch { seq: 3 })
    );

    // Corrompre directement le prev_hash d'une entrée → PrevHashMismatch à cette entrée.
    let mut entries = build_entries(5);
    entries[4].prev_hash[0] ^= 0x01;
    assert_eq!(
        verify_chain(&entries, &key),
        Err(VerifyError::PrevHashMismatch { seq: 4 })
    );
}

#[test]
fn dropped_entry_is_seq_gap() {
    let mut entries = build_entries(5);
    entries.remove(3); // on supprime l'entrée seq 3
    let key = control_key().verifying_key();
    assert_eq!(
        verify_chain(&entries, &key),
        Err(VerifyError::SeqGap {
            expected: 3,
            got: 4
        })
    );
}

#[test]
fn swapped_entries_is_seq_gap() {
    // L'inversion de deux entrées consécutives crée d'abord une discontinuité de séquence
    // (l'entrée portant seq 3 arrive en position 2) → SeqGap, puis le lien prev cassé.
    let mut entries = build_entries(5);
    entries.swap(2, 3);
    let key = control_key().verifying_key();
    assert_eq!(
        verify_chain(&entries, &key),
        Err(VerifyError::SeqGap {
            expected: 2,
            got: 3
        })
    );
}

#[test]
fn bad_signature_is_detected() {
    // Chaîne signée par une autre clé → BadSignature dès la première entrée réelle.
    let attacker = attacker_key();
    let mut entries = vec![Ledger::genesis()];
    for i in 1..=3u64 {
        let prev = entries.last().unwrap();
        entries.push(prev.next(payload_hash(&sample_payload("t", i)), 1_700_000_000 + i, &attacker));
    }
    let key = control_key().verifying_key();
    assert_eq!(verify_chain(&entries, &key), Err(VerifyError::BadSignature { seq: 1 }));
}

#[test]
fn truncated_signature_is_bad_signature() {
    let mut entries = build_entries(2);
    entries[1].sig = vec![0u8; 16]; // 16 octets ≠ 64
    let key = control_key().verifying_key();
    assert_eq!(
        verify_chain(&entries, &key),
        Err(VerifyError::BadSignature { seq: 1 })
    );
}

#[test]
fn timestamp_regression_is_detected() {
    // La régression est détectée quand l'entrée reste auto-cohérente (entry_hash recomputé
    // depuis le ts modifié) : le contrôle ts précède le contrôle de signature.
    let mut entries = build_entries(3);
    entries[2].ts_unix = entries[1].ts_unix - 1;
    entries[2].entry_hash = LedgerEntry::compute_entry_hash(
        entries[2].seq,
        &entries[2].prev_hash,
        &entries[2].payload_hash,
        entries[2].ts_unix,
    );
    let key = control_key().verifying_key();
    assert_eq!(
        verify_chain(&entries, &key),
        Err(VerifyError::TimestampRegressed { seq: 2 })
    );
}

#[test]
fn verify_entry_isolated() {
    let entries = build_entries(3);
    let key = control_key().verifying_key();
    assert_eq!(verify_entry(&entries[1], &entries[0].entry_hash, &key), Ok(()));
    assert_eq!(
        verify_entry(&entries[2], &entries[1].entry_hash, &key),
        Ok(())
    );
    // Mauvais prev_hash attendu.
    assert_eq!(
        verify_entry(&entries[1], &[0x99; 32], &key),
        Err(VerifyError::PrevHashMismatch { seq: 1 })
    );
}

#[test]
fn inclusion_by_hash() {
    let entries = build_entries(4);
    let key = control_key().verifying_key();
    assert_eq!(verify_chain(&entries, &key), Ok(()));

    let p2 = payload_hash(&sample_payload("t", 2));
    let p9 = payload_hash(&sample_payload("t", 9));
    assert!(prove_inclusion(&entries, &p2));
    assert!(!prove_inclusion(&entries, &p9));
    // La genèse (payload nul) n'est pas une inclusion.
    assert!(!prove_inclusion(&entries, &[0u8; 32]));

    // Preuve structurée : cible + préfixe de hashes.
    let proof = find_inclusion(&entries, &p2).expect("payload 2 is present");
    assert_eq!(proof.target_seq, 2);
    assert_eq!(proof.target_payload_hash, p2);
    assert_eq!(proof.prefix_hashes.len(), 2);
    assert_eq!(proof.prefix_hashes[0], entries[1].entry_hash);
    assert_eq!(proof.prefix_hashes[1], entries[2].entry_hash);
    assert_eq!(proof.head_seq, 4);
    assert_eq!(proof.head_entry_hash, entries.last().unwrap().entry_hash);
    assert!(find_inclusion(&entries, &p9).is_none());
}

#[test]
fn entries_json_round_trip() {
    // Le chemin WASM consomme un JSON de LedgerEntry : vérifions la sérialisation.
    let entries = build_entries(3);
    let json = serde_json::to_string(&entries).unwrap();
    let parsed: Vec<LedgerEntry> = serde_json::from_str(&json).unwrap();
    assert_eq!(parsed, entries);
    let key = control_key().verifying_key();
    assert_eq!(verify_chain(&parsed, &key), Ok(()));
}

// ---------------------------------------------------------------------------
// Checkpoints — anti-troncature
// ---------------------------------------------------------------------------

/// Construit un checkpoint signé par `key` ancrant la tête de `entries`.
fn checkpoint_at(entries: &[LedgerEntry], key: &SigningKey) -> cloison_ledger::Checkpoint {
    let head = entries.last().unwrap();
    cloison_ledger::Checkpoint::sign(
        head.seq,
        head.entry_hash,
        &cloison_ledger::Checkpoint::genesis(),
        1_700_000_100,
        key,
    )
}

#[test]
fn checkpoint_valid_chain_passes() {
    let entries = build_entries(6);
    let key = control_key();
    let cp = checkpoint_at(&entries, &key);
    assert_eq!(
        verify_chain_with_checkpoint(
            &entries,
            &cp,
            &key.verifying_key(),
            &key.verifying_key()
        ),
        Ok(())
    );
}

#[test]
fn checkpoint_detects_truncated_chain() {
    let entries = build_entries(6);
    let key = control_key();
    let cp = checkpoint_at(&entries, &key);

    // Le miroir tronque les 3 dernières entrées : verify_chain passe (la chaîne reste
    // auto-cohérente) mais le checkpoint révèle la troncature.
    let truncated = entries[..4].to_vec();
    assert_eq!(verify_chain(&truncated, &key.verifying_key()), Ok(()));
    assert_eq!(
        verify_chain_with_checkpoint(
            &truncated,
            &cp,
            &key.verifying_key(),
            &key.verifying_key()
        ),
        Err(VerifyError::TruncatedChain {
            checkpoint_seq: 6,
            head_seq: 3
        })
    );
}

#[test]
fn checkpoint_detects_divergent_head() {
    let mut entries = build_entries(5);
    let key = control_key();
    let cp = checkpoint_at(&entries, &key);

    // Réécriture de la tête (même seq, contenu différent, re-signée) : la chaîne reste
    // valide mais l'entry_hash ancré ne correspond plus.
    let mut head = entries[5].clone();
    head.payload_hash = [0xEE; 32];
    head.entry_hash = LedgerEntry::compute_entry_hash(
        head.seq,
        &head.prev_hash,
        &head.payload_hash,
        head.ts_unix,
    );
    head.sig = key.sign(&head.entry_hash).to_bytes().to_vec();
    entries[5] = head;
    assert_eq!(verify_chain(&entries, &key.verifying_key()), Ok(()));
    assert_eq!(
        verify_chain_with_checkpoint(
            &entries,
            &cp,
            &key.verifying_key(),
            &key.verifying_key()
        ),
        Err(VerifyError::CheckpointMismatch { seq: 5 })
    );
}

#[test]
fn checkpoint_bad_signature_is_rejected() {
    let entries = build_entries(4);
    let key = control_key();
    let mut cp = checkpoint_at(&entries, &key);
    cp.sig[3] ^= 0x01;
    assert_eq!(
        verify_chain_with_checkpoint(
            &entries,
            &cp,
            &key.verifying_key(),
            &key.verifying_key()
        ),
        Err(VerifyError::CheckpointInvalid { seq: 4 })
    );
    // Checkpoint signé par une AUTRE clé → CheckpointInvalid aussi.
    let cp = checkpoint_at(&entries, &attacker_key());
    assert_eq!(
        verify_chain_with_checkpoint(
            &entries,
            &cp,
            &key.verifying_key(),
            &key.verifying_key()
        ),
        Err(VerifyError::CheckpointInvalid { seq: 4 })
    );
}

#[test]
fn checkpoint_requires_valid_chain_first() {
    let mut entries = build_entries(5);
    let key = control_key();
    let cp = checkpoint_at(&entries, &key);
    entries[2].payload_hash[0] ^= 0x01; // chaîne corrompue
    assert_eq!(
        verify_chain_with_checkpoint(
            &entries,
            &cp,
            &key.verifying_key(),
            &key.verifying_key()
        ),
        Err(VerifyError::EntryHashMismatch { seq: 2 })
    );
}
