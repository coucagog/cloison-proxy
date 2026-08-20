//! # cloison-control — plan de contrôle aveugle (STACK-5)
//!
//! Gestion des locataires, licences, politiques et jetons `mn_`, plus le **pipeline
//! control → ledger** (STACK-5 §5) :
//!
//! - **jetons hachés** : le stockage ne contient que `token_hash = SHA-256(domaine ‖ clair)`,
//!   comparaison en temps constant ; le clair n'existe que dans la réponse d'émission ;
//! - **zéro PII** : ni texte utilisateur, ni session, ni valeur — uniquement des
//!   identifiants opérateur, des hash et des compteurs ;
//! - **contresignature** : [`contersign::contresigner_reçu`] vérifie `sig_agent` sur
//!   `receipt.signing_bytes()` (JSON canonique STACK-4) puis ajoute `sig_control`
//!   (Ed25519) — double preuve indépendante ;
//! - **ingest** : `POST /v1/control/ingest` vérifie les reçus, applique le k-anonymat
//!   (`cloison-audit`), construit un payload redacté, crée l'entrée contresignée et
//!   l'append au journal (`cloison-ledger`) — persistant via `CLOISON_LEDGER_FILE`
//!   (JSONL append-only) ou mémoire (tests) ;
//! - **IDOR** : rotation/révocation vérifient l'appartenance du jeton au tenant du chemin ;
//! - **propagation** : `tokens_version` par tenant + `GET /v1/control/version`
//!   (ETag/long-poll pour les caches proxy) ; rotation avec période de grâce
//!   (`CLOISON_ROTATION_GRACE_SECONDS`, défaut 300).
//!
//! Persistance : trait [`store::Store`] complet + [`store::InMemoryStore`] (tests).
//! `PostgresStore` (sqlx) est documenté en TODO STACK-7 dans `store.rs`.

pub mod api;
pub mod contersign;
pub mod error;
pub mod model;
pub mod store;
pub mod token;

pub use error::{ControlError, ControlResult};
pub use model::{
    ApiToken, License, LicenseLimites, Plan, Policy, Tenant, TenantStatut, TokenIssued,
};
pub use store::{InMemoryStore, Store};

use std::time::{SystemTime, UNIX_EPOCH};

/// Horodatage Unix courant en secondes (UTC).
pub fn now_unix() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}
