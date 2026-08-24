#!/usr/bin/env bash
# =============================================================================
# CLOISON STACK-7 — E2E anti-pass-through (faux LLM local) + E2E LLM réel.
#
# PHASE MOCK (défaut) — prouve le MASQUAGE AMONT :
#   un faux LLM local (deploy/mock_llm.py, echo + journal du corps reçu) est
#   lancé DANS le réseau docker du compose. Le script ASSERTE que le corps
#   reçu par le faux LLM contient des SENTINELLES CLOISON (⟦…) et PAS la PII
#   en clair, puis que la réponse finale au client contient la PII RESTAURÉE
#   et aucun jeton résiduel. Un proxy pass-through ÉCHOUE ce test (le corps
#   amont contiendrait la PII en clair et zéro sentinelle).
#
# PHASE RÉELLE (CLOISON_E2E_MODE=real|both) — restauration réelle :
#   contre le LLM réel (OpenRouter), assertions restauration + aucun jeton
#   résiduel (le téléphone est comparé en chiffres normalisés : non flaky).
#
# Pré-requis :
#   - docker + compose v2, python3, curl, openssl ;
#   - le monorepo fusionné à la racine (Cargo.toml workspace + crates/ +
#     services/ + deploy/) — exécuter depuis la racine du dépôt.
#
# Usage :
#   deploy/e2e_reel.sh                          # phase mock (aucune clé requise)
#   CLOISON_E2E_MODE=real OPENROUTER_API_KEY=sk-or-v1-... deploy/e2e_reel.sh
#
# Variables :
#   CLOISON_E2E_MODE     mock (défaut) | real | both
#   OPENROUTER_API_KEY   requise en mode real/both (SECRET, environnement seul)
#   CLOISON_UPSTREAM_BASE_URL  (défaut https://openrouter.ai/api/v1)
#   CLOISON_ACCESS_TOKEN       (jeton mn_* local ; généré si absent)
#   OPENROUTER_MODEL           (défaut openai/gpt-4o-mini)
#
# Retour : 0 = succès ; 1 = échec d'assertion ; 2 = pré-requis manquant.
# =============================================================================
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
COMPOSE_FILE="$ROOT/deploy/docker-compose.dev.yml"
BASE_URL="http://127.0.0.1:8787"
MOCK_NAME="cloison-mock-llm"
MOCK_PORT="8799"
E2E_MODE="${CLOISON_E2E_MODE:-mock}"

# --- Pré-requis ----------------------------------------------------------------
for tool in docker curl python3 openssl; do
  command -v "$tool" >/dev/null 2>&1 || {
    echo "ERREUR: '$tool' introuvable dans PATH." >&2
    exit 2
  }
done

case "$E2E_MODE" in
  mock|real|both) ;;
  *) echo "ERREUR: CLOISON_E2E_MODE='$E2E_MODE' invalide (attendu mock|real|both)" >&2; exit 2 ;;
esac

if [ "$E2E_MODE" != "mock" ]; then
  : "${OPENROUTER_API_KEY:?ERREUR: exportez OPENROUTER_API_KEY (clé fournisseur réelle, jamais en fichier)}"
fi

# --- Configuration (défauts sûrs, tout surchargeable par env) ------------------
CLOISON_TENANT_KEY_HEX="${CLOISON_TENANT_KEY_HEX:-$(openssl rand -hex 32)}"
CLOISON_ACCESS_TOKEN="${CLOISON_ACCESS_TOKEN:-mn_$(openssl rand -hex 16)}"
MODEL="${OPENROUTER_MODEL:-openai/gpt-4o-mini}"

# Masquage/restauration actif (AUDIT_MODE=0) : indispensable pour vérifier le
# masquage amont ET la restauration (le mode audit observe-only ne masque pas).
CLOISON_AUDIT_MODE=0

export CLOISON_TENANT_KEY_HEX \
       CLOISON_ACCESS_TOKEN \
       CLOISON_AUDIT_MODE \
       CLOISON_EXPECTED_ACCESS_TOKEN="$CLOISON_ACCESS_TOKEN"

# PII simulée — choisie pour être détectée par les DÉTECTEURS EMBARQUÉS du
# proxy (pas de wiring detect) : "Aminata" (gazetteer nom_sn), "+221 77 123
# 45 67" (regex téléphone sénégalais, exige le préfixe +221), email (regex).
# Toutes surchargeables par env (ex. preuve des préfixes 71/75 — N3).
PII_NAME="${CLOISON_E2E_PII_NAME:-Aminata}"
PII_PHONE="${CLOISON_E2E_PII_PHONE:-+221 77 123 45 67}"
PII_PHONE_DIGITS="${CLOISON_E2E_PII_PHONE_DIGITS:-771234567}"
PII_EMAIL="${CLOISON_E2E_PII_EMAIL:-e2e.cloison@example.com}"
PROMPT="Répète mot pour mot les informations suivantes dans une seule phrase : nom « ${PII_NAME} Diop », téléphone « ${PII_PHONE} », email « ${PII_EMAIL} ». Réponds uniquement avec la phrase répétée, sans commentaire."

# --- Helpers --------------------------------------------------------------------
MOCK_DIR=""
fail=0

# Snapshot des valeurs amont fournies par l'utilisateur : la phase mock écrase
# CLOISON_UPSTREAM_* (URL du faux LLM) — la phase réelle doit retrouver les
# valeurs d'origine (ou le défaut OpenRouter) en mode `both`.
USER_UPSTREAM_BASE_URL="${CLOISON_UPSTREAM_BASE_URL:-}"
USER_UPSTREAM_CHAT_PATH="${CLOISON_UPSTREAM_CHAT_PATH:-}"
USER_UPSTREAM_COMPLETIONS_PATH="${CLOISON_UPSTREAM_COMPLETIONS_PATH:-}"
USER_UPSTREAM_MODELS_PATH="${CLOISON_UPSTREAM_MODELS_PATH:-}"

cleanup() {
  sudo docker rm -f "$MOCK_NAME" >/dev/null 2>&1 || true
  sudo -E docker compose -f "$COMPOSE_FILE" down --remove-orphans >/dev/null 2>&1 || true
  if [ -n "$MOCK_DIR" ]; then rm -rf "$MOCK_DIR"; fi
}
trap cleanup EXIT

step()  { echo "==> $1"; }
ok()    { echo "    [PASS] $1"; }
ko()    { echo "    [FAIL] $1" >&2; fail=1; }

# JSON OpenAI-compatible du prompt (évite tout piège de quoting shell).
build_payload() {
  python3 - "$MODEL" "$PROMPT" <<'PY'
import json, sys
model, prompt = sys.argv[1], sys.argv[2]
print(json.dumps({
    "model": model,
    "messages": [{"role": "user", "content": prompt}],
    "stream": False,
}, ensure_ascii=False))
PY
}

# Extrait choices[0].message.content de la réponse JSON (sortie 3 si invalide).
extract_content() {
  python3 -c '
import sys, json
d = json.load(sys.stdin)
print(d["choices"][0]["message"]["content"])
'
}

# Attente de /v1/models (auth composite) — sortie 1 si timeout.
wait_edge() {  # $1 = header Authorization
  local up=0
  for _ in $(seq 1 60); do
    if curl -fsS -m 5 -H "Authorization: $1" "$BASE_URL/v1/models" >/dev/null 2>&1; then
      up=1
      break
    fi
    sleep 2
  done
  if [ "$up" != "1" ]; then
    echo "ERREUR: edge ne répond pas après 120 s — logs :" >&2
    sudo -E docker compose -f "$COMPOSE_FILE" logs edge >&2 || true
    exit 1
  fi
  ok "edge opérationnel (GET /v1/models)"
}

# Assertions « restauration » sur la réponse client.
assert_restored() {  # $1 = contenu de la réponse
  local content="$1"
  if printf '%s' "$content" | grep -qi -- "$PII_NAME"; then ok "PII restaurée — nom ($PII_NAME)"; else ko "nom ($PII_NAME) absent de la réponse client"; fi
  if printf '%s' "$content" | grep -qF -- "$PII_PHONE"; then ok "PII restaurée — téléphone ($PII_PHONE)"; else ko "téléphone ($PII_PHONE) absent de la réponse client"; fi
  if printf '%s' "$content" | grep -qF -- "$PII_EMAIL"; then ok "PII restaurée — email ($PII_EMAIL)"; else ko "email ($PII_EMAIL) absent de la réponse client"; fi
  if printf '%s' "$content" | python3 -c '
import sys, re
text = sys.stdin.read()
if re.search(r"⟦[a-z2-7]{26}·[A-Z]{2,4}⟧", text):
    sys.exit(1)
'; then
    ok "aucun jeton CLOISON résiduel dans la réponse client"
  else
    ko "sentinelle CLOISON détectée dans la réponse client (restauration incomplète)"
  fi
}

assert_openai_json() {  # $1 = réponse JSON brute
  printf '%s' "$1" | python3 -c '
import sys, json
d = json.load(sys.stdin)
assert "choices" in d and d["choices"], "pas de choices"
msg = d["choices"][0].get("message", {})
assert msg.get("role") == "assistant", "role invalide"
' && ok "réponse OpenAI valide (choices[0].message.role=assistant)" \
  || { ko "réponse OpenAI invalide"; return 1; }
}

# --- Phase 1 : faux LLM local — preuve du MASQUAGE AMONT ------------------------
mock_phase() {
  step "[mock] Lancement du faux LLM local (echo + journal du corps reçu)"
  MOCK_DIR="$(mktemp -d)"
  export CLOISON_UPSTREAM_BASE_URL="http://$MOCK_NAME:$MOCK_PORT"
  export CLOISON_UPSTREAM_CHAT_PATH="/chat/completions"
  export CLOISON_UPSTREAM_COMPLETIONS_PATH="/v1/completions"
  export CLOISON_UPSTREAM_MODELS_PATH="/models"

  if ! sudo -E docker compose -f "$COMPOSE_FILE" up -d --build edge; then
    echo "ERREUR: échec du lancement edge (build ou démarrage) — logs :" >&2
    sudo -E docker compose -f "$COMPOSE_FILE" logs edge >&2 || true
    exit 1
  fi

  # Réseau du compose : projet `cloison-dev` (champ `name:`), réseau interne.
  local net
  net="$(sudo docker network ls --format '{{.Name}}' | grep -x 'cloison-dev_cloison-net' || true)"
  if [ -z "$net" ]; then
    echo "ERREUR: réseau docker 'cloison-dev_cloison-net' introuvable — le service edge n'a pas démarré (échec de build ?). Logs :" >&2
    sudo -E docker compose -f "$COMPOSE_FILE" logs edge >&2 || true
    exit 1
  fi

  sudo docker run -d --rm --name "$MOCK_NAME" \
    --network "$net" \
    --user "$(id -u):$(id -g)" \
    -v "$ROOT/deploy/mock_llm.py:/mock_llm.py:ro" \
    -v "$MOCK_DIR:/mock-data" \
    -e "MOCK_PORT=$MOCK_PORT" \
    -e "MOCK_LOG_FILE=/mock-data/last_body.json" \
    python:3.11-slim python /mock_llm.py >/dev/null

  step "[mock] Attente de /v1/models (auth composite)"
  wait_edge "Bearer ${CLOISON_ACCESS_TOKEN}.mock-upstream-key"

  step "[mock] Requête chat non-stream avec PII simulée"
  local payload resp content mock_body needle
  payload="$(build_payload)"
  resp="$(curl -fsS -m 120 \
    -H "Authorization: Bearer ${CLOISON_ACCESS_TOKEN}.mock-upstream-key" \
    -H "Content-Type: application/json" \
    -d "$payload" \
    "$BASE_URL/v1/chat/completions")"
  content="$(printf '%s' "$resp" | extract_content)" \
    || { ko "réponse non exploitable (choices[0].message.content)"; return 1; }

  echo "    réponse du faux LLM (post-restauration) :"
  printf '    %s\n' "$content" | head -5

  step "[mock] Assertions — restauration côté client"
  assert_restored "$content"
  assert_openai_json "$resp" || true

  step "[mock] Assertions — corps reçu par l'amont (preuve anti-pass-through)"
  if [ -s "$MOCK_DIR/last_body.json" ]; then
    ok "le faux LLM a journalisé le corps reçu"
  else
    ko "aucun corps journalisé par le faux LLM ($MOCK_DIR/last_body.json absent/vide)"
  fi
  mock_body="$(cat "$MOCK_DIR/last_body.json" 2>/dev/null || true)"

  # 1. Le corps amont contient des SENTINELLES CLOISON.
  if printf '%s' "$mock_body" | grep -q '⟦'; then
    ok "corps amont tokenisé (sentinelles ⟦ présentes)"
  else
    ko "aucune sentinelle ⟦ dans le corps reçu par l'amont — le proxy ne masque rien"
  fi

  # 2. Le corps amont ne contient PAS la PII en clair.
  for needle in "$PII_NAME" "$PII_PHONE" "$PII_EMAIL"; do
    if printf '%s' "$mock_body" | grep -qF -- "$needle"; then
      ko "PII en clair « $needle » présente dans le corps reçu par l'amont"
    else
      ok "PII « $needle » absente du corps amont (masquée)"
    fi
  done

  sudo docker rm -f "$MOCK_NAME" >/dev/null 2>&1 || true
  sudo -E docker compose -f "$COMPOSE_FILE" down --remove-orphans >/dev/null 2>&1 || true
  rm -rf "$MOCK_DIR"; MOCK_DIR=""
}

# --- Phase 2 : LLM réel — restauration réelle ------------------------------------
real_phase() {
  step "[réel] Lancement du proxy edge (amont: ${USER_UPSTREAM_BASE_URL:-https://openrouter.ai/api/v1})"
  export CLOISON_UPSTREAM_BASE_URL="${USER_UPSTREAM_BASE_URL:-https://openrouter.ai/api/v1}"
  export CLOISON_UPSTREAM_CHAT_PATH="${USER_UPSTREAM_CHAT_PATH:-/chat/completions}"
  export CLOISON_UPSTREAM_COMPLETIONS_PATH="${USER_UPSTREAM_COMPLETIONS_PATH:-/v1/completions}"
  export CLOISON_UPSTREAM_MODELS_PATH="${USER_UPSTREAM_MODELS_PATH:-/models}"

  if ! sudo -E docker compose -f "$COMPOSE_FILE" up -d --build edge; then
    echo "ERREUR: échec du lancement edge (build ou démarrage) — logs :" >&2
    sudo -E docker compose -f "$COMPOSE_FILE" logs edge >&2 || true
    exit 1
  fi

  local auth="Bearer ${CLOISON_ACCESS_TOKEN}.${OPENROUTER_API_KEY}"
  step "[réel] Attente de /v1/models (auth composite)"
  wait_edge "$auth"

  step "[réel] Requête chat non-stream avec PII simulée ($MODEL)"
  local payload resp content
  payload="$(build_payload)"
  resp="$(curl -fsS -m 120 \
    -H "Authorization: $auth" \
    -H "Content-Type: application/json" \
    -d "$payload" \
    "$BASE_URL/v1/chat/completions")"
  content="$(printf '%s' "$resp" | extract_content)" \
    || { ko "réponse non exploitable (choices[0].message.content)"; return 1; }

  echo "    réponse du modèle (post-restauration) :"
  printf '    %s\n' "$content" | head -5

  step "[réel] Assertions"
  if printf '%s' "$content" | grep -qi -- "$PII_NAME"; then ok "PII restaurée — nom ($PII_NAME)"; else ko "nom ($PII_NAME) absent de la réponse"; fi
  # Téléphone : comparaison sur les CHIFFRES uniquement (reformulation possible).
  if printf '%s' "$content" | python3 -c '
import re, sys
digits = re.sub(r"\D", "", sys.stdin.read())
needle = sys.argv[1]
sys.exit(0 if needle in digits else 1)
' "$PII_PHONE_DIGITS"; then
    ok "PII restaurée — téléphone (chiffres $PII_PHONE_DIGITS)"
  else
    ko "chiffres téléphone ($PII_PHONE_DIGITS) absents de la réponse"
  fi
  if printf '%s' "$content" | grep -qF -- "$PII_EMAIL"; then ok "PII restaurée — email ($PII_EMAIL)"; else ko "email ($PII_EMAIL) absent de la réponse"; fi
  if printf '%s' "$content" | python3 -c '
import sys, re
text = sys.stdin.read()
if re.search(r"⟦[a-z2-7]{26}·[A-Z]{2,4}⟧", text):
    sys.exit(1)
'; then
    ok "aucun jeton CLOISON résiduel dans la réponse"
  else
    ko "sentinelle CLOISON détectée dans la réponse (restauration incomplète)"
  fi
  assert_openai_json "$resp" || true

  sudo -E docker compose -f "$COMPOSE_FILE" down --remove-orphans >/dev/null 2>&1 || true
}

# --- Exécution --------------------------------------------------------------------
echo "==> Mode E2E : $E2E_MODE"

case "$E2E_MODE" in
  mock) mock_phase || true ;;
  real) real_phase || true ;;
  both) mock_phase || true; real_phase || true ;;
esac

echo
if [ "$fail" = "0" ]; then
  echo "==> E2E : SUCCÈS — masquage amont prouvé (sentinelles, pas de PII) et PII restaurée côté client."
  exit 0
fi
echo "==> E2E : ÉCHEC (voir ci-dessus)." >&2
exit 1
