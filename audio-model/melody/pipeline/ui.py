#!/usr/bin/env python3
"""The compiler's page: add songs from Rekordbox, follow them through ae-charlie, see what the checks found, correct and recompile.

    python3 audio-model/melody/pipeline/ui.py --open     # http://127.0.0.1:8767

Anything that changes something runs song.py as a separate process, so the page and the command line do exactly the same thing,
and this server never logs in to ae-charlie: it only asks ssh locally whether the gateway is open. Standard library only,
bound to localhost, and POSTs are accepted from this page only, because they start jobs and can cost money.
"""

import argparse
import itertools
import json
import os
import re
import subprocess
import sys
import threading
import time
import webbrowser
from http import HTTPStatus
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
from pathlib import Path
from urllib.parse import parse_qs, urlparse

sys.dont_write_bytecode = True
HERE = Path(__file__).resolve().parent
sys.path.insert(0, str(HERE))
import compile_song  # noqa: E402
import library as lib  # noqa: E402
import song as cli  # noqa: E402

ID = re.compile(r"^[0-9a-f]{12}$")
KEEP_TASKS, KEEP_LINES = 30, 400
tasks: list[dict] = []
tasks_lock = threading.Lock()
next_task = itertools.count(1)


def run_task(argv: list[str], label: str, song: str | None) -> dict:
    """Starts song.py and collects its output. The same command on the same song can't run twice at once."""
    with tasks_lock:
        if any(t["running"] and t["argv"][:2] == argv[:2] for t in tasks):
            raise ValueError("That's already running.")
        task = {"id": next(next_task), "label": label, "song": song, "argv": argv, "running": True, "started": time.time(), "ended": None, "rc": None, "lines": []}
        tasks.insert(0, task)
        del tasks[KEEP_TASKS:]
    proc = subprocess.Popen([sys.executable, str(HERE / "song.py"), *argv], stdout=subprocess.PIPE, stderr=subprocess.STDOUT, text=True, cwd=lib.MODEL_DIR.parent, env={**os.environ, "PYTHONUNBUFFERED": "1"})

    def collect() -> None:
        for line in proc.stdout:
            task["lines"] = (task["lines"] + [line.rstrip()])[-KEEP_LINES:]
        task.update(rc=proc.wait(), ended=time.time(), running=False)

    threading.Thread(target=collect, daemon=True).start()
    return task


def command(body: dict) -> tuple[list[str], str, str | None]:
    """What the page may ask for, as song.py's arguments. Nothing goes through a shell."""
    cmd, sid = body.get("cmd"), body.get("song")
    extra = ["--extra"] if body.get("extra") else []
    if cmd == "add":
        path = body.get("path")
        if not isinstance(path, str) or not path.startswith("/"):
            raise ValueError("add needs the song's full path")
        return ["add", path, "--wait", *extra], f"Add {Path(path).stem}", cli.song_id(path)
    if cmd == "compile" and body.get("scope") in ("stale", "all"):
        return ["compile", "--" + body["scope"]], f"Compile {'stale songs' if body['scope'] == 'stale' else 'all songs'}", None
    if not isinstance(sid, str) or not ID.match(sid):
        raise ValueError("no such song")
    if cmd == "analyze":
        return ["analyze", sid, "--wait", *extra], "Analyze", sid
    if cmd in ("follow", "fetch", "compile", "review"):
        return [cmd, sid], {"follow": "Follow", "fetch": "Fetch", "compile": "Compile", "review": "LLM review"}[cmd], sid
    if cmd == "fix":
        if body.get("clear"):
            return ["fix", sid, "--clear"], "Remove corrections", sid
        grid, shift, bars, note = body.get("grid"), body.get("shift_16ths"), body.get("bar_shift"), body.get("note") or ""
        if grid not in ("auto", "rekordbox", "trackers") or not (isinstance(shift, int) and -16 <= shift <= 16) or not (isinstance(bars, int) and 0 <= bars <= 3) or not isinstance(note, str):
            raise ValueError("That correction isn't valid.")
        return ["fix", sid, f"--grid={grid}", f"--shift-16ths={shift}", f"--bar-shift={bars}", f"--note={note[:300]}"], "Correct the grid", sid
    raise ValueError("unknown command")


def read_jobs() -> list[dict]:
    return [json.loads(p.read_text()) for p in sorted(lib.JOBS.glob("*.json"))] if lib.JOBS.exists() else []


def song_rows() -> list[dict]:
    songs, meta = lib.load_songs()
    code, index = compile_song.code_hash(), cli.read_index()
    return [{"id": s["id"], "name": s["name"], "genre": s["genre"], "has_audio": s["has_audio"], "bpm": index.get(s["id"], {}).get("bpm", s.get("bpm")),
             "check": index.get(s["id"], {}).get("check"), "compiled": s["id"] in index, "stale": cli.is_stale(s, meta, code)} for s in songs]


def state() -> dict:
    index = lib.COMPILED / "index.json"
    return {"gateway": cli.gateway_open(), "stamp": index.stat().st_mtime_ns if index.exists() else 0, "jobs": read_jobs(),
            "tasks": [{k: t[k] for k in ("id", "label", "song", "running", "started", "ended", "rc", "lines")} for t in tasks], "now": time.time()}


def corrections(sid: str) -> dict:
    return compile_song.read_json(lib.CORRECTIONS / f"{sid}.json") or {}


class Handler(BaseHTTPRequestHandler):
    viewer = "http://127.0.0.1:8766"

    def log_message(self, fmt, *args):  # quiet: only errors
        if not str(args[1] if len(args) > 1 else "").startswith(("2", "3")):
            super().log_message(fmt, *args)

    def reply(self, body, status: HTTPStatus = HTTPStatus.OK, ctype: str = "application/json") -> None:
        data = body if isinstance(body, bytes) else json.dumps(body, ensure_ascii=False).encode()
        self.send_response(status)
        self.send_header("Content-Type", ctype)
        self.send_header("Content-Length", str(len(data)))
        self.send_header("Cache-Control", "no-store")
        self.end_headers()
        self.wfile.write(data)

    def do_GET(self):
        url = urlparse(self.path)
        if url.path == "/":
            return self.reply((HERE / "ui.html").read_bytes(), ctype="text/html; charset=utf-8")
        if url.path == "/api/state":
            return self.reply({**state(), "viewer": self.viewer})
        if url.path == "/api/songs":
            return self.reply(song_rows())
        if url.path == "/api/find":
            q = (parse_qs(url.query).get("q") or [""])[0].strip()
            if len(q) < 2:
                return self.reply({"error": "Type at least two letters."}, HTTPStatus.BAD_REQUEST)
            try:
                return self.reply(cli.find(q))
            except SystemExit as e:  # song.py stops with a message if Rekordbox can't be read
                return self.reply({"error": str(e)}, HTTPStatus.INTERNAL_SERVER_ERROR)
        parts = url.path.strip("/").split("/")
        if len(parts) == 3 and parts[:2] == ["api", "corrections"] and ID.match(parts[2]):
            return self.reply(corrections(parts[2]))
        self.send_error(HTTPStatus.NOT_FOUND)

    def do_POST(self):
        # Only this page: other sites can't send JSON without a preflight (which fails), or reach us under their name.
        if self.path != "/api/run" or self.headers.get_content_type() != "application/json" or self.headers.get("Host", "").split(":")[0] not in ("127.0.0.1", "localhost"):
            return self.send_error(HTTPStatus.FORBIDDEN)
        try:
            body = json.loads(self.rfile.read(min(int(self.headers.get("Content-Length") or 0), 10_000)))
            argv, label, song = command(body if isinstance(body, dict) else {})
            task = run_task(argv, label, song)
        except ValueError as e:  # includes malformed JSON
            return self.reply({"error": str(e)}, HTTPStatus.BAD_REQUEST)
        self.reply({"task": task["id"]})


def main() -> None:
    ap = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    ap.add_argument("--port", type=int, default=8767)
    ap.add_argument("--viewer-port", type=int, default=8766)
    ap.add_argument("--open", action="store_true", help="open the page in the default browser")
    args = ap.parse_args()
    Handler.viewer = f"http://127.0.0.1:{args.viewer_port}"
    url = f"http://127.0.0.1:{args.port}/"
    server = ThreadingHTTPServer(("127.0.0.1", args.port), Handler)
    print(f"Compiler page at {url}", flush=True)
    if args.open:
        webbrowser.open(url)
    try:
        server.serve_forever()
    except KeyboardInterrupt:
        pass


if __name__ == "__main__":
    main()
