#!/usr/bin/env bash
# =============================================================================
# build-cloison-edge.sh — construit l'image LOCALE `mania-cloison-edge` (VPS)
# =============================================================================
# L'image GHCR de CLOISON n'est pas publiée (constat sonde 02-03/09/2026) :
# on construit une image locale à partir de la RELEASE PUBLIQUE cloison-proxy
# (binaire + NER léger + onnxruntime, checksums vérifiés par install-n0.sh),
# sur une base debian:bookworm-slim. C'est l'image que le compose du gabarit
# (nouveau-tenant.sh v4) déploie comme service `${SLUG}-edge` par tenant.
#
# Usage :  sudo bash ops/build-cloison-edge.sh [tag]
# Prérequis : docker, curl, tar, sha256sum, ~200 Mo de disque.
# =============================================================================
set -euo pipefail

TAG="${1:-latest}"
TMP="$(mktemp -d)"
trap 'rm -rf "$TMP"' EXIT
cd "$TMP"

echo "==> téléchargement + installation de la release publique (prefix local)"
curl -fsSL https://raw.githubusercontent.com/coucagog/cloison-proxy/main/install-n0.sh -o install-n0.sh
bash install-n0.sh --prefix "$TMP/prefix"

# Le chemin réel de la lib onnxruntime dépend de la release : on la cherche et
# on l'expose à un chemin stable attendu par le compose (ner/libonnxruntime.so).
# NB : l'installeur la place déjà dans prefix/ner/ — ne copier QUE si différent
# (cp même-fichier retourne 1 → set -e tuerait le script, constat 03/09).
LIB="$(find "$TMP/prefix" -name 'libonnxruntime.so*' | head -1 || true)"
if [ -n "$LIB" ] && [ "$LIB" != "$TMP/prefix/ner/libonnxruntime.so" ]; then
  mkdir -p "$TMP/prefix/ner"
  cp -f "$LIB" "$TMP/prefix/ner/libonnxruntime.so"
fi

cat > Dockerfile <<'EOF'
FROM debian:bookworm-slim
# Copie du bundle release : binaire cloison-proxy + ner/ (modèle ONNX int8,
# tokenizer.json, label_map.json) + ner/libonnxruntime.so (chargée dynamiquement).
COPY prefix/ /opt/cloison/
ENTRYPOINT ["/opt/cloison/cloison-proxy"]
EOF

echo "==> build de l'image mania-cloison-edge:$TAG"
docker build -t "mania-cloison-edge:$TAG" -t mania-cloison-edge:latest .
echo "==> image construite :"
docker images --format '{{.Repository}}:{{.Tag}}' | grep mania-cloison-edge
echo
echo "NB : si la lib onnxruntime n'a pas été trouvée, le NER léger sera inactif"
echo "(dégradation gracieuse : gazetteers + alias) — ajuster CLOISON_ONNX_LIB."
