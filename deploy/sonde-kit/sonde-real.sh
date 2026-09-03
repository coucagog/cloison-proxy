#!/usr/bin/env bash
# =============================================================================
# sonde-real.sh — SONDE A CLÉ RÉELLE (OpenRouter) : tenant jetable -> edge
# CLOISON (image locale mania-cloison-edge) -> fournisseur RÉEL.
# Prérequis : /opt/hermes/sonde-cloison/.env contient COMPOSITE=<mn_jeton>.<clé>
# (livré séparément, 0600 — la clé n'apparaît JAMAIS dans les logs).
# Données : synthétiques uniquement. Dépouillement : sudo bash /tmp/sonde-cleanup.sh
# =============================================================================
set -uo pipefail

SLUG="sonde-cloison"
BASE="/opt/hermes/$SLUG"
NET="${SLUG}_sonde-net"
EDGE="cloison-edge"
IMG="mania-cloison-edge:latest"
LOG="/tmp/sonde-cloison-e2e.log"

exec > >(tee -a "$LOG") 2>&1
echo "=== SONDE REELLE OpenRouter — $(date -u +%FT%TZ) ==="

[ -f "$BASE/.env" ] || { echo "FAIL: .env composite absent"; exit 1; }
COMPOSITE="$(sudo grep '^COMPOSITE=' "$BASE/.env" | cut -d= -f2-)"
TOKEN="$(printf '%s' "$COMPOSITE" | cut -d. -f1)"
TENANT_KEY="$(openssl rand -hex 32)"
VAULT_PASS="$(openssl rand -hex 24)"
printf 'CLOISON_TENANT_KEY_HEX=%s\nCLOISON_VAULT_PASSPHRASE=%s\n' "$TENANT_KEY" "$VAULT_PASS" | sudo tee -a "$BASE/.env" >/dev/null
sudo chmod 600 "$BASE/.env"
docker image inspect "$IMG" >/dev/null 2>&1 || { echo "FAIL: image $IMG absente"; exit 1; }
echo "PREVOL OK (image locale presente, composite lu, jamais affiche)"

# --- Tenant (agent seul, reseau internal) --------------------------------------
sudo mkdir -p "$BASE/data/workspace"
sudo chown -R 1000:1000 "$BASE/data"
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
( cd "$BASE" && sudo docker compose up -d --force-recreate )
# NB --force-recreate : l'env OPENROUTER_API_KEY (composite) est figé à la
# création du conteneur — après réécriture du .env, il FAUT recréer l'agent.
echo "TENANT OK"

# --- Edge CLOISON (image locale, mode N0 vault+NER, masquage actif) -------------
docker rm -f "$EDGE" >/dev/null 2>&1 || true
docker volume rm cloison-edge-audit >/dev/null 2>&1 || true
docker run -d --name "$EDGE" --network none --restart no \
  --read-only --tmpfs /tmp --cap-drop ALL --security-opt no-new-privileges:true \
  -v cloison-edge-audit:/data \
  -e CLOISON_ROLE=edge \
  -e CLOISON_LISTEN_ADDR=0.0.0.0:8787 \
  -e CLOISON_UPSTREAM_BASE_URL=https://openrouter.ai/api/v1 \
  -e CLOISON_UPSTREAM_CHAT_PATH=/chat/completions \
  -e CLOISON_UPSTREAM_MODELS_PATH=/models \
  -e CLOISON_AUDIT_MODE=0 \
  -e CLOISON_EXPECTED_ACCESS_TOKEN="$TOKEN" \
  -e CLOISON_TENANT_KEY_HEX="$TENANT_KEY" \
  -e CLOISON_VAULT_PATH=/data/vault.redb \
  -e CLOISON_VAULT_PASSPHRASE="$VAULT_PASS" \
  -e CLOISON_NER_MODEL_ONNX=/opt/cloison/ner/model-int8.onnx \
  -e CLOISON_NER_TOKENIZER=/opt/cloison/ner/tokenizer.json \
  -e CLOISON_ONNX_LIB=/opt/cloison/ner/libonnxruntime.so \
  "$IMG" >/dev/null
sleep 4
ST="$(docker inspect "$EDGE" --format '{{.State.Status}}')"
echo "EDGE state=$ST"
if [ "$ST" != "running" ]; then docker logs "$EDGE" --tail 15 2>&1 | tail -15; exit 1; fi
docker network disconnect none "$EDGE" >/dev/null 2>&1 || true
docker network connect "$NET" "$EDGE" || { echo "FAIL raccord edge"; exit 1; }
# EGRESS AMONT : le reseau tenant est internal (aucune sortie) — l'edge doit
# AUSSI etre sur bridge pour joindre OpenRouter. L'agent, lui, n'y est jamais.
docker network connect bridge "$EDGE" || { echo "FAIL raccord bridge"; exit 1; }
echo "EDGE RACCORDE (tenant + bridge, amont OpenRouter reel)"
sleep 2

# --- Câblage Hermes ---------------------------------------------------------------
CFG=0
for i in $(seq 1 40); do
  if docker exec "$SLUG-agent" test -f /home/hermes/.hermes/config.yaml 2>/dev/null; then CFG=1; break; fi
  sleep 3
done
[ "$CFG" = "1" ] || { echo "FAIL: config.yaml absent"; exit 1; }
hset(){ timeout 30 docker exec "$SLUG-agent" hermes config set "$1" "$2" 2>&1 \
          | sed -E 's@(mn_)?[a-f0-9]{16,}@<CAVIARDE>@g' || echo "WARN hset $1"; }
hset providers.cloison.base_url       "http://$EDGE:8787/v1"
hset providers.cloison.key_env        "OPENROUTER_API_KEY"
hset providers.cloison.api_mode       "chat_completions"
hset providers.cloison.default_model  "deepseek/deepseek-v4-flash"
hset model.default                    "deepseek/deepseek-v4-flash"
hset model.provider                   "custom:cloison"
docker exec "$SLUG-agent" sed -i '/^  base_url: /d' /home/hermes/.hermes/config.yaml || true
docker exec "$SLUG-agent" sh -c 'chown hermes:hermes /home/hermes/.hermes/config.yaml* 2>/dev/null || chown 1000:1000 /home/hermes/.hermes/config.yaml*' || true
( cd "$BASE" && sudo docker compose restart "$SLUG-agent" )
echo "CABLAGE OK"

# --- Vérifications ------------------------------------------------------------------
E="$(docker exec "$SLUG-agent" sh -c 'curl -s -o /dev/null --max-time 8 https://1.1.1.1/ ; echo $?' 2>/dev/null | tail -n1 || echo 000)"
echo "EGRESS curl_exit=$E (attendu 7)"
P="$(docker exec "$SLUG-agent" sh -c "curl -s -o /dev/null -w '%{http_code}' --max-time 8 http://$EDGE:8787/ -H 'Authorization: Bearer $COMPOSITE'" 2>/dev/null || echo 000)"
echo "EDGE probe http=$P (attendu 404 = auth acceptee)"
G="$(docker exec "$SLUG-agent" sh -c "grep -cE '^  provider: .?custom:cloison.?$' /home/hermes/.hermes/config.yaml" 2>/dev/null | tr -d '[:space:]' || echo 0)"
echo "PROFIL custom:cloison count=$G (attendu 1)"

# --- Activations réelles (coût minime, données synthétiques) ------------------------
echo "=== ACTIVATION 1/2 : tour trivial ==="
timeout 180 docker exec "$SLUG-agent" hermes -z "Bonjour. Reponds uniquement par le mot OK." 2>&1 | sed -E 's@(mn_)?[a-f0-9]{16,}@<CAVIARDE>@g' | head -25
echo "ACT1_EXIT=${PIPESTATUS[0]:-?}"
echo "=== ACTIVATION 2/2 : roundtrip PII reel ==="
timeout 180 docker exec "$SLUG-agent" hermes -z "Contactez Aminata Diop, aminata.diop@example.sn, tel +221 77 123 45 67, elle habite a Ziguinchor. Reponds en une phrase." 2>&1 | sed -E 's@(mn_)?[a-f0-9]{16,}@<CAVIARDE>@g' | head -30
echo "ACT2_EXIT=${PIPESTATUS[0]:-?}"

echo "=== LOGS EDGE (tail 12) ==="
docker logs "$EDGE" --tail 12 2>&1 | sed -E 's@(mn_)?[a-f0-9]{16,}@<CAVIARDE>@g' || true
echo "=== SONDE REELLE TERMINEE — depouillement : sudo bash /tmp/sonde-cleanup.sh ==="
echo "FIN $(date -u +%FT%TZ)"
