#!/usr/bin/env bash
# =============================================================================
# sonde-phase6.sh — mock SSE v2 + activations + preuves finales
# =============================================================================
set -uo pipefail

SLUG="sonde-cloison"
BASE="/opt/hermes/$SLUG"
EDGE="cloison-edge"
MOCK="mock-llm"
LOG="/tmp/sonde-cloison-e2e.log"

exec > >(tee -a "$LOG") 2>&1
echo "=== PHASE6 — $(date -u +%FT%TZ) ==="

COMPOSITE="$(sudo grep '^COMPOSITE=' "$BASE/.env" | cut -d= -f2-)"

# --- Mock redémarré avec la version SSE ----------------------------------------
docker rm -f "$MOCK" >/dev/null 2>&1 || true
docker run -d --name "$MOCK" --network none --restart no \
  -v /tmp/mock-llm.py:/mock-llm.py:ro \
  python:3.11-slim python3 /mock-llm.py
docker network disconnect none "$MOCK" >/dev/null 2>&1 || true
docker network connect sonde-cloison_sonde-net "$MOCK" || { echo "FAIL raccord mock"; exit 1; }
echo "MOCK SSE OK"
sleep 2

# --- Activations ---------------------------------------------------------------
TXT1="Bonjour. Contactez Aminata Diop, aminata.diop@example.sn, tel +221 77 123 45 67, elle habite a Ziguinchor."
TXT2="Rappel : Aminata Diop, +221 77 123 45 67, Ziguinchor."
echo "=== ACTIVATION 1/2 ==="
timeout 180 docker exec "$SLUG-agent" hermes -z "$TXT1" 2>&1 | sed -E 's@(mn_)?[a-f0-9]{16,}@<CAVIARDE>@g' | head -40
echo "ACT1_EXIT=${PIPESTATUS[0]:-?}"
echo "=== ACTIVATION 2/2 ==="
timeout 180 docker exec "$SLUG-agent" hermes -z "$TXT2" 2>&1 | sed -E 's@(mn_)?[a-f0-9]{16,}@<CAVIARDE>@g' | head -40
echo "ACT2_EXIT=${PIPESTATUS[0]:-?}"

# --- Preuves ---------------------------------------------------------------------
echo "=== CE QUE LE MOCK A RECU (anti-pass-through) ==="
docker logs "$MOCK" --tail 12 2>&1 | sed -E 's@(mn_)?[a-f0-9]{16,}@<CAVIARDE>@g' || true
echo "=== LOGS EDGE (tail 20) ==="
docker logs "$EDGE" --tail 20 2>&1 | sed -E 's@(mn_)?[a-f0-9]{16,}@<CAVIARDE>@g' || true
echo "=== RAPPORT AUDIT (k=2) ==="
docker exec "$SLUG-agent" sh -c "curl -s --max-time 8 'http://$EDGE:8787/v1/audit/report?period=all' -H 'Authorization: Bearer $COMPOSITE'" 2>/dev/null | sed -E 's@(mn_)?[a-f0-9]{16,}@<CAVIARDE>@g' || true

echo "=== PHASE6 TERMINEE — depouillement : sudo bash /tmp/sonde-cleanup.sh ==="
echo "FIN $(date -u +%FT%TZ)"
