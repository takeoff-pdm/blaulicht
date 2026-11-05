"""The blind A/B page's server, port 8768, bound to localhost; stop it with Ctrl-C.

    python3 audio-model/melody/decision/serve_ab.py    # then open http://127.0.0.1:8768/

Serves ab.html, club_plan.js, a trial's song document with one side's plan (never the variant's name) and the
song's audio from its Rekordbox path, and appends each answer to ab_results/answers.jsonl. Standard library only.
"""

import argparse
import base64
import json
from datetime import datetime, timezone
from http import HTTPStatus
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
from pathlib import Path

HERE = Path(__file__).resolve().parent
DATA = HERE.parent.parent / "data/melody/viz"
TRIALS = json.loads((HERE / "ab_trials.json").read_text())
PATHS = {s["id"]: s["path"] for s in json.loads((DATA / "songs.json").read_text())}
RESULTS = HERE / "ab_results"
STATIC = {"/": ("ab.html", "text/html; charset=utf-8"), "/club_plan.js": ("club_plan.js", "text/javascript; charset=utf-8"),
          "/club_looks.mjs": ("../viz/club_looks.mjs", "text/javascript; charset=utf-8")}


def answered():
    f = RESULTS / "answers.jsonl"
    if not f.exists():
        return {}
    done = {}
    for line in f.read_text().splitlines():
        a = json.loads(line)
        done[a["n"]] = a
    return done


class Handler(BaseHTTPRequestHandler):
    def log_message(self, fmt, *args):
        pass

    def send(self, body, ctype="application/json", cache=False, status=HTTPStatus.OK):
        self.send_response(status)
        self.send_header("Content-Type", ctype)
        self.send_header("Content-Length", str(len(body)))
        self.send_header("Cache-Control", "max-age=3600" if cache else "no-store")
        self.end_headers()
        self.wfile.write(body)

    def do_GET(self):
        path = self.path.split("?", 1)[0]
        if path in STATIC:
            name, ctype = STATIC[path]
            return self.send((HERE / name).read_bytes(), ctype)
        parts = path.strip("/").split("/")
        if path == "/api/trials":  # no variant names here
            done = answered()
            return self.send(json.dumps([{"n": t["n"], "song": t["song"], "windows": t["windows"], "done": t["n"] in done} for t in TRIALS]).encode())
        if len(parts) == 4 and parts[:2] == ["api", "doc"]:  # /api/doc/<n>/<A|B>
            t = TRIALS[int(parts[2])]
            variant = t[parts[3]]
            doc = json.loads((HERE / "cache" / f"{t['song']}.json").read_text())
            doc["plan"] = None if variant == "rules" else json.loads((HERE / "plans" / variant / f"{t['song']}.json").read_text())
            return self.send(json.dumps(doc, separators=(",", ":")).encode())
        if len(parts) == 3 and parts[:2] == ["api", "audio"] and parts[2] in PATHS:
            p = Path(PATHS[parts[2]])
            return self.send(p.read_bytes(), "audio/mpeg", cache=True)
        self.send_error(HTTPStatus.NOT_FOUND)

    def do_POST(self):
        if self.path != "/api/answer":
            return self.send_error(HTTPStatus.NOT_FOUND)
        a = json.loads(self.rfile.read(int(self.headers.get("Content-Length", 0))))
        if not isinstance(a.get("n"), int) or not 0 <= a["n"] < len(TRIALS) or a.get("choice") not in ("A", "B", "same"):
            return self.send_error(HTTPStatus.BAD_REQUEST)
        a["time"] = datetime.now(timezone.utc).isoformat()
        RESULTS.mkdir(exist_ok=True)
        with open(RESULTS / "answers.jsonl", "a") as f:
            f.write(json.dumps(a) + "\n")
        self.send(b"{}")


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--port", type=int, default=8768)
    args = ap.parse_args()
    print(f"{len(TRIALS)} trials at http://127.0.0.1:{args.port}/", flush=True)
    ThreadingHTTPServer(("127.0.0.1", args.port), Handler).serve_forever()


if __name__ == "__main__":
    main()
