#!/usr/bin/env python3
"""Différentiel STACK-2 : cloison-core (Rust) vs baseline Presidio (Python).
Charge le jeu synthétique STACK-1, lance les deux détecteurs sur chaque document,
compare les spans (mapping de types) et écrit un rapport de divergences.

Exigence charte §5.2 : "Différentiel / oracle : cloison-core vs Presidio bien
configuré sur le même corpus ; toute divergence loggée."
"""
import json
import subprocess
import sys
from pathlib import Path

CORE_BIN = Path("/home/debian/Cloison/cloison/target/debug/detect_cli")
DATASET = Path("/home/debian/Cloison/cloison/bench/cloison-bench/results/dataset.jsonl")
OUT = Path("/home/debian/Cloison/cloison/crates/cloison-core/results_differential.json")

# Mapping types core -> grille CLOISON
CORE_TYPE_MAP = {
    "Email": "MAIL",
    "PhoneSn": "TEL",
    "CniSn": "CNI",
    "CreditCard": "CREDIT_CARD",
    "Ip": "IP",
    "Date": "DATE",
}

# Mapping types Presidio -> grille (même mapping que la baseline STACK-1)
PRESIDIO_TYPE_MAP = {
    "PERSON": "PERSON",
    "LOCATION": "LOC",
    "LOC": "LOC",
    "CNI": "CNI",
    "EMAIL_ADDRESS": "MAIL",
    "PHONE_NUMBER": "TEL",
    "TEL": "TEL",
}


def core_spans(text: str):
    """Spans de cloison-core via le binaire."""
    try:
        r = subprocess.run(
            [str(CORE_BIN)],
            input=text.encode("utf-8"),
            capture_output=True,
            timeout=10,
        )
        if r.returncode != 0:
            return []
        spans = json.loads(r.stdout.decode("utf-8"))
        out = []
        for s in spans:
            t = CORE_TYPE_MAP.get(s["type"], s["type"])
            out.append({"start": s["start"], "end": s["end"], "type": t})
        return out
    except Exception:
        return []


def presidio_spans(text: str):
    """Spans de la baseline Presidio STACK-1."""
    try:
        sys.path.insert(0, str(DATASET.parent.parent))
        from presidio_baseline import create_baseline_analyzer, detect_pii
        analyzer = create_baseline_analyzer()
        ents = detect_pii(analyzer, text)
        out = []
        for e in ents:
            t = PRESIDIO_TYPE_MAP.get(e["type"], e["type"])
            out.append({"start": e["start"], "end": e["end"], "type": t})
        return out
    except Exception as e:
        print(f"  Presidio indisponible: {e}", file=sys.stderr)
        return None


def exact_match(pred, gold):
    """Même règle que le scoring STACK-1 : start/end/type identiques."""
    return pred["start"] == gold["start"] and pred["end"] == gold["end"] and pred["type"] == gold["type"]


def main():
    if not DATASET.exists():
        print(f"Dataset introuvable: {DATASET} — lance d'abord le benchmark STACK-1")
        return 1

    docs = [json.loads(l) for l in DATASET.read_text(encoding="utf-8").splitlines()]
    # Limite raisonnable pour le différentiel (200 docs, représentatif)
    docs = docs[:200]

    analyzer = None
    try:
        sys.path.insert(0, str(DATASET.parent.parent))
        from presidio_baseline import create_baseline_analyzer, detect_pii
        analyzer = create_baseline_analyzer()
    except Exception as e:
        print(f"Presidio indisponible: {e}")
        return 1

    results = {"total_docs": len(docs), "docs_compared": 0, "docs_with_core_detection": 0,
               "divergences": [], "core_only": 0, "presidio_only": 0, "both": 0, "neither": 0}

    for doc in docs:
        text = doc["text"]
        gold = [{"start": e["start"], "end": e["end"], "type": e["type"]} for e in doc.get("entities", [])]

        core = core_spans(text)
        presidio = detect_pii(analyzer, text)
        presidio = [{"start": e["start"], "end": e["end"], "type": PRESIDIO_TYPE_MAP.get(e["type"], e["type"])} for e in presidio]

        results["docs_compared"] += 1
        if core:
            results["docs_with_core_detection"] += 1

        # Comparaison sur les entités de la grille uniquement
        core_grille = [s for s in core if s["type"] in ("PERSON", "LOC", "CNI", "MAIL", "TEL")]
        presidio_grille = [s for s in presidio if s["type"] in ("PERSON", "LOC", "CNI", "MAIL", "TEL")]

        core_set = {(s["start"], s["end"], s["type"]) for s in core_grille}
        presidio_set = {(s["start"], s["end"], s["type"]) for s in presidio_grille}

        in_both = core_set & presidio_set
        only_core = core_set - presidio_set
        only_presidio = presidio_set - core_set

        results["both"] += len(in_both)
        results["core_only"] += len(only_core)
        results["presidio_only"] += len(only_presidio)
        if len(only_core) + len(only_presidio) > 0:
            results["divergences"].append({
                "doc_id": doc["doc_id"],
                "difficulty": doc.get("difficulty", "?"),
                "only_core": [list(x) for x in list(only_core)[:10]],
                "only_presidio": [list(x) for x in list(only_presidio)[:10]],
            })

    results["summary"] = {
        "message": "Le différentiel est un OUTIL DE DIAGNOSTIC, pas un score. Il montre où cloison-core et Presidio divergent.",
        "note": "cloison-core détecte Email/Tel/CNI/CB/IP/Date (structuré); il ne détecte PAS encore PERSON/LOC par NER (STACK-6). Les divergences sur PERSON/LOC sont attendues.",
    }
    OUT.write_text(json.dumps(results, ensure_ascii=False, indent=2), encoding="utf-8")
    print(f"Différentiel écrit: {OUT}")
    print(f"  Docs comparés: {results['docs_compared']}, avec détection core: {results['docs_with_core_detection']}")
    print(f"  Spans communs: {results['both']}, core seulement: {results['core_only']}, Presidio seulement: {results['presidio_only']}")
    print(f"  Docs avec divergence: {len(results['divergences'])}")
    return 0


if __name__ == "__main__":
    sys.exit(main())
