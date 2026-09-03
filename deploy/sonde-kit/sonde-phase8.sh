#!/usr/bin/env bash
# =============================================================================
# sonde-phase8.sh — edge N0 (vault+NER) + MASQUAGE ACTIF, un seul essai propre
# =============================================================================
set -uo pipefail

SLUG="sonde-cloison"
BASE="/opt/hermes/$SLUG"
NET="sonde-cloison_sonde-net"
EDGE="cloison-edge"
MOCK="mock-llm"
LOG="/tmp/sonde-cloison-e2e.log"

exec > >(tee -a "$LOG") 2>&1
echo "=== PHASE8 — $(date -u +%FT%TZ) ==="

COMPOSITE="$(sudo grep '^COMPOSITE=' "$BASE/.env" | cut -d= -f2-)"
TOKEN="$(printf '%s' "$COMPOSITE" | cut -d. -f1)"
TENANT_KEY="$(docker inspect "$EDGE" --format '{{range .Config.Env}}{{println .}}{{end}}' | grep '^CLOISON_TENANT_KEY_HEX=' | cut -d= -f2)"

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
  -e CLOISON_AUDIT_MODE=0 \
  -e CLOISON_EXPECTED_ACCESS_TOKEN="$TOKEN" \
  -e CLOISON_TENANT_KEY_HEX="$TENANT_KEY" \
  -e CLOISON_VAULT_PATH=/data/vault.redb \
  -e CLOISON_VAULT_PASSPHRASE=sonde-passphrase-test \
  -e CLOISON_NER_MODEL_ONNX=/opt/cloison/ner/model-int8.onnx \
  -e CLOISON_NER_TOKENIZER=/opt/cloison/ner/tokenizer.json \
  -e CLOISON_ONNX_LIB=/opt/cloison/ner/libonnxruntime.so \
  debian:bookworm-slim /opt/cloison/cloison-proxy >/dev/null
sleep 5
ST="$(docker inspect "$EDGE" --format '{{.State.Status}}')"
echo "EDGE state=$ST"
if [ "$ST" != "running" ]; then docker logs "$EDGE" --tail 15 2>&1 | tail -15; exit 1; fi
docker network disconnect none "$EDGE" >/dev/null 2>&1 || true
docker network connect "$NET" "$EDGE" || { echo "FAIL raccord edge"; exit 1; }
echo "EDGE RACCORDE (N0 vault+NER, masquage actif)"
sleep 2

E="$(docker exec "$SLUG-agent" sh -c 'curl -s -o /dev/null --max-time 8 https://1.1.1.1/ ; echo $?' 2>/dev/null | tail -n1 || echo 000)"
echo "EGRESS curl_exit=$E (attendu 7)"
P="$(docker exec "$SLUG-agent" sh -c "curl -s -o /dev/null -w '%{http_code}' --max-time 8 http://$EDGE:8787/v1/models -H 'Authorization: Bearer $COMPOSITE'" 2>/dev/null || echo 000)"
echo "PROXY /v1/models http=$P (attendu 200)"

TXT1="Bonjour. Contactez Aminata Diop, aminata.diop@example.sn, tel +221 77 123 45 67, elle habite a Ziguinchor."
TXT2="Rappel : Aminata Diop, +221 77 123 45 67, Ziguinchor."
echo "=== ACTIVATION 1/2 ==="
timeout 180 docker exec "$SLUG-agent" hermes -z "$TXT1" 2>&1 | sed -E 's@(mn_)?[a-f0-9]{16,}@<CAVIARDE>@g' | head -40
echo "ACT1_EXIT=${PIPESTATUS[0]:-?}"
echo "=== ACTIVATION 2/2 ==="
timeout 180 docker exec "$SLUG-agent" hermes -z "$TXT2" 2>&1 | sed -E 's@(mn_)?[a-f0-9]{16,}@<CAVIARDE>@g' | head -40
echo "ACT2_EXIT=${PIPESTATUS[0]:-?}"

echo "=== CE QUE LE MOCK A RECU (attendu : sentinelles ⟦, AUCUNE PII) ==="
docker logs "$MOCK" --tail 6 2>&1 | sed -E 's@(mn_)?[a-f0-9]{16,}@<CAVIARDE>@g' || true
echo "=== LOGS EDGE (tail 12) ==="
docker logs "$EDGE" --tail 12 2>&1 | sed -E 's@(mn_)?[a-f0-9]{16,}@<CAVIARDE>@g' || true

echo "=== PHASE8 TERMINEE — depouillement : sudo bash /tmp/sonde-cleanup.sh ==="
echo "FIN $(date -u +%FT%TZ)"
