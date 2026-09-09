#!/usr/bin/env bash
# Benchmark CLOISON N0 : latence de détection vs taille de contexte.
# Prérequis : daemon N0 qui écoute sur 127.0.0.1:8787 (voir install-cloison-omarchy.md),
# payloads générés (python3 gen_bench.py), clé composite mn_<jeton>.<clé>.
# Usage : bench-hote.sh [jeton]
set -u
TOKEN="${1:-mn_poc-token.sk-fake}"
OUT="${BENCH_OUT:-/home/mls/MesProjets/cloison-poc/bench-resultats.txt}"
AUTH="Authorization: Bearer $TOKEN"
CT="Content-Type: application/json"
BASE="http://127.0.0.1:8787/v1/chat/completions"

echo "Benchmark CLOISON N0 — $(date -Is)" | tee "$OUT"
echo "Endpoint : $BASE" | tee -a "$OUT"

for sz in 10k 50k 100k; do
  f="/home/mls/MesProjets/cloison-poc/bench/payload-$sz.json"
  echo "--- ~$sz tokens ---" | tee -a "$OUT"
  for run in 1 2; do
    t=$(curl -s -o /dev/null -w "%{time_total}" -m 400 "$BASE" -H "$AUTH" -H "$CT" --data-binary @"$f")
    echo "run $run : ${t}s" | tee -a "$OUT"
  done
done
echo "Terminé. Résultats dans $OUT"
