#!/usr/bin/env bash
# =============================================================================
# CLOISON N0 — Installation du daemon desktop local (moteur Rust seul).
#
# Ce que fait le script :
#   1. Construit le binaire `cloison-proxy` (mode N0 = même binaire, rôle edge) ;
#   2. Crée le répertoire ~/.cloison (0600) ;
#   3. Génère la clé locataire (jamais persistée en clair — affichée une fois) ;
#   4. Affiche la configuration N0 à placer dans votre environnement
#      (la passphrase du coffre est fournie par VOUS au premier lancement —
#      jamais générée, jamais stockée par ce script).
#
# Usage : ./deploy/install-n0.sh [--prefix /opt/cloison]
# Après installation : suivez `docs/N0.md` §3 pour lancer le daemon.
# =============================================================================
set -euo pipefail

PREFIX="${1:-$HOME/.cloison}"
BIN_SRC="$(cd "$(dirname "$0")/.." && pwd)/target/release/cloison-proxy"

echo "==> CLOISON N0 — installation daemon desktop"
mkdir -p "$PREFIX"
chmod 700 "$PREFIX"

# 1. Construire le binaire (mode release, rôle edge)
if [[ ! -x "$BIN_SRC" ]]; then
  echo "==> build du binaire (release)…"
  cargo build --release -p cloison-proxy
fi
install -m 0755 "$BIN_SRC" "$PREFIX/cloison-proxy"
echo "==> binaire : $PREFIX/cloison-proxy"

# 2. Clé locataire (affichée UNE fois — c'est elle qui dérive les jetons)
TENANT_KEY="$(openssl rand -hex 32)"
echo "==> clé locataire (à conserver précieusement, jamais à committer) :"
echo "    CLOISON_TENANT_KEY_HEX=$TENANT_KEY"

# 3. Configuration minimale affichée
cat <<'EOF'

==> Configuration N0 (à placer dans votre profil / service, voir docs/N0.md) :

export CLOISON_ROLE=edge
export CLOISON_LISTEN_ADDR=127.0.0.1:8787
export CLOISON_UPSTREAM_BASE_URL=<votre fournisseur LLM, ex. https://openrouter.ai/api/v1>
export CLOISON_VAULT_PATH=~/.cloison/vault.redb
export CLOISON_VAULT_PASSPHRASE='<VOTRE passphrase — choisie par vous, jamais stockée>'
export CLOISON_EXPECTED_ACCESS_TOKEN=<votre jeton mn_ local>
export CLOISON_TENANT_KEY_HEX=<la clé affichée ci-dessus>

# N0 v1.1 (optionnel — défauts documentés dans docs/N0.md §3) :
# export CLOISON_ALIAS_EXPANSION=1        # alias intra-session (défaut 1)
# export CLOISON_QUASI_ID_GAUGE=1         # jauge quasi-id (défaut 0 = off)
# export CLOISON_QUASI_ID_THRESHOLD=0.5   # seuil de la jauge (défaut 0.5)
# export CLOISON_VAULT_KEYCHAIN_SERVICE=cloison-n0  # passphrase via le keychain OS
#                                                   # (sinon CLOISON_VAULT_PASSPHRASE)

Lancement : $PREFIX/cloison-proxy
EOF

echo "==> Installation N0 terminée. Le coffre persistant sera créé au premier lancement (fail-loud si la passphrase ne correspond pas à un coffre existant)."
