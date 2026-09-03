#!/usr/bin/env bash
# =============================================================================
# sonde-phase4.sh — mock LLM amont + edge en mode N0 (vault+NER) + preuves
# =============================================================================
set -euo pipefail

SLUG="sonde-cloison"
BASE="/opt/hermes/$SLUG"
NET="sonde-cloison_sonde-net"
EDGE="cloison-edge"
MOCK="mock-llm"
LOG="/tmp/sonde-cloison-e2e.log"

exec > >(tee -a "$LOG") 2>&1
echo "=== PHASE4 — $(date -u +%FT%TZ) ==="

COMPOSITE="$(sudo grep '^COMPOSITE=' "$BASE/.env" | cut -d= -f2-)"
TOKEN="$(printf '%s' "$COMPOSITE" | cut -d. -f1)"
TENANT_KEY="$(docker inspect "$EDGE" --format '{{range .Config.Env}}{{println .}}{{end}}' | grep '^CLOISON_TENANT_KEY_HEX=' | cut -d= -f2)"

# --- 1. Mock LLM (conteneur python, réseau tenant, écho) ------------------------
docker pull python:3.11-slim || { echo "FAIL pull python"; exit 1; }
docker rm -f "$MOCK" >/dev/null 2>&1 || true
docker run -d --name "$MOCK" --network none --restart no \
  -v /tmp/mock-llm.py:/mock-llm.py:ro \
  python:3.11-slim python3 /mock-llm.py || { echo "FAIL mock"; exit 1; }
docker network disconnect none "$MOCK" >/dev/null 2>&1 || true
docker network connect "$NET" "$MOCK" || { echo "FAIL raccord mock"; exit 1; }
echo "MOCK OK (reseau tenant)"

# --- 2. Edge recréé : amont=mock, mode N0 (vault+NER actifs) ---------------------
docker rm -f "$EDGE" || true
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
  -e CLOISON_VAULT_PATH=/data/vault.redb \
  -e CLOISON_VAULT_PASSPHRASE=sonde-passphrase-test \
  -e CLOISON_AUDIT_KEYS=/data/audit_key \
  -e CLOISON_AUDIT_LEDGER_FILE=/data/audit_ledger.jsonl \
  -e CLOISON_NER_MODEL_ONNX=/opt/cloison/ner/model-int8.onnx \
  -e CLOISON_NER_TOKENIZER=/opt/cloison/ner/tokenizer.json \
  -e CLOISON_ONNX_LIB=/opt/cloison/ner/libonnxruntime.so \
  debian:bookworm-slim /opt/cloison/cloison-proxy || { echo "FAIL edge"; exit 1; }
sleep 4
docker network disconnect none "$EDGE" >/dev/null 2>&1 || true
docker network connect "$NET" "$EDGE" || { echo "FAIL raccord edge"; exit 1; }
echo "EDGE OK (amont=mock-llm, N0 vault+NER, audit k=2)"
sleep 2

# --- 3. Vérifications --------------------------------------------------------------
E="$(docker exec "$SLUG-agent" sh -c 'curl -s -o /dev/null --max-time 8 https://1.1.1.1/ ; echo $?' 2>/dev/null | tail -n1)"
echo "EGRESS curl_exit=$E (attendu 7)"
P="$(docker exec "$SLUG-agent" sh -c "curl -s -o /dev/null -w '%{http_code}' --max-time 8 http://$EDGE:8787/v1/models -H 'Authorization: Bearer $COMPOSITE'" 2>/dev/null)"
echo "PROXY /v1/models http=$P (attendu 200)"

# --- 4. Activation ×2 (k=2 → rapport publiable) -------------------------------------
echo "=== ACTIVATION 1/2 hermes -z (timeout 180 s) ==="
timeout 180 docker exec "$SLUG-agent" hermes -z "Bonjour. Appelez Xolani Ndlovu au 77 123 45 67, il habite a Ziguinchor." 2>&1 | sed -E 's@(mn_)?[a-f0-9]{16,}@<CAVIARDE>@g' | head -30
echo "ACT1_EXIT=${PIPESTATUS[0]:-?}"
echo "=== ACTIVATION 2/2 hermes -z (timeout 180 s) ==="
timeout 180 docker exec "$SLUG-agent" hermes -z "Rappel : Xolani Ndlovu au 77 123 45 67." 2>&1 | sed -E 's@(mn_)?[a-f0-9]{16,}@<CAVIARDE>@g' | head -30
echo "ACT2_EXIT=${PIPESTATUS[0]:-?}"

# --- 5. Preuves ----------------------------------------------------------------------
echo "=== CE QUE LE MOCK A RECU (anti-pass-through) ==="
docker logs "$MOCK" --tail 10 2>&1 | sed -E 's@(mn_)?[a-f0-9]{16,}@<CAVIARDE>@g' || true
echo "=== LOGS EDGE (tail 30) ==="
docker logs "$EDGE" --tail 30 2>&1 | sed -E 's@(mn_)?[a-f0-9]{16,}@<CAVIARDE>@g' || true
echo "=== RAPPORT AUDIT (k=2) ==="
docker exec "$SLUG-agent" sh -c "curl -s --max-time 8 'http://$EDGE:8787/v1/audit/report?period=all' -H 'Authorization: Bearer $COMPOSITE'" 2>/dev/null | sed -E 's@(mn_)?[a-f0-9]{16,}@<CAVIARDE>@g' || true

echo "=== PHASE4 TERMINEE — depouillement : sudo bash /tmp/sonde-cleanup.sh ==="
echo "FIN $(date -u +%FT%TZ)"
