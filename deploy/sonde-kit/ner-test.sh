#!/usr/bin/env bash
# ner-test.sh — teste un bundle NER (release) avec le binaire correspondant
# Usage : sudo bash ner-test.sh /tmp/ner031
set -u
PREFIX="${1:-/tmp/ner031}"
docker rm -f ner-test >/dev/null 2>&1 || true
docker run -d --name ner-test -p 18788:8787 \
  -v "$PREFIX:/opt/cloison:ro" \
  -e CLOISON_ROLE=edge -e CLOISON_LISTEN_ADDR=0.0.0.0:8787 \
  -e CLOISON_MOCK_MODE=1 \
  -e CLOISON_EXPECTED_ACCESS_TOKEN=mn_test \
  -e CLOISON_TENANT_KEY_HEX=4242424242424242424242424242424242424242424242424242424242424242 \
  -e CLOISON_VAULT_PATH=/tmp/vault.redb -e CLOISON_VAULT_PASSPHRASE=t \
  -e CLOISON_NER_MODEL_ONNX=/opt/cloison/ner/model-int8.onnx \
  -e CLOISON_NER_TOKENIZER=/opt/cloison/ner/tokenizer.json \
  -e CLOISON_ONNX_LIB=/opt/cloison/ner/libonnxruntime.so \
  debian:bookworm-slim /opt/cloison/cloison-proxy >/dev/null
sleep 4
echo "state=$(docker inspect ner-test --format '{{.State.Status}}')"
curl -s -o /dev/null -w 'chat_http=%{http_code}\n' -X POST http://127.0.0.1:18788/v1/chat/completions \
  -H 'Authorization: Bearer mn_test.x' -H 'Content-Type: application/json' \
  -d '{"model":"x","messages":[{"role":"user","content":"Appelez Xolani Ndlovu au 77 123 45 67"}]}'
sleep 1
echo "ner_actif=$(docker logs ner-test 2>&1 | grep -c 'NER léger embarqué actif')"
echo "ner_echecs=$(docker logs ner-test 2>&1 | grep -c 'inférence échouée')"
docker rm -f ner-test >/dev/null 2>&1 || true
