#!/usr/bin/env python3
# =============================================================================
# CLOISON — Simulateur de trafic client REPRÉSENTATIF (chantier ② : 1er client
# N3 / calibration). Envoie des documents synthétiques sénégalais (générateur
# STACK-1, seed 42, ZÉRO PII réelle — charte §10) à travers l'edge DÉPLOYÉ,
# comme le ferait un client réel branché sur `api.wonkom.ai`.
#
# Deux modes (comportement de l'edge, CLOISON_AUDIT_MODE du .env de prod) :
#   masking (défaut)  : le client reçoit la PII restaurée, aucune sentinelle
#                       résiduelle ; latence mesurée.
#   audit  (observe-only) : le texte passe intact (aucun masquage) et des
#                       REÇUS signés sont générés → ingest automatique →
#                       journal de transparence (ledger seq N+).
#
# Usage :
#   CLOISON_COMPOSITE_KEY='mn_<jeton>.<clé_amont>' \
#   CLOISON_SIM_MODE=audit CLOISON_SIM_COUNT=30 \
#   python3 deploy/simulate_client.py
#
# Sortie : résumé (requêtes, erreurs, latence min/médiane/p95, sentinelles
# résiduelles). Après un run « audit », vérifier le ledger :
#   curl -s https://journal.wonkom.ai/ledger.jsonl | wc -l
# =============================================================================
import json
import os
import ssl
import sys
import time
import urllib.request
from pathlib import Path

BASE = os.environ.get("CLOISON_EDGE_BASE", "https://api.wonkom.ai/v1").rstrip("/")
KEY = os.environ.get("CLOISON_COMPOSITE_KEY", "")
SEED = int(os.environ.get("CLOISON_SIM_SEED", "42"))
COUNT = int(os.environ.get("CLOISON_SIM_COUNT", "20"))
MODE = os.environ.get("CLOISON_SIM_MODE", "masking").strip().lower()
MODEL = os.environ.get("CLOISON_SIM_MODEL", "openai/gpt-4o-mini")
CONCURRENCY = int(os.environ.get("CLOISON_SIM_CONCURRENCY", "1"))
ONLY_PII = os.environ.get("CLOISON_SIM_ONLY_PII", "1") in ("1", "true", "yes")

if not KEY:
    print("❌ CLOISON_COMPOSITE_KEY requise (mn_<jeton>.<clé_amont>)", file=sys.stderr)
    sys.exit(2)
if MODE not in ("masking", "audit"):
    print("❌ CLOISON_SIM_MODE : masking | audit", file=sys.stderr)
    sys.exit(2)

# Générateur synthétique STACK-1 (stdlib pure — charte §10, 0 PII réelle).
sys.path.insert(0, str(Path(__file__).resolve().parent.parent / "bench" / "cloison-bench"))
from generator import DatasetGenerator  # noqa: E402


def call(content: str) -> tuple:
    """POST /v1/chat/completions (non-stream). Retourne (durée_s, status, body)."""
    body = json.dumps({
        "model": MODEL,
        "messages": [{"role": "user", "content": content}],
        "max_tokens": 64,
    }).encode("utf-8")
    req = urllib.request.Request(
        f"{BASE}/chat/completions",
        data=body,
        headers={
            "Authorization": f"Bearer {KEY}",
            "Content-Type": "application/json",
        },
        method="POST",
    )
    t0 = time.monotonic()
    try:
        with urllib.request.urlopen(req, timeout=120, context=ssl.create_default_context()) as r:
            raw = r.read()
        dur = time.monotonic() - t0
        return dur, r.status, raw
    except urllib.error.HTTPError as e:
        dur = time.monotonic() - t0
        return dur, e.code, e.read()
    except Exception as e:  # réseau / timeout
        dur = time.monotonic() - t0
        return dur, 0, str(e).encode()


def main() -> int:
    docs = DatasetGenerator(seed=SEED).generate(COUNT * 3)
    # Trafic représentatif : documents PII (uniquement) — les docs non-PII ne
    # produisent pas de reçus en mode audit et ne testent pas le masquage.
    pool = [d for d in docs if getattr(d, "entities", None)] if ONLY_PII else docs
    if len(pool) < COUNT:
        pool = (pool * ((COUNT // len(pool)) + 1))[:COUNT]
    pool = pool[:COUNT]

    print(f"==> simulateur client CLOISON — mode {MODE} | {len(pool)} requêtes | seed {SEED}")
    print(f"    edge : {BASE}/chat/completions | modèle : {MODEL} | concurrence : {CONCURRENCY}")

    lat = []
    errors = 0
    leaks = 0
    ok = 0

    # Concurrence simple (threads stdlib) — CLOISON_SIM_CONCURRENCY>1 pour la
    # mesure de latence sous charge (CLOISON_DETECT_CONCURRENCY côté prod).
    if CONCURRENCY > 1:
        import concurrent.futures
        with concurrent.futures.ThreadPoolExecutor(max_workers=CONCURRENCY) as ex:
            results = list(ex.map(lambda d: call(d.text), pool))
    else:
        results = [call(d.text) for d in pool]

    for dur, status, raw in results:
        lat.append(dur)
        if status != 200:
            errors += 1
            print(f"  ⚠ HTTP {status} : {raw[:120]!r}")
            continue
        ok += 1
        try:
            data = json.loads(raw)
        except Exception:
            continue
        content = (data.get("choices") or [{}])[0].get("message", {}).get("content", "")
        if "\u27e6" in content:  # sentinelle ⟦ résiduelle → fuite potentielle
            leaks += 1
            print(f"  ⚠ sentinelle résiduelle dans la réponse : {content[:80]!r}")

    lat.sort()
    n = len(lat)
    p95 = lat[int(n * 0.95) - 1] if n else 0
    med = lat[n // 2] if n else 0
    print("-" * 60)
    print(f"  OK {ok} / {len(pool)}   erreurs {errors}   sentinelles résiduelles {leaks}")
    if lat:
        print(f"  latence : min {lat[0]*1000:.0f} ms · médiane {med*1000:.0f} ms · p95 {p95*1000:.0f} ms")
    print("-" * 60)
    if MODE == "audit":
        print("  ⇒ mode audit : les reçus signés sont ingérés automatiquement "
              "(intervalle contrôle) — vérifier le ledger :")
        print("    curl -s https://journal.wonkom.ai/ledger.jsonl | wc -l   (attendu : seq N+1+)")
    else:
        print("  ⇒ mode masking : vérifier qu'aucune sentinelle ⟦ ne subsiste (ci-dessus).")
    return 1 if (errors or leaks) else 0


if __name__ == "__main__":
    sys.exit(main())
