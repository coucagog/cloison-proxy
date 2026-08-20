#!/usr/bin/env python3
"""GO/NO-GO final STACK-1 : cloison-detect (détecteur cible) vs baseline_ref.

Charge le jeu synthétique STACK-1, fait tourner le pipeline de cloison-detect
(Presidio + GLiNER + africains + alias) comme détecteur cible, calcule les
mêmes métriques que le benchmark (exact match, F1 par entité), et applique les
critères GO/NO-GO de la grille v1.1 contre les valeurs baseline enregistrées.

Porte : le GO exige 5 conditions simultanées (grille v1.1) :
  macro >= baseline + 0.10 ; PERSON >= baseline + 0.12 ; LOC >= baseline + 0.15 ;
  CNI non-régression ; spécificité >= 0.60.

Le NER ouest-africain est sélectionnable par environnement :
  CLOISON_AFRICAN_MODEL=serengeti|afroxlmr|masakha   (défaut : serengeti,
  le défaut produit ; afroxlmr = vrai fine-tune MasakhaNER, recommandé pour
  le run « modèles réels »). La grille v1.1 n'est jamais modifiée ici.
"""
import json
import os
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
    os.environ["CLOISON_OFFLINE"] = "1"

AFRICAN_MODEL = os.environ.get("CLOISON_AFRICAN_MODEL", "serengeti").strip().lower()


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

    # --- détecteur cible : PIPELINE COMPLET CLOISON -------------------------
    # core Rust (CNI/MAIL/TEL structures via detect_cli) + sidecar Python
    # (PERSON/LOC par NER, Presidio + GLiNER + africains + alias).
    sys.path.insert(0, str(DETECT))
    from src.detect_service import DetectRequest, DetectService
    from src.spans import Policy, SessionContext, SpanType
    from src.config import Config, AfricanConfig

    # Sélection du NER ouest-africain (env) ; défaut = défaut produit.
    print(f"  NER ouest-africain : {AFRICAN_MODEL!r}")
    svc = DetectService(Config(african=AfricanConfig(model_name=AFRICAN_MODEL)))
    CORE_BIN = ROOT.parent.parent / "target" / "debug" / "detect_cli"
    import subprocess

    # Warm-up : charger presidio/gliner/africain AVANT la boucle (le 1er appel
    # paierait 20-60 s de chargement dans un budget court → partial systématique).
    try:
        svc.detect(DetectRequest(
            text="Bonjour, document de réchauffement sans donnée personnelle.",
            locale="fr-SN",
            policy=Policy(),
            core_spans=(),
            session=SessionContext(),
        ))
        print("  Warm-up OK (presidio/gliner/africain chargés)")
    except Exception as e:
        print(f"  WARN warm-up: {e}")

    # Mapping core Rust vers les types de la grille CLOISON
    CORE_TYPE_MAP = {
        "Email": "MAIL", "PhoneSn": "TEL", "CniSn": "CNI",
        "CreditCard": "CREDIT_CARD", "Ip": "IP", "Date": "DATE",
    }

    def core_spans(text: str) -> list:
        # Spans structures du core Rust (CNI/MAIL/TEL).
        if not CORE_BIN.exists():
            return []
        try:
            r = subprocess.run([str(CORE_BIN)], input=text.encode(), capture_output=True, timeout=10)
            if r.returncode != 0:
                return []
            spans = json.loads(r.stdout.decode())
            out = []
            for sp in spans:
                t = CORE_TYPE_MAP.get(sp["type"], sp["type"])
                if t in ("CNI", "MAIL", "TEL"):
                    out.append({"start": sp["start"], "end": sp["end"], "type": t})
            return out
        except Exception:
            return []

    pred_docs = []
    for doc in gold_docs:
        text = doc["text"]
        # 1) core Rust (structure)
        core = core_spans(text)
        # 2) sidecar NER (PERSON/LOC)
        try:
            resp = svc.detect(DetectRequest(
                text=text,
                locale="fr-SN",
                policy=Policy(),
                core_spans=(),
                session=SessionContext(),
            ))
            ner_preds = [{"start": s.start, "end": s.end, "type": s.type.value}
                         for s in resp.spans]
        except Exception as e:
            print(f"  WARN detect echec {doc['doc_id']}: {e}")
            ner_preds = []
        # 3) fusion : core + sidecar = le pipeline complet CLOISON
        preds = core + [p for p in ner_preds if p["type"] in ("PERSON", "LOC")]
        pred_docs.append({"doc_id": doc["doc_id"], "text": text, "entities": preds})

    # --- scoring (réutilise scoring.py de cloison-bench) --------------------
    sys.path.insert(0, str(BENCH))
    from scoring import Scorer
    scorer = Scorer()
    result = scorer.bootstrap_ci(gold_docs, pred_docs, n_iterations=200, confidence_level=0.95)

    # --- critères GO/NO-GO (grille v1.1) ------------------------------------
    c = baseline["criteria"]
    checks = {
        "macro_improvement": bool(result.macro_f1 >= baseline["macro_f1"] + c["macro_improvement"]),
        "person_improvement": bool(result.entity_metrics["PERSON"].f1 >= baseline["f1_person"] + c["person_improvement"]),
        "loc_improvement": bool(result.entity_metrics["LOC"].f1 >= baseline["f1_loc"] + c["loc_improvement"]),
        "cni_no_regression": bool(result.entity_metrics["CNI"].f1 >= baseline["f1_cni"]),
        "specificity_min": bool(result.specificity >= c["specificity_min"]),
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
        "metrics": {e: float(result.entity_metrics[e].f1) for e in result.entity_metrics},
        "macro_f1": result.macro_f1,
        "specificity": result.specificity,
        "baseline_ref": baseline,
        "note": ("OFFLINE" if OFFLINE else "avec modèles") + f" · africain={AFRICAN_MODEL}",
    }, ensure_ascii=False, indent=2))
    print(f"  Rapport écrit : {out}")
    return 0 if go else 1


if __name__ == "__main__":
    sys.exit(main())
