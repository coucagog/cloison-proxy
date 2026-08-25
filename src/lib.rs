//! CLOISON STACK-3 — `cloison-proxy`.
//!
//! Passerelle compatible OpenAI qui s'intercale entre une interface IA et un
//! fournisseur LLM :
//!
//! - `POST /v1/chat/completions` (non-stream puis stream SSE buffer-and-scan) ;
//! - `POST /v1/completions` (legacy, non-stream) ;
//! - `GET  /v1/models` (pass-through).
//!
//! Auth par clé composite `Authorization: Bearer mn_<jeton_acces>.<cle_amont>`
//! (découpage sur le premier point, la clé amont est transmise telle quelle au
//! fournisseur). À l'aller : tokenisation PII via `cloison-core` (STACK-2).
//! Au retour : restauration des jetons émis par la requête en cours uniquement
//! (registre d'émission par requête + MAC vérifié). Fail-loud : jeton non
//! résoluble → marqueur neutre + compteur.

pub mod auth;
pub mod config;
pub mod control;
pub mod detect;
pub mod engine;
pub mod errors;
pub mod fsperm;
pub mod handlers;
pub mod light_ner;
pub mod openai;
pub mod passphrase;
pub mod routes;
pub mod stream;
pub mod upstream;
