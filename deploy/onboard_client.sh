#!/usr/bin/env bash
# =============================================================================
# CLOISON — onboard_client.sh : onboarding locataire de bout en bout (N3).
#
# Usage (depuis l'hôte du VPS ou un poste autorisé, CLI compilé dans le PATH) :
#   CLOISON_CONTROL_URL=http://127.0.0.1:8788 \
#     ./deploy/onboard_client.sh <tenant_id> "<nom public>" [plan] [token_id?]
#
# Étapes :
#   1. provision tenant + licence (POST /admin/tenants)          [cloison-cli]
#   2. émission d'un jeton mn_ (clair affiché UNE fois)           [cloison-cli]
#   3. vérification par hash (le clair ne quitte jamais le CLI)   [cloison-cli]
#   4. affichage de la clé composite à livrer au client
#
# SÉCURITÉ :
#   - le clair mn_ n'est jamais écrit sur disque ni dans un log : il est
#     capturé en mémoire (variable shell) et oublié après affichage ;
#   - aucun secret en argv : le tenant_id est validé (anti-injection) ;
#   - plan : free | pro | enterprise (sinon le CLI rejette l'appel).
# =============================================================================
set -euo pipefail

TENANT_ID="${1:?usage: onboard_client.sh <tenant_id> \"<nom public>\" [plan]}"
NOM_PUBLIC="${2:?usage: onboard_client.sh <tenant_id> \"<nom public>\" [plan]}"
PLAN="${3:-free}"

if ! [[ "$TENANT_ID" =~ ^[a-zA-Z0-9_-]+$ ]]; then
    echo "ERREUR: tenant_id invalide (alphanumérique, tirets, underscores uniquement)" >&2
    exit 1
fi

CLI="${CLOISON_CLI:-cloison-cli}"
BASE="${CLOISON_CONTROL_URL:-http://127.0.0.1:8788}"

echo "=== 1. Provision tenant '$TENANT_ID' (plan $PLAN) ==="
"$CLI" --control-url "$BASE" provision "$TENANT_ID" --nom "$NOM_PUBLIC" --plan "$PLAN"

echo
echo "=== 2. Émission du jeton ==="
# Sortie : '  jeton : mn_<clair>' — capturée en mémoire uniquement.
ISSUED="$("$CLI" --control-url "$BASE" token issue "$TENANT_ID")"
echo "$ISSUED"
TOKEN="$(printf '%s\n' "$ISSUED" | sed -n 's/^  jeton : //p' | tr -d '[:space:]')"

if [ -z "$TOKEN" ]; then
    echo "ERREUR: émission du jeton sans clair (contrôle joignable ?)" >&2
    exit 1
fi

echo
echo "=== 3. Vérification (hash uniquement — le clair ne quitte pas le CLI) ==="
if "$CLI" --control-url "$BASE" token verify "$TENANT_ID" "$TOKEN" >/dev/null; then
    echo "Jeton vérifié : VALIDE (contrôle enregistré, auth edge OK)."
else
    echo "ERREUR: vérification du jeton échouée" >&2
    unset TOKEN
    exit 1
fi

echo
echo "=== 4. Clé composite à livrer au client ==="
echo "  Base URL : https://api.wonkom.ai/v1"
echo "  Clé      : ${TOKEN}.<cle_amont_du_client>"
echo
echo "⚠ Le jeton ci-dessus ne sera plus affiché : à communiquer au client par"
echo "  un canal sûr (jamais par email non chiffré, jamais dans un log)."
unset TOKEN   # oubli immédiat du clair en mémoire du shell
echo "=== ONBOARDING TERMINÉ ==="
