# /// script
# requires-python = ">=3.11"
# dependencies = ["fastapi>=0.110", "uvicorn>=0.29", "httpx>=0.27"]
# ///
"""Show server: rekordbox playlists from the Pro DJ Link network in, compiled light shows out.

    uv run audio-model/melody/showserver/server.py [--port 8770] [--host 0.0.0.0] [--iface en0] [--player 7]

Starts the Pro DJ Link bridge (prolink-bridge/bridge.mjs), follows what's on each deck, and runs
a queue: one download from the players at a time, analysis on the enabled workers, then compile.
The page is at /, the API under /api (see README.md).
"""

import argparse
import asyncio
import contextlib
import json
import os
import subprocess
import sys
import threading
import time
from pathlib import Path

import httpx
import uvicorn
from fastapi import Body, FastAPI, HTTPException, Request
from fastapi.responses import FileResponse, HTMLResponse, StreamingResponse

HERE = Path(__file__).resolve().parent
sys.path.insert(0, str(HERE))
sys.dont_write_bytecode = True
import ingest  # noqa: E402
import workers as W  # noqa: E402
from store import ACTIVE, Store  # noqa: E402
from workers import lib  # noqa: E402

PREFETCH = 3  # downloaded songs waiting for a worker before the next download starts (deck tracks always go)

app = FastAPI(title="Show server")
state = {"bridge": None, "decks": [], "bridge_state": "starting", "version": 0}
store: Store
workers: dict[str, W.Worker] = {}
running: dict[int, str] = {}  # job id -> worker name
hold: dict[int, float] = {}  # job id -> don't retry before (after a Retry)
downloading = {"job": None}


def bump():
    state["version"] += 1


# ---- bridge process and deck status
def start_bridge(args) -> subprocess.Popen | None:
    if args.bridge:
        return None
    env = {**os.environ, "BRIDGE_PORT": str(args.bridge_port), "VCDJ_ID": str(args.player)}
    if args.iface:
        env["PROLINK_IFACE"] = args.iface
    bdir = HERE / "prolink-bridge"
    if not (bdir / "node_modules").exists():
        subprocess.run(["npm", "install", "--no-audit", "--no-fund"], cwd=bdir, check=True)
    log = open(lib.MODEL_DIR / "data/melody/showserver-bridge.log", "a")
    return subprocess.Popen(["node", "bridge.mjs"], cwd=bdir, env=env, stdout=log, stderr=subprocess.STDOUT)


async def supervise_bridge(args):
    proc = start_bridge(args)
    try:
        while proc is not None:
            await asyncio.sleep(2)
            if proc.poll() is not None:
                state["bridge_state"] = f"bridge exited ({proc.returncode}), restarting"
                bump()
                await asyncio.sleep(3)
                proc = start_bridge(args)
    finally:
        if proc is not None and proc.poll() is None:
            proc.terminate()


async def follow_decks():
    """The bridge's deck events. Reconnects forever: over WiFi the stream may drop."""
    while True:
        try:
            async with httpx.AsyncClient(timeout=httpx.Timeout(10, read=60)) as c:
                h = (await c.get(f"{state['bridge']}/health")).json()
                state["bridge_state"] = h["state"]
                async with c.stream("GET", f"{state['bridge']}/events") as r:
                    event = None
                    async for line in r.aiter_lines():
                        if line.startswith("event: "):
                            event = line[7:]
                        elif line.startswith("data: ") and event == "decks":
                            state["decks"] = json.loads(line[6:])
                            bump()
        except Exception as e:  # bridge starting, restarting, or gone
            state["bridge_state"] = f"bridge unreachable ({type(e).__name__})"
            bump()
        await asyncio.sleep(2)


async def poll_health():
    while True:
        try:
            async with httpx.AsyncClient(timeout=5) as c:
                h = (await c.get(f"{state['bridge']}/health")).json()
            if h["state"] != state["bridge_state"]:
                state["bridge_state"] = h["state"]
                bump()
        except Exception:
            pass
        await asyncio.sleep(5)


def compiled(sid: str | None) -> bool:
    return bool(sid) and (lib.COMPILED / f"{sid}.json").exists()


def deck_rows() -> list[dict]:
    rows = []
    for d in state["decks"]:
        d = dict(d)
        t, m = d.get("track"), d.get("meta") or {}
        job = store.by_track(t["device"], t["slot"], t["id"]) if t else None
        sid = job and job["song_id"]
        if t and not sid and m.get("file_path"):
            k = store.known(m["file_path"], m.get("file_size"))
            sid = k and k["song_id"]
        d.update(job_id=job and job["id"], job_state=job and job["state"], song_id=sid, show_ready=compiled(sid),
                 show_url=f"/api/shows/{sid}" if compiled(sid) else None)
        rows.append(d)
    return rows


# ---- queue
def priority(job: dict) -> tuple:
    deck = 0
    for d in state["decks"]:
        t = d.get("track")
        if t and (t["device"], t["slot"], t["id"]) == (job["device"], job["slot"], job["track_id"]):
            deck = max(deck, 3 if d.get("on_air") and d.get("playing") else 2 if d.get("playing") else 1)
    return (-deck, -(job["bump"] or 0), job["id"])


def on_deck(job: dict) -> bool:
    return priority(job)[0] < 0


def run_download(job: dict):
    store.update(job["id"], state="downloading", step="downloading", error=None)
    bump()
    try:
        fields = ingest.ingest(state["bridge"], store, job)
        ready = compiled(fields["song_id"])
        store.update(job["id"], **fields, state="ready" if ready else "waiting", step="already compiled" if ready else None)
    except Exception as e:
        cur = store.get(job["id"])
        if cur and cur["state"] == "downloading":
            store.update(job["id"], state="queued", step=None, error=f"download: {e}")
            hold[job["id"]] = time.time() + 15
    finally:
        downloading["job"] = None
        bump()


def run_analysis(job: dict, worker: W.Worker, resume: bool):
    jid = job["id"]

    def progress(**fields):
        cur = store.get(jid)
        if cur and cur["state"] == "cancelled":
            raise RuntimeError("cancelled")
        store.update(jid, **fields)
        bump()

    store.update(jid, state="analysing", worker=worker.name, error=None, step="starting")
    bump()
    try:
        worker.run(store.get(jid), progress, resume=resume)
        store.update(jid, state="ready", step=None)
    except W.Retry as e:
        store.update(jid, state="waiting", step=None, error=str(e))
        hold[jid] = time.time() + 30
    except Exception as e:
        cur = store.get(jid)
        if cur and cur["state"] != "cancelled":
            store.update(jid, state="failed", step=None, error=str(e)[:500])
    finally:
        running.pop(jid, None)
        bump()


async def scheduler():
    resumed = False
    while True:
        await asyncio.sleep(1)
        now = time.time()
        free = lambda j: hold.get(j["id"], 0) <= now and j["id"] not in running
        # 1. one download at a time
        if downloading["job"] is None and state["bridge_state"] == "connected":
            queued = sorted((j for j in store.in_state("queued") if free(j)), key=priority)
            buffered = len([j for j in store.in_state("waiting")])
            if queued and (buffered < PREFETCH or on_deck(queued[0])):
                downloading["job"] = queued[0]["id"]
                threading.Thread(target=run_download, args=(queued[0],), daemon=True).start()
        # 2. analyses on every enabled, available worker; after a restart, follow the ones that were running.
        # A worker that prefers others (the Mac) only takes a song when no other worker has room for it.
        avail = {}
        for w in workers.values():
            avail[w.name] = w.enabled and (await asyncio.to_thread(w.available))[0]
        room = lambda w: w.concurrency - sum(1 for n in running.values() if n == w.name)
        for w in workers.values():
            if not avail[w.name]:
                continue
            if w.prefer_others and resumed and any(avail[o.name] and room(o) > 0 for o in workers.values() if o is not w):
                continue
            if not resumed:
                for j in store.in_state("analysing"):
                    if j["worker"] == w.name and j["id"] not in running:
                        running[j["id"]] = w.name
                        threading.Thread(target=run_analysis, args=(j, w, True), daemon=True).start()
            busy = sum(1 for n in running.values() if n == w.name)
            for j in sorted((j for j in store.in_state("waiting") if free(j)), key=priority)[: max(0, w.concurrency - busy)]:
                running[j["id"]] = w.name
                threading.Thread(target=run_analysis, args=(j, w, False), daemon=True).start()
        resumed = True


# ---- API
def bridge_get(path: str, **params):
    try:
        r = httpx.get(f"{state['bridge']}{path}", params={k: v for k, v in params.items() if v is not None}, timeout=60)
    except httpx.HTTPError as e:
        raise HTTPException(503, f"bridge unreachable: {e}")
    if r.status_code != 200:
        raise HTTPException(r.status_code, r.json().get("error", r.text))
    return r.json()


def job_row(j: dict) -> dict:
    return {**j, "show_ready": compiled(j["song_id"]), "show_url": f"/api/shows/{j['song_id']}" if compiled(j["song_id"]) else None,
            "deck_priority": -priority(j)[0]}


@app.get("/", response_class=HTMLResponse)
def page():
    return (HERE / "ui.html").read_text()


@app.get("/api/health")
def health():
    return {"bridge": state["bridge_state"], "downloading": downloading["job"], "running": running,
            "workers": [w.info() for w in workers.values()]}


@app.get("/api/devices")
def devices():
    """Players and their media slots. Only rekordbox-exported USB/SD media can be browsed."""
    out = []
    for d in bridge_get("/devices"):
        d["slots"] = [{"slot": 3, "name": "USB"}, {"slot": 2, "name": "SD"}] if d["type"] == "CDJ" else []
        out.append(d)
    return out


@app.get("/api/browse")
def browse(device: int, slot: int, id: int | None = None):
    """A folder or playlist on a player's media; tracks come with their job state."""
    doc = bridge_get("/playlists", device=device, slot=slot, id=id)
    for t in doc["tracks"]:
        j = store.by_track(device, slot, t["id"])
        t["job"] = job_row(j) if j else None
    return doc


@app.post("/api/queue")
def queue(body: dict = Body(...)):
    """{device, slot, playlist_id?, playlist?, track_ids?}: a whole playlist in order, or some tracks."""
    device, slot = int(body["device"]), int(body["slot"])
    if body.get("track_ids"):
        tracks = [bridge_get("/track", device=device, slot=slot, id=int(i)) for i in body["track_ids"]]
    elif body.get("playlist_id") is not None:
        tracks = bridge_get("/playlists", device=device, slot=slot, id=int(body["playlist_id"]))["tracks"]
    else:
        raise HTTPException(400, "give track_ids or playlist_id")
    jobs = [store.add(device, slot, t, body.get("playlist"), i) for i, t in enumerate(tracks)]
    bump()
    return {"queued": len(jobs), "jobs": [job_row(j) for j in jobs]}


@app.get("/api/jobs")
def jobs():
    rows = [job_row(j) for j in store.jobs()]
    active = sorted((r for r in rows if r["state"] in ACTIVE), key=lambda r: priority(r))
    return {"active": active, "done": [r for r in rows if r["state"] not in ACTIVE]}


@app.post("/api/jobs/{jid}/{action}")
def job_action(jid: int, action: str):
    j = store.get(jid)
    if not j:
        raise HTTPException(404, "no such job")
    if action == "bump":
        top = max([x["bump"] or 0 for x in store.in_state(*ACTIVE)] + [0])
        store.update(jid, bump=top + 1)
    elif action == "cancel":
        if j["state"] in ACTIVE:
            store.update(jid, state="cancelled", step=None)  # a song already on ae-charlie finishes there
    elif action == "retry":
        if j["state"] in ("failed", "cancelled"):
            store.update(jid, state="waiting" if j["song_id"] and Path(j["listed"] or "").exists() else "queued", error=None, step=None)
            hold.pop(jid, None)
    else:
        raise HTTPException(400, "bump, cancel or retry")
    bump()
    return job_row(store.get(jid))


@app.get("/api/decks")
def decks():
    return deck_rows()


@app.get("/api/workers")
def get_workers():
    return [w.info() for w in workers.values()]


@app.put("/api/workers/{name}")
def put_worker(name: str, body: dict = Body(...)):
    w = workers.get(name)
    if not w:
        raise HTTPException(404, "no such worker")
    if "enabled" in body:
        w.enabled = bool(body["enabled"])
    if "concurrency" in body:
        w.concurrency = max(1, min(8, int(body["concurrency"])))
    bump()
    return w.info()


@app.get("/api/shows")
def shows():
    """Every compiled show the queue produced, newest first."""
    seen, out = set(), []
    for j in store.jobs(2000):
        if j["song_id"] and j["song_id"] not in seen and compiled(j["song_id"]):
            seen.add(j["song_id"])
            out.append({"song_id": j["song_id"], "title": j["title"], "artist": j["artist"], "url": f"/api/shows/{j['song_id']}"})
    return out


@app.get("/api/shows/{sid}")
def show(sid: str):
    if not sid.isalnum() or not compiled(sid):
        raise HTTPException(404, "no compiled show for this song (yet)")
    return FileResponse(lib.COMPILED / f"{sid}.json", media_type="application/json")


@app.get("/api/events")
async def events(request: Request):
    """Server-sent events: `decks` and `jobs` snapshots whenever something changes."""
    async def gen():
        last = -1
        while not await request.is_disconnected():
            if state["version"] != last:
                last = state["version"]
                d, j = await asyncio.to_thread(lambda: (deck_rows(), jobs()))
                yield f"event: decks\ndata: {json.dumps(d)}\n\n"
                yield f"event: jobs\ndata: {json.dumps(j)}\n\n"
                yield f"event: health\ndata: {json.dumps(health())}\n\n"
            else:
                await asyncio.sleep(0.5)
    return StreamingResponse(gen(), media_type="text/event-stream", headers={"cache-control": "no-cache"})


def main():
    global store
    ap = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    ap.add_argument("--host", default="0.0.0.0", help="0.0.0.0 so consumers on the network can fetch shows")
    ap.add_argument("--port", type=int, default=8770)
    ap.add_argument("--iface", help="network interface or IPv4 of the DJ link (default: where the first player shows up)")
    ap.add_argument("--player", type=int, default=7, help="our virtual player number; keep it clear of real players and blaulicht's plugin")
    ap.add_argument("--bridge-port", type=int, default=8771)
    ap.add_argument("--bridge", help="use a bridge that's already running at this URL instead of starting one")
    ap.add_argument("--mac-worker", action="store_true", help="also analyse on this Mac (a child process, one song at a time) when ae-charlie is busy or unreachable")
    ap.add_argument("--db", default=str(lib.MODEL_DIR / "data/melody/showserver.db"))
    args = ap.parse_args()
    state["bridge"] = args.bridge or f"http://127.0.0.1:{args.bridge_port}"
    store = Store(Path(args.db))
    workers.update({w.name: w for w in W.all_workers()})
    if args.mac_worker and "mac" in workers:
        workers["mac"].enabled = True

    @contextlib.asynccontextmanager
    async def lifespan(_app):
        tasks = [asyncio.create_task(c) for c in (supervise_bridge(args), follow_decks(), poll_health(), scheduler())]
        yield
        for t in tasks:
            t.cancel()
        W.MacWorker.shutdown()

    app.router.lifespan_context = lifespan
    uvicorn.run(app, host=args.host, port=args.port, log_level="warning")


if __name__ == "__main__":
    main()
