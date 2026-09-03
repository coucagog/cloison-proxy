#!/usr/bin/env bash
# recon-mania.sh — inventaire LECTURE SEULE du VPS Mania (51.38.179.242)
# Aucune écriture, aucun secret affiché. Usage : bash /tmp/recon-mania.sh
set -u

echo "=== docker ps (conteneurs) ==="
docker ps --format '{{.Names}}  {{.Image}}  {{.Status}}'

echo
echo "=== images hermes/cloison/mania ==="
docker images --format '{{.Repository}}:{{.Tag}}' | grep -Ei 'cloison|hermes|mania' || echo "(aucune image cloison/hermes/mania)"

echo
echo "=== reseaux docker ==="
docker network ls --format '{{.Name}} {{.Driver}} {{.Internal}}'

echo
echo "=== gabarit (/opt/hermes/gabarit) ==="
ls -la /opt/hermes/gabarit/ | head -40
echo "-- git ? --"
ls -d /opt/hermes/gabarit/.git 2>/dev/null || echo "(pas de .git dans le gabarit)"

echo
echo "=== cloison sur l hote ==="
command -v cloison-proxy 2>/dev/null || echo "(cloison-proxy absent du PATH)"
ls /opt/cloison 2>/dev/null || echo "(pas de /opt/cloison)"

echo
echo "=== outils dans un conteneur agent (lecture seule) ==="
docker exec ridwan-agent sh -c 'command -v curl; command -v wget; command -v python3' 2>/dev/null || echo "(exec impossible)"

echo
echo "=== hermes version (conteneur ridwan-agent) ==="
docker exec ridwan-agent sh -c 'hermes --version 2>/dev/null || hermes -V 2>/dev/null' 2>/dev/null | head -3

echo
echo "=== config.yaml (lignes structurelles uniquement) ==="
docker exec ridwan-agent sh -c "grep -nE '_config_version|^model|^  provider|^stt|key_env' /home/hermes/.hermes/config.yaml 2>/dev/null | head -20" 2>/dev/null || echo "(lecture impossible)"

echo
echo "=== fin recon ==="
