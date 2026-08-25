#!/usr/bin/env bash
# =============================================================================
# CLOISON N0 v1.2 — Provisionnement du NER léger embarqué (chantier ④).
#
# Télécharge le modèle `distilbert-base-multilingual-cased-ner-hrl` (export
# ONNX maison int8, 135 Mo) + la lib onnxruntime dans le répertoire N0, afin
# que le daemon desktop détecte PERSON/LOC **in-core** (jamais un sidecar
# Python — charte §4).
#
# Artefacts provisionnés (JAMAIS committés — licence AFL-3.0 du modèle,
# documentée dans journal/ARBITRAGE-04-NER-LEGER.md) :
#   <PREFIX>/ner/model-int8.onnx   (modèle ONNX int8)
#   <PREFIX>/ner/tokenizer.json    (tokenizer HF)
#   <PREFIX>/ner/label_map.json    (id2label)
#   <PREFIX>/ner/libonnxruntime.so (lib onnxruntime, load-dynamic)
#
# Usage : ./deploy/provision_ner_lite.sh [--prefix ~/.cloison] [--venv <python3>]
#   --venv : python3 avec torch/transformers/onnxruntime (défaut : onnxdev
#            du VPS ou un venv équivalent). Requis UNE fois pour l'export.
# Après : configurez CLOISON_NER_MODEL_ONNX / CLOISON_NER_TOKENIZER /
#         CLOISON_ONNX_LIB (docs/N0.md §3, docs/CONFIG.md §1).
# =============================================================================
set -euo pipefail

PREFIX="${1:-$HOME/.cloison}"
PYTHON="${2:-python3}"
NER_DIR="$PREFIX/ner"
MODEL_ID="Davlan/distilbert-base-multilingual-cased-ner-hrl"

echo "==> CLOISON N0 v1.2 — provisionnement NER léger (chantier ④)"
mkdir -p "$NER_DIR"
chmod 700 "$NER_DIR"

# 1. Export ONNX maison + quantisation int8 (mécanique DEPLOY-8/ARBITRAGE-04)
if [[ ! -f "$NER_DIR/model-int8.onnx" ]]; then
  echo "==> export ONNX int8 du modèle ($MODEL_ID) — première exécution (~1-2 min)"
  "$PYTHON" - "$NER_DIR" <<'PYEOF'
import json, sys, time
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
else
  echo "==> modèle int8 déjà présent ($NER_DIR/model-int8.onnx)"
fi

# 2. Lib onnxruntime (load-dynamic — jamais embarquée dans le binaire)
if [[ ! -f "$NER_DIR/libonnxruntime.so" ]]; then
  echo "==> lib onnxruntime absente — installez-la depuis votre environnement"
  echo "    (ex. : cp \$(python3 -c 'import onnxruntime,os;print(os.path.join(os.path.dirname(onnxruntime.__file__),\"capi\",\"libonnxruntime.so\"))') $NER_DIR/)"
else
  echo "==> lib onnxruntime présente ($NER_DIR/libonnxruntime.so)"
fi

echo ""
echo "==> NER léger provisionné dans $NER_DIR"
echo "    Variables à configurer (docs/CONFIG.md §1) :"
echo "      CLOISON_NER_MODEL_ONNX=$NER_DIR/model-int8.onnx"
echo "      CLOISON_NER_TOKENIZER=$NER_DIR/tokenizer.json"
echo "      CLOISON_ONNX_LIB=$NER_DIR/libonnxruntime.so"
echo "      CLOISON_NER_THRESHOLD=0.70   (calibration ARBITRAGE-04)"
