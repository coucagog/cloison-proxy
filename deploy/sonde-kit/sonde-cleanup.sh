#!/usr/bin/env bash
# =============================================================================
# sonde-cleanup.sh — dépouillement complet de la sonde CLOISON×Hermes
# Usage : sudo bash /tmp/sonde-cleanup.sh
# =============================================================================
set -euo pipefail

SLUG="sonde-cloison"
BASE="/opt/hermes/$SLUG"
NET="${SLUG}_${SLUG}-net"
EDGE="cloison-edge"

echo "=== DEPOUILLEMENT SONDE ==="

if [ -f "$BASE/docker-compose.yml" ]; then
  ( cd "$BASE" && sudo docker compose down -v --remove-orphans ) || true
fi
sudo rm -rf "$BASE" || true
docker network rm "$NET" >/dev/null 2>&1 || echo "reseau $NET deja absent"
docker rm -f "$EDGE" >/dev/null 2>&1 || echo "conteneur $EDGE deja absent"
docker rm -f mock-llm >/dev/null 2>&1 || echo "conteneur mock-llm deja absent"
docker volume rm cloison-edge-audit >/dev/null 2>&1 || echo "volume cloison-edge-audit deja absent"
docker rmi debian:bookworm-slim >/dev/null 2>&1 || echo "image debian:bookworm-slim deja absente"
docker rmi python:3.11-slim >/dev/null 2>&1 || echo "image python:3.11-slim deja absente"
rm -rf /tmp/cloison-n0 /tmp/install-n0.sh /tmp/mock-llm.py || true
rm -f /tmp/sonde-cloison-e2e.sh /tmp/sonde-phase2.sh /tmp/sonde-phase3.sh /tmp/sonde-phase4.sh /tmp/sonde-cleanup.sh /tmp/recon-mania.sh

echo "--- etat final ---"
docker ps -a --format '{{.Names}}' | grep -E "^($SLUG|$EDGE)" && echo "RESIDU RESTANT !" || echo "aucun residu sonde/edge"
ls -d "$BASE" 2>/dev/null && echo "RESIDU DOSSIER !" || echo "dossier $BASE absent"
docker network ls --format '{{.Name}}' | grep -qx "$NET" && echo "RESIDU RESEAU !" || echo "reseau $NET absent"
echo "=== DEPOUILLEMENT TERMINE ==="
# NB : l'image ghcr.io/coucagog/cloison-proxy:edge est conservée (réutilisable, ~Mo).
