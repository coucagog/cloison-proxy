#!/usr/bin/env bash
# =============================================================================
# CLOISON — provision_control.sh : provisionne le tenant + le hash du jeton
# d'accès edge dans le plan de contrôle (PostgresStore).
#
# Usage (le jeton est LU SUR STDIN — jamais en argv, jamais en log) :
#   printf '%s' "$CLOISON_EXPECTED_ACCESS_TOKEN" \
#     | ./deploy/provision_control.sh default
#
# RÈGLES :
#   - le clair du jeton N'EST JAMAIS écrit sur disque ni affiché : il est lu
#     sur stdin, haché en mémoire, puis SEUL le hash est inséré (le stockage
#     du contrôle ne contient que des hash — invariant I2, charte §9.2) ;
#   - idempotent : le tenant est créé s'il manque, l'insertion du hash est
#     conditionnée à son absence ;
#   - à exécuter sur l'hôte du VPS avec la stack compose (profil db) active.
#
# Prérequis d'activation du wiring C (CLOISON_CONTROL_URL posé sur edge) :
# ce script DOIT être passé AVANT le redémarrage d'edge, sinon l'auth
# fail-closed renvoie 401 (charte I8 : échouer bruyamment).
# =============================================================================
set -euo pipefail

TENANT_ID="${1:?usage: provision_control.sh <tenant_id> < jeton}"

# Tenant id : identifiant opérateur simple (anti-injection SQL).
if ! [[ "$TENANT_ID" =~ ^[a-zA-Z0-9_-]+$ ]]; then
    echo "ERREUR: tenant_id invalide (alphanumérique, tirets, underscores uniquement)" >&2
    exit 1
fi

# Le clair arrive par stdin (jamais en argv, jamais dans un log).
TOKEN="$(cat)"
[ -n "$TOKEN" ] || { echo "ERREUR: jeton vide sur stdin" >&2; exit 1; }

# Hash du domaine partagé (cloison-mn-token-v1:) — même formule que le contrôle.
HASH="$(printf 'cloison-mn-token-v1:%s' "$TOKEN" | sha256sum | cut -d' ' -f1)"
unset TOKEN   # oubli immédiat du clair en mémoire du shell

# Conteneur postgres (compose d'abord, repli par nom).
PG_CID="$(sudo docker compose --profile db -f deploy/docker-compose.dev.yml ps -q postgres 2>/dev/null | head -1 || true)"
[ -n "$PG_CID" ] || PG_CID="$(sudo docker ps -q --filter 'name=postgres' | head -1)"
if [ -z "$PG_CID" ]; then
    echo "ERREUR: conteneur postgres introuvable (stack compose démarrée ?)" >&2
    exit 1
fi
PSQL="sudo docker exec -i ${PG_CID} psql -U cloison -d cloison -v ON_ERROR_STOP=1"

# Tenant (idempotent) + upsert du jeton haché.
printf "INSERT INTO tenants (id, nom_public, statut, created_at, tokens_version)\n\
VALUES ('%s', 'tenant %s', 'actif', extract(epoch from now())::bigint, 0)\n\
ON CONFLICT (id) DO NOTHING;\n" "$TENANT_ID" "$TENANT_ID" | $PSQL >/dev/null

printf "INSERT INTO api_tokens (id, tenant_id, token_hash, scopes, created_at)\n\
SELECT 'tok-edge-%s', '%s', '%s', '[]', extract(epoch from now())::bigint\n\
WHERE NOT EXISTS (SELECT 1 FROM api_tokens WHERE token_hash = '%s');\n" \
    "$(date +%s)" "$TENANT_ID" "$HASH" "$HASH" | $PSQL

echo "OK: tenant '$TENANT_ID' provisionné (hash uniquement — le clair n'a pas été persisté)."
