#!/usr/bin/env bash
# =============================================================================
# CLOISON N0 — Assemble et publie les ARTEFACTS de la release (exécuté depuis
# le VPS, APRÈS que la CI `release-n0` ait créé la release avec les binaires).
#
#   ./deploy/n0-release-assets.sh v0.3.0
#
# Publie sur la release du tag :
#   cloison-n0-ner-lite.tar.gz                       (modèle ONNX int8 + tokenizer
#                                                     + label_map + notice licence
#                                                     AFL-3.0 — artefacts validés
#                                                     du volume docker detect)
#   cloison-n0-onnxruntime-<target>.tar.gz ×4       (lib onnxruntime 1.29.0 :
#                                                     Linux depuis onnxdev, Win/macOS
#                                                     depuis les archives officielles
#                                                     microsoft/onnxruntime v1.29.0)
#   checksums.txt                                    (SHA-256 de tous les assets)
#
# Puis PUBLIE la release (draft=false) — la CI la crée en draft pour éviter un
# « latest » incomplet.
#
# Charte §12 : tout est scripté et réexécutable ; le jeton GitHub est lu dans
# ~/.git-credentials (0600) et JAMAIS affiché. Zéro PII / zéro secret publié.
# =============================================================================
set -euo pipefail

TAG="${1:?usage: n0-release-assets.sh <tag, ex. v0.3.0>}"
REPO="coucagog/cloison"
API="https://api.github.com/repos/$REPO"
ORT_VER="1.29.0"   # épinglé : version validée du modèle exporté (DEPLOY-8/10)
WORK="$(mktemp -d)"
trap 'rm -rf "$WORK"' EXIT

# --- Jeton GitHub (jamais affiché) -------------------------------------------
TOKEN="$(sed -nE 's#https://[^:]+:([^@]+)@github\.com#\1#p' "$HOME/.git-credentials" | head -1)"
[[ -n "$TOKEN" ]] || { echo "❌ jeton GitHub introuvable dans ~/.git-credentials" >&2; exit 1; }

# --- 1. Bundle NER léger (artefacts validés du volume detect) ----------------
# Le volume docker est root-only : accès via sudo (le script tourne sur le VPS,
# debian NOPASSWD — charte §12).
NER_SRC="/var/lib/docker/volumes/cloison-dev_detect-models/_data/ner-lite-distil"
sudo test -f "$NER_SRC/model-int8.onnx" || { echo "❌ $NER_SRC/model-int8.onnx absent" >&2; exit 1; }

cat > "$WORK/NOTICE.txt" <<'EOF'
CLOISON N0 — NER léger embarqué (artefact provisionné, jamais committé)

Modèle  : Davlan/distilbert-base-multilingual-cased-ner-hrl
Source  : https://huggingface.co/Davlan/distilbert-base-multilingual-cased-ner-hrl
Licence : AFL-3.0 (Academic Free License v3.0)
Usage   : détection PERSON/LOC in-core du daemon N0 via ONNX Runtime
          (charte §4 — jamais un sidecar Python). Export ONNX int8 réalisé
          par CLOISON (mécanique DEPLOY-8/ARBITRAGE-04) ; le modèle est un
          checkpoint public, aucune donnée client.
Réf.    : journal/ARBITRAGE-04-NER-LEGER.md (verdict GO pré-enregistré)
EOF

echo "==> assemblage du bundle NER léger…"
sudo cp "$NER_SRC/model-int8.onnx" "$NER_SRC/tokenizer.json" "$NER_SRC/label_map.json" \
   "$NER_SRC/special_tokens_map.json" "$NER_SRC/tokenizer_config.json" "$NER_SRC/vocab.txt" "$WORK/"
cp "$WORK/NOTICE.txt" "$WORK/LICENSE-AFL-3.0.txt"
tar -czf "$WORK/cloison-n0-ner-lite.tar.gz" -C "$WORK" \
   model-int8.onnx tokenizer.json label_map.json special_tokens_map.json \
   tokenizer_config.json vocab.txt NOTICE.txt LICENSE-AFL-3.0.txt

# --- 2. Libs onnxruntime 1.29.0 (load-dynamic, une par cible publiée) --------
# Linux : lib du conteneur onnxdev (même version que le modèle exporté).
echo "==> lib onnxruntime Linux…"
sudo docker cp onnxdev:/usr/local/lib/python3.11/site-packages/onnxruntime/capi/libonnxruntime.so.1.29.0 "$WORK/libonnxruntime.so" 2>/dev/null \
  || sudo docker exec onnxdev sh -c 'cat /usr/local/lib/python3.11/site-packages/onnxruntime/capi/libonnxruntime.so.1.29.0' > "$WORK/libonnxruntime.so"
tar -czf "$WORK/cloison-n0-onnxruntime-x86_64-unknown-linux-gnu.tar.gz" -C "$WORK" libonnxruntime.so

# Windows / macOS : archives officielles microsoft/onnxruntime (épinglées).
# NB : microsoft ne publie PLUS d'archive osx-x86_64 (Intel) depuis ~1.27 —
# la cible macos-x64 du binaire est fournie sans lib (dégradation gracieuse
# N0 v1 : gazetteers + alias, warn — voir docs/N0.md §8).
echo "==> libs onnxruntime Windows + macOS arm64 (archives officielles)…"
dl_win()  { curl -fsSL --retry 3 --retry-delay 2 -o "$WORK/ort-win.zip"  "https://github.com/microsoft/onnxruntime/releases/download/v$ORT_VER/onnxruntime-win-x64-$ORT_VER.zip"; }
dl_osx()  { curl -fsSL --retry 3 --retry-delay 2 -o "$WORK/ort-osx.tgz"  "https://github.com/microsoft/onnxruntime/releases/download/v$ORT_VER/onnxruntime-osx-$1-$ORT_VER.tgz"; }

dl_win
python3 -c 'import zipfile,sys; zipfile.ZipFile(sys.argv[1]).extractall(sys.argv[2])' "$WORK/ort-win.zip" "$WORK/win"
cp "$WORK/win/onnxruntime-win-x64-$ORT_VER/lib/onnxruntime.dll" "$WORK/onnxruntime.dll"
tar -czf "$WORK/cloison-n0-onnxruntime-x86_64-pc-windows-msvc.tar.gz" -C "$WORK" onnxruntime.dll

dl_osx arm64
tar -xzf "$WORK/ort-osx.tgz" -C "$WORK"
cp "$WORK/onnxruntime-osx-arm64-$ORT_VER/lib/libonnxruntime.dylib" "$WORK/libonnxruntime.dylib"
tar -czf "$WORK/cloison-n0-onnxruntime-aarch64-apple-darwin.tar.gz" -C "$WORK" libonnxruntime.dylib

# --- 3. Checksums -------------------------------------------------------------
echo "==> checksums…"
(cd "$WORK" && sha256sum cloison-n0-ner-lite.tar.gz cloison-n0-onnxruntime-*.tar.gz > checksums.txt)

# --- 4. Publication via l'API GitHub ------------------------------------------
RELEASE_ID="$(curl -fsSL -H "Authorization: token $TOKEN" "$API/releases/tags/$TAG" | sed -nE 's/.*"id": ([0-9]+),.*/\1/p' | head -1)"
[[ -n "$RELEASE_ID" ]] || { echo "❌ release $TAG introuvable (la CI l'a-t-elle créée ?)" >&2; exit 1; }

upload() { # upload <fichier>
  local f="$1" name
  name="$(basename "$f")"
  echo "==> upload $name ($(du -h "$f" | cut -f1))"
  curl -fsSL -X POST -H "Authorization: token $TOKEN" \
    -H "Content-Type: application/octet-stream" \
    --data-binary "@$f" \
    "$API/releases/$RELEASE_ID/assets?name=$name" >/dev/null
}

for f in "$WORK"/cloison-n0-ner-lite.tar.gz "$WORK"/cloison-n0-onnxruntime-*.tar.gz "$WORK"/checksums.txt; do
  upload "$f"
done

# --- 5. Publication de la release (draft → publiée) ---------------------------
echo "==> publication de la release ($TAG)…"
curl -fsSL -X PATCH -H "Authorization: token $TOKEN" \
  -H "Content-Type: application/json" \
  -d '{"draft":false}' \
  "$API/releases/$RELEASE_ID" >/dev/null

echo "==> vérification des assets publiés :"
curl -fsSL -H "Authorization: token $TOKEN" "$API/releases/$RELEASE_ID/assets" \
  | sed -nE 's/.*"name": "([^"]+)".*/\1/p' | sort
echo "==> n0-release-assets terminé ✅ (release $TAG complète)"
