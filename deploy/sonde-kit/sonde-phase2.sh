#!/usr/bin/env bash
# =============================================================================
# sonde-phase2.sh — poursuite de la sonde E2E (raccord + câblage + activation)
# Prérequis : agent et edge déjà créés par sonde-cloison-e2e.sh (v2).
# Règle absolue : la valeur du jeton n'est JAMAIS affichée.
# =============================================================================
set -euo pipefail

SLUG="sonde-cloison"
BASE="/opt/hermes/$SLUG"
NET="sonde-cloison_sonde-net"
EDGE="cloison-edge"
LOG="/tmp/sonde-cloison-e2e.log"

exec > >(tee -a "$LOG") 2>&1
echo "=== PHASE2 — $(date -u +%FT%TZ) ==="

COMPOSITE="$(sudo grep '^COMPOSITE=' "$BASE/.env" | cut -d= -f2-)"

# --- Raccord edge -> réseau tenant (interne) -----------------------------------
docker network connect "$NET" "$EDGE" 2>/dev/null && echo "EDGE RACCORDE" || echo "EDGE DEJA RACCORDE"

# --- Attente config.yaml --------------------------------------------------------
CFG=0
for i in $(seq 1 40); do
  if docker exec "$SLUG-agent" test -f /home/hermes/.hermes/config.yaml 2>/dev/null; then CFG=1; break; fi
  sleep 3
done
[ "$CFG" = "1" ] || { echo "FAIL: config.yaml jamais cree"; exit 1; }
echo "CONFIG OK"

# --- Câblage du profil custom:cloison --------------------------------------------
hset(){ timeout 30 docker exec "$SLUG-agent" hermes config set "$1" "$2" 2>&1 \
          | sed -E 's@(mn_)?[a-f0-9]{16,}@<CAVIARDE>@g' || echo "WARN hset $1"; }
hset providers.cloison.base_url       "http://$EDGE:8787/v1"
hset providers.cloison.key_env        "OPENROUTER_API_KEY"
hset providers.cloison.api_mode       "chat_completions"
hset providers.cloison.default_model  "gpt-4o-mini"
hset model.default                    "gpt-4o-mini"
hset model.provider                   "custom:cloison"
docker exec "$SLUG-agent" sed -i '/^  base_url: /d' /home/hermes/.hermes/config.yaml || true
docker exec "$SLUG-agent" sh -c 'chown hermes:hermes /home/hermes/.hermes/config.yaml* 2>/dev/null || chown 1000:1000 /home/hermes/.hermes/config.yaml*' || true
( cd "$BASE" && sudo docker compose restart "$SLUG-agent" )
echo "CABLAGE OK"

# --- Vérifications ----------------------------------------------------------------
E="$(docker exec "$SLUG-agent" sh -c 'curl -s -o /dev/null --max-time 8 https://1.1.1.1/ ; echo $?' 2>/dev/null | tail -n1)"
echo "EGRESS curl_exit=$E (attendu 7)"
P="$(docker exec "$SLUG-agent" sh -c "curl -s -o /dev/null -w '%{http_code}' --max-time 8 http://$EDGE:8787/v1/models -H 'Authorization: Bearer $COMPOSITE'" 2>/dev/null)"
echo "PROXY /v1/models http=$P (attendu 200)"
G="$(docker exec "$SLUG-agent" sh -c "grep -cE '^  provider: .?custom:cloison.?$' /home/hermes/.hermes/config.yaml" 2>/dev/null | tr -d '[:space:]')"
echo "PROFIL custom:cloison count=$G (attendu 1)"

# --- Activation --------------------------------------------------------------------
echo "=== ACTIVATION hermes -z (timeout 180 s) ==="
timeout 180 docker exec "$SLUG-agent" hermes -z "Bonjour. Appelez Xolani Ndlovu au 77 123 45 67, il habite a Ziguinchor." 2>&1 | sed -E 's@(mn_)?[a-f0-9]{16,}@<CAVIARDE>@g' | head -40
echo "ACTIVATION_EXIT=${PIPESTATUS[0]:-?}"

# --- Preuves côté edge ---------------------------------------------------------------
echo "=== LOGS EDGE (tail 50) ==="
docker logs "$EDGE" --tail 50 2>&1 | sed -E 's@(mn_)?[a-f0-9]{16,}@<CAVIARDE>@g' || true
echo "=== RAPPORT AUDIT (k=1) ==="
docker exec "$SLUG-agent" sh -c "curl -s --max-time 8 'http://$EDGE:8787/v1/audit/report?period=all' -H 'Authorization: Bearer $COMPOSITE'" 2>/dev/null | sed -E 's@(mn_)?[a-f0-9]{16,}@<CAVIARDE>@g' || true

echo "=== PHASE2 TERMINEE — depouillement : sudo bash /tmp/sonde-cleanup.sh ==="
echo "FIN $(date -u +%FT%TZ)"
