//! Tests d'intégration `PostgresStore` (feature `pg`).
//!
//! Nécessitent une base réelle : `CLOISON_DATABASE_URL=postgres://…` — sans
//! cette variable, les tests sont **ignorés** (la CI et les runs hors-ligne
//! n'ont pas besoin de PostgreSQL ; InMemoryStore couvre le contrat).
//!
//! ```bash
//! cargo test -p cloison-control --features pg --test postgres_store
//!   CLOISON_DATABASE_URL=postgres://cloison:cloison@localhost:5432/cloison \
//!   cargo test -p cloison-control --features pg --test postgres_store -- --ignored
//! ```

#![cfg(feature = "pg")]

use cloison_control::error::ControlError;
use cloison_control::model::{ApiToken, License, LicenseLimites, Plan, Policy, Tenant, TenantStatut};
use cloison_control::postgres::PostgresStore;
use cloison_control::store::Store;

fn tenant(id: &str) -> Tenant {
    Tenant {
        id: id.to_string(),
        nom_public: format!("tenant-{id}"),
        statut: TenantStatut::Actif,
        created_at: 1_700_000_000,
        tokens_version: 0,
    }
}

fn token(tenant_id: &str, id: &str, clair: &str) -> ApiToken {
    ApiToken::issue(
        id.to_string(),
        tenant_id.to_string(),
        clair,
        vec!["chat".to_string()],
        1_700_000_100,
    )
}

async fn connect() -> PostgresStore {
    let url = std::env::var("CLOISON_DATABASE_URL")
        .expect("CLOISON_DATABASE_URL requis pour les tests PostgresStore");
    PostgresStore::connect(&url, 2).await.expect("connexion + schéma")
}

#[tokio::test(flavor = "multi_thread")]
#[ignore = "nécessite CLOISON_DATABASE_URL (base PostgreSQL réelle)"]
async fn tenant_token_license_policy_roundtrip() {
    let store = connect().await;

    // Tenant
    assert!(store.create_tenant(&tenant("t1")).is_ok());
    assert!(matches!(
        store.create_tenant(&tenant("t1")),
        Err(ControlError::TenantConflict(_))
    ));
    let t = store.get_tenant("t1").unwrap().expect("tenant présent");
    assert_eq!(t.nom_public, "tenant-t1");

    // Jeton : seul le hash est persisté — le clair ne doit apparaître nulle part.
    let tk = token("t1", "tok-1", "mn_cleartext-test");
    store.create_token(&tk).unwrap();
    let fetched = store.get_token("tok-1").unwrap().expect("jeton présent");
    assert_eq!(fetched.token_hash, tk.token_hash);
    assert_ne!(fetched.token_hash, "mn_cleartext-test");

    // Validation par le clair (temps constant sur le digest).
    let valid = store.validate_token("mn_cleartext-test").unwrap().expect("jeton valide");
    assert_eq!(valid.id, "tok-1");
    assert!(store.validate_token("mn_mauvais-clair").unwrap().is_none());

    // Politique
    let pol = Policy {
        tenant_id: "t1".into(),
        json_policy: r#"{"detectors":{"NOM":"on"}}"#.into(),
        version: 1,
        updated_at: 1_700_000_200,
    };
    store.set_policy(&pol).unwrap();
    let got = store.get_policy("t1").unwrap().expect("politique présente");
    assert_eq!(got.json_policy, pol.json_policy);

    // Licence (upsert)
    let lic = License {
        tenant_id: "t1".into(),
        plan: Plan::Pro,
        limites: LicenseLimites { max_requests_per_day: 5000, max_tokens: 32 },
        expires_at: Some(1_800_000_000),
        created_at: 1_700_000_300,
    };
    store.add_license(&lic).unwrap();
    let got_lic = store.get_license("t1").unwrap().expect("licence présente");
    assert_eq!(got_lic.plan, Plan::Pro);
    assert_eq!(got_lic.limites.max_requests_per_day, 5000);
}

#[tokio::test(flavor = "multi_thread")]
#[ignore = "nécessite CLOISON_DATABASE_URL (base PostgreSQL réelle)"]
async fn rotate_and_revoke_with_idor() {
    let store = connect().await;
    store.create_tenant(&tenant("t2")).unwrap();
    store.create_token(&token("t2", "tok-a", "mn_old")).unwrap();
    assert!(store.tokens_version("t2").is_ok());

    // IDOR : un jeton d'un autre tenant n'est pas touchable.
    store.create_tenant(&tenant("t3")).unwrap();
    assert!(matches!(
        store.rotate_token("t3", "tok-a", &token("t3", "tok-b", "mn_new"), 300),
        Err(ControlError::TokenNotFound(_))
    ));

    // Rotation avec grâce : l'ancien reste valide pendant la grâce.
    store.rotate_token("t2", "tok-a", &token("t2", "tok-c", "mn_new"), 300).unwrap();
    assert!(store.validate_token("mn_old").unwrap().is_some(), "grâce en cours");
    assert!(store.validate_token("mn_new").unwrap().is_some());
    let v = store.tokens_version("t2").unwrap();
    assert_eq!(v, 1, "rotation incrémente tokens_version");

    // Révocation immédiate.
    store.revoke_token("t2", "tok-c").unwrap();
    assert!(store.validate_token("mn_new").unwrap().is_none(), "révoqué = invalide");
    assert_eq!(store.tokens_version("t2").unwrap(), 2);
}
