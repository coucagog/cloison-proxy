#!/usr/bin/env python3
# =============================================================================
# CLOISON STACK-7 — Faux LLM pour l'e2e réel anti-pass-through (mock_llm.py).
#
# Rôle : remplacer le fournisseur LLM pendant le test. Deux comportements :
#   1. JOURNALISE le corps JSON reçu (complet, tel quel) dans MOCK_LOG_FILE —
#      c'est cette trace qui permet d'ASSERTER que l'amont a reçu des
#      SENTINELLES CLOISON (⟦…) et PAS la PII en clair ;
#   2. répond en **echo** : le contenu du dernier message utilisateur est
#      renvoyé tel quel comme contenu de la réponse (style OpenAI
#      /chat/completions) — le proxy restaure ensuite les sentinelles, ce qui
#      permet d'asserter la restauration côté client.
#
# Aucune dépendance hors stdlib (http.server) : exécutable dans
# python:3.11-slim monté dans le réseau docker du compose :
#   docker run -d --rm --name cloison-mock-llm --network <cloison-dev_cloison-net> \
#     --user "$(id -u):$(id -g)" \
#     -v deploy/mock_llm.py:/mock_llm.py:ro -v <dir>:/mock-data \
#     -e MOCK_PORT=8799 -e MOCK_LOG_FILE=/mock-data/last_body.json \
#     python:3.11-slim python /mock_llm.py
#
# Variables : MOCK_PORT (défaut 8799), MOCK_LOG_FILE (défaut
# /mock-data/last_body.json). Répond 200 sur toute route (GET/POST) — le
# proxy n'utilise que /chat/completions (POST) et /models (GET).
# =============================================================================
import json
import os
import sys
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer

PORT = int(os.environ.get("MOCK_PORT", "8799"))
LOG_FILE = os.environ.get("MOCK_LOG_FILE", "/mock-data/last_body.json")


class Handler(BaseHTTPRequestHandler):
    protocol_version = "HTTP/1.1"

    def log_message(self, fmt, *args):
        # Silencieux de toute façon : les logs contiendraient des sentinelles
        # (le corps reçu est TOKENISÉ, mais ne pas les exposer inutilement).
        pass

    def _read_body(self) -> bytes:
        length = int(self.headers.get("Content-Length", "0") or 0)
        return self.rfile.read(length) if length else b""

    def _reply(self, obj: dict, status: int = 200):
        data = json.dumps(obj, ensure_ascii=False).encode("utf-8")
        self.send_response(status)
        self.send_header("Content-Type", "application/json")
        self.send_header("Content-Length", str(len(data)))
        self.end_headers()
        self.wfile.write(data)

    def _log_body(self, raw: bytes):
        try:
            with open(LOG_FILE, "wb") as f:
                f.write(raw)
        except OSError as e:
            # Non bloquant : le test échouera faute de trace, jamais en crash.
            sys.stderr.write("mock_llm: journalisation impossible: %s\n" % e)

    def do_POST(self):
        raw = self._read_body()
        self._log_body(raw)
        try:
            body = json.loads(raw or b"{}")
        except json.JSONDecodeError:
            self._reply({"error": {"message": "mock_llm: corps JSON invalide"}}, 400)
            return
        # Echo : contenu du dernier message utilisateur, tel que reçu
        # (avec les sentinelles ⟦…⟧ — la restauration est le travail du proxy).
        content = ""
        for message in reversed(body.get("messages", [])):
            if message.get("role") == "user":
                content = message.get("content", "")
                break
        self._reply(
            {
                "id": "mock-llm-echo",
                "object": "chat.completion",
                "created": 0,
                "model": body.get("model", "mock"),
                "choices": [
                    {
                        "index": 0,
                        "message": {"role": "assistant", "content": content},
                        "finish_reason": "stop",
                    }
                ],
                "usage": {"prompt_tokens": 0, "completion_tokens": 0, "total_tokens": 0},
            }
        )

    def do_GET(self):
        # /models (et toute autre route) : liste minimale OpenAI-compatible.
        self._reply(
            {
                "object": "list",
                "data": [{"id": "mock-model", "object": "model", "owned_by": "mock"}],
            }
        )


if __name__ == "__main__":
    print("mock_llm: écoute sur 0.0.0.0:%d, journal -> %s" % (PORT, LOG_FILE), flush=True)
    ThreadingHTTPServer(("0.0.0.0", PORT), Handler).serve_forever()
