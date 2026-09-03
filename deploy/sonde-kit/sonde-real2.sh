#!/usr/bin/env bash
# sonde-real2.sh — correction egress edge (bridge) + activations réelles
set -uo pipefail

SLUG="sonde-cloison"
BASE="/opt/hermes/$SLUG"
EDGE="cloison-edge"
LOG="/tmp/sonde-cloison-e2e.log"

exec > >(tee -a "$LOG") 2>&1
echo "=== SONDE REAL2 (egress edge corrige) — $(date -u +%FT%TZ) ==="

docker network connect bridge "$EDGE" && echo "EDGE +BRIDGE (egress amont)" || echo "bridge deja raccorde"
sleep 2

echo "=== ACTIVATION 1/2 : tour trivial ==="
timeout 180 docker exec "$SLUG-agent" hermes -z "Bonjour. Reponds uniquement par le mot OK." 2>&1 | sed -E 's@(mn_)?[a-f0-9]{16,}@<CAVIARDE>@g' | head -25
echo "ACT1_EXIT=${PIPESTATUS[0]:-?}"
echo "=== ACTIVATION 2/2 : roundtrip PII reel ==="
timeout 180 docker exec "$SLUG-agent" hermes -z "Contactez Aminata Diop, aminata.diop@example.sn, tel +221 77 123 45 67, elle habite a Ziguinchor. Reponds en une phrase." 2>&1 | sed -E 's@(mn_)?[a-f0-9]{16,}@<CAVIARDE>@g' | head -30
echo "ACT2_EXIT=${PIPESTATUS[0]:-?}"

echo "=== LOGS EDGE (tail 8) ==="
docker logs "$EDGE" --tail 8 2>&1 | sed -E 's@(mn_)?[a-f0-9]{16,}@<CAVIARDE>@g' || true
echo "=== FIN REAL2 — depouillement : sudo bash /tmp/sonde-cleanup.sh ==="
