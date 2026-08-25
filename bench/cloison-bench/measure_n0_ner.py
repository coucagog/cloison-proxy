#!/usr/bin/env python3
"""Mesure d'arbitrage du chantier ④ (journal/ARBITRAGE-04-NER-LEGER.md).

Compare, sur le jeu STACK-1 (seed 42, 500 docs) :
  A — état N0 actuel : détection embarquée du core (regex + gazetteers +
      Luhn), fidèle au produit N0 (les noms/toponymes des gazetteers sont
      masqués) — SANS sidecar NER ;
  B — N0 + NER léger embarqué (candidat mBERT NER-hrl ONNX int8) : spans
      PERSON/LOC du NER fusionnés à la détection embarquée, avec la règle
      de fusion englobante N0 (un span NER complet prime sur les spans
      gazetteer partiels qu'il englobe).

Scoring : scoring.py (exact-match strict start/end/type, spécificité au
niveau document). Critères GO/NO-GO : ARBITRAGE-04 §2 (C1–C5).

Usage (VPS, venv bench/onnxdev, modèles provisionnés) :
  CLOISON_NER_MODEL_ONNX=/models/ner-lite/model-int8.onnx \
  CLOISON_NER_TOKENIZER=/models/ner-lite/tokenizer.json \
  CLOISON_CORE_BIN=/home/debian/Cloison/cloison/target/debug/detect_cli \
  python3 measure_n0_ner.py

Sans CLOISON_NER_MODEL_ONNX : seule la mesure A est produite.
Sortie : JSON sur stdout (resultats A et B, critères, verdict).
"""
from __future__ import annotations

import json
import os
import subprocess
import sys
import time
from pathlib import Path

ROOT = Path(__file__).resolve().parent
DATASET = ROOT / "results" / "dataset.jsonl"
RAPPORT = ROOT / "results" / "rapport.json"

CORE_BIN = os.environ.get(
    "CLOISON_CORE_BIN", str(ROOT.parent.parent / "target" / "debug" / "detect_cli")
)
NER_ONNX = os.environ.get("CLOISON_NER_MODEL_ONNX", "")
NER_TOKENIZER = os.environ.get("CLOISON_NER_TOKENIZER", "")

# Mapping core -> grille (miroir de run_detect_target.py + nom_sn -> PERSON :
# le produit N0 masque réellement les noms du gazetteer — les compter est
# FIDÈLE au produit ; l'exact-match sera dur (spans partiels) et c'est
# précisément le fossé que le NER comble).
CORE_TYPE_MAP = {
    "Email": "MAIL", "PhoneSn": "TEL", "CniSn": "CNI",
    "CreditCard": "CREDIT_CARD", "Ip": "IP", "Date": "DATE",
    "Gazetteer(ville_sn)": "LOC",
    "Gazetteer(nom_sn)": "PERSON",
}
CORE_KEEP = ("CNI", "MAIL", "TEL", "LOC", "PERSON")

NER_THRESHOLD = 0.50  # seuil de score minimal du NER (calibration candidat)
NER_TYPES = ("PERSON", "LOC")


def load_gold() -> list[dict]:
    return [json.loads(line) for line in DATASET.read_text().splitlines()]


def core_spans(text: str) -> list[dict]:
    """Spans du core (detect_cli) mappés vers la grille, offsets points de code."""
    if not Path(CORE_BIN).exists():
        return []
    try:
        r = subprocess.run(
            [str(CORE_BIN)], input=text.encode(), capture_output=True, timeout=10
        )
        if r.returncode != 0:
            return []
        out = []
        for sp in json.loads(r.stdout.decode()):
            t = CORE_TYPE_MAP.get(sp["type"])
            if t in CORE_KEEP:
                out.append({"start": sp["start"], "end": sp["end"], "type": t})
        return out
    except Exception:
        return []


class LightNer:
    """Candidat NER léger (ONNX int8) — stub sans modèle = désactivé."""

    def __init__(self, onnx_path: str, tokenizer_path: str) -> None:
        self._session = None
        self._tokenizer = None
        self._labels = None
        if not onnx_path or not tokenizer_path or not Path(onnx_path).exists():
            return
        try:
            import onnxruntime as ort
            from tokenizers import Tokenizer

            self._session = ort.InferenceSession(
                str(onnx_path), providers=["CPUExecutionProvider"]
            )
            self._tokenizer = Tokenizer.from_file(str(tokenizer_path))
            # label_map à côté du modèle (même convention DEPLOY-8).
            labels_path = Path(onnx_path).parent / "label_map.json"
            if labels_path.exists():
                self._labels = {int(k): str(v) for k, v in
                                json.loads(labels_path.read_text()).items()}
        except Exception as exc:  # pragma: no cover
            print(f"  WARN LightNer indisponible : {exc}", file=sys.stderr)
            self._session = None

    def available(self) -> bool:
        return self._session is not None

    def detect(self, text: str) -> list[dict]:
        """Spans PERSON/LOC (offsets caractères) ou []."""
        if not self.available() or not text:
            return []
        try:
            import numpy as np

            enc = self._tokenizer.encode(text)
            ids = np.array([enc.ids], dtype=np.int64)
            mask = np.array([enc.attention_mask], dtype=np.int64)
            logits = self._session.run(None, {"input_ids": ids, "attention_mask": mask})[0]
            pred_ids = logits.argmax(axis=-1)[0].tolist()
            e = np.exp(logits - logits.max(axis=-1, keepdims=True))
            probs = e / e.sum(axis=-1, keepdims=True)
            offsets = enc.offsets
            spans: list[dict] = []
            cur_type = None
            cur_start = 0
            cur_end = 0
            probs_list: list[float] = []

            def flush() -> None:
                nonlocal cur_type, probs_list
                if cur_type is None or not probs_list:
                    return
                score = sum(probs_list) / len(probs_list)
                if score >= NER_THRESHOLD and cur_type in NER_TYPES:
                    spans.append({"start": cur_start, "end": cur_end, "type": cur_type,
                                  "score": round(score, 4)})
                cur_type = None
                probs_list = []

            for i, (tok_start, tok_end) in enumerate(offsets):
                if tok_start is None or tok_end is None or tok_start >= tok_end:
                    flush()
                    continue
                label = ""
                if self._labels is not None:
                    label = str(self._labels.get(int(pred_ids[i]), "O"))
                else:
                    label = str(pred_ids[i])
                core = label
                if core[:1] in ("B", "I", "E", "S") and "-" in core:
                    core = core.split("-", 1)[1]
                t = core.upper()
                t = {"PER": "PERSON", "PERSON": "PERSON", "LOC": "LOC",
                     "LOCATION": "LOC", "GPE": "LOC"}.get(t)
                prob = float(probs[0, i, int(pred_ids[i])])
                if t is not None and t == cur_type:
                    cur_end = tok_end
                    probs_list.append(prob)
                else:
                    flush()
                    if t is not None:
                        cur_type = t
                        cur_start = tok_start
                        cur_end = tok_end
                        probs_list = [prob]
            flush()
            return spans
        except Exception as exc:  # pragma: no cover
            print(f"  WARN LightNer detect : {exc}", file=sys.stderr)
            return []


def merge_enclosing_ner(core: list[dict], ner: list[dict]) -> list[dict]:
    """Fusion englobante N0 : un span NER complet prime sur les spans
    gazetteer (core) partiels qu'il englobe (même type) ; sinon le span NER
    est ignoré s'il chevauche un span structuré (MAIL/TEL/CNI — le core
    prime) ; dédup stricte (start, end, type)."""
    ner = [s for s in ner if s["type"] in NER_TYPES]
    # 1) spans NER englobants : retirer les spans core partiels englobés.
    core_kept = []
    for c in core:
        enclosed = any(
            n["start"] <= c["start"] and n["end"] >= c["end"] and n["type"] == c["type"]
            for n in ner
        )
        if not enclosed:
            core_kept.append(c)
    # 2) spans NER en conflit avec un span structuré (non PERSON/LOC) -> ignorés.
    structured = [c for c in core if c["type"] not in NER_TYPES]
    ner_kept = []
    for n in ner:
        if any(s["start"] < n["end"] and n["start"] < s["end"] for s in structured):
            continue
        ner_kept.append(n)
    # 3) dédup stricte.
    merged = core_kept + ner_kept
    seen: set[tuple] = set()
    out = []
    for p in merged:
        k = (p["start"], p["end"], p["type"])
        if k not in seen:
            seen.add(k)
            out.append(p)
    return out


def run_pipeline(gold_docs: list[dict], use_ner: bool, ner: LightNer | None):
    pred_docs = []
    for doc in gold_docs:
        text = doc["text"]
        core = core_spans(text)
        if use_ner and ner is not None and ner.available():
            ner_spans = ner.detect(text)
            preds = merge_enclosing_ner(core, ner_spans)
        else:
            preds = core
        pred_docs.append({"doc_id": doc["doc_id"], "text": text, "entities": preds})
    return pred_docs


def main() -> int:
    if not DATASET.exists():
        print("Jeu STACK-1 introuvable — lance d'abord run_benchmark.py")
        return 2
    gold = load_gold()
    sys.path.insert(0, str(ROOT))
    from scoring import Scorer

    scorer = Scorer()
    ner = LightNer(NER_ONNX, NER_TOKENIZER)

    print(f"  core_bin      : {CORE_BIN}")
    print(f"  NER onnx      : {NER_ONNX or '(aucun — mesure A seule)'}")
    print(f"  NER tokenizer : {NER_TOKENIZER or '(aucun)'}")
    print(f"  NER chargé    : {ner.available()}")
    print(f"  NER_THRESHOLD : {NER_THRESHOLD}")

    # Mesure A — état N0 actuel.
    t0 = time.monotonic()
    pred_a = run_pipeline(gold, use_ner=False, ner=None)
    dt_a = time.monotonic() - t0
    res_a = scorer.score(gold, pred_a)

    # Mesure B — N0 + NER léger.
    res_b = None
    dt_b = 0.0
    if ner.available():
        t0 = time.monotonic()
        pred_b = run_pipeline(gold, use_ner=True, ner=ner)
        dt_b = time.monotonic() - t0
        res_b = scorer.score(gold, pred_b)

    def row(label: str, res) -> str:
        m = res.entity_metrics
        return (
            f"  {label:5s} PERSON {m['PERSON'].f1:.4f} · LOC {m['LOC'].f1:.4f} · "
            f"CNI {m['CNI'].f1:.4f} · MAIL {m['MAIL'].f1:.4f} · TEL {m['TEL'].f1:.4f} · "
            f"macro {res.macro_f1:.4f} · spécificité {res.specificity:.2%}"
        )

    print("=" * 78)
    print("ARBITRAGE ④ — état N0 actuel (A) vs N0 + NER léger (B)")
    print("=" * 78)
    print(row("A", res_a))
    if res_b is not None:
        print(row("B", res_b))
        ma, mb = res_a.entity_metrics, res_b.entity_metrics
        gains = {
            "F1_PERSON": mb["PERSON"].f1 - ma["PERSON"].f1,
            "F1_LOC": mb["LOC"].f1 - ma["LOC"].f1,
            "spec": res_b.specificity - res_a.specificity,
        }
        print("-" * 78)
        for k, v in gains.items():
            print(f"  gain {k:10s} : {v:+.4f}")
        # Critères C1–C5 (ARBITRAGE-04 §2).
        c1 = mb["PERSON"].f1 >= ma["PERSON"].f1 + 0.10
        c2 = mb["LOC"].f1 >= ma["LOC"].f1 + 0.05
        c3 = res_b.specificity >= 0.60 and res_b.specificity >= res_a.specificity
        c4 = all(
            mb[t].f1 >= ma[t].f1 for t in ("CNI", "MAIL", "TEL")
        )
        checks = {"C1_person_+0.10": c1, "C2_loc_+0.05": c2,
                  "C3_spec_>=0.60_et_non_reg": c3, "C4_struct_non_reg": c4}
        print("-" * 78)
        for k, v in checks.items():
            print(f"  [{'PASS' if v else 'FAIL'}] {k}")
        go = all(checks.values())
        print(f"  VERDICT ARBITRAGE ④ : {'GO' if go else 'NO-GO'} (C5 latence mesurée à part)")
    else:
        checks = {}
        go = False

    out = {
        "verdict": "GO" if go else "NO-GO",
        "checks": checks,
        "A": {e: round(res_a.entity_metrics[e].f1, 4)
              for e in ("PERSON", "LOC", "CNI", "MAIL", "TEL")},
        "A_macro": round(res_a.macro_f1, 4),
        "A_specificity": round(res_a.specificity, 4),
        "A_duration_s": round(dt_a, 1),
        "B": ({e: round(res_b.entity_metrics[e].f1, 4)
               for e in ("PERSON", "LOC", "CNI", "MAIL", "TEL")}
              if res_b is not None else None),
        "B_macro": round(res_b.macro_f1, 4) if res_b is not None else None,
        "B_specificity": round(res_b.specificity, 4) if res_b is not None else None,
        "B_duration_s": round(dt_b, 1) if res_b is not None else None,
        "ner_model": os.path.basename(NER_ONNX) if NER_ONNX else None,
        "ner_threshold": NER_THRESHOLD,
    }
    out_path = ROOT / "results" / "arbitrage04.json"
    out_path.write_text(json.dumps(out, ensure_ascii=False, indent=2))
    print(f"  Résultat écrit : {out_path}")
    return 0 if go else 1


if __name__ == "__main__":
    sys.exit(main())
