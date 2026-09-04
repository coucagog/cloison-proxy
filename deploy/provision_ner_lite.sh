#!/usr/bin/env bash
# =============================================================================
# CLOISON N0 v1.2 — Provisionnement du NER léger embarqué (chantier ④).
#
# Par défaut : TÉLÉCHARGE les artefacts publiés (modèle ONNX int8 + tokenizer
# + label_map + lib onnxruntime) depuis la release GitHub — aucun torch requis
# (installation grand public, ≤ 10 min).
#
#   ./deploy/provision_ner_lite.sh [--prefix ~/.cloison] [--version v0.3.0]
#
# Mode avancé --export (reproductibilité ARBITRAGE-04) : ré-exporte le modèle
# `Davlan/distilbert-base-multilingual-cased-ner-hrl` en ONNX int8 depuis
# Hugging Face (torch/transformers/onnxruntime requis UNE fois) :
#
#   ./deploy/provision_ner_lite.sh --export [--venv python3] [--prefix ~/.cloison]
#
# Artefacts (JAMAIS committés — licence AFL-3.0 du modèle, notice dans le
# bundle publié et journal/ARBITRAGE-04-NER-LEGER.md) :
#   <PREFIX>/ner/model-int8.onnx · tokenizer.json · label_map.json ·
#   special_tokens_map.json · tokenizer_config.json · vocab.txt ·
#   libonnxruntime.{so|dylib|dll} (load-dynamic)
#
# Après : CLOISON_NER_MODEL_ONNX / CLOISON_NER_TOKENIZER / CLOISON_ONNX_LIB
# (docs/N0.md §3, docs/CONFIG.md §1).
# =============================================================================
set -euo pipefail

PREFIX="${HOME}/.cloison"
VERSION="latest"
MODE="download"
PYTHON="python3"
while [[ $# -gt 0 ]]; do
  case "$1" in
    --export)  MODE="export"; shift ;;
    --prefix)  PREFIX="$2"; shift 2 ;;
    --version) VERSION="$2"; shift 2 ;;
    --venv)    PYTHON="$2"; shift 2 ;;
    *) echo "usage: provision_ner_lite.sh [--export] [--prefix DIR] [--version TAG] [--venv python3]" >&2; exit 2 ;;
  esac
done

NER_DIR="$PREFIX/ner"
echo "==> CLOISON N0 — provisionnement NER léger (mode $MODE)"
mkdir -p "$NER_DIR"
chmod 700 "$NER_DIR"

if [[ "$MODE" == "download" ]]; then
  # --- Téléchargement des artefacts publiés (aucun torch) -------------------
  need() { command -v "$1" >/dev/null 2>&1 || { echo "❌ outil requis : $1" >&2; exit 2; }; }
  need curl; need tar
  BASE_URL="https://github.com/coucagog/cloison/releases/download"
  LATEST_URL="https://github.com/coucagog/cloison/releases/latest/download"

  OS="$(uname -s)"
  case "$OS" in
    Linux)  LIB="libonnxruntime.so" ;;
    Darwin) LIB="libonnxruntime.dylib" ;;
    *) echo "❌ OS non supporté par le téléchargement ($OS) — Windows : install-n0.ps1 ; sinon --export" >&2; exit 2 ;;
  esac

  ner="$BASE_URL/$VERSION/cloison-n0-ner-lite.tar.gz"
  sum="$BASE_URL/$VERSION/checksums.txt"
  [[ "$VERSION" == "latest" ]] && ner="$LATEST_URL/cloison-n0-ner-lite.tar.gz" && sum="$LATEST_URL/checksums.txt"

  curl -fsSL -o "$NER_DIR/ner.tgz" "$ner"
  if command -v sha256sum >/dev/null 2>&1; then
    curl -fsSL -o "$NER_DIR/checksums.txt" "$sum"
    exp="$(awk '$2=="cloison-n0-ner-lite.tar.gz" {print $1}' "$NER_DIR/checksums.txt")"
    act="$(sha256sum "$NER_DIR/ner.tgz" | awk '{print $1}')"
    [[ -n "$exp" && "$exp" == "$act" ]] || { echo "❌ checksum du bundle NER invalide" >&2; exit 1; }
    rm -f "$NER_DIR/checksums.txt"
  fi
  tar -xzf "$NER_DIR/ner.tgz" -C "$NER_DIR"
  rm -f "$NER_DIR/ner.tgz"
  echo "==> bundle NER extrait dans $NER_DIR"

  if [[ ! -f "$NER_DIR/$LIB" ]]; then
    echo "==> lib onnxruntime absente — téléchargement de la lib publiée…"
    ort="$BASE_URL/$VERSION/cloison-n0-onnxruntime-x86_64-unknown-linux-gnu.tar.gz"
    [[ "$VERSION" == "latest" ]] && ort="$LATEST_URL/cloison-n0-onnxruntime-x86_64-unknown-linux-gnu.tar.gz"
    curl -fsSL -o "$NER_DIR/ort.tgz" "$ort"
    tar -xzf "$NER_DIR/ort.tgz" -C "$NER_DIR"
    rm -f "$NER_DIR/ort.tgz"
  fi
elif [[ "$MODE" == "export" ]]; then
  # --- Export ONNX maison + quantisation int8 (mécanique DEPLOY-8/ARBITRAGE-04)
  if [[ -f "$NER_DIR/model-int8.onnx" ]]; then
    echo "==> modèle int8 déjà présent ($NER_DIR/model-int8.onnx)"
  else
    echo "==> export ONNX int8 (torch requis — ~1-2 min, première exécution)"
    "$PYTHON" - "$NER_DIR" <<'PYEOF'
import json, sys
from pathlib import Path
import torch
from transformers import AutoModelForTokenClassification, AutoTokenizer
from onnxruntime.quantization import QuantType, quantize_dynamic

out = Path(sys.argv[1])
model_id = "Davlan/distilbert-base-multilingual-cased-ner-hrl"
print("chargement modèle + tokenizer…")
tok = AutoTokenizer.from_pretrained(model_id)
model = AutoModelForTokenClassification.from_pretrained(model_id)
model.eval()
(out / "label_map.json").write_text(json.dumps(model.config.id2label, ensure_ascii=False))
tok.save_pretrained(str(out))

dummy = tok("texte d'exemple", return_tensors="pt", truncation=True, max_length=512)
fp32 = out / "model.onnx"
print("export fp32 (opset 17)…")
torch.onnx.export(model, (dummy["input_ids"], dummy["attention_mask"]), str(fp32),
    input_names=["input_ids", "attention_mask"], output_names=["logits"],
    dynamic_axes={"input_ids": {0: "batch", 1: "seq"},
                  "attention_mask": {0: "batch", 1: "seq"},
                  "logits": {0: "batch", 1: "seq"}},
    opset_version=17, do_constant_folding=True)
print("quantisation int8…")
quantize_dynamic(str(fp32), str(out / "model-int8.onnx"), weight_type=QuantType.QInt8)
fp32.unlink(missing_ok=True)
print(f"export terminé : {sorted(p.name for p in out.iterdir())}")
PYEOF
  fi
  if [[ ! -f "$NER_DIR/libonnxruntime.so" ]]; then
    echo "==> lib onnxruntime absente — copiez-la depuis votre environnement :"
    echo "    cp \$(python3 -c 'import onnxruntime,os;print(os.path.join(os.path.dirname(onnxruntime.__file__),\"capi\",\"libonnxruntime.so\"))') $NER_DIR/"
  fi
fi

echo ""
echo "==> NER léger provisionné dans $NER_DIR"
echo "    Variables à configurer (docs/CONFIG.md §1) :"
echo "      CLOISON_NER_MODEL_ONNX=$NER_DIR/model-int8.onnx"
echo "      CLOISON_NER_TOKENIZER=$NER_DIR/tokenizer.json"
echo "      CLOISON_ONNX_LIB=$NER_DIR/libonnxruntime.{so|dylib|dll}"
echo "      CLOISON_NER_THRESHOLD=0.70   (calibration ARBITRAGE-04)"
