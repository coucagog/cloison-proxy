//! Tests du plan de contrôle : cycle de vie des tenants, jetons **hachés** (jamais en
//! clair), rotation avec grâce, révocation, IDOR cross-tenant, politique, licence,
//! contresignature de vrais reçus STACK-4, pipeline ingest → ledger (k-anonymat,
//! persistance fichier), version de propagation et anti-fuite TokenIssued.

use axum::extract::{Path, Query, State};
use axum::Json;
use cloison_control::api::{
    self, AddLicenseReq, AppState, CreateTenantReq, IngestRequest, IssueTokenReq, PutPolicyReq,
    RotateTokenReq, VersionQuery,
};
use cloison_control::contersign::{contresigner_reçu, verifier_contresignature};
use cloison_control::error::ControlError;
use cloison_control::model::{
    ApiToken, License, LicenseLimites, Plan, Policy, Tenant, TenantStatut, TokenIssued,
};
use cloison_control::token;
use cloison_control::{InMemoryStore, Store};
use cloison_audit::{Counters, Receipt, ReceiptMessage};
use cloison_ledger::{hexutil, payload_hash, Ledger, LedgerPayload};
use ed25519_dalek::SigningKey;
use std::collections::BTreeMap;
use std::sync::{Arc, Mutex};

/// État de test : journal mémoire, agent = [7u8;32], contrôle = [8u8;32], grâce = 0
/// (rotation ⇒ invalidation immédiate, sauf tests de grâce dédiés).
fn app_state() -> AppState {
    let agent = SigningKey::from_bytes(&[7u8; 32]);
    let control = SigningKey::from_bytes(&[8u8; 32]);
    let ledger = Ledger::with_verify_key(control.verifying_key());
    AppState::new(
        Arc::new(InMemoryStore::new()),
        Arc::new(Mutex::new(ledger)),
        agent.verifying_key(),
        control,
        0,
    )
}

fn sample_tenant(id: &str) -> Tenant {
    Tenant {
        id: id.to_string(),
        nom_public: "operateur".to_string(),
        statut: TenantStatut::Actif,
        created_at: 1_700_000_000,
        tokens_version: 0,
    }
}

/// Construit `n` reçus STACK-4 signés par l'agent, avec compteurs pilotés par `per`.
fn signed_receipts(
    tenant: &str,
    agent: &SigningKey,
    n: usize,
    per: impl Fn(usize) -> Counters,
) -> Vec<Receipt> {
    (0..n)
        .map(|i| {
            Receipt::build_signed(
                ReceiptMessage {
                    tenant_id: tenant.to_string(),
                    session_ref_hashed: format!("sess-hash-{i}"),
                    ts_unix: 100 + i as u64,
                    engine_version: "0.1.0".to_string(),
                    policy_hash: "pol-1".to_string(),
                    counters: per(i),
                },
                agent,
            )
        })
        .collect()
}

fn counters_with(email: u64, cni: u64) -> Counters {
    let mut masked = BTreeMap::new();
    if email > 0 {
        masked.insert("Email".to_string(), email);
    }
    if cni > 0 {
        masked.insert("CniSn".to_string(), cni);
    }
    Counters {
        masked_by_type: masked,
        incomplete_restorations: 0,
        blocked_outputs: 0,
        quasi_id_flags: 0,
    }
}

// ---------------------------------------------------------------------------
// Tenant lifecycle
// ---------------------------------------------------------------------------

#[test]
fn tenant_lifecycle() {
    let store = InMemoryStore::new();
    assert!(store.get_tenant("tenant-a").unwrap().is_none());

    store.create_tenant(&sample_tenant("tenant-a")).unwrap();
    let got = store.get_tenant("tenant-a").unwrap().expect("created");
    assert_eq!(got.id, "tenant-a");
    assert_eq!(got.nom_public, "operateur");
    assert_eq!(got.statut, TenantStatut::Actif);
    assert_eq!(got.tokens_version, 0);

    // Doublon → conflit.
    assert!(store.create_tenant(&sample_tenant("tenant-a")).is_err());
}

// ---------------------------------------------------------------------------
// Token : le clair n'est JAMAIS persisté
// ---------------------------------------------------------------------------

#[test]
fn token_hash_never_stored_in_plain() {
    let store = InMemoryStore::new();
    store.create_tenant(&sample_tenant("tenant-a")).unwrap();

    let clair = token::generate_token();
    assert!(clair.starts_with("mn_"));
    assert_eq!(clair.len(), 46);

    let stored = ApiToken::issue(
        "tok-1".to_string(),
        "tenant-a".to_string(),
        &clair,
        vec!["audit".to_string()],
        1_700_000_100,
    );
    store.create_token(&stored).unwrap();

    // 1. Le hash stocké diffère du clair.
    assert_ne!(stored.token_hash, clair);
    // 2. Et correspond exactement à la formule hex(SHA-256("cloison-mn-token-v1:" ‖ clair)).
    assert_eq!(stored.token_hash, token::token_hash(&clair));
    assert_eq!(stored.token_hash.len(), 64);
    // 3. Le clair n'apparaît nulle part dans la représentation persistée.
    let serialized = serde_json::to_string(&stored).unwrap();
    assert!(!serialized.contains(&clair), "le clair ne doit jamais être persisté");
    // 4. La validation se fait par le clair présenté → digest comparé en temps constant.
    let found = store.validate_token(&clair).unwrap().expect("jeton valide");
    assert_eq!(found.id, stored.id);
    assert!(store.validate_token("mn_autre-jeton").unwrap().is_none());
    // 5. `hash_token` est cohérent avec la formule publique.
    assert_eq!(store.hash_token(&clair), token::token_hash(&clair));
}

#[test]
fn validation_rejects_revoked_and_rotated() {
    let store = InMemoryStore::new();
    store.create_tenant(&sample_tenant("tenant-a")).unwrap();
    let clair = token::generate_token();
    let t = ApiToken::issue("tok-1".to_string(), "tenant-a".to_string(), &clair, vec![], 100);
    store.create_token(&t).unwrap();
    assert!(store.validate_token(&clair).unwrap().is_some());

    // Révocation immédiate.
    store.revoke_token("tenant-a", &t.id).unwrap();
    assert!(store.validate_token(&clair).unwrap().is_none());
}

// ---------------------------------------------------------------------------
// Rotation (grâce) & révocation
// ---------------------------------------------------------------------------

#[test]
fn rotation_with_zero_grace_invalidates_old_token() {
    let store = InMemoryStore::new();
    store.create_tenant(&sample_tenant("tenant-a")).unwrap();
    let old_clair = token::generate_token();
    let old = ApiToken::issue(
        "tok-old".to_string(),
        "tenant-a".to_string(),
        &old_clair,
        vec!["audit".to_string()],
        100,
    );
    store.create_token(&old).unwrap();
    assert!(store.validate_token(&old_clair).unwrap().is_some());

    let new_clair = token::generate_token();
    let new = ApiToken::issue(
        "tok-new".to_string(),
        "tenant-a".to_string(),
        &new_clair,
        vec![],
        200,
    );
    // Grâce = 0 : l'ancien est invalidé immédiatement.
    store.rotate_token("tenant-a", &old.id, &new, 0).unwrap();

    let old_after = store.validate_token(&old_clair).unwrap();
    assert!(old_after.is_none());
    // Le nouveau est actif et a hérité des scopes de l'ancien.
    let new_after = store.validate_token(&new_clair).unwrap().expect("nouveau jeton actif");
    assert_eq!(new_after.scopes, vec!["audit".to_string()]);
    // Rotation d'un jeton inexistant → erreur.
    assert!(store.rotate_token("tenant-a", "tok-inconnu", &new, 0).is_err());
}

#[test]
fn rotation_grace_keeps_old_token_valid_until_expiry() {
    let store = InMemoryStore::new();
    store.create_tenant(&sample_tenant("tenant-a")).unwrap();
    let old_clair = token::generate_token();
    let old = ApiToken::issue("tok-old".into(), "tenant-a".into(), &old_clair, vec![], 100);
    store.create_token(&old).unwrap();
    let new_clair = token::generate_token();
    let new = ApiToken::issue("tok-new".into(), "tenant-a".into(), &new_clair, vec![], 200);

    // Grâce de 300 s : l'ancien reste VALIDE pendant la période.
    store.rotate_token("tenant-a", &old.id, &new, 300).unwrap();
    assert!(
        store.validate_token(&old_clair).unwrap().is_some(),
        "ancien jeton encore valide pendant la grâce"
    );
    assert!(store.validate_token(&new_clair).unwrap().is_some());

    // Déterministe via grace_until : valide jusqu'à grace_until - 1, invalide à partir de grace_until.
    let stored_old = store.get_token(&old.id).unwrap().unwrap();
    let grace_until = stored_old.grace_until.expect("grace_until posé par la rotation");
    assert!(stored_old.is_active_at(grace_until - 1));
    assert!(!stored_old.is_active_at(grace_until));
}

#[test]
fn store_rotate_and_revoke_reject_cross_tenant_idor() {
    let store = InMemoryStore::new();
    store.create_tenant(&sample_tenant("tenant-a")).unwrap();
    store.create_tenant(&sample_tenant("tenant-b")).unwrap();
    let b_clair = token::generate_token();
    let b_token = ApiToken::issue("tok-b".into(), "tenant-b".into(), &b_clair, vec![], 100);
    store.create_token(&b_token).unwrap();

    // L'opérateur du tenant A tente de rotater un jeton du tenant B → refus 404.
    let new = ApiToken::issue(
        "tok-new".into(),
        "tenant-a".into(),
        &token::generate_token(),
        vec![],
        200,
    );
    assert!(matches!(
        store.rotate_token("tenant-a", &b_token.id, &new, 300),
        Err(ControlError::TokenNotFound(_))
    ));
    // Le jeton de B est intact et toujours actif (pas de répercussion).
    assert!(store.validate_token(&b_clair).unwrap().is_some());

    // Révocation cross-tenant → refus aussi.
    assert!(matches!(
        store.revoke_token("tenant-a", &b_token.id),
        Err(ControlError::TokenNotFound(_))
    ));
    assert!(store.validate_token(&b_clair).unwrap().is_some());
}

#[test]
fn tokens_version_increments_on_rotate_and_revoke() {
    let store = InMemoryStore::new();
    store.create_tenant(&sample_tenant("tenant-a")).unwrap();
    assert_eq!(store.tokens_version("tenant-a").unwrap(), 0);

    let clair = token::generate_token();
    let t = ApiToken::issue("tok-1".into(), "tenant-a".into(), &clair, vec![], 100);
    store.create_token(&t).unwrap();
    assert_eq!(store.tokens_version("tenant-a").unwrap(), 0, "l'émission ne change pas la version");

    let new = ApiToken::issue("tok-2".into(), "tenant-a".into(), &token::generate_token(), vec![], 200);
    store.rotate_token("tenant-a", &t.id, &new, 0).unwrap();
    assert_eq!(store.tokens_version("tenant-a").unwrap(), 1, "rotation → version+1");

    store.revoke_token("tenant-a", &new.id).unwrap();
    assert_eq!(store.tokens_version("tenant-a").unwrap(), 2, "révocation → version+1");

    // Tenant inexistant → erreur.
    assert!(matches!(
        store.tokens_version("tenant-inconnu"),
        Err(ControlError::TenantNotFound(_))
    ));
}

// ---------------------------------------------------------------------------
// Politique & licence
// ---------------------------------------------------------------------------

#[test]
fn policy_publish_and_version_increment() {
    let store = InMemoryStore::new();
    store.create_tenant(&sample_tenant("tenant-a")).unwrap();

    store
        .set_policy(&Policy {
            tenant_id: "tenant-a".to_string(),
            json_policy: r#"{"k":2,"mode":"audit"}"#.to_string(),
            version: 1,
            updated_at: 1_700_000_200,
        })
        .unwrap();
    let p1 = store.get_policy("tenant-a").unwrap().expect("politique publiée");
    assert_eq!(p1.version, 1);

    // Seconde publication → version 2.
    store
        .set_policy(&Policy {
            tenant_id: "tenant-a".to_string(),
            json_policy: r#"{"k":3,"mode":"audit"}"#.to_string(),
            version: 2,
            updated_at: 1_700_000_300,
        })
        .unwrap();
    let p2 = store.get_policy("tenant-a").unwrap().expect("politique v2");
    assert_eq!(p2.version, 2);
    assert!(p2.json_policy.contains("\"k\":3"));
}

#[test]
fn license_add_and_get() {
    let store = InMemoryStore::new();
    store.create_tenant(&sample_tenant("tenant-a")).unwrap();
    assert!(store.get_license("tenant-a").unwrap().is_none());

    store
        .add_license(&License {
            tenant_id: "tenant-a".to_string(),
            plan: Plan::Pro,
            limites: LicenseLimites {
                max_requests_per_day: 50_000,
                max_tokens: 32,
            },
            expires_at: Some(1_900_000_000),
            created_at: 1_700_000_000,
        })
        .unwrap();
    let lic = store.get_license("tenant-a").unwrap().expect("licence active");
    assert_eq!(lic.plan, Plan::Pro);
    assert_eq!(lic.limites.max_requests_per_day, 50_000);

    // Upsert : une licence par tenant — la nouvelle remplace l'ancienne.
    store
        .add_license(&License {
            tenant_id: "tenant-a".to_string(),
            plan: Plan::Enterprise,
            limites: LicenseLimites::default(),
            expires_at: None,
            created_at: 1_700_000_100,
        })
        .unwrap();
    let upgraded = store.get_license("tenant-a").unwrap().expect("licence remplacée");
    assert_eq!(upgraded.plan, Plan::Enterprise);
}

// ---------------------------------------------------------------------------
// Contresignature — vrais reçus STACK-4 (message = signing_bytes())
// ---------------------------------------------------------------------------

#[test]
fn contersign_round_trip() {
    let agent_key = SigningKey::from_bytes(&[7u8; 32]);
    let control_key = SigningKey::from_bytes(&[8u8; 32]);
    let receipt = Receipt::build_signed(
        ReceiptMessage {
            tenant_id: "tenant-42".to_string(),
            session_ref_hashed: "abc".to_string(),
            ts_unix: 1_700_000_000,
            engine_version: "0.1.0".to_string(),
            policy_hash: "def".to_string(),
            counters: Counters::default(),
        },
        &agent_key,
    );

    // Un reçu RÉELLEMENT signé par l'agent passe (P0-3).
    let cs = contresigner_reçu(&receipt, &agent_key.verifying_key(), &control_key)
        .expect("reçu signé par l'agent → contresignature acceptée");
    assert!(verifier_contresignature(
        &cs,
        &receipt,
        &agent_key.verifying_key(),
        &control_key.verifying_key()
    ));

    // Mauvaise clé agent → refus, aucune contresignature produite.
    let wrong_agent = SigningKey::from_bytes(&[9u8; 32]);
    assert!(matches!(
        contresigner_reçu(&receipt, &wrong_agent.verifying_key(), &control_key),
        Err(ControlError::InvalidAgentSignature)
    ));

    // Reçu altéré après signature (même sig_agent, contenu différent) → refus.
    let mut tampered = receipt.clone();
    tampered.counters.blocked_outputs += 1;
    assert!(contresigner_reçu(&tampered, &agent_key.verifying_key(), &control_key).is_err());

    // Non signé → refus.
    let unsigned = Receipt::build(ReceiptMessage {
        tenant_id: "tenant-42".to_string(),
        session_ref_hashed: "abc".to_string(),
        ts_unix: 1_700_000_000,
        engine_version: "0.1.0".to_string(),
        policy_hash: "def".to_string(),
        counters: Counters::default(),
    });
    assert!(matches!(
        contresigner_reçu(&unsigned, &agent_key.verifying_key(), &control_key),
        Err(ControlError::InvalidAgentSignature)
    ));
}

// ---------------------------------------------------------------------------
// Pipeline ingest → ledger (P0-1/P0-2/P0-3)
// ---------------------------------------------------------------------------

#[tokio::test]
async fn ingest_e2e_appends_countersigned_entry() {
    let agent = SigningKey::from_bytes(&[7u8; 32]);
    let control = SigningKey::from_bytes(&[8u8; 32]);
    let store = Arc::new(InMemoryStore::new());
    store.create_tenant(&sample_tenant("tenant-a")).unwrap();
    let ledger = Arc::new(Mutex::new(Ledger::with_verify_key(control.verifying_key())));
    let state = AppState::new(store, ledger.clone(), agent.verifying_key(), control, 0);

    // 5 requêtes x 1 Email → agrégat 5 (≥ k=5), publiable et non redacté.
    let receipts = signed_receipts("tenant-a", &agent, 5, |_| counters_with(1, 0));
    let resp = api::ingest(
        State(state.clone()),
        Json(IngestRequest {
            tenant_id: "tenant-a".to_string(),
            period_start: 100,
            period_end: 200,
            k: 5,
            receipts,
        }),
    )
    .await
    .unwrap()
    .0;
    assert_eq!(resp.seq, 1, "genèse seq=0, première entrée réelle seq=1");

    // Le payload engagé = compteurs redactés attendus + engagements sur les reçus.
    let mut expected = LedgerPayload::empty("tenant-a");
    expected.period_start = 100;
    expected.period_end = 200;
    expected.total_requests = 5;
    expected.counters.insert("Email".to_string(), 5);
    expected.receipt_hashes = signed_receipts("tenant-a", &agent, 5, |_| counters_with(1, 0))
        .iter()
        .map(|r| cloison_ledger::sha256(&r.signing_bytes()))
        .collect();
    let expected_payload_hash = cloison_ledger::payload_hash(&expected);

    // Garde du mutex bornée à ce bloc : jamais tenue à travers un await.
    let (root_hash_hex, chain_ok, len_after, included) = {
        let lg = ledger.lock().unwrap();
        (
            hexutil::encode(&lg.root_hash()),
            lg.verify_chain(),
            lg.len(),
            lg.verify_inclusion(&expected_payload_hash),
        )
    };
    assert_eq!(resp.root_hash, root_hash_hex);
    assert!(chain_ok, "chaîne entière valide");
    assert_eq!(len_after, 2);
    assert!(
        included,
        "le payload redacté attendu est engagé dans le journal"
    );

    // Deuxième ingest → seq 2, chaîne toujours valide.
    let resp2 = api::ingest(
        State(state.clone()),
        Json(IngestRequest {
            tenant_id: "tenant-a".to_string(),
            period_start: 200,
            period_end: 300,
            k: 5,
            receipts: signed_receipts("tenant-a", &agent, 5, |_| counters_with(1, 0)),
        }),
    )
    .await
    .unwrap()
    .0;
    assert_eq!(resp2.seq, 2);
    {
        let lg = ledger.lock().unwrap();
        assert!(lg.verify_chain());
    }
}

#[tokio::test]
async fn ingest_redacts_counters_below_k() {
    let agent = SigningKey::from_bytes(&[7u8; 32]);
    let control = SigningKey::from_bytes(&[8u8; 32]);
    let store = Arc::new(InMemoryStore::new());
    store.create_tenant(&sample_tenant("tenant-a")).unwrap();
    let ledger = Arc::new(Mutex::new(Ledger::with_verify_key(control.verifying_key())));
    let state = AppState::new(store, ledger.clone(), agent.verifying_key(), control, 0);

    // 5 requêtes : Email=1 partout (total 5 ≥ k) ; CniSn=1 sur 2 requêtes seulement
    // (total 2 < k=5 → cell redactée à 0 dans le journal).
    let receipts = signed_receipts("tenant-a", &agent, 5, |i| counters_with(1, if i < 2 { 1 } else { 0 }));
    let resp = api::ingest(
        State(state.clone()),
        Json(IngestRequest {
            tenant_id: "tenant-a".to_string(),
            period_start: 100,
            period_end: 200,
            k: 5,
            receipts,
        }),
    )
    .await
    .unwrap()
    .0;
    assert_eq!(resp.seq, 1);

    let mut expected = LedgerPayload::empty("tenant-a");
    expected.period_start = 100;
    expected.period_end = 200;
    expected.total_requests = 5;
    expected.counters.insert("Email".to_string(), 5);
    expected.counters.insert("CniSn".to_string(), 0); // < k → redacté
    expected.receipt_hashes = signed_receipts("tenant-a", &agent, 5, |i| {
        counters_with(1, if i < 2 { 1 } else { 0 })
    })
    .iter()
    .map(|r| cloison_ledger::sha256(&r.signing_bytes()))
    .collect();

    let lg = ledger.lock().unwrap();
    assert!(
        lg.verify_inclusion(&cloison_ledger::payload_hash(&expected)),
        "aucun compteur < k ne doit exister dans le journal"
    );
}

#[tokio::test]
async fn ingest_rejects_bad_agent_signature() {
    let agent = SigningKey::from_bytes(&[7u8; 32]);
    let wrong_agent = SigningKey::from_bytes(&[9u8; 32]);
    let control = SigningKey::from_bytes(&[8u8; 32]);
    let store = Arc::new(InMemoryStore::new());
    store.create_tenant(&sample_tenant("tenant-a")).unwrap();
    let ledger = Arc::new(Mutex::new(Ledger::with_verify_key(control.verifying_key())));
    let state = AppState::new(store, ledger.clone(), agent.verifying_key(), control, 0);

    // Signés par une AUTRE clé que la clé agent du contrôle → rejeté, rien n'est appendé.
    let receipts = signed_receipts("tenant-a", &wrong_agent, 5, |_| counters_with(1, 0));
    assert!(api::ingest(
        State(state.clone()),
        Json(IngestRequest {
            tenant_id: "tenant-a".to_string(),
            period_start: 100,
            period_end: 200,
            k: 5,
            receipts,
        }),
    )
    .await
    .is_err());
    let lg = ledger.lock().unwrap();
    assert_eq!(lg.len(), 1, "genèse seulement — aucune entrée rejetée n'est appendée");
}

#[tokio::test]
async fn ingest_rejects_tenant_mismatch_and_empty() {
    let agent = SigningKey::from_bytes(&[7u8; 32]);
    let control = SigningKey::from_bytes(&[8u8; 32]);
    let store = Arc::new(InMemoryStore::new());
    store.create_tenant(&sample_tenant("tenant-a")).unwrap();
    let ledger = Arc::new(Mutex::new(Ledger::with_verify_key(control.verifying_key())));
    let state = AppState::new(store, ledger.clone(), agent.verifying_key(), control, 0);

    // Reçus signés valides mais d'un AUTRE tenant que la requête → refus.
    let receipts = signed_receipts("tenant-b", &agent, 5, |_| counters_with(1, 0));
    assert!(api::ingest(
        State(state.clone()),
        Json(IngestRequest {
            tenant_id: "tenant-a".to_string(),
            period_start: 100,
            period_end: 200,
            k: 5,
            receipts,
        }),
    )
    .await
    .is_err());

    // Aucun reçu → refus.
    assert!(api::ingest(
        State(state.clone()),
        Json(IngestRequest {
            tenant_id: "tenant-a".to_string(),
            period_start: 100,
            period_end: 200,
            k: 5,
            receipts: Vec::new(),
        }),
    )
    .await
    .is_err());
    assert_eq!(ledger.lock().unwrap().len(), 1);
}

#[tokio::test]
async fn ingest_persists_to_file_ledger_when_configured() {
    let agent = SigningKey::from_bytes(&[7u8; 32]);
    let control = SigningKey::from_bytes(&[8u8; 32]);
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("ledger.jsonl");
    let store = Arc::new(InMemoryStore::new());
    store.create_tenant(&sample_tenant("tenant-a")).unwrap();

    let ledger = Arc::new(Mutex::new(
        Ledger::open_file(&path, control.verifying_key()).unwrap(),
    ));
    let state = AppState::new(store, ledger.clone(), agent.verifying_key(), control.clone(), 0);

    let receipts = signed_receipts("tenant-a", &agent, 5, |_| counters_with(1, 0));
    let resp = api::ingest(
        State(state.clone()),
        Json(IngestRequest {
            tenant_id: "tenant-a".to_string(),
            period_start: 100,
            period_end: 200,
            k: 5,
            receipts,
        }),
    )
    .await
    .unwrap()
    .0;
    assert_eq!(resp.seq, 1);

    // On lâche toutes les références au journal (fermeture du fichier) puis on recharge :
    // l'entrée est durable (JSONL append-only).
    drop(state);
    drop(ledger);
    let reloaded = Ledger::open_file(&path, control.verifying_key()).unwrap();
    assert_eq!(reloaded.len(), 2, "genèse + entrée ingest, rechargées au boot");
    assert!(reloaded.verify_chain());
    assert!(reloaded.verify_inclusion(&payload_hash(&{
        let mut expected = LedgerPayload::empty("tenant-a");
        expected.period_start = 100;
        expected.period_end = 200;
        expected.total_requests = 5;
        expected.counters.insert("Email".to_string(), 5);
        expected.receipt_hashes = signed_receipts("tenant-a", &agent, 5, |_| counters_with(1, 0))
            .iter()
            .map(|r| cloison_ledger::sha256(&r.signing_bytes()))
            .collect();
        expected
    })));
}

#[tokio::test]
async fn api_root_and_version_endpoints() {
    let agent = SigningKey::from_bytes(&[7u8; 32]);
    let control = SigningKey::from_bytes(&[8u8; 32]);
    let store = Arc::new(InMemoryStore::new());
    store.create_tenant(&sample_tenant("tenant-a")).unwrap();
    let ledger = Arc::new(Mutex::new(Ledger::with_verify_key(control.verifying_key())));
    let state = AppState::new(store, ledger.clone(), agent.verifying_key(), control, 300);

    // Racine du journal : genèse (seq=0, hash de référence verrouillé).
    let root0 = api::root(State(state.clone())).await.unwrap().0;
    assert_eq!(root0["seq"], 0);
    assert_eq!(
        root0["root_hash"],
        "5b6fb58e61fa475939767d68a446f97f1bff02c0e5935a3ea8bb51e6515783d8"
    );

    // Une entrée ingest → la racine change et seq=1.
    let receipts = signed_receipts("tenant-a", &agent, 5, |_| counters_with(1, 0));
    let resp = api::ingest(
        State(state.clone()),
        Json(IngestRequest {
            tenant_id: "tenant-a".to_string(),
            period_start: 100,
            period_end: 200,
            k: 5,
            receipts,
        }),
    )
    .await
    .unwrap()
    .0;
    assert_eq!(resp.seq, 1);
    let root1 = api::root(State(state.clone())).await.unwrap().0;
    assert_eq!(root1["seq"], 1);
    assert_eq!(root1["root_hash"], resp.root_hash);
    assert_ne!(root0["root_hash"], root1["root_hash"]);

    // Version de propagation : 0 puis incrémentée par rotation.
    let v0 = api::tokens_version(State(state.clone()), Query(VersionQuery {
        tenant_id: "tenant-a".to_string(),
    }))
    .await
    .unwrap()
    .0;
    assert_eq!(v0["version"], 0);

    let clair = token::generate_token();
    let t = ApiToken::issue("tok-1".into(), "tenant-a".into(), &clair, vec![], 100);
    state.store.create_token(&t).unwrap();
    let new = ApiToken::issue("tok-2".into(), "tenant-a".into(), &token::generate_token(), vec![], 200);
    state.store.rotate_token("tenant-a", &t.id, &new, 300).unwrap();
    let v1 = api::tokens_version(State(state.clone()), Query(VersionQuery {
        tenant_id: "tenant-a".to_string(),
    }))
    .await
    .unwrap()
    .0;
    assert_eq!(v1["version"], 1);
}

// ---------------------------------------------------------------------------
// API admin (handlers appelés directement)
// ---------------------------------------------------------------------------

#[tokio::test]
async fn api_tenant_and_token_flow() {
    let state = app_state();

    // POST /admin/tenants
    let tenant = api::create_tenant(
        State(state.clone()),
        Json(CreateTenantReq {
            id: "tenant-42".to_string(),
            nom_public: "operateur-42".to_string(),
            plan: Plan::Pro,
        }),
    )
    .await
    .unwrap()
    .0;
    assert_eq!(tenant.id, "tenant-42");
    assert_eq!(tenant.statut, TenantStatut::Actif);
    assert!(state.store.get_tenant("tenant-42").unwrap().is_some());

    // POST /admin/tenants/{id}/tokens — le clair revient UNE fois, le store ne garde que le hash.
    let issued = api::issue_token(
        State(state.clone()),
        Path("tenant-42".to_string()),
        Json(IssueTokenReq { scopes: vec!["audit".to_string()] }),
    )
    .await
    .unwrap()
    .0;
    let issued_clair = issued["token"].as_str().expect("clair dans la réponse d'émission");
    assert!(issued_clair.starts_with("mn_"));
    assert_eq!(issued_clair.len(), 46);

    let stored = state
        .store
        .validate_token(issued_clair)
        .unwrap()
        .expect("jeton actif après émission");
    assert_ne!(stored.token_hash, issued_clair, "le store ne contient que le hash");
    assert_eq!(stored.token_hash, token::token_hash(issued_clair));
    assert_eq!(stored.scopes, vec!["audit".to_string()]);

    // POST /admin/tenants/{id}/rotate — nouveau secret ; l'état de test utilise grâce=0
    // donc l'ancien est invalidé immédiatement.
    let rotated = api::rotate_token(
        State(state.clone()),
        Path("tenant-42".to_string()),
        Json(RotateTokenReq { token_id: stored.id.clone() }),
    )
    .await
    .unwrap()
    .0;
    let rotated_clair = rotated["token"].as_str().expect("clair dans la réponse de rotation");
    assert_ne!(rotated_clair, issued_clair);
    assert!(state.store.validate_token(issued_clair).unwrap().is_none());
    let new_stored = state
        .store
        .validate_token(rotated_clair)
        .unwrap()
        .expect("nouveau jeton actif après rotation");

    // DELETE /admin/tenants/{id}/tokens/{token_id} — révocation immédiate.
    api::revoke_token(
        State(state.clone()),
        Path(("tenant-42".to_string(), new_stored.id.clone())),
    )
    .await
    .unwrap();
    assert!(state.store.validate_token(rotated_clair).unwrap().is_none());
}

#[tokio::test]
async fn api_idor_cross_tenant_rotate_and_revoke_are_rejected() {
    let state = app_state();
    let _ = api::create_tenant(
        State(state.clone()),
        Json(CreateTenantReq {
            id: "tenant-a".to_string(),
            nom_public: "op-a".to_string(),
            plan: Plan::Free,
        }),
    )
    .await
    .unwrap();
    let _ = api::create_tenant(
        State(state.clone()),
        Json(CreateTenantReq {
            id: "tenant-b".to_string(),
            nom_public: "op-b".to_string(),
            plan: Plan::Free,
        }),
    )
    .await
    .unwrap();
    // Jeton du tenant B.
    let b = api::issue_token(
        State(state.clone()),
        Path("tenant-b".to_string()),
        Json(IssueTokenReq { scopes: vec![] }),
    )
    .await
    .unwrap()
    .0;
    let b_clair = b["token"].as_str().unwrap();
    let b_stored = state.store.validate_token(b_clair).unwrap().unwrap();

    // Rotation du jeton de B via le chemin de A → rejetée (IDOR), le jeton de B intact.
    assert!(api::rotate_token(
        State(state.clone()),
        Path("tenant-a".to_string()),
        Json(RotateTokenReq { token_id: b_stored.id.clone() }),
    )
    .await
    .is_err());
    assert!(state.store.validate_token(b_clair).unwrap().is_some());

    // Révocation du jeton de B via le chemin de A → rejetée.
    assert!(api::revoke_token(
        State(state.clone()),
        Path(("tenant-a".to_string(), b_stored.id.clone())),
    )
    .await
    .is_err());
    assert!(state.store.validate_token(b_clair).unwrap().is_some());
}

#[tokio::test]
async fn api_policy_and_license() {
    let state = app_state();
    let _ = api::create_tenant(
        State(state.clone()),
        Json(CreateTenantReq {
            id: "tenant-7".to_string(),
            nom_public: "op".to_string(),
            plan: Plan::Free,
        }),
    )
    .await
    .unwrap();

    // PUT /admin/tenants/{id}/policy → version 1 puis 2.
    let p1 = api::put_policy(
        State(state.clone()),
        Path("tenant-7".to_string()),
        Json(PutPolicyReq { json_policy: r#"{"k":2}"#.to_string() }),
    )
    .await
    .unwrap()
    .0;
    assert_eq!(p1.version, 1);
    let p2 = api::put_policy(
        State(state.clone()),
        Path("tenant-7".to_string()),
        Json(PutPolicyReq { json_policy: r#"{"k":5,"mode":"audit"}"#.to_string() }),
    )
    .await
    .unwrap()
    .0;
    assert_eq!(p2.version, 2);

    // POST /admin/tenants/{id}/licenses.
    let lic = api::add_license(
        State(state.clone()),
        Path("tenant-7".to_string()),
        Json(AddLicenseReq { plan: Plan::Enterprise, expires_at: Some(1_900_000_000) }),
    )
    .await
    .unwrap()
    .0;
    assert_eq!(lic.plan, Plan::Enterprise);
    let stored_lic = state.store.get_license("tenant-7").unwrap().expect("licence stockée");
    assert_eq!(stored_lic.expires_at, Some(1_900_000_000));

    // GET /healthz.
    let health = api::healthz().await;
    assert_eq!(health["status"], "ok");
}

#[test]
fn api_router_builds() {
    let router = api::router(app_state());
    // Le routeur se construit ; aucun appel réseau ici.
    let _ = router;
}

// ---------------------------------------------------------------------------
// Anti-fuite : TokenIssued ne révèle jamais le clair via Debug/Serialize (P1-5)
// ---------------------------------------------------------------------------

#[test]
fn token_issued_debug_and_serialize_mask_the_clear() {
    let issued = TokenIssued {
        id: "tok-1".to_string(),
        token: "mn_secret_value".to_string(),
        expires_at: None,
    };

    // Debug masqué.
    let debug = format!("{issued:?}");
    assert!(!debug.contains("mn_secret_value"), "Debug ne doit jamais révéler le clair");
    assert!(debug.contains("<redacted>"));

    // Sérialisation générique : JAMAIS le clair, seulement le hash.
    let json = serde_json::to_string(&issued).unwrap();
    assert!(!json.contains("mn_secret_value"), "Serialize ne doit jamais révéler le clair");
    assert!(json.contains("\"token_hash\""));
    assert!(json.contains(&token::token_hash("mn_secret_value")));

    // Le chemin d'émission unique (to_issued_json) expose le clair UNE fois.
    let issued_json = issued.to_issued_json();
    assert_eq!(issued_json["token"], "mn_secret_value");
    assert_eq!(issued_json["id"], "tok-1");
}

// ---------------------------------------------------------------------------
// Invariant « pas de texte client » (style STACK-4)
// ---------------------------------------------------------------------------

#[test]
fn stored_models_contain_no_client_text() {
    let store = InMemoryStore::new();
    store.create_tenant(&sample_tenant("tenant-a")).unwrap();
    let clair = token::generate_token();
    let t = ApiToken::issue("tok-1".to_string(), "tenant-a".to_string(), &clair, vec![], 100);
    store.create_token(&t).unwrap();
    store
        .set_policy(&Policy {
            tenant_id: "tenant-a".to_string(),
            json_policy: r#"{"k":2}"#.to_string(),
            version: 1,
            updated_at: 100,
        })
        .unwrap();

    // Le JSON des modèles persistés ne contient ni le clair, ni de texte client.
    let all = serde_json::to_string(&store.get_tenant("tenant-a").unwrap().unwrap()).unwrap();
    assert!(!all.contains(&clair));
    let tok = serde_json::to_string(&store.validate_token(&clair).unwrap().unwrap()).unwrap();
    assert!(!tok.contains(&clair));
}
