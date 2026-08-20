#!/usr/bin/env python3
"""GO/NO-GO final STACK-1 : cloison-detect (détecteur cible) vs baseline_ref.

Charge le jeu synthétique STACK-1, fait tourner le pipeline de cloison-detect
(Presidio + GLiNER + africains + alias) comme détecteur cible, calcule les
mêmes métriques que le benchmark (exact match, F1 par entité), et applique les
critères GO/NO-GO de la grille v1.1 contre les valeurs baseline enregistrées.

Porte : le GO exige 5 conditions simultanées (grille v1.1) :
  macro >= baseline + 0.10 ; PERSON >= baseline + 0.12 ; LOC >= baseline + 0.15 ;
  CNI non-régression ; spécificité >= 0.60.
"""
import json
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parent
BENCH = ROOT
DETECT = ROOT.parent.parent / "services" / "cloison-detect"
DATASET = BENCH / "results" / "dataset.jsonl"
RAPPORT = BENCH / "results" / "rapport.json"

# --- options ---------------------------------------------------------------
OFFLINE = "--offline" in sys.argv
if OFFLINE:
    import os
    os.environ["CLOISON_OFFLINE"] = "1"


def load_baseline_ref() -> dict:
    r = json.loads(RAPPORT.read_text())
    return r["baseline_ref"]


def load_gold() -> list[dict]:
    return [json.loads(l) for l in DATASET.read_text().splitlines()]


def main() -> int:
    if not DATASET.exists() or not RAPPORT.exists():
        print("Jeu ou rapport STACK-1 introuvable — lance d'abord run_benchmark.py")
        return 2
    baseline = load_baseline_ref()
    gold_docs = load_gold()

    # --- détecteur cible : pipeline cloison-detect --------------------------
    sys.path.insert(0, str(DETECT))
    from src.detect_service import DetectRequest, DetectService
    from src.spans import Policy, SessionContext, SpanType
    from src.config import Config

    svc = DetectService(Config())
    # PII simulée -> spans du pipeline (Presidio + GLiNER + africains + alias)
    pred_docs = []
    for doc in gold_docs:
        text = doc["text"]
        try:
            resp = svc.detect(DetectRequest(
                text=text,
                locale="fr-SN",
                policy=Policy(),
                core_spans=(),
                session=SessionContext(),
            ))
            preds = [{"start": s.start, "end": s.end, "type": s.type.value}
                     for s in resp.spans]
        except Exception as e:  # dégradation : spans vides
            print(f"  WARN detect échec {doc['doc_id']}: {e}")
            preds = []
        pred_docs.append({"doc_id": doc["doc_id"], "text": text, "entities": preds})

    # --- scoring (réutilise scoring.py de cloison-bench) --------------------
    sys.path.insert(0, str(BENCH))
    from scoring import Scorer
    scorer = Scorer()
    result = scorer.bootstrap_ci(gold_docs, pred_docs, n_iterations=200, confidence_level=0.95)

    # --- critères GO/NO-GO (grille v1.1) ------------------------------------
    c = baseline["criteria"]
    checks = {
        "macro_improvement": result.macro_f1 >= baseline["macro_f1"] + c["macro_improvement"],
        "person_improvement": result.entity_metrics["PERSON"].f1 >= baseline["f1_person"] + c["person_improvement"],
        "loc_improvement": result.entity_metrics["LOC"].f1 >= baseline["f1_loc"] + c["loc_improvement"],
        "cni_no_regression": result.entity_metrics["CNI"].f1 >= baseline["f1_cni"],
        "specificity_min": result.specificity >= c["specificity_min"],
    }
    go = all(checks.values())

    print("=" * 60)
    print("GO/NO-GO FINAL — cloison-detect vs baseline Presidio (grille v1.1)")
    print("=" * 60)
    print(f"  Macro F1 cible : {result.macro_f1:.4f}   (baseline {baseline['macro_f1']:.4f})")
    for ent in ("PERSON", "LOC", "CNI", "MAIL", "TEL"):
        m = result.entity_metrics[ent]
        b = baseline.get(f"f1_{ent.lower()}")
        print(f"  {ent:7s} F1 {m.f1:.4f}   (baseline {b:.4f})" if b else f"  {ent:7s} F1 {m.f1:.4f}")
    print(f"  Spécificité non-PII : {result.specificity:.2%}   (min {c['specificity_min']:.2%})")
    print("-" * 60)
    for k, v in checks.items():
        print(f"  [{'PASS' if v else 'FAIL'}] {k}")
    print("-" * 60)
    verdict = "GO — le fossé est prouvé, le produit se justifie" if go else \
              "NO-GO — fossé insuffisant, réévaluer la différenciation"
    print(f"  VERDICT : {verdict}")

    out = BENCH / "results" / "go_nogo_final.json"
    out.write_text(json.dumps({
        "verdict": "GO" if go else "NO-GO",
        "checks": checks,
        "metrics": {e: result.entity_metrics[e].f1 for e in result.entity_metrics},
        "macro_f1": result.macro_f1,
        "specificity": result.specificity,
        "baseline_ref": baseline,
        "note": "OFFLINE" if OFFLINE else "avec modèles",
    }, ensure_ascii=False, indent=2))
    print(f"  Rapport écrit : {out}")
    return 0 if go else 1


if __name__ == "__main__":
    sys.exit(main())
