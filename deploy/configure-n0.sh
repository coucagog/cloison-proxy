#!/usr/bin/env bash
# =============================================================================
# CLOISON N0 — configure-n0.sh : configuration du daemon local (Linux / macOS)
# =============================================================================
# Prérequis : cloison-proxy installé via install-n0.sh (ou présent dans le PATH).
# Ce script génère ~/.cloison/n0.env (0600) + ~/.cloison/start-n0.sh, et
# affiche la clé composite à saisir dans votre interface IA.
# Idempotent : rejouable ; il ne démarre le daemon qu'avec --start.
#
# Usage :
#   bash configure-n0.sh          # interactif
#   bash configure-n0.sh --start  # configure PUIS démarre (premier plan)
# =============================================================================
set -euo pipefail

PREFIX="${CLOISON_PREFIX:-$HOME/.cloison}"
ENV_FILE="$PREFIX/n0.env"
START_SCRIPT="$PREFIX/start-n0.sh"
START=0

for a in "$@"; do
  case "$a" in
    --start) START=1 ;;
    *) echo "usage: $0 [--start]" >&2; exit 2 ;;
  esac
done

command -v cloison-proxy >/dev/null 2>&1 || {
  echo "ERREUR: 'cloison-proxy' introuvable dans le PATH." >&2
  echo "  Installez d'abord :" >&2
  echo "  bash <(curl -fsSL https://raw.githubusercontent.com/coucagog/cloison-proxy/main/install-n0.sh)" >&2
  exit 1
}

mkdir -p "$PREFIX"
chmod 700 "$PREFIX"

read -r -p "Base URL amont [https://openrouter.ai/api/v1] : " UPSTREAM
UPSTREAM="${UPSTREAM:-https://openrouter.ai/api/v1}"

read -r -s -p "Passphrase du coffre (vide = générer) : " PASSPHRASE; echo
if [ -z "$PASSPHRASE" ]; then
  PASSPHRASE="$(openssl rand -base64 24 2>/dev/null || head -c 24 /dev/urandom | base64)"
  echo "  Passphrase générée (stockée dans $ENV_FILE, 0600)."
fi

read -r -p "Jeton d'acces local mn_ (vide = générer) : " TOKEN
if [ -z "$TOKEN" ]; then TOKEN="mn_$(openssl rand -hex 16)"; fi

TENANT_KEY="$(openssl rand -hex 32)"

# NER léger embarqué : ajouté à la config si le bundle est présent.
NER_OPTS=""
if [ -f "$PREFIX/ner/model-int8.onnx" ] && [ -f "$PREFIX/ner/tokenizer.json" ]; then
  NER_OPTS="export CLOISON_NER_MODEL_ONNX=\"$PREFIX/ner/model-int8.onnx\"
export CLOISON_NER_TOKENIZER=\"$PREFIX/ner/tokenizer.json\""
fi

umask 077
cat > "$ENV_FILE" <<EOF
# CLOISON N0 — configuration locale générée par configure-n0.sh ($(date -u +%Y-%m-%dT%H:%M:%SZ))
# NE JAMAIS COMMITER, NE JAMAIS PUBLIER.
# Clé composite à saisir côté client :  ${TOKEN}.<votre clé fournisseur>
export CLOISON_ROLE=edge
export CLOISON_LISTEN_ADDR=127.0.0.1:8787
export CLOISON_UPSTREAM_BASE_URL="$UPSTREAM"
export CLOISON_VAULT_PATH="$PREFIX/vault.redb"
export CLOISON_VAULT_PASSPHRASE='$PASSPHRASE'
export CLOISON_EXPECTED_ACCESS_TOKEN="$TOKEN"
export CLOISON_TENANT_KEY_HEX="$TENANT_KEY"
$NER_OPTS
EOF

cat > "$START_SCRIPT" <<EOF
#!/usr/bin/env bash
# Démarre le daemon CLOISON N0 avec la configuration locale.
set -a; . "$ENV_FILE"; set +a
exec cloison-proxy
EOF
chmod 700 "$START_SCRIPT"

echo
echo "Configuration écrite : $ENV_FILE (0600)"
echo
echo "Clé composite pour votre interface IA :"
echo "  Base URL : http://localhost:8787/v1"
echo "  Clé      : ${TOKEN}.<votre clé fournisseur>"
echo
if [ "$START" = "1" ]; then
  echo "Démarrage du daemon (Ctrl-C pour arrêter)..."
  "$START_SCRIPT"
else
  echo "Pour démarrer :  $START_SCRIPT"
fi
