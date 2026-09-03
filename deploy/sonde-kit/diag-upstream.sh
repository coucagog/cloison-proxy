#!/usr/bin/env bash
# diag-upstream.sh — diagnostic amont OpenRouter (statuts + corps d'erreur,
# JAMAIS la clé). Lecture seule.
set -u
COMPOSITE="$(sudo grep '^COMPOSITE=' /opt/hermes/sonde-cloison/.env | cut -d= -f2-)"
KEY="${COMPOSITE#*.}"
echo "=== GET /models ==="
curl -s -o /tmp/up-models.json -w 'HTTP=%{http_code}\n' https://openrouter.ai/api/v1/models -H "Authorization: Bearer $KEY"
head -c 300 /tmp/up-models.json; echo; echo
echo "=== POST /chat/completions (deepseek/deepseek-v4-flash) ==="
curl -s -o /tmp/up-chat.json -w 'HTTP=%{http_code}\n' https://openrouter.ai/api/v1/chat/completions \
  -H "Authorization: Bearer $KEY" -H "Content-Type: application/json" \
  -d '{"model":"deepseek/deepseek-v4-flash","messages":[{"role":"user","content":"Dis OK"}]}'
head -c 400 /tmp/up-chat.json; echo
