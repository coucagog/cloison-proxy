#!/usr/bin/env python3
"""mock-llm.py v2 — faux fournisseur OpenAI-compatible (écho) pour la sonde.
GET /v1/models · POST /v1/chat/completions (non-stream JSON + stream SSE).
Imprime MOCK_RECU=<contenu> sur stdout : preuve anti-pass-through.
Le contenu est découpé en 3 chunks (sentinelles potentiellement coupées) :
exerce le buffer-and-scan du edge CLOISON.
"""
import json
from http.server import BaseHTTPRequestHandler, HTTPServer

def chunk(i, delta_content=None, finish=None):
    c = {"index": 0, "delta": {}, "finish_reason": finish}
    if delta_content is not None:
        c["delta"] = {"content": delta_content}
    return {"id": "chatcmpl-mock", "object": "chat.completion.chunk", "created": 0,
            "model": "gpt-4o-mini", "choices": [c]}

class H(BaseHTTPRequestHandler):
    def do_GET(self):
        if self.path == "/v1/models":
            body = json.dumps({"object": "list", "data": [
                {"id": "gpt-4o-mini", "object": "model", "owned_by": "mock"}]}).encode()
            self.send_response(200)
            self.send_header("Content-Type", "application/json")
            self.send_header("Content-Length", str(len(body)))
            self.end_headers()
            self.wfile.write(body)
        else:
            self.send_response(404); self.end_headers()

    def do_POST(self):
        if self.path != "/v1/chat/completions":
            self.send_response(404); self.end_headers(); return
        n = int(self.headers.get("Content-Length", 0))
        try:
            req = json.loads(self.rfile.read(n) or b"{}")
        except Exception:
            req = {}
        msgs = [m for m in req.get("messages", []) if isinstance(m.get("content"), str)]
        content = msgs[-1]["content"] if msgs else ""
        print("MOCK_RECU: " + content, flush=True)

        if req.get("stream"):
            self.send_response(200)
            self.send_header("Content-Type", "text/event-stream")
            self.send_header("Cache-Control", "no-cache")
            self.end_headers()
            # découpage volontaire en 3 (coupe possible en plein jeton)
            third = max(1, len(content) // 3)
            parts = [content[:third], content[third:2 * third], content[2 * third:]]
            self.wfile.write(("data: " + json.dumps(chunk(0, parts[0])) + "\n\n").encode())
            self.wfile.write(("data: " + json.dumps(chunk(1, parts[1])) + "\n\n").encode())
            self.wfile.write(("data: " + json.dumps(chunk(2, parts[2], finish="stop")) + "\n\n").encode())
            self.wfile.write(b"data: [DONE]\n\n")
            self.wfile.flush()
        else:
            body = json.dumps({
                "id": "chatcmpl-mock", "object": "chat.completion", "created": 0,
                "model": "gpt-4o-mini",
                "choices": [{"index": 0, "message": {"role": "assistant", "content": content},
                             "finish_reason": "stop"}],
                "usage": {"prompt_tokens": 0, "completion_tokens": 0, "total_tokens": 0},
            }).encode()
            self.send_response(200)
            self.send_header("Content-Type", "application/json")
            self.send_header("Content-Length", str(len(body)))
            self.end_headers()
            self.wfile.write(body)

    def log_message(self, *args):
        pass

HTTPServer(("0.0.0.0", 8000), H).serve_forever()
