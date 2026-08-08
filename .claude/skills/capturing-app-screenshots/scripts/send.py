"""Send a JS command to the in-app autopilot and wait for its result.

Usage:
    python3 send.py [timeout_seconds] <<'JS'
    return document.title;
    JS

Writes cmd.json into $AP_DIR (default /tmp/autogroup-autopilot, shared
with server.py) and polls results.log for the matching result line.
Exit code 0 on ok, 1 on JS error, 2 on timeout.

Never hand-write cmd.json: raw heredocs produce invalid JSON for
multi-line code and the autopilot silently ignores unparseable files.
A timeout is expected when the command reloads the page (location.reload
or a triggered Vite reload) - the result report dies with the page.
"""

import json
import os
import sys
import time

AP_DIR = os.environ.get("AP_DIR", "/tmp/autogroup-autopilot")
CMD_FILE = os.path.join(AP_DIR, "cmd.json")
LOG_FILE = os.path.join(AP_DIR, "results.log")

code = sys.stdin.read()
new_id = int(time.time() * 1000)
os.makedirs(AP_DIR, exist_ok=True)
with open(CMD_FILE, "w", encoding="utf-8") as f:
    json.dump({"id": new_id, "code": code}, f)

deadline = time.time() + (float(sys.argv[1]) if len(sys.argv) > 1 else 15)
while time.time() < deadline:
    try:
        with open(LOG_FILE, encoding="utf-8") as f:
            for line in f:
                rec = json.loads(line)
                if rec.get("id") == str(new_id):
                    print(json.dumps(rec, ensure_ascii=False))
                    sys.exit(0 if rec.get("ok") == "1" else 1)
    except FileNotFoundError:
        pass
    time.sleep(0.3)

print("TIMEOUT waiting for result of id", new_id)
sys.exit(2)
