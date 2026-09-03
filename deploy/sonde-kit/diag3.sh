#!/usr/bin/env bash
# diag3.sh — test amont OpenRouter : deux modèles deepseek, clé jamais affichée
set -u
K="$(sudo grep '^COMPOSITE=' /opt/hermes/sonde-cloison/.env | cut -d= -f2- | cut -d. -f2-)"
for M in deepseek/deepseek-chat deepseek/deepseek-v4-flash-20260731 deepseek/deepseek-v4-flash; do
  echo "== $M =="
  curl -s -o /tmp/t.json -w 'HTTP=%{http_code}\n' https://openrouter.ai/api/v1/chat/completions \
    -H "Authorization: Bearer $K" -H "Content-Type: application/json" \
    -d "{\"model\":\"$M\",\"messages\":[{\"role\":\"user\",\"content\":\"Dis OK\"}]}"
  head -c 200 /tmp/t.json; echo; echo
done
