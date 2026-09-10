//! Bootstrap : config → état → routeur → serveur.
//!
//! Port par défaut : 8787 (`CLOISON_PROXY_PORT` ou `CLOISON_LISTEN_ADDR`).

use std::process::ExitCode;
use std::sync::Arc;

use cloison_proxy::{config, handlers::AppState, routes};

#[tokio::main]
async fn main() -> ExitCode {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("cloison_proxy=info")),
        )
        .init();

    // S8 (rapport client 09/09 §11.4) : référence d'environnement MACHINE —
    // `--help-env` imprime les variables + défauts (source = code, anti-dérive
    // docs/code) puis sort.
    if std::env::args().any(|a| a == "--help-env") {
        print_env_help();
        return ExitCode::SUCCESS;
    }

    // Dispatch de rôle (dette STACK-9 « CLOISON_ROLE non lu ») : ce binaire
    // EST le rôle `edge`. `CLOISON_ROLE` (posé par le compose) est vérifié —
    // une valeur autre que `edge` (ex. `control`) échoue BRUYAMMENT au boot
    // plutôt que de servir silencieusement le mauvais rôle dans un conteneur.
    // Absent = défaut `edge` (compatibilité dev/harnais). La charte §5.1
    // « une même image joue les deux rôles » reste une décision écartée :
    // deux binaires distincts (le contrôle exige la feature `pg` que l'edge
    // ne doit pas embarquer — surface d'attaque et taille d'image).
    match std::env::var("CLOISON_ROLE").ok().as_deref() {
        None | Some("edge") => {}
        Some(other) => {
            tracing::error!(
                role = %other,
                "CLOISON_ROLE attend `edge` pour ce binaire (cloison-proxy) — \
                 rôle incompatible, refus de démarrer"
            );
            return ExitCode::FAILURE;
        }
    }

    let config = match config::load() {
        Ok(c) => c,
        Err(e) => {
            tracing::error!(error = %e, "configuration error");
            return ExitCode::FAILURE;
        }
    };

    let state = match AppState::new(&config) {
        Ok(s) => Arc::new(s),
        Err(e) => {
            tracing::error!(error = %e, "failed to initialize application state");
            return ExitCode::FAILURE;
        }
    };

    // Wiring C — tâches de fond (ingest des reçus d'audit, long-poll des
    // versions) : lancées uniquement quand le contrôle est configuré.
    state.start_background_tasks();

    let listener = match tokio::net::TcpListener::bind(config.listen_addr).await {
        Ok(l) => l,
        Err(e) => {
            tracing::error!(addr = %config.listen_addr, error = %e, "failed to bind listener");
            return ExitCode::FAILURE;
        }
    };

    tracing::info!(addr = %config.listen_addr, mock_mode = config.mock_mode, "cloison-proxy listening");

    if let Err(e) = axum::serve(listener, routes::router(state)).await {
        tracing::error!(error = %e, "server terminated with error");
        return ExitCode::FAILURE;
    }
    ExitCode::SUCCESS
}

/// S8 — référence d'environnement machine (source = code, jamais une doc
/// externe). Garder cette liste alignée sur `config::load()`.
fn print_env_help() {
    println!(
        "CLOISON — variables d'environnement (défauts du binaire)\n\
         \n\
         Écoute :\n\
         \x20 CLOISON_LISTEN_ADDR            adresse d'écoute (défaut : 0.0.0.0:8787 en edge,\n\
         \x20                                 127.0.0.1:8787 en mode N0 — local par défaut)\n\
         \x20 CLOISON_PROXY_PORT             port seul (alternative)\n\
         \n\
         Amont :\n\
         \x20 CLOISON_UPSTREAM_BASE_URL      URL de base du fournisseur LLM (requis hors mock)\n\
         \x20 CLOISON_UPSTREAM_CHAT_PATH     chemin chat/completions (défaut /v1/chat/completions)\n\
         \x20 CLOISON_UPSTREAM_COMPLETIONS_PATH  chemin legacy (défaut /v1/completions)\n\
         \x20 CLOISON_UPSTREAM_MODELS_PATH   chemin models (défaut /v1/models)\n\
         \x20 CLOISON_UPSTREAM_CONNECT_TIMEOUT_MS  connexion (défaut 5000)\n\
         \x20 CLOISON_UPSTREAM_TIMEOUT_MS    requête amont (défaut 300000 — modèles thinking lents)\n\
         \x20 CLOISON_MAX_BODY_BYTES         corps entrant (défaut 8388608 = 8 MiB)\n\
         \n\
         Auth / session :\n\
         \x20 CLOISON_EXPECTED_ACCESS_TOKEN  jeton mn_ local (comparaison temps constant)\n\
         \x20 CLOISON_TENANT_KEY_HEX         clé locataire 32 octets en hex (requise hors mock)\n\
         \x20 CLOISON_SESSION_SALT_HEX       sel de session 16 octets (32 hex)\n\
         \n\
         Détection / politique :\n\
         \x20 CLOISON_NER_MODEL_ONNX         modèle NER léger (avec CLOISON_NER_TOKENIZER)\n\
         \x20 CLOISON_NER_TOKENIZER          tokenizer HF du NER léger\n\
         \x20 CLOISON_ONNX_LIB               lib onnxruntime (load-dynamic)\n\
         \x20 CLOISON_NER_THRESHOLD          seuil NER (défaut 0.70)\n\
         \x20 CLOISON_DISABLE_DETECTORS      classes désactivées, virgules : email,phone,cni,\n\
         \x20                                 creditcard,ip,date,person,location,passport,\n\
         \x20                                 driverlicense,matricule,nom_sn,ville_sn\n\
         \x20 CLOISON_GEO_WHITELIST          noms de pays jamais masqués (défaut 1)\n\
         \x20 CLOISON_RESTORE_BARE_INNARDS   restauration des intérieurs de jetons nus (défaut 1)\n\
         \x20 CLOISON_REALISTIC_FAKE         faux réaliste irréversible (défaut 0)\n\
         \n\
         Audit (observe-only, opt-in) :\n\
         \x20 CLOISON_AUDIT_MODE             1/true/yes/on = observe-only (défaut 0 = masquage actif)\n\
         \x20 CLOISON_AUDIT_KEYS             fichier clé Ed25519 (0600, générée si absente)\n\
         \x20 CLOISON_AUDIT_K                seuil k-anonyme (défaut 5, ≥ 2)\n\
         \x20 CLOISON_AUDIT_LEDGER_FILE      journal des reçus (JSONL 0600)\n\
         \n\
         N0 (coffre local) :\n\
         \x20 CLOISON_VAULT_PATH             active le mode N0 (coffre redb chiffré)\n\
         \x20 CLOISON_VAULT_PASSPHRASE       passphrase du coffre (jamais persistée)\n\
         \x20 CLOISON_VAULT_KEYCHAIN_SERVICE keychain OS (à la place de la passphrase env)\n\
         \x20 CLOISON_VAULT_TTL_SECS         TTL du coffre (défaut 604800)\n\
         \x20 CLOISON_ALIAS_EXPANSION        alias intra-session (défaut 1)\n\
         \x20 CLOISON_QUASI_ID_GAUGE         jauge quasi-id (défaut 0)\n\
         \x20 CLOISON_QUASI_ID_THRESHOLD     seuil de la jauge (défaut 0.5)\n\
         \x20 CLOISON_ALIAS_MAX_MENTIONS     borne de session (défaut 200)\n\
         \n\
         Stream :\n\
         \x20 CLOISON_STREAM_MAX_TOKEN_LEN   borne tampon/sentinelle (défaut 64, plafond 256)\n\
         \x20 CLOISON_STREAM_NEUTRAL_MARKER  marqueur fail-loud (défaut [REDACTED])\n\
         \x20 CLOISON_STREAM_KEEP_ALIVE_MS   keep-alive SSE (défaut 15000)\n\
         \n\
         Contrôle (wiring C, optionnel) :\n\
         \x20 CLOISON_CONTROL_URL            URL du plan de contrôle\n\
         \x20 CLOISON_TENANT_ID              identifiant de tenant (défaut default)\n\
         \x20 CLOISON_ROLE                   edge uniquement pour ce binaire\n"
    );
}
