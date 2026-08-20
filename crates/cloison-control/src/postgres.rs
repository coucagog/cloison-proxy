//! Persistance PostgreSQL du plan de contrôle (`feature = "pg"`).
//!
//! Implémente [`Store`] derrière un pool `sqlx` — **0 PII** : les jetons ne
//! sont jamais persistés en clair (hash SHA-256 uniquement), le schéma est
//! celui de `migrations/001_init.sql`. Le trait [`Store`] est synchronique :
//! les appels sqlx sont exécutés via `block_in_place` sur le pool (le contrôle
//! tourne sur un runtime multi-thread, cf. `main.rs`).
//!
//! La feature est **optionnelle** : sans elle, le crate compile hors-ligne
//! (décision STACK-5 — sqlx est une chaîne de compilation lourde).

use crate::error::{ControlError, ControlResult};
use crate::model::{ApiToken, License, Policy, Tenant};
use crate::store::Store;
use sqlx::postgres::{PgPool, PgPoolOptions};
use std::time::Duration;

/// Exécute une future sqlx sur le pool depuis un contexte synchrone.
fn block_on<F: std::future::Future>(fut: F) -> F::Output {
    let handle = tokio::runtime::Handle::current();
    tokio::task::block_in_place(|| handle.block_on(fut))
}

/// Store PostgreSQL — pool interne, requêtes paramétrées (jamais de
/// concaténation SQL). Les contraintes d'intégrité (IDOR, conflits) sont
/// portées par les requêtes elles-mêmes.
pub struct PostgresStore {
    pool: PgPool,
}

impl PostgresStore {
    /// Crée le pool et applique le schéma (`migrations/001_init.sql`).
    pub async fn connect(database_url: &str, max_connections: u32) -> ControlResult<Self> {
        let pool = PgPoolOptions::new()
            .max_connections(max_connections.max(1))
            .acquire_timeout(Duration::from_secs(10))
            .connect(database_url)
            .await
            .map_err(|e| ControlError::Store(format!("postgres connect: {e}")))?;
        // Schéma idempotent : exécuté en protocole simple (multi-commandes).
        sqlx::raw_sql(include_str!("../migrations/001_init.sql"))
            .execute(&pool)
            .await
            .map_err(|e| ControlError::Store(format!("postgres migrate: {e}")))?;
        Ok(Self { pool })
    }
}

impl Store for PostgresStore {
    fn create_tenant(&self, tenant: &Tenant) -> ControlResult<()> {
        block_on(async {
            sqlx::query(
                "INSERT INTO tenants (id, nom_public, statut, created_at, tokens_version)
                 VALUES ($1, $2, $3, $4, $5)",
            )
            .bind(&tenant.id)
            .bind(&tenant.nom_public)
            .bind(statut_sql(tenant.statut))
            .bind(tenant.created_at as i64)
            .bind(tenant.tokens_version as i64)
            .execute(&self.pool)
            .await
            .map_err(map_pg_conflict(|| ControlError::TenantConflict(tenant.id.clone())))?;
            Ok(())
        })
    }

    fn get_tenant(&self, id: &str) -> ControlResult<Option<Tenant>> {
        block_on(async {
            let row = sqlx::query_as::<_, TenantRow>(
                "SELECT id, nom_public, statut, created_at, tokens_version FROM tenants WHERE id = $1",
            )
            .bind(id)
            .fetch_optional(&self.pool)
            .await
            .map_err(pg_err)?;
            Ok(row.map(TenantRow::into_model))
        })
    }

    fn create_token(&self, token: &ApiToken) -> ControlResult<()> {
        block_on(async {
            sqlx::query(
                "INSERT INTO api_tokens (id, tenant_id, token_hash, scopes, created_at, rotated_at, grace_until, revoked_at)
                 VALUES ($1, $2, $3, $4, $5, $6, $7, $8)",
            )
            .bind(&token.id)
            .bind(&token.tenant_id)
            .bind(&token.token_hash)
            .bind(serde_json::to_string(&token.scopes).unwrap_or_else(|_| "[]".into()))
            .bind(token.created_at as i64)
            .bind(token.rotated_at.map(|v| v as i64))
            .bind(token.grace_until.map(|v| v as i64))
            .bind(token.revoked_at.map(|v| v as i64))
            .execute(&self.pool)
            .await
            .map_err(map_pg_conflict(|| ControlError::TokenConflict))?;
            Ok(())
        })
    }

    fn get_token(&self, token_id: &str) -> ControlResult<Option<ApiToken>> {
        block_on(async {
            let row = sqlx::query_as::<_, ApiTokenRow>(
                "SELECT id, tenant_id, token_hash, scopes, created_at, rotated_at, grace_until, revoked_at
                 FROM api_tokens WHERE id = $1",
            )
            .bind(token_id)
            .fetch_optional(&self.pool)
            .await
            .map_err(pg_err)?;
            Ok(row.map(ApiTokenRow::into_model))
        })
    }

    fn validate_token(&self, token_clair: &str) -> ControlResult<Option<ApiToken>> {
        let digest = self.hash_token(token_clair);
        let now = crate::now_unix();
        block_on(async {
            let row = sqlx::query_as::<_, ApiTokenRow>(
                "SELECT id, tenant_id, token_hash, scopes, created_at, rotated_at, grace_until, revoked_at
                 FROM api_tokens WHERE token_hash = $1",
            )
            .bind(&digest)
            .fetch_optional(&self.pool)
            .await
            .map_err(pg_err)?;
            let Some(token) = row.map(ApiTokenRow::into_model) else {
                return Ok(None);
            };
            // État : ni révoqué, ni roté hors grâce (même règle que InMemoryStore).
            Ok(if token.is_active_at(now) { Some(token) } else { None })
        })
    }

    fn rotate_token(
        &self,
        tenant_id: &str,
        old_id: &str,
        new_token: &ApiToken,
        grace_secs: u64,
    ) -> ControlResult<()> {
        let now = crate::now_unix();
        block_on(async {
            // IDOR + existence en une requête : l'ancien jeton doit appartenir au tenant.
            let updated = sqlx::query(
                "UPDATE api_tokens
                 SET rotated_at = $3, grace_until = $4
                 WHERE id = $1 AND tenant_id = $2 AND revoked_at IS NULL",
            )
            .bind(old_id)
            .bind(tenant_id)
            .bind(now as i64)
            .bind(now.saturating_add(grace_secs) as i64)
            .execute(&self.pool)
            .await
            .map_err(pg_err)?
            .rows_affected();
            if updated == 0 {
                return Err(ControlError::TokenNotFound(old_id.to_string()));
            }
            // Le nouveau jeton hérite des scopes de l'ancien (règle InMemoryStore).
            let scopes_json: String = sqlx::query_scalar(
                "SELECT scopes FROM api_tokens WHERE id = $1",
            )
            .bind(old_id)
            .fetch_one(&self.pool)
            .await
            .map_err(pg_err)?;
            let mut new = new_token.clone();
            new.scopes = parse_scopes(scopes_json);
            sqlx::query(
                "INSERT INTO api_tokens (id, tenant_id, token_hash, scopes, created_at, rotated_at, grace_until, revoked_at)
                 VALUES ($1, $2, $3, $4, $5, $6, $7, $8)",
            )
            .bind(&new.id)
            .bind(&new.tenant_id)
            .bind(&new.token_hash)
            .bind(serde_json::to_string(&new.scopes).unwrap_or_else(|_| "[]".into()))
            .bind(new.created_at as i64)
            .bind(new.rotated_at.map(|v| v as i64))
            .bind(new.grace_until.map(|v| v as i64))
            .bind(new.revoked_at.map(|v| v as i64))
            .execute(&self.pool)
            .await
            .map_err(map_pg_conflict(|| ControlError::TokenConflict))?;
            // Propagation : version des jetons du tenant.
            sqlx::query("UPDATE tenants SET tokens_version = tokens_version + 1 WHERE id = $1")
                .bind(tenant_id)
                .execute(&self.pool)
                .await
                .map_err(pg_err)?;
            Ok(())
        })
    }

    fn revoke_token(&self, tenant_id: &str, token_id: &str) -> ControlResult<()> {
        block_on(async {
            let updated = sqlx::query(
                "UPDATE api_tokens SET revoked_at = $3
                 WHERE id = $1 AND tenant_id = $2 AND revoked_at IS NULL",
            )
            .bind(token_id)
            .bind(tenant_id)
            .bind(crate::now_unix() as i64)
            .execute(&self.pool)
            .await
            .map_err(pg_err)?
            .rows_affected();
            if updated == 0 {
                return Err(ControlError::TokenNotFound(token_id.to_string()));
            }
            sqlx::query("UPDATE tenants SET tokens_version = tokens_version + 1 WHERE id = $1")
                .bind(tenant_id)
                .execute(&self.pool)
                .await
                .map_err(pg_err)?;
            Ok(())
        })
    }

    fn tokens_version(&self, tenant_id: &str) -> ControlResult<u64> {
        block_on(async {
            let v: Option<i64> = sqlx::query_scalar(
                "SELECT tokens_version FROM tenants WHERE id = $1",
            )
            .bind(tenant_id)
            .fetch_optional(&self.pool)
            .await
            .map_err(pg_err)?;
            v.map(|v| v as u64)
                .ok_or_else(|| ControlError::TenantNotFound(tenant_id.to_string()))
        })
    }

    fn set_policy(&self, policy: &Policy) -> ControlResult<()> {
        block_on(async {
            sqlx::query(
                "INSERT INTO policies (tenant_id, json_policy, version, updated_at)
                 VALUES ($1, $2, $3, $4)
                 ON CONFLICT (tenant_id) DO UPDATE
                 SET json_policy = EXCLUDED.json_policy,
                     version = EXCLUDED.version,
                     updated_at = EXCLUDED.updated_at",
            )
            .bind(&policy.tenant_id)
            .bind(&policy.json_policy)
            .bind(policy.version as i64)
            .bind(policy.updated_at as i64)
            .execute(&self.pool)
            .await
            .map_err(pg_err)?;
            Ok(())
        })
    }

    fn get_policy(&self, tenant_id: &str) -> ControlResult<Option<Policy>> {
        block_on(async {
            let row = sqlx::query_as::<_, PolicyRow>(
                "SELECT tenant_id, json_policy, version, updated_at FROM policies WHERE tenant_id = $1",
            )
            .bind(tenant_id)
            .fetch_optional(&self.pool)
            .await
            .map_err(pg_err)?;
            Ok(row.map(PolicyRow::into_model))
        })
    }

    fn add_license(&self, license: &License) -> ControlResult<()> {
        block_on(async {
            sqlx::query(
                "INSERT INTO licenses (tenant_id, plan, max_requests_per_day, max_tokens, expires_at, created_at)
                 VALUES ($1, $2, $3, $4, $5, $6)
                 ON CONFLICT (tenant_id) DO UPDATE
                 SET plan = EXCLUDED.plan,
                     max_requests_per_day = EXCLUDED.max_requests_per_day,
                     max_tokens = EXCLUDED.max_tokens,
                     expires_at = EXCLUDED.expires_at,
                     created_at = EXCLUDED.created_at",
            )
            .bind(&license.tenant_id)
            .bind(plan_sql(license.plan))
            .bind(license.limites.max_requests_per_day as i64)
            .bind(license.limites.max_tokens as i32)
            .bind(license.expires_at.map(|v| v as i64))
            .bind(license.created_at as i64)
            .execute(&self.pool)
            .await
            .map_err(pg_err)?;
            Ok(())
        })
    }

    fn get_license(&self, tenant_id: &str) -> ControlResult<Option<License>> {
        block_on(async {
            let row = sqlx::query_as::<_, LicenseRow>(
                "SELECT tenant_id, plan, max_requests_per_day, max_tokens, expires_at, created_at
                 FROM licenses WHERE tenant_id = $1",
            )
            .bind(tenant_id)
            .fetch_optional(&self.pool)
            .await
            .map_err(pg_err)?;
            Ok(row.map(LicenseRow::into_model))
        })
    }
}

// ---------------------------------------------------------------------------
// Rows sqlx (types scalaires — pas de PII)
// ---------------------------------------------------------------------------

#[derive(sqlx::FromRow)]
struct TenantRow {
    id: String,
    nom_public: String,
    statut: String,
    created_at: i64,
    tokens_version: i64,
}

impl TenantRow {
    fn into_model(self) -> Tenant {
        Tenant {
            id: self.id,
            nom_public: self.nom_public,
            statut: statut_from_sql(&self.statut),
            created_at: self.created_at as u64,
            tokens_version: self.tokens_version as u64,
        }
    }
}

#[derive(sqlx::FromRow)]
struct ApiTokenRow {
    id: String,
    tenant_id: String,
    token_hash: String,
    scopes: String,
    created_at: i64,
    rotated_at: Option<i64>,
    grace_until: Option<i64>,
    revoked_at: Option<i64>,
}

impl ApiTokenRow {
    fn into_model(self) -> ApiToken {
        ApiToken {
            id: self.id,
            tenant_id: self.tenant_id,
            token_hash: self.token_hash,
            scopes: parse_scopes(self.scopes),
            created_at: self.created_at as u64,
            rotated_at: self.rotated_at.map(|v| v as u64),
            grace_until: self.grace_until.map(|v| v as u64),
            revoked_at: self.revoked_at.map(|v| v as u64),
        }
    }
}

#[derive(sqlx::FromRow)]
struct PolicyRow {
    tenant_id: String,
    json_policy: String,
    version: i64,
    updated_at: i64,
}

impl PolicyRow {
    fn into_model(self) -> Policy {
        Policy {
            tenant_id: self.tenant_id,
            json_policy: self.json_policy,
            version: self.version as u64,
            updated_at: self.updated_at as u64,
        }
    }
}

#[derive(sqlx::FromRow)]
struct LicenseRow {
    tenant_id: String,
    plan: String,
    max_requests_per_day: i64,
    max_tokens: i32,
    expires_at: Option<i64>,
    created_at: i64,
}

impl LicenseRow {
    fn into_model(self) -> License {
        use crate::model::LicenseLimites;
        License {
            tenant_id: self.tenant_id,
            plan: plan_from_sql(&self.plan),
            limites: LicenseLimites {
                max_requests_per_day: self.max_requests_per_day as u64,
                max_tokens: self.max_tokens as u32,
            },
            expires_at: self.expires_at.map(|v| v as u64),
            created_at: self.created_at as u64,
        }
    }
}

// ---------------------------------------------------------------------------
// Aides sérialisation / erreurs
// ---------------------------------------------------------------------------

fn statut_sql(s: crate::model::TenantStatut) -> &'static str {
    match s {
        crate::model::TenantStatut::Actif => "actif",
        crate::model::TenantStatut::Suspendu => "suspendu",
        crate::model::TenantStatut::Supprime => "supprime",
    }
}

fn statut_from_sql(s: &str) -> crate::model::TenantStatut {
    match s {
        "suspendu" => crate::model::TenantStatut::Suspendu,
        "supprime" => crate::model::TenantStatut::Supprime,
        _ => crate::model::TenantStatut::Actif,
    }
}

fn plan_sql(p: crate::model::Plan) -> &'static str {
    match p {
        crate::model::Plan::Free => "free",
        crate::model::Plan::Pro => "pro",
        crate::model::Plan::Enterprise => "enterprise",
    }
}

fn plan_from_sql(s: &str) -> crate::model::Plan {
    match s {
        "pro" => crate::model::Plan::Pro,
        "enterprise" => crate::model::Plan::Enterprise,
        _ => crate::model::Plan::Free,
    }
}

fn parse_scopes(s: String) -> Vec<String> {
    serde_json::from_str(&s).unwrap_or_default()
}

fn pg_err(e: sqlx::Error) -> ControlError {
    ControlError::Store(format!("postgres: {e}"))
}

/// Transforme une violation de contrainte unique en conflit métier.
fn map_pg_conflict(
    conflict: impl Fn() -> ControlError,
) -> impl FnOnce(sqlx::Error) -> ControlError {
    move |e: sqlx::Error| match e {
        sqlx::Error::Database(db) if db.is_unique_violation() => conflict(),
        other => pg_err(other),
    }
}
