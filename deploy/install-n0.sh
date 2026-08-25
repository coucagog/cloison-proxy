#!/usr/bin/env bash
# =============================================================================
# CLOISON N0 — Installation du daemon desktop (moteur Rust seul) DEPUIS LES
# RELEASES : binaire + NER léger embarqué (ONNX int8) + lib onnxruntime.
#
# Aucune toolchain Rust, aucun torch : tout est téléchargé depuis la release
# GitHub (charte §4 — N0 = moteur Rust seul ; §12 — install reproductible).
#
# Usage :
#   bash <(curl -fsSL https://raw.githubusercontent.com/coucagog/cloison/main/deploy/install-n0.sh)
#   bash install-n0.sh [--version v0.3.0] [--prefix ~/.cloison] [--skip-ner]
#
#   --version   tag de release (défaut : latest publiée)
#   --prefix    répertoire d'installation (défaut ~/.cloison)
#   --skip-ner  n'installe pas le NER léger (gazetteers + alias seuls — la
#               limite « texte libre » de docs/N0.md §4.1 reste alors assumée)
#
# Après installation : configurez l'environnement (affiché à la fin) puis
# lancez <prefix>/cloison-proxy — voir docs/N0.md §3.
# =============================================================================
set -euo pipefail

# --- Options ----------------------------------------------------------------
VERSION="latest"
PREFIX="${HOME}/.cloison"
SKIP_NER=0
while [[ $# -gt 0 ]]; do
  case "$1" in
    --version) VERSION="$2"; shift 2 ;;
    --prefix)  PREFIX="$2";  shift 2 ;;
    --skip-ner) SKIP_NER=1;  shift ;;
    *) echo "usage: install-n0.sh [--version TAG] [--prefix DIR] [--skip-ner]" >&2; exit 2 ;;
  esac
done

BASE_URL="https://github.com/coucagog/cloison/releases/download"
LATEST_URL="https://github.com/coucagog/cloison/releases/latest/download"
REPO_RAW="https://raw.githubusercontent.com/coucagog/cloison/main"

# --- Détection OS / arch → cible de release ---------------------------------
OS="$(uname -s)"
ARCH="$(uname -m)"
case "$OS-$ARCH" in
  Linux-x86_64)  TARGET="x86_64-unknown-linux-gnu"   LIB_EXT="so" ;;
  Linux-aarch64) TARGET="aarch64-unknown-linux-gnu"  LIB_EXT="so" ;;
  Darwin-x86_64) TARGET="x86_64-apple-darwin"        LIB_EXT="dylib" ;;
  Darwin-arm64)  TARGET="aarch64-apple-darwin"       LIB_EXT="dylib" ;;
  *)
    echo "❌ Cible non publiée ($OS-$ARCH). Windows : utilisez install-n0.ps1. Pour les autres cibles, compilez depuis le dépôt (docs/N0.md §2 — build depuis le code)." >&2
    exit 2 ;;
esac

DL_BIN="$BASE_URL/$VERSION/cloison-proxy-$TARGET"
DL_NER="$BASE_URL/$VERSION/cloison-n0-ner-lite.tar.gz"
DL_LIB="$BASE_URL/$VERSION/cloison-n0-onnxruntime-$TARGET.tar.gz"
DL_SUM="$BASE_URL/$VERSION/checksums.txt"
if [[ "$VERSION" == "latest" ]]; then
  DL_BIN="$LATEST_URL/cloison-proxy-$TARGET"
  DL_NER="$LATEST_URL/cloison-n0-ner-lite.tar.gz"
  DL_LIB="$LATEST_URL/cloison-n0-onnxruntime-$TARGET.tar.gz"
  DL_SUM="$LATEST_URL/checksums.txt"
fi

echo "==> CLOISON N0 — installation daemon desktop (moteur Rust seul)"
echo "    cible : $TARGET   →   $PREFIX"
mkdir -p "$PREFIX" "$PREFIX/ner"
chmod 700 "$PREFIX" "$PREFIX/ner"

need() { command -v "$1" >/dev/null 2>&1 || { echo "❌ outil requis : $1" >&2; exit 2; }; }
need curl; need tar
# SHA-256 : sha256sum (Linux) ou shasum -a 256 (macOS).
if command -v sha256sum >/dev/null 2>&1; then
  SHA_CMD="sha256sum"
elif command -v shasum >/dev/null 2>&1; then
  SHA_CMD="shasum -a 256"
else
  echo "❌ outil requis : sha256sum ou shasum" >&2; exit 2
fi

# --- 1. Binaire --------------------------------------------------------------
BIN="$PREFIX/cloison-proxy"
echo "==> téléchargement du binaire ($TARGET)…"
curl -fsSL -o "$BIN" "$DL_BIN"
chmod 0755 "$BIN"

# --- 2. Checksums (vérification d'intégrité — échec bruyant si absent) ------
SUM_FILE="$PREFIX/checksums.txt"
echo "==> vérification d'intégrité (checksums.txt)…"
curl -fsSL -o "$SUM_FILE" "$DL_SUM"
cd "$PREFIX"
if ! grep -q "cloison-proxy-$TARGET" "$SUM_FILE"; then
  echo "❌ checksums.txt ne référence pas le binaire $TARGET — release incomplète ?" >&2
  exit 1
fi
verify() { # verify <fichier> <nom-dans-checksums>
  local f="$1" n="$2" expected actual
  expected="$(awk -v n="$n" '$2==n {print $1}' "$SUM_FILE")"
  [[ -n "$expected" ]] || { echo "❌ $n absent de checksums.txt" >&2; exit 1; }
  actual="$($SHA_CMD "$f" | awk '{print $1}')"
  [[ "$actual" == "$expected" ]] || { echo "❌ checksum invalide pour $n (attendu $expected, obtenu $actual)" >&2; exit 1; }
}
verify "$BIN" "cloison-proxy-$TARGET"

# --- 3. NER léger embarqué (ONNX int8) + lib onnxruntime --------------------
NER_OK=0
if [[ "$SKIP_NER" == "1" ]]; then
  echo "==> --skip-ner : NER léger non installé (limite « texte libre » assumée)"
else
  echo "==> téléchargement du NER léger (modèle ONNX int8, ~135 Mo)…"
  curl -fsSL -o "$PREFIX/ner-lite.tar.gz" "$DL_NER"
  verify "$PREFIX/ner-lite.tar.gz" "cloison-n0-ner-lite.tar.gz"
  tar -xzf "$PREFIX/ner-lite.tar.gz" -C "$PREFIX/ner"
  rm -f "$PREFIX/ner-lite.tar.gz"

  echo "==> téléchargement de la lib onnxruntime ($OS)…"
  if curl -fsSL -o "$PREFIX/ort.tar.gz" "$DL_LIB" 2>/dev/null; then
    verify "$PREFIX/ort.tar.gz" "cloison-n0-onnxruntime-$TARGET.tar.gz"
    tar -xzf "$PREFIX/ort.tar.gz" -C "$PREFIX/ner"
    rm -f "$PREFIX/ort.tar.gz"
    NER_OK=1
  else
    # Cible sans lib publiée (ex. macOS Intel — microsoft n'archive plus
    # osx-x86_64) : le daemon dégrade gracieusement vers N0 v1 (gazetteers +
    # alias, warn — jamais d'erreur, ARBITRAGE-04 §4.3).
    echo "⚠ lib onnxruntime indisponible pour $TARGET — NER léger désactivé (N0 v1, docs/N0.md §8)"
  fi

  if [[ "$NER_OK" == "1" ]]; then
    ls "$PREFIX/ner/model-int8.onnx" >/dev/null 2>&1 || { echo "❌ modèle introuvable après extraction" >&2; exit 1; }
  fi
fi
rm -f "$SUM_FILE"

# --- 4. Clé locataire (affichée UNE fois — dérive les jetons) ----------------
TENANT_KEY="$(openssl rand -hex 32 2>/dev/null || od -An -N32 -tx1 /dev/urandom | tr -d ' \n')"
echo "==> clé locataire (à conserver précieusement, JAMAIS à committer) :"
echo "    CLOISON_TENANT_KEY_HEX=$TENANT_KEY"

# --- 5. Configuration minimale affichée --------------------------------------
cat <<EOF

==> Configuration N0 (docs/N0.md §3 — à placer dans votre profil / service) :

export CLOISON_ROLE=edge
export CLOISON_LISTEN_ADDR=127.0.0.1:8787
export CLOISON_UPSTREAM_BASE_URL=<votre fournisseur LLM, ex. https://openrouter.ai/api/v1>
export CLOISON_VAULT_PATH=$PREFIX/vault.redb
export CLOISON_VAULT_PASSPHRASE='<VOTRE passphrase — choisie par vous, jamais stockée>'
export CLOISON_EXPECTED_ACCESS_TOKEN=<votre jeton mn_ local>
export CLOISON_TENANT_KEY_HEX=<la clé affichée ci-dessus>
EOF

if [[ "$NER_OK" == "1" ]]; then
  cat <<EOF

# NER léger embarqué (PERSON/LOC in-core — N0 v1.2, ARBITRAGE-04) :
export CLOISON_NER_MODEL_ONNX=$PREFIX/ner/model-int8.onnx
export CLOISON_NER_TOKENIZER=$PREFIX/ner/tokenizer.json
export CLOISON_ONNX_LIB=$PREFIX/ner/libonnxruntime.$LIB_EXT
# export CLOISON_NER_THRESHOLD=0.70   # défaut 0.70 (calibration GO)

# Passphrase via le keychain OS (recommandé — jamais en clair par CLOISON) :
# export CLOISON_VAULT_KEYCHAIN_SERVICE=cloison-n0
EOF
fi

echo ""
echo "==> Lancement : $PREFIX/cloison-proxy"
echo "    Vérification : docs/N0.md §5. Installation terminée ✅"
