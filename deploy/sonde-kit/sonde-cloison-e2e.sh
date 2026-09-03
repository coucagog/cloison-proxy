#!/usr/bin/env bash
# =============================================================================
# sonde-cloison-e2e.sh (v2) — SONDE E2E Hermes -> CLOISON (tenant jetable)
# Repli image GHCR non publique : binaire release v0.3.x (cloison-proxy) via
# l'installeur officiel, exécuté dans debian:bookworm-slim (Docker Hub).
# AUCUN tenant existant touché, aucune clé LLM réelle, aucune donnée réelle.
# Règle absolue : la valeur du jeton mn_ n'est JAMAIS affichée.
# Exécution : sudo bash /tmp/sonde-cloison-e2e.sh   (log: /tmp/sonde-cloison-e2e.log)
# =============================================================================
set -euo pipefail

SLUG="sonde-cloison"
BASE="/opt/hermes/$SLUG"
NET="${SLUG}_${SLUG}-net"
LOG="/tmp/sonde-cloison-e2e.log"
EDGE="cloison-edge"
PREFIX="/tmp/cloison-n0"

exec > >(tee -a "$LOG") 2>&1
echo "=== SONDE CLOISON x HERMES v2 — $(date -u +%FT%TZ) ==="
echo "hote=$(hostname) edge=$EDGE slug=$SLUG"

fail(){ echo "FAIL: $*" >&2; exit 1; }

# --- 0. Pré-vol ---------------------------------------------------------------
sudo -n true || fail "sudo sans mot de passe requis"
docker info >/dev/null 2>&1 || fail "docker injoignable"
[ -e "$BASE" ] && fail "$BASE existe (residu) — lancer sudo bash /tmp/sonde-cleanup.sh d abord"
docker ps -a --format '{{.Names}}' | grep -qx "$SLUG-agent" && fail "conteneur $SLUG-agent residuel"
docker ps -a --format '{{.Names}}' | grep -qx "$EDGE" && fail "conteneur $EDGE residuel"
echo "PREVOL OK"

# --- 1. Secrets de la sonde (jamais affiches) ---------------------------------
TOKEN="mn_$(openssl rand -hex 16)"
COMPOSITE="${TOKEN}.mock"
TENANT_KEY="$(openssl rand -hex 32)"
sudo mkdir -p "$BASE/data/workspace"
sudo chown -R 1000:1000 "$BASE/data"
umask 077
printf 'COMPOSITE=%s\n' "$COMPOSITE" | sudo tee "$BASE/.env" >/dev/null
sudo chmod 600 "$BASE/.env"
echo "SECRETS OK (jeton genere, jamais affiche)"

# --- 2. Binaire CLOISON (release publique, checksums vérifiés) ------------------
rm -rf "$PREFIX"
curl -fsSL https://raw.githubusercontent.com/coucagog/cloison-proxy/main/install-n0.sh -o /tmp/install-n0.sh
bash /tmp/install-n0.sh --prefix "$PREFIX" || fail "install-n0.sh"
[ -x "$PREFIX/cloison-proxy" ] || fail "binaire absent apres install"
NERV="$(find "$PREFIX" -name 'model-int8.onnx' | head -1)"
NERT="$(find "$PREFIX" -name 'tokenizer.json' | head -1)"
LIB="$(find "$PREFIX" -name 'libonnxruntime.so*' | head -1)"
echo "BINAIRE OK (ner_modele=${NERV:+present} ner_tokenizer=${NERT:+present} onnx_lib=${LIB:+present})"

# --- 3. Edge CLOISON (mock + audit k=1, read-only, reseau none) ------------------
docker pull debian:bookworm-slim || fail "pull debian:bookworm-slim"
docker rm -f "$EDGE" >/dev/null 2>&1 || true
docker volume rm cloison-edge-audit >/dev/null 2>&1 || true
docker run -d --name "$EDGE" --network none --restart no \
  --read-only --tmpfs /tmp --cap-drop ALL --security-opt no-new-privileges:true \
  -v "$PREFIX:/opt/cloison:ro" \
  -v cloison-edge-audit:/data \
  -e CLOISON_ROLE=edge \
  -e CLOISON_LISTEN_ADDR=0.0.0.0:8787 \
  -e CLOISON_MOCK_MODE=1 \
  -e CLOISON_AUDIT_MODE=1 \
  -e CLOISON_AUDIT_K=1 \
  -e CLOISON_EXPECTED_ACCESS_TOKEN="$TOKEN" \
  -e CLOISON_TENANT_KEY_HEX="$TENANT_KEY" \
  -e CLOISON_AUDIT_KEYS=/data/audit_key \
  -e CLOISON_AUDIT_LEDGER_FILE=/data/audit_ledger.jsonl \
  -e CLOISON_NER_MODEL_ONNX=/opt/cloison/ner/model-int8.onnx \
  -e CLOISON_NER_TOKENIZER=/opt/cloison/ner/tokenizer.json \
  -e CLOISON_ONNX_LIB=/opt/cloison/ner/libonnxruntime.so \
  debian:bookworm-slim /opt/cloison/cloison-proxy || fail "demarrage edge"
sleep 4
docker inspect "$EDGE" --format '{{.State.Status}}' | grep -qx running || fail "edge pas running"
echo "EDGE OK (mock, audit k=1, read-only, NER embarqué)"

# --- 4. Tenant jetable (agent seul, reseau internal) -----------------------------
sudo tee "$BASE/docker-compose.yml" >/dev/null <<EOF
services:
  ${SLUG}-agent:
    image: nousresearch/hermes-agent:latest
    container_name: ${SLUG}-agent
    command: gateway run
    restart: "no"
    networks: [sonde-net]
    environment:
      - HERMES_HOME=/home/hermes/.hermes
      - HERMES_UID=1000
      - HERMES_GID=1000
      - OPENROUTER_API_KEY=\${COMPOSITE}
    volumes:
      - hermes-home:/home/hermes/.hermes
      - hermes-agent-src:/opt/hermes
      - ./data/workspace:/workspace
    mem_limit: 512m
    cpus: 1.0
networks:
  sonde-net:
    driver: bridge
    internal: true
volumes:
  hermes-home:
  hermes-agent-src:
EOF
( cd "$BASE" && sudo docker compose up -d )
docker network connect "$NET" "$EDGE" || fail "raccord edge au reseau tenant"
echo "TENANT OK (agent seul, reseau internal:true, edge raccorde)"

# --- 5. Attente config.yaml ------------------------------------------------------
CFG=0
for i in $(seq 1 40); do
  if docker exec "$SLUG-agent" test -f /home/hermes/.hermes/config.yaml 2>/dev/null; then CFG=1; break; fi
  sleep 3
done
[ "$CFG" = "1" ] || fail "config.yaml jamais cree (120 s)"
echo "CONFIG OK"

# --- 6. Câblage du profil fournisseur custom:cloison ------------------------------
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
echo "CABLAGE OK (profil custom:cloison, model.base_url retire — Hermes #25107)"

# --- 7. Vérifications --------------------------------------------------------------
E="$(docker exec "$SLUG-agent" sh -c 'curl -s -o /dev/null --max-time 8 https://1.1.1.1/ ; echo $?' 2>/dev/null | tail -n1)"
echo "EGRESS curl_exit=$E (attendu 7)"
P="$(docker exec "$SLUG-agent" sh -c "curl -s -o /dev/null -w '%{http_code}' --max-time 8 http://$EDGE:8787/v1/models -H 'Authorization: Bearer $COMPOSITE'" 2>/dev/null)"
echo "PROXY /v1/models http=$P (attendu 200)"
G="$(docker exec "$SLUG-agent" sh -c "grep -cE '^  provider: .?custom:cloison.?$' /home/hermes/.hermes/config.yaml" 2>/dev/null | tr -d '[:space:]')"
echo "PROFIL custom:cloison count=$G (attendu 1)"

# --- 8. Activation : un vrai tour d'agent à travers le edge -------------------------
echo "=== ACTIVATION hermes -z (timeout 180 s) ==="
timeout 180 docker exec "$SLUG-agent" hermes -z "Bonjour. Appelez Xolani Ndlovu au 77 123 45 67, il habite a Ziguinchor." 2>&1 | sed -E 's@(mn_)?[a-f0-9]{16,}@<CAVIARDE>@g' | head -40
echo "ACTIVATION_EXIT=${PIPESTATUS[0]:-?}"

# --- 9. Preuves côté edge -----------------------------------------------------------
echo "=== LOGS EDGE (tail 50) ==="
docker logs "$EDGE" --tail 50 2>&1 | sed -E 's@(mn_)?[a-f0-9]{16,}@<CAVIARDE>@g' || true
echo "=== RAPPORT AUDIT (k=1) ==="
docker exec "$SLUG-agent" sh -c "curl -s --max-time 8 'http://$EDGE:8787/v1/audit/report?period=all' -H 'Authorization: Bearer $COMPOSITE'" 2>/dev/null | sed -E 's@(mn_)?[a-f0-9]{16,}@<CAVIARDE>@g' || true

echo "=== SONDE TERMINEE — depouillement : sudo bash /tmp/sonde-cleanup.sh ==="
echo "FIN $(date -u +%FT%TZ)"
