-- CLOISON — Plan de contrôle (PostgreSQL). AUCUNE PII : les jetons ne sont
-- stockés QUE hachés (SHA-256) ; le schéma ne contient aucun champ texte
-- utilisateur. Idempotent (IF NOT EXISTS) — exécuté au boot par
-- PostgresStore::connect (feature `pg`).

CREATE TABLE IF NOT EXISTS tenants (
    id              TEXT PRIMARY KEY,
    nom_public      TEXT NOT NULL,
    statut          TEXT NOT NULL DEFAULT 'actif',
    created_at      BIGINT NOT NULL,
    tokens_version  BIGINT NOT NULL DEFAULT 0
);

CREATE TABLE IF NOT EXISTS api_tokens (
    id          TEXT PRIMARY KEY,
    tenant_id   TEXT NOT NULL REFERENCES tenants(id) ON DELETE CASCADE,
    token_hash  TEXT NOT NULL UNIQUE,      -- hex(SHA-256(domaine ‖ clair)) — JAMAIS le clair
    scopes      TEXT NOT NULL DEFAULT '[]', -- JSON array de chaînes
    created_at  BIGINT NOT NULL,
    rotated_at  BIGINT,
    grace_until BIGINT,
    revoked_at  BIGINT
);
CREATE INDEX IF NOT EXISTS idx_api_tokens_tenant ON api_tokens(tenant_id);

CREATE TABLE IF NOT EXISTS policies (
    tenant_id   TEXT PRIMARY KEY REFERENCES tenants(id) ON DELETE CASCADE,
    json_policy TEXT NOT NULL,             -- JSON canonique, jamais de texte client
    version     BIGINT NOT NULL,
    updated_at  BIGINT NOT NULL
);

CREATE TABLE IF NOT EXISTS licenses (
    tenant_id              TEXT PRIMARY KEY REFERENCES tenants(id) ON DELETE CASCADE,
    plan                   TEXT NOT NULL DEFAULT 'free',
    max_requests_per_day   BIGINT NOT NULL DEFAULT 1000,
    max_tokens             INTEGER NOT NULL DEFAULT 16,
    expires_at             BIGINT,
    created_at             BIGINT NOT NULL
);
