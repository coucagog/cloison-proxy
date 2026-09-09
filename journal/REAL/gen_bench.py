"""Génère des payloads de requête synthétiques (code + PII factice) pour benchmark.

Taille cible en tokens (~4 caractères/token) :
  10k tokens  → ~40 000 chars
  50k tokens  → ~200 000 chars
  100k tokens → ~400 000 chars
"""

from __future__ import annotations

import json
from pathlib import Path

OUT = Path("/home/mls/MesProjets/cloison-poc/bench")


def make_body(n_chars: int) -> dict:
    block = "\n".join(
        [
            "def process_item_{i}(payload, config):",
            "    # norme interne : pas de secret dans les logs",
            "    # Contact: Moussa Faye (moussa.faye@example.sn) tel +221 70 123 45 67",
            "    email = config.get('support', 'support@exemple.sn')",
            "    result = transform(payload, max_depth=4, retries=2)",
            "    return result",
        ]
    ).replace("{i}", "")
    # un bloc ≈ 300 chars ; dupliquer jusqu'à atteindre la cible
    chunks = []
    total = 0
    i = 0
    while total < n_chars:
        b = block.replace("process_item_", f"process_item_{i}_")
        chunks.append(b)
        total += len(b) + 1
        i += 1
    content = "\n".join(chunks)[:n_chars]
    return {
        "model": "gpt-4o",
        "messages": [{"role": "user", "content": content}],
    }


def main() -> None:
    OUT.mkdir(exist_ok=True)
    for label, tokens in [("10k", 10_000), ("50k", 50_000), ("100k", 100_000)]:
        body = make_body(tokens * 4)
        path = OUT / f"payload-{label}.json"
        path.write_text(json.dumps(body), encoding="utf-8")
        print(f"{label} tokens → {path} ({path.stat().st_size/1024:.0f} Ko)")


if __name__ == "__main__":
    main()
