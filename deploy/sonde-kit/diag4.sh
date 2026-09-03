#!/usr/bin/env bash
# diag4.sh — test amont OpenRouter (extraction corrigée, longueurs seulement)
set -u
COMPOSITE="$(sudo grep '^COMPOSITE=' /opt/hermes/sonde-cloison/.env | cut -d= -f2-)"
K="${COMPOSITE#*.}"
echo "COMPOSITE_LEN=${#COMPOSITE} KEY_LEN=${#K} KEY_PREFIX=$(printf '%.6s' "$K")"
for M in deepseek/deepseek-chat deepseek/deepseek-v4-flash; do
  echo "== $M =="
  curl -s -o /tmp/t.json -w 'HTTP=%{http_code}\n' https://openrouter.ai/api/v1/chat/completions \
    -H "Authorization: Bearer $K" -H "Content-Type: application/json" \
    -d "{\"model\":\"$M\",\"messages\":[{\"role\":\"user\",\"content\":\"Dis OK\"}]}"
  head -c 200 /tmp/t.json; echo; echo
done
