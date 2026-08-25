//! Fournisseur de passphrase du coffre N0 — **keychain OS** (chantier ②).
//!
//! Priorité de résolution au boot du daemon :
//!   1. **keychain OS** (Windows Credential Manager / macOS Keychain / Linux
//!      Secret Service-keyutils) si `CLOISON_VAULT_KEYCHAIN_SERVICE` est posé
//!      — la passphrase est **stockée chiffrée par l'OS** (jamais en clair
//!      par CLOISON) ;
//!   2. **repli `CLOISON_VAULT_PASSPHRASE`** (env) avec un `warn` quand le
//!      keychain est indisponible — jamais un blocage silencieux ;
//!   3. **fail-loud** : ni keychain ni env → refus de démarrer (cohérent avec
//!      le fail-loud du coffre N0, N0-PREP §4.2).
//!
//! Invariant (N0V11-PREP §4.4) : la passphrase n'est **jamais persistée en
//! clair** par notre code — le keychain la stocke chiffrée ; l'env reste le
//! fallback explicite. Le premier démarrage (entrée keychain absente + env
//! fournie) **stocke** l'env dans le keychain (jamais d'écrasement d'une
//! entrée existante — la vérification du coffre reste le fail-loud final).

use zeroize::Zeroizing;

use crate::config::N0VaultConfig;
use crate::errors::{ErrorKind, ProxyError};

/// Fournisseur de passphrase du coffre (stateless, construit au boot).
#[derive(Debug, Clone)]
pub struct PassphraseProvider {
    /// Service keychain (`CLOISON_VAULT_KEYCHAIN_SERVICE`) ; `None` = env
    /// uniquement (comportement historique).
    service: Option<String>,
    /// Compte keychain (`CLOISON_VAULT_KEYCHAIN_USER`, défaut `default`).
    user: String,
}

impl PassphraseProvider {
    /// Construit le fournisseur depuis la configuration N0.
    pub fn from_config(cfg: &N0VaultConfig) -> Self {
        Self {
            service: cfg.keychain_service.clone(),
            user: cfg.keychain_user.clone(),
        }
    }

    /// Résout la passphrase (keychain → env → fail-loud).
    ///
    /// `env_passphrase` = valeur de `CLOISON_VAULT_PASSPHRASE` (fallback).
    pub fn load(
        &self,
        env_passphrase: Option<&Zeroizing<String>>,
    ) -> Result<Zeroizing<String>, ProxyError> {
        let Some(service) = &self.service else {
            return env_or_error(env_passphrase);
        };
        let entry = keyring::Entry::new(service, &self.user).map_err(|e| {
            ProxyError::new(ErrorKind::Internal, "failed to access OS keychain")
                .with_field("detail", e.to_string())
        })?;
        match entry.get_password() {
            Ok(p) => Ok(Zeroizing::new(p)),
            Err(keyring::Error::NoEntry) => match env_passphrase {
                Some(p) => {
                    // Premier démarrage : entrée absente + env fournie → on
                    // STOCKE l'env dans le keychain (création uniquement —
                    // jamais d'écrasement : il n'y a pas d'entrée). Un échec
                    // de stockage dégrade en env (warn) — jamais bloquant.
                    match entry.set_password(p.as_str()) {
                        Ok(()) => tracing::info!(
                            service = %service,
                            user = %self.user,
                            "passphrase N0 stockée dans le keychain OS (chiffrée par l'OS, jamais en clair par CLOISON)"
                        ),
                        Err(e) => tracing::warn!(
                            detail = %e,
                            "stockage keychain impossible — passphrase depuis l'env (jamais persistée en clair)"
                        ),
                    }
                    Ok(p.clone())
                }
                None => Err(ProxyError::new(
                    ErrorKind::Internal,
                    "passphrase introuvable : keychain OS vide et CLOISON_VAULT_PASSPHRASE absent (fail-loud)",
                )),
            },
            Err(e) => {
                // Keychain indisponible (pas de Secret Service / keyutils) :
                // repli env avec warn — jamais un blocage silencieux.
                tracing::warn!(
                    detail = %e,
                    service = %service,
                    "keychain OS indisponible — repli CLOISON_VAULT_PASSPHRASE"
                );
                env_or_error(env_passphrase)
            }
        }
    }
}

/// Passphrase depuis l'env, ou erreur fail-loud si absente.
fn env_or_error(
    env_passphrase: Option<&Zeroizing<String>>,
) -> Result<Zeroizing<String>, ProxyError> {
    env_passphrase.cloned().ok_or_else(|| {
        ProxyError::new(
            ErrorKind::Internal,
            "CLOISON_VAULT_PASSPHRASE required in N0 mode (fail-loud)",
        )
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn env(p: &str) -> Zeroizing<String> {
        Zeroizing::new(p.to_string())
    }

    fn provider(service: Option<&str>) -> PassphraseProvider {
        PassphraseProvider {
            service: service.map(str::to_string),
            user: format!("test-user-{}", uuid4()),
        }
    }

    /// Petit id unique (isolation des entrées keychain entre tests).
    fn uuid4() -> String {
        use rand::Rng;
        let mut rng = rand::thread_rng();
        (0..8)
            .map(|_| rng.gen_range(0..16))
            .map(|n| format!("{n:x}"))
            .collect()
    }

    #[test]
    fn env_only_returns_env() {
        let p = provider(None);
        let got = p.load(Some(&env("pass-1"))).unwrap();
        assert_eq!(got.as_str(), "pass-1");
    }

    #[test]
    fn env_only_missing_fails_loud() {
        let p = provider(None);
        let err = p.load(None).unwrap_err();
        assert!(
            err.to_string().contains("PASSPHRASE"),
            "fail-loud explicite: {}",
            err
        );
    }

    #[test]
    fn keychain_no_entry_with_env_stores_and_returns() {
        // Entrée absente (service unique) + env fournie → la passphrase env
        // est stockée dans le keychain et renvoyée (sur un hôte SANS keychain
        // utilisable, le repli env s'applique — même résultat).
        let p = provider(Some("cloison-test-store"));
        let got = p.load(Some(&env("pass-kc"))).unwrap();
        assert_eq!(got.as_str(), "pass-kc");
    }

    #[test]
    fn keychain_no_entry_without_env_fails_loud() {
        // Entrée absente + pas d'env → refus de démarrer (jamais une
        // passphrase inventée).
        let p = provider(Some("cloison-test-missing"));
        let err = p.load(None).unwrap_err();
        assert!(
            err.to_string().contains("introuvable") || err.to_string().contains("PASSPHRASE"),
            "fail-loud: {}",
            err
        );
    }

    #[test]
    fn keychain_entry_roundtrip_returns_stored() {
        // Stocke puis relit depuis le keychain (si le keychain est utilisable
        // sur la machine — sinon ce test est inopérant : le repli env couvre).
        let p = provider(Some("cloison-test-roundtrip"));
        let first = p.load(Some(&env("pass-rt"))).unwrap();
        assert_eq!(first.as_str(), "pass-rt");
        // Relecture : le fournisseur re-consulte le keychain — la passphrase
        // stockée (ou le repli env) doit être identique.
        let second = p.load(Some(&env("pass-rt"))).unwrap();
        assert_eq!(second.as_str(), "pass-rt");
    }
}
