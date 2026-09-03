#!/usr/bin/env bash
# =============================================================================
# sonde-phase3.sh — raccord edge (fix none) + vérifications + activation + preuves
# =============================================================================
set -euo pipefail

SLUG="sonde-cloison"
BASE="/opt/hermes/$SLUG"
NET="sonde-cloison_sonde-net"
EDGE="cloison-edge"
LOG="/tmp/sonde-cloison-e2e.log"

exec > >(tee -a "$LOG") 2>&1
echo "=== PHASE3 — $(date -u +%FT%TZ) ==="

COMPOSITE="$(sudo grep '^COMPOSITE=' "$BASE/.env" | cut -d= -f2-)"

# --- Raccord (déconnexion de none d'abord — contrainte Docker) --------------------
docker network disconnect none "$EDGE" 2>/dev/null || true
docker network connect "$NET" "$EDGE" && echo "EDGE RACCORDE AU RESEAU TENANT" || { echo "FAIL raccord"; exit 1; }
sleep 2

# --- Vérifications ------------------------------------------------------------------
E="$(docker exec "$SLUG-agent" sh -c 'curl -s -o /dev/null --max-time 8 https://1.1.1.1/ ; echo $?' 2>/dev/null | tail -n1)"
echo "EGRESS curl_exit=$E (attendu 7)"
P="$(docker exec "$SLUG-agent" sh -c "curl -s -o /dev/null -w '%{http_code}' --max-time 8 http://$EDGE:8787/v1/models -H 'Authorization: Bearer $COMPOSITE'" 2>/dev/null)"
echo "PROXY /v1/models http=$P (attendu 200)"
G="$(docker exec "$SLUG-agent" sh -c "grep -cE '^  provider: .?custom:cloison.?$' /home/hermes/.hermes/config.yaml" 2>/dev/null | tr -d '[:space:]')"
echo "PROFIL custom:cloison count=$G (attendu 1)"

# --- Activation ----------------------------------------------------------------------
echo "=== ACTIVATION hermes -z (timeout 180 s) ==="
timeout 180 docker exec "$SLUG-agent" hermes -z "Bonjour. Appelez Xolani Ndlovu au 77 123 45 67, il habite a Ziguinchor." 2>&1 | sed -E 's@(mn_)?[a-f0-9]{16,}@<CAVIARDE>@g' | head -40
echo "ACTIVATION_EXIT=${PIPESTATUS[0]:-?}"

# --- Preuves côté edge -----------------------------------------------------------------
echo "=== LOGS EDGE (tail 50) ==="
docker logs "$EDGE" --tail 50 2>&1 | sed -E 's@(mn_)?[a-f0-9]{16,}@<CAVIARDE>@g' || true
echo "=== RAPPORT AUDIT (k=1) ==="
docker exec "$SLUG-agent" sh -c "curl -s --max-time 8 'http://$EDGE:8787/v1/audit/report?period=all' -H 'Authorization: Bearer $COMPOSITE'" 2>/dev/null | sed -E 's@(mn_)?[a-f0-9]{16,}@<CAVIARDE>@g' || true

echo "=== PHASE3 TERMINEE — depouillement : sudo bash /tmp/sonde-cleanup.sh ==="
echo "FIN $(date -u +%FT%TZ)"
