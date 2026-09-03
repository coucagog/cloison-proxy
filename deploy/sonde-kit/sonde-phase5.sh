#!/usr/bin/env bash
# =============================================================================
# sonde-phase5.sh — edge (audit k=2, amont mock, SANS vault — la combinaison
# N0+audit de la release crashe au boot : "failed to hash audit policy")
# =============================================================================
set -uo pipefail

SLUG="sonde-cloison"
BASE="/opt/hermes/$SLUG"
NET="sonde-cloison_sonde-net"
EDGE="cloison-edge"
MOCK="mock-llm"
LOG="/tmp/sonde-cloison-e2e.log"

exec > >(tee -a "$LOG") 2>&1
echo "=== PHASE5 — $(date -u +%FT%TZ) ==="

COMPOSITE="$(sudo grep '^COMPOSITE=' "$BASE/.env" | cut -d= -f2-)"
TOKEN="$(printf '%s' "$COMPOSITE" | cut -d. -f1)"
TENANT_KEY="$(docker inspect "$EDGE" --format '{{range .Config.Env}}{{println .}}{{end}}' | grep '^CLOISON_TENANT_KEY_HEX=' | cut -d= -f2)"

# --- Edge recréé : amont=mock, audit k=2, pas de vault -------------------------
docker rm -f "$EDGE" >/dev/null 2>&1 || true
docker run -d --name "$EDGE" --network none --restart no \
  --read-only --tmpfs /tmp --cap-drop ALL --security-opt no-new-privileges:true \
  -v /tmp/cloison-n0:/opt/cloison:ro \
  -v cloison-edge-audit:/data \
  -e CLOISON_ROLE=edge \
  -e CLOISON_LISTEN_ADDR=0.0.0.0:8787 \
  -e CLOISON_MOCK_MODE=0 \
  -e CLOISON_UPSTREAM_BASE_URL=http://mock-llm:8000 \
  -e CLOISON_UPSTREAM_CHAT_PATH=/v1/chat/completions \
  -e CLOISON_UPSTREAM_MODELS_PATH=/v1/models \
  -e CLOISON_AUDIT_MODE=1 \
  -e CLOISON_AUDIT_K=2 \
  -e CLOISON_EXPECTED_ACCESS_TOKEN="$TOKEN" \
  -e CLOISON_TENANT_KEY_HEX="$TENANT_KEY" \
  -e CLOISON_AUDIT_KEYS=/data/audit_key \
  -e CLOISON_AUDIT_LEDGER_FILE=/data/audit_ledger.jsonl \
  debian:bookworm-slim /opt/cloison/cloison-proxy
sleep 4
ST="$(docker inspect "$EDGE" --format '{{.State.Status}}')"
echo "EDGE state=$ST (attendu running)"
if [ "$ST" != "running" ]; then echo "--- logs boot ---"; docker logs "$EDGE" --tail 15 2>&1 | tail -15; exit 1; fi
docker network disconnect none "$EDGE" >/dev/null 2>&1 || true
docker network connect "$NET" "$EDGE" || { echo "FAIL raccord edge"; exit 1; }
echo "EDGE RACCORDE (audit k=2, amont mock)"
sleep 2

# --- Vérifications (captures sûres : pas de mort silencieuse) --------------------
E="$(docker exec "$SLUG-agent" sh -c 'curl -s -o /dev/null --max-time 8 https://1.1.1.1/ ; echo $?' 2>/dev/null | tail -n1 || echo 000)"
echo "EGRESS curl_exit=$E (attendu 7)"
P="$(docker exec "$SLUG-agent" sh -c "curl -s -o /dev/null -w '%{http_code}' --max-time 8 http://$EDGE:8787/v1/models -H 'Authorization: Bearer $COMPOSITE'" 2>/dev/null || echo 000)"
echo "PROXY /v1/models http=$P (attendu 200)"
G="$(docker exec "$SLUG-agent" sh -c "grep -cE '^  provider: .?custom:cloison.?$' /home/hermes/.hermes/config.yaml" 2>/dev/null | tr -d '[:space:]' || echo 0)"
echo "PROFIL custom:cloison count=$G (attendu 1)"

# --- Activation ×2 (k=2 → rapport publiable) --------------------------------------
TXT1="Bonjour. Contactez Aminata Diop, aminata.diop@example.sn, tel +221 77 123 45 67, elle habite a Ziguinchor."
TXT2="Rappel : Aminata Diop, +221 77 123 45 67, Ziguinchor."
echo "=== ACTIVATION 1/2 ==="
timeout 180 docker exec "$SLUG-agent" hermes -z "$TXT1" 2>&1 | sed -E 's@(mn_)?[a-f0-9]{16,}@<CAVIARDE>@g' | head -30
echo "ACT1_EXIT=${PIPESTATUS[0]:-?}"
echo "=== ACTIVATION 2/2 ==="
timeout 180 docker exec "$SLUG-agent" hermes -z "$TXT2" 2>&1 | sed -E 's@(mn_)?[a-f0-9]{16,}@<CAVIARDE>@g' | head -30
echo "ACT2_EXIT=${PIPESTATUS[0]:-?}"

# --- Preuves ------------------------------------------------------------------------
echo "=== CE QUE LE MOCK A RECU (anti-pass-through) ==="
docker logs "$MOCK" --tail 10 2>&1 | sed -E 's@(mn_)?[a-f0-9]{16,}@<CAVIARDE>@g' || true
echo "=== LOGS EDGE (tail 20) ==="
docker logs "$EDGE" --tail 20 2>&1 | sed -E 's@(mn_)?[a-f0-9]{16,}@<CAVIARDE>@g' || true
echo "=== RAPPORT AUDIT (k=2) ==="
docker exec "$SLUG-agent" sh -c "curl -s --max-time 8 'http://$EDGE:8787/v1/audit/report?period=all' -H 'Authorization: Bearer $COMPOSITE'" 2>/dev/null | sed -E 's@(mn_)?[a-f0-9]{16,}@<CAVIARDE>@g' || true

echo "=== PHASE5 TERMINEE — depouillement : sudo bash /tmp/sonde-cleanup.sh ==="
echo "FIN $(date -u +%FT%TZ)"
