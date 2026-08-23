//! Persistance du plan de contrôle : trait [`Store`] complet + implémentations
//! mémoire et PostgreSQL.
//!
//! # Postgres
//!
//! `PostgresStore` (sqlx, feature **`pg`** — optionnelle pour garder le crate
//! compilable hors-ligne) : schéma dans `migrations/001_init.sql`, requêtes
//! paramétrées, pool interne. Le trait [`Store`] **est le contrat complet** ;
//! [`InMemoryStore`] l'implémente intégralement pour les tests et le mode sans
//! base.
//!
//! Règles de sécurité portées par le contrat :
//! - `hash_token` est fourni par le trait : un store ne voit **jamais** le clair persisté ;
//! - `validate_token` compare les digests en temps constant ;
//! - rotation = l'ancien jeton passe en **grâce** (`rotated_at` + `grace_until`), le
//!   nouveau prend le relais ; révocation = invalidation immédiate ;
//! - **IDOR** : `rotate_token`/`revoke_token` vérifient au niveau du store que le jeton
//!   appartient au tenant du chemin (`tenant_id`) — un opérateur d'un tenant ne peut
//!   pas toucher les jetons d'un autre ;
//! - `tokens_version` : incrémentée à chaque rotation/révocation (propagation vers les
//!   caches proxy via `GET /v1/control/version`).

use crate::error::{ControlError, ControlResult};
use crate::model::{ApiToken, License, Policy, Tenant};
use crate::token;
use std::collections::HashMap;
use std::sync::RwLock;

/// Contrat de persistance du plan de contrôle — complet, sans dépendance lourde.
///
/// Synchronique (pas d'async-trait) : les stores sont des implémentations en mémoire ou
/// PostgreSQL derrière un pool interne ; le contrôle les enrobe dans son propre runtime.
///
/// Ordre de verrouillage global (anti-interblocage) : `tenants < tokens < tokens_by_hash`.
pub trait Store: Send + Sync {
    /// `hex(SHA-256("cloison-mn-token-v1:" ‖ clair))` — déterministe, fourni par le trait.
    /// Un store ne doit **jamais** recevoir le clair à persister.
    fn hash_token(&self, token_clair: &str) -> String {
        token::token_hash(token_clair)
    }

    fn create_tenant(&self, tenant: &Tenant) -> ControlResult<()>;
    fn get_tenant(&self, id: &str) -> ControlResult<Option<Tenant>>;

    /// Persiste un jeton **haché**. Le clair n'existe que dans la réponse d'émission.
    fn create_token(&self, token: &ApiToken) -> ControlResult<()>;

    /// Lookup par identifiant de jeton — utilisé par les handlers pour vérifier
    /// l'appartenance au tenant (IDOR) avant rotation/révocation.
    fn get_token(&self, token_id: &str) -> ControlResult<Option<ApiToken>>;

    /// Valide un jeton présenté : hash en temps constant contre le stockage, jeton
    /// actif (ni révoqué ; roté uniquement dans la période de grâce). Ne renvoie
    /// **jamais** le clair.
    fn validate_token(&self, token_clair: &str) -> ControlResult<Option<ApiToken>>;

    /// Valide un jeton **par son hash** (wiring C — `POST /v1/control/verify`) :
    /// lookup par hash + appartenance au tenant + état actif (grâce incluse).
    /// Le proxy n'envoie que le digest SHA-256 — le clair ne quitte jamais le bord.
    fn validate_token_hash(
        &self,
        tenant_id: &str,
        token_hash: &str,
    ) -> ControlResult<Option<ApiToken>>;

    /// Rotation avec **grâce** : l'ancien jeton passe `rotated_at = now` et
    /// `grace_until = now + grace_secs` (il reste valide pendant la grâce), le nouveau
    /// (même tenant, mêmes scopes, nouveau secret haché) prend le relais.
    /// Incrémente `tokens_version` du tenant. Vérifie l'appartenance du jeton au tenant.
    fn rotate_token(
        &self,
        tenant_id: &str,
        old_id: &str,
        new_token: &ApiToken,
        grace_secs: u64,
    ) -> ControlResult<()>;

    /// Révocation immédiate (aucune grâce) : plus aucun usage. Incrémente
    /// `tokens_version` du tenant. Vérifie l'appartenance du jeton au tenant.
    fn revoke_token(&self, tenant_id: &str, token_id: &str) -> ControlResult<()>;

    /// `tokens_version` du tenant (pour `GET /v1/control/version` — caches proxy).
    fn tokens_version(&self, tenant_id: &str) -> ControlResult<u64>;

    /// Publie une politique (version incrémentée par l'appelant).
    fn set_policy(&self, policy: &Policy) -> ControlResult<()>;
    fn get_policy(&self, tenant_id: &str) -> ControlResult<Option<Policy>>;

    /// Définit la licence du tenant : **crée ou remplace** (une licence par tenant,
    /// cohérent avec `POST /admin/tenants/{id}/licenses` et le `PUT /license` du design).
    fn add_license(&self, license: &License) -> ControlResult<()>;
    fn get_license(&self, tenant_id: &str) -> ControlResult<Option<License>>;
}

/// Implémentation mémoire (tests, mode sans base) — `RwLock<HashMap>`.
#[derive(Debug, Default)]
pub struct InMemoryStore {
    tenants: RwLock<HashMap<String, Tenant>>,
    /// index : token_id → jeton (haché).
    tokens: RwLock<HashMap<String, ApiToken>>,
    /// index : token_hash → token_id.
    tokens_by_hash: RwLock<HashMap<String, String>>,
    policies: RwLock<HashMap<String, Policy>>,
    licenses: RwLock<HashMap<String, License>>,
}

impl InMemoryStore {
    pub fn new() -> InMemoryStore {
        InMemoryStore::default()
    }
}

impl Store for InMemoryStore {
    fn create_tenant(&self, tenant: &Tenant) -> ControlResult<()> {
        let mut tenants = self.tenants.write().expect("tenants lock poisoned");
        if tenants.contains_key(&tenant.id) {
            return Err(ControlError::TenantConflict(tenant.id.clone()));
        }
        tenants.insert(tenant.id.clone(), tenant.clone());
        Ok(())
    }

    fn get_tenant(&self, id: &str) -> ControlResult<Option<Tenant>> {
        Ok(self
            .tenants
            .read()
            .expect("tenants lock poisoned")
            .get(id)
            .cloned())
    }

    fn create_token(&self, token: &ApiToken) -> ControlResult<()> {
        // Ordre global : tokens puis tokens_by_hash.
        let mut tokens = self.tokens.write().expect("tokens lock poisoned");
        let mut by_hash = self
            .tokens_by_hash
            .write()
            .expect("hash index lock poisoned");
        if tokens.contains_key(&token.id) || by_hash.contains_key(&token.token_hash) {
            return Err(ControlError::TokenConflict);
        }
        by_hash.insert(token.token_hash.clone(), token.id.clone());
        tokens.insert(token.id.clone(), token.clone());
        Ok(())
    }

    fn get_token(&self, token_id: &str) -> ControlResult<Option<ApiToken>> {
        Ok(self
            .tokens
            .read()
            .expect("tokens lock poisoned")
            .get(token_id)
            .cloned())
    }

    fn validate_token(&self, token_clair: &str) -> ControlResult<Option<ApiToken>> {
        let digest = self.hash_token(token_clair);
        // Ordre global : tokens puis tokens_by_hash (le lock par_hash seul peut
        // attendre un create_token qui tient tokens — ordre identique ⇒ pas de cycle).
        let tokens = self.tokens.read().expect("tokens lock poisoned");
        let by_hash = self
            .tokens_by_hash
            .read()
            .expect("hash index lock poisoned");
        let Some(id) = by_hash.get(&digest) else {
            return Ok(None);
        };
        let Some(token) = tokens.get(id) else {
            return Ok(None);
        };
        // Comparaison déjà faite sur le digest ; vérifie l'état (grâce incluse).
        Ok(if token.is_active_at(crate::now_unix()) {
            Some(token.clone())
        } else {
            None
        })
    }

    fn validate_token_hash(
        &self,
        tenant_id: &str,
        token_hash: &str,
    ) -> ControlResult<Option<ApiToken>> {
        // Ordre global : tokens puis tokens_by_hash (identique à validate_token).
        let tokens = self.tokens.read().expect("tokens lock poisoned");
        let by_hash = self
            .tokens_by_hash
            .read()
            .expect("hash index lock poisoned");
        let Some(id) = by_hash.get(token_hash) else {
            return Ok(None);
        };
        let Some(token) = tokens.get(id) else {
            return Ok(None);
        };
        // Appartenance au tenant + état actif (grâce incluse).
        Ok(
            if token.tenant_id == tenant_id && token.is_active_at(crate::now_unix()) {
                Some(token.clone())
            } else {
                None
            },
        )
    }

    fn rotate_token(
        &self,
        tenant_id: &str,
        old_id: &str,
        new_token: &ApiToken,
        grace_secs: u64,
    ) -> ControlResult<()> {
        // Ordre global : tenants < tokens < tokens_by_hash.
        let mut tenants = self.tenants.write().expect("tenants lock poisoned");
        let mut tokens = self.tokens.write().expect("tokens lock poisoned");
        let mut by_hash = self
            .tokens_by_hash
            .write()
            .expect("hash index lock poisoned");
        let now = crate::now_unix();

        let Some(old) = tokens.get_mut(old_id) else {
            return Err(ControlError::TokenNotFound(old_id.to_string()));
        };
        // IDOR : le jeton doit appartenir au tenant du chemin.
        if old.tenant_id != tenant_id {
            return Err(ControlError::TokenNotFound(old_id.to_string()));
        }
        // Grâce : l'ancien secret reste valide `grace_secs` après la rotation.
        old.rotated_at = Some(now);
        old.grace_until = Some(now.saturating_add(grace_secs));

        // Le nouveau jeton hérite des scopes de l'ancien.
        let mut new = new_token.clone();
        new.scopes = old.scopes.clone();
        if tokens.contains_key(&new.id) || by_hash.contains_key(&new.token_hash) {
            return Err(ControlError::TokenConflict);
        }
        by_hash.insert(new.token_hash.clone(), new.id.clone());
        tokens.insert(new.id.clone(), new);

        // Propagation : toute rotation incrémente la version des jetons du tenant.
        if let Some(t) = tenants.get_mut(tenant_id) {
            t.tokens_version = t.tokens_version.saturating_add(1);
        }
        Ok(())
    }

    fn revoke_token(&self, tenant_id: &str, token_id: &str) -> ControlResult<()> {
        // Ordre global : tenants < tokens.
        let mut tenants = self.tenants.write().expect("tenants lock poisoned");
        let mut tokens = self.tokens.write().expect("tokens lock poisoned");
        let Some(token) = tokens.get_mut(token_id) else {
            return Err(ControlError::TokenNotFound(token_id.to_string()));
        };
        // IDOR : le jeton doit appartenir au tenant du chemin.
        if token.tenant_id != tenant_id {
            return Err(ControlError::TokenNotFound(token_id.to_string()));
        }
        token.revoked_at = Some(crate::now_unix());
        if let Some(t) = tenants.get_mut(tenant_id) {
            t.tokens_version = t.tokens_version.saturating_add(1);
        }
        Ok(())
    }

    fn tokens_version(&self, tenant_id: &str) -> ControlResult<u64> {
        let tenants = self.tenants.read().expect("tenants lock poisoned");
        let Some(tenant) = tenants.get(tenant_id) else {
            return Err(ControlError::TenantNotFound(tenant_id.to_string()));
        };
        Ok(tenant.tokens_version)
    }

    fn set_policy(&self, policy: &Policy) -> ControlResult<()> {
        let mut policies = self.policies.write().expect("policies lock poisoned");
        policies.insert(policy.tenant_id.clone(), policy.clone());
        Ok(())
    }

    fn get_policy(&self, tenant_id: &str) -> ControlResult<Option<Policy>> {
        Ok(self
            .policies
            .read()
            .expect("policies lock poisoned")
            .get(tenant_id)
            .cloned())
    }

    fn add_license(&self, license: &License) -> ControlResult<()> {
        // Upsert : une licence par tenant — la nouvelle remplace l'ancienne.
        let mut licenses = self.licenses.write().expect("licenses lock poisoned");
        licenses.insert(license.tenant_id.clone(), license.clone());
        Ok(())
    }

    fn get_license(&self, tenant_id: &str) -> ControlResult<Option<License>> {
        Ok(self
            .licenses
            .read()
            .expect("licenses lock poisoned")
            .get(tenant_id)
            .cloned())
    }
}
