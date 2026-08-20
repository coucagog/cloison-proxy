//! CLOISON STACK-4 — `cloison-audit`
//!
//! Le premier produit livrable : **audit de conformité en observation seule**.
//! Le proxy détecte et **compte** les PII sans rien masquer ni casser. Chaque
//! requête auditée produit un **reçu signé** contenant **uniquement des
//! compteurs entiers** (jamais de texte, jamais de span, jamais de valeur
//! claire) ; les compteurs publiés respectent des **seuils d'agrégation
//! k-anonymes** (défaut k = 5) ; un **rapport de conformité** agrège les reçus
//! d'une période.
//!
//! # Séparation corpus (règle stricte)
//!
//! Le flux d'audit **n'alimente jamais** le pipeline CORPUS : un reçu ne
//! contient que des entiers, donc même un bug dans l'agrégateur ne peut pas
//! faire fuiter de texte vers le corpus. Le pipeline CORPUS (sources
//! publiques + synthétique + opt-in explicite, jamais le trafic de
//! production) est documenté séparément et reste hors de ce crate.
//!
//! # Modules
//!
//! - `error` : `AuditError` / `AuditResult`
//! - `receipt` : `Counters`, `ReceiptMessage`, `Receipt` (signature Ed25519
//!   sur un message canonique), encodage base64url pour le header client
//! - `k_anonymity` : seuils d'agrégation k-anonymes
//! - `report` : `ConformanceReport` (agrégats k-anonymes, jamais de texte)

#![warn(missing_docs)]

pub mod error;
pub mod k_anonymity;
pub mod receipt;
pub mod report;

// Ré-export du crate de signature : les consommateurs (proxy, tests,
// vérificateur hors-ligne) construisent/vérifient les clés sans dépendre
// directement de ed25519-dalek.
pub use ed25519_dalek;

pub use error::{AuditError, AuditResult};
pub use k_anonymity::KAnonymity;
pub use receipt::{Counters, Receipt, ReceiptMessage, AUDIT_SCHEMA_VERSION};
pub use report::ConformanceReport;
