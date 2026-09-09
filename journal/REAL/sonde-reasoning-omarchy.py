#!/usr/bin/env python3
# -*- coding: utf-8 -*-
"""
sonde-reasoning-omarchy.py — sonde reasoning_content roundtrip pour la
session conjointe R2 (Omarchy). Données 100 % synthétiques, aucun secret.

Usage :
  export CLOISON_N0_KEY="mn_omarchy.<votre clé amont>"
  python3 sonde-reasoning-omarchy.py

Variables optionnelles :
  CLOISON_N0_URL   (défaut http://127.0.0.1:8787/v1)
  CLOISON_MODEL    (défaut deepseek-v4-flash)

Contrôles (critères F1 v0.3.3.2) :
  1. Tour 1 non-stream : 200, PII restaurées, zéro sentinelle ⟦…⟧ en sortie
     (raisonnement inclus), reasoning_content présent.
  2. Tour 2 non-stream avec reasoning_content REJOUÉ : 200 (preuve que le
     champ est transmis, tokenisé, et que le fournisseur l'accepte).
  3. Tour 3 stream : [DONE], deltas reasoning reçus, zéro ⟦/⟧ dans tous les
     deltas, PII restaurées dans le contenu réassemblé.
Sortie : PASS/FAIL par contrôle + bilan ; exit 0 si tout PASS, 1 sinon.
"""
import json
import os
import sys
import urllib.request

# Sortie UTF-8 même sur les consoles Windows (sinon UnicodeEncodeError).
if hasattr(sys.stdout, "reconfigure"):
    sys.stdout.reconfigure(encoding="utf-8", errors="replace")
    sys.stderr.reconfigure(encoding="utf-8", errors="replace")

URL = os.environ.get("CLOISON_N0_URL", "http://127.0.0.1:8787/v1")
KEY = os.environ.get("CLOISON_N0_KEY", "")
MODEL = os.environ.get("CLOISON_MODEL", "deepseek-v4-flash")

L_OPEN = "\u27e6"   # ⟦
L_CLOSE = "\u27e7"  # ⟧

PII_PROMPT = (
    "Voici un contact fictif pour un test : Aminata Diop, "
    "aminata.diop@example.sn, téléphone +221 77 123 45 67, ville Ziguinchor. "
    "Raisonne brièvement sur ce contact, puis réponds exactement : "
    "nom, email, téléphone, ville — rien d'autre."
)
PII_EXPECT = ["Aminata Diop", "aminata.diop@example.sn", "+221 77 123 45 67"]

CHECKS = []


def check(name, ok, detail=""):
    CHECKS.append((name, ok))
    print(f"  [{'PASS' if ok else 'FAIL'}] {name}" + (f" — {detail}" if detail else ""))


def post_json(payload, timeout=180):
    req = urllib.request.Request(
        URL + "/chat/completions",
        data=json.dumps(payload).encode("utf-8"),
        headers={
            "Authorization": "Bearer " + KEY,
            "Content-Type": "application/json",
        },
        method="POST",
    )
    with urllib.request.urlopen(req, timeout=timeout) as r:
        return r.status, r.read().decode("utf-8")


def has_sentinel(text):
    return L_OPEN in text or L_CLOSE in text


def check_restored(text, label):
    for v in PII_EXPECT:
        check(f"{label} — PII restaurée : {v}", v in text, f"sortie: {text[:120]}")
    ville_ok = "Ziguinchor" in text or "[VILLE_SN]" in text
    check(
        f"{label} — ville {('[VILLE_SN] généralisée' if '[VILLE_SN]' in text and 'Ziguinchor' not in text else 'restaurée')}",
        ville_ok,
        f"sortie: {text[:120]}",
    )
    check(f"{label} — zéro sentinelle en sortie", not has_sentinel(text), f"sortie: {text[:120]}")


def main():
    if not KEY:
        print("ERREUR : CLOISON_N0_KEY manquante (clé composite mn_….<clé amont>).")
        return 1
    print(f"N0 : {URL} | modèle : {MODEL}")
    print("=== TOUR 1 — non-stream (aller tokenisé + retour restauré) ===")
    status, body = post_json(
        {"model": MODEL, "stream": False,
         "messages": [{"role": "user", "content": PII_PROMPT}]}
    )
    check("Tour 1 — HTTP 200", status == 200, f"status={status}")
    t1 = {}
    if status == 200:
        t1 = json.loads(body)
        msg = t1.get("choices", [{}])[0].get("message", {})
        content = msg.get("content") or ""
        reasoning = msg.get("reasoning_content") or ""
        check_restored(content, "Tour 1 content")
        check("Tour 1 — reasoning_content présent", bool(reasoning), f"début: {reasoning[:80]}")
        check("Tour 1 — zéro sentinelle dans reasoning_content", not has_sentinel(reasoning),
              f"début: {reasoning[:80]}")
        check("Tour 1 — zéro sentinelle dans le JSON brut", not has_sentinel(body))

    print("=== TOUR 2 — non-stream multi-tours (reasoning_content REJOUÉ) ===")
    if status == 200 and t1:
        msg1 = t1.get("choices", [{}])[0].get("message", {})
        status2, body2 = post_json(
            {"model": MODEL, "stream": False,
             "messages": [
                 {"role": "user", "content": PII_PROMPT},
                 {"role": "assistant",
                  "content": msg1.get("content"),
                  "reasoning_content": msg1.get("reasoning_content")},
                 {"role": "user", "content": "Redis exactement les quatre informations en une ligne."},
             ]}
        )
        check("Tour 2 — HTTP 200 (reasoning rejoué transmis)", status2 == 200,
              f"status={status2}" + (f", corps={body2[:200]}" if status2 != 200 else ""))
        if status2 == 200:
            t2 = json.loads(body2)
            content2 = t2.get("choices", [{}])[0].get("message", {}).get("content") or ""
            check_restored(content2, "Tour 2 content")
            check("Tour 2 — zéro sentinelle dans le JSON brut", not has_sentinel(body2))
    else:
        check("Tour 2 — HTTP 200 (reasoning rejoué transmis)", False, "tour 1 non réussi, sauté")

    print("=== TOUR 3 — stream SSE (deltas reasoning + content) ===")
    status3, body3 = post_json(
        {"model": MODEL, "stream": True,
         "messages": [{"role": "user", "content": PII_PROMPT}]}
    )
    check("Tour 3 — HTTP 200", status3 == 200, f"status={status3}")
    if status3 == 200:
        content_deltas, reasoning_deltas, sentinel_hit, done = [], 0, False, False
        for line in body3.splitlines():
            if not line.startswith("data:"):
                continue
            data = line[5:].strip()
            if data == "[DONE]":
                done = True
                continue
            try:
                ev = json.loads(data)
            except json.JSONDecodeError:
                continue
            delta = ev.get("choices", [{}])[0].get("delta", {})
            if has_sentinel(data):
                sentinel_hit = True
            if isinstance(delta.get("content"), str) and delta["content"]:
                content_deltas.append(delta["content"])
            if isinstance(delta.get("reasoning_content"), str) and delta["reasoning_content"]:
                reasoning_deltas += 1
        reassembled = "".join(content_deltas)
        check("Tour 3 — terminaison [DONE]", done)
        check("Tour 3 — deltas reasoning reçus", reasoning_deltas > 0, f"n={reasoning_deltas}")
        check("Tour 3 — zéro sentinelle dans les deltas (content ET reasoning)",
              not sentinel_hit)
        check_restored(reassembled, "Tour 3 content réassemblé")

    print()
    failed = [n for n, ok in CHECKS if not ok]
    print(f"=== BILAN : {len(CHECKS) - len(failed)}/{len(CHECKS)} PASS ===")
    if failed:
        print("ÉCHECS : " + ", ".join(failed))
        return 1
    print("TOUS LES CONTRÔLES : PASS")
    return 0


if __name__ == "__main__":
    sys.exit(main())
