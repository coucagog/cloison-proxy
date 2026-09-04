#!/usr/bin/env bash
# =============================================================================
# CLOISON — déploiement de docs.wonkom.ai (STACK-N0V13 §12)
#
# Site STATIQUE servi par Caddy (file_server) : zéro conteneur, zéro log
# d'accès, zéro secret. Contenu source : deploy/docs-site/ (repo, source de
# vérité). Usage (hôte VPS, repo à jour) :
#   ./deploy/deploy-docs.sh
#
# Ce script copie le contenu + le Caddyfile puis recharge Caddy. Réexécutable
# (idempotent). Charte §12 : scripté, aucun secret, aucun log d'Authorization.
# =============================================================================
set -euo pipefail

REPO_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
SRC="$REPO_DIR/deploy/docs-site"
DST=/var/www/docs.wonkom.ai

echo "==> docs.wonkom.ai : $SRC -> $DST"

# 1. Contenu statique (copie, pas de rsync requis sur l'hôte).
sudo mkdir -p "$DST"
sudo cp -r "$SRC/." "$DST/"
sudo chown -R www-data:www-data "$DST" 2>/dev/null || true
sudo chmod -R a+rX "$DST"

# 2. Caddyfile (bloc docs.wonkom.ai) + rechargement.
sudo cp "$REPO_DIR/deploy/Caddyfile" /etc/caddy/Caddyfile
sudo caddy validate --config /etc/caddy/Caddyfile >/dev/null
sudo systemctl reload caddy

# 3. Vérification immédiate (TLS ACME peut prendre quelques secondes au 1er hit).
sleep 2
HTTP=$(curl -s -o /dev/null -w '%{http_code}' https://docs.wonkom.ai/ || true)
echo "==> https://docs.wonkom.ai/ -> HTTP $HTTP"
[ "$HTTP" = "200" ] || { echo "!! HTTP != 200 (TLS en cours d'émission ? relancer la vérification)" >&2; exit 1; }
echo "==> docs.wonkom.ai déployé."
