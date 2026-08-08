"""Control server for the temporary in-app autopilot.

State lives OUTSIDE the repo (Vite full-reloads the app when any file
under the project root changes, wiping frontend state mid-flow):
  $AP_DIR (default /tmp/autogroup-autopilot)/
    cmd.json     command the autopilot polls for (written by send.py)
    results.log  one JSON line per executed command
    shots/       PNG screenshots posted by the autopilot

Endpoints (all CORS *):
  GET  /cmd             -> current cmd.json
  GET  /ap?id=&ok=&msg= -> append execution result to results.log
  POST /shot?name=NAME  -> body is a data:image/png;base64 URL; saved as NAME.png
"""

import base64
import json
import os
import urllib.parse
from http.server import BaseHTTPRequestHandler, HTTPServer

AP_DIR = os.environ.get("AP_DIR", "/tmp/autogroup-autopilot")
CMD_FILE = os.path.join(AP_DIR, "cmd.json")
LOG_FILE = os.path.join(AP_DIR, "results.log")
SHOTS_DIR = os.path.join(AP_DIR, "shots")


class Handler(BaseHTTPRequestHandler):
    def _send(self, code, body, ctype="application/json"):
        data = body.encode("utf-8")
        self.send_response(code)
        self.send_header("Content-Type", ctype)
        self.send_header("Access-Control-Allow-Origin", "*")
        self.send_header("Cache-Control", "no-store")
        self.send_header("Content-Length", str(len(data)))
        self.end_headers()
        self.wfile.write(data)

    def do_GET(self):
        parsed = urllib.parse.urlparse(self.path)
        if parsed.path == "/cmd":
            try:
                with open(CMD_FILE, "r", encoding="utf-8") as f:
                    self._send(200, f.read())
            except FileNotFoundError:
                self._send(200, json.dumps({"id": 0, "code": ""}))
        elif parsed.path == "/ap":
            qs = urllib.parse.parse_qs(parsed.query)
            line = json.dumps(
                {
                    "id": qs.get("id", [""])[0],
                    "ok": qs.get("ok", [""])[0],
                    "msg": qs.get("msg", [""])[0],
                },
                ensure_ascii=False,
            )
            with open(LOG_FILE, "a", encoding="utf-8") as f:
                f.write(line + "\n")
            self._send(200, "{}")
        else:
            self._send(404, "{}")

    def do_POST(self):
        parsed = urllib.parse.urlparse(self.path)
        if parsed.path == "/shot":
            qs = urllib.parse.parse_qs(parsed.query)
            name = qs.get("name", ["shot"])[0]
            name = "".join(c for c in name if c.isalnum() or c in "-_")
            length = int(self.headers.get("Content-Length", "0"))
            body = self.rfile.read(length).decode("utf-8")
            prefix = "data:image/png;base64,"
            if body.startswith(prefix):
                os.makedirs(SHOTS_DIR, exist_ok=True)
                out = os.path.join(SHOTS_DIR, name + ".png")
                with open(out, "wb") as f:
                    f.write(base64.b64decode(body[len(prefix):]))
                self._send(200, json.dumps({"saved": name}))
            else:
                self._send(400, json.dumps({"error": "not a png data url"}))
        else:
            self._send(404, "{}")

    def do_OPTIONS(self):
        self.send_response(204)
        self.send_header("Access-Control-Allow-Origin", "*")
        self.send_header("Access-Control-Allow-Methods", "GET, POST, OPTIONS")
        self.send_header("Access-Control-Allow-Headers", "*")
        self.end_headers()

    def log_message(self, *args):
        pass


if __name__ == "__main__":
    os.makedirs(AP_DIR, exist_ok=True)
    HTTPServer(("127.0.0.1", 8799), Handler).serve_forever()
