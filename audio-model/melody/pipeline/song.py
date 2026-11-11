#!/usr/bin/env python3
"""Take songs from Rekordbox to the viewer, one at a time. See pipeline/README.md.

    song.py find "lean on"            search Rekordbox
    song.py add "lean on" [--wait]    pull the song and start its analysis on ae-charlie; --wait also fetches and compiles it
    song.py analyze <song> [--wait]   start or resume the analysis of a song that's been pulled
    song.py follow <song>             wait for a running analysis, then fetch and compile it
    song.py fetch <song>              bring a finished analysis back and compile it
    song.py compile <song>|--all|--stale
    song.py status [<song>] [--flags] [--all]
    song.py fix <song> --bar-shift 1 --note "..."     a hand correction, applied by the compiler
    song.py review <song>             an LLM's flags on a compiled song

A <song> is an id, or a piece of its name if that's unique.
"""

import argparse
import hashlib
import json
import multiprocessing as mp
import shlex
import subprocess
import sys
import time
from datetime import datetime, timezone
from pathlib import Path

sys.dont_write_bytecode = True
sys.path.insert(0, str(Path(__file__).resolve().parent))
import compile_song  # noqa: E402
import library as lib  # noqa: E402

GATEWAY, HOST = "ssh-proxy-gorilla", "ae-charlie"
REMOTE = "/localhome/sokrates/melody"
OPEN_IT = "Open it with `ssh -fN ssh-proxy-gorilla` (it asks for your password), then run this again."


class GatewayClosed(Exception):
    pass


# ---- ae-charlie. A login attempt through a closed gateway counts as a failed login on the HPI account, so nothing here
# tries one: the gateway is checked first, and the first connection failure stops everything.
def gateway_open() -> bool:
    return subprocess.run(["ssh", "-O", "check", GATEWAY], capture_output=True).returncode == 0


def ssh(command: str) -> subprocess.CompletedProcess:
    if not gateway_open():
        raise GatewayClosed
    r = subprocess.run(["ssh", "-o", "BatchMode=yes", HOST, command], capture_output=True, text=True)
    if r.returncode == 255:
        raise GatewayClosed
    return r


def rsync(*args: str) -> None:
    if not gateway_open():
        raise GatewayClosed
    r = subprocess.run(["rsync", "-a", "-s", "-e", "ssh -o BatchMode=yes", *args], capture_output=True, text=True)
    if r.returncode == 255:
        raise GatewayClosed
    if r.returncode:
        sys.exit(f"rsync failed: {r.stderr.strip()[-400:]}")


# ---- jobs: songs on their way through ae-charlie
def song_id(listed: str) -> str:
    return hashlib.sha1(f"{REMOTE}/audio{listed}".encode()).hexdigest()[:12]


def now() -> str:
    return datetime.now(timezone.utc).isoformat(timespec="seconds")


def read_job(sid: str) -> dict | None:
    p = lib.JOBS / f"{sid}.json"
    return json.loads(p.read_text()) if p.exists() else None


def write_job(job: dict, **changes) -> dict:
    job.update(changes, updated=now())
    lib.write_atomic(lib.JOBS / f"{job['id']}.json", json.dumps(job, ensure_ascii=False, indent=1) + "\n")
    return job


def remote_status(sid: str) -> dict | None:
    r = ssh(f"cat {REMOTE}/out/songs/{sid}.json 2>/dev/null")
    return json.loads(r.stdout) if r.returncode == 0 and r.stdout.strip() else None


def resolve_job(query: str) -> dict:
    q = query.lower()
    jobs = [json.loads(p.read_text()) for p in sorted(lib.JOBS.glob("*.json"))] if lib.JOBS.exists() else []
    hits = [j for j in jobs if j["id"].startswith(q)] or [j for j in jobs if q in j["name"].lower()]
    if len(hits) != 1:
        sys.exit(f"{len(hits)} pulled songs match {query!r}" + "".join(f"\n  {j['id']}  {j['name']}" for j in hits[:12]))
    return hits[0]


# ---- 1. pull
def rekordbox(*args: str) -> list | dict:
    r = subprocess.run(["uv", "run", "--quiet", str(lib.HERE / "rekordbox_show.py"), *args], capture_output=True, text=True)
    if r.returncode:
        sys.exit(f"reading Rekordbox failed: {r.stderr.strip()[-400:]}")
    return json.loads(r.stdout)


def find(query: str) -> list[dict]:
    """Songs in Rekordbox matching the words, and whether each is analysed, new, or has no audio on this Mac."""
    known = {s["path"] for s in json.loads((lib.DATA / "songs.json").read_text())}
    return [{**h, "id": song_id(h["path"]), "where": "analysed" if h["path"] in known else "new" if Path(h["path"]).exists() else "no audio"} for h in rekordbox("--find", query)]


def cmd_find(args) -> int:
    hits = find(args.query)
    for h in hits:
        print(f"{h['id']}  {h['bpm']:6.1f}  {h['where']:8}  {h['path']}")
    print(f"{len(hits)} songs")
    return 0


def pull(listed: str) -> dict:
    """The song's row from Rekordbox into rekordbox.json, and a job for it."""
    if not Path(listed).exists():
        sys.exit(f"The audio isn't on this Mac: {listed}")
    row = rekordbox("--path", listed)
    with lib.shared_lists():
        rows = json.loads(lib.REKORDBOX.read_text()) if lib.REKORDBOX.exists() else []
        rows = [r for r in rows if r["path"] != listed] + [row]
        lib.write_atomic(lib.REKORDBOX, json.dumps(rows))
    print(f"Pulled from Rekordbox: {Path(listed).stem} ({row['bpm']} BPM, {'with' if row.get('beats') else 'no'} beat grid)", flush=True)
    return read_job(song_id(listed)) or write_job({"id": song_id(listed), "name": Path(listed).stem, "listed": listed}, state="pulled")


# ---- 2. analyze
def start(job: dict, extra: bool) -> None:
    """Code and audio to ae-charlie, then the job, which keeps running there if the gateway closes."""
    try:
        rsync("--exclude", "__pycache__", "--exclude", "pilots", "--exclude", "edm_finetune", "--exclude", "fx_train", f"{lib.HERE.parent}/analyze/", f"{HOST}:{REMOTE}/code/analyze/")
        ssh(f"mkdir -p {REMOTE}/code/pipeline {REMOTE}/logs/songs {shlex.quote(str(Path(REMOTE + '/audio' + job['listed']).parent))}")
        rsync(str(lib.HERE / "remote_song.py"), str(lib.HERE / "build_viz_data.py"), f"{HOST}:{REMOTE}/code/pipeline/")
        rsync(job["listed"], f"{HOST}:{REMOTE}/audio{job['listed']}")
        # the old status goes first, so following the new job can't read the last run's "done" or "failed"
        r = ssh(f"cd {REMOTE} && rm -f out/songs/{job['id']}.json && nohup python3 code/pipeline/remote_song.py {shlex.quote(job['listed'])} {'--extra' if extra else ''} > logs/songs/{job['id']}.log 2>&1 < /dev/null &")
    except GatewayClosed:
        write_job(job, state="waiting for ae-charlie", step=None, steps_done=0, steps_total=None)
        print(f"Waiting for ae-charlie: the gateway is closed. {OPEN_IT}")
        raise SystemExit(3)
    if r.returncode:
        sys.exit(f"Could not start the job: {r.stderr.strip()[-400:]}")
    write_job(job, state="analysing", started=now(), step=None, steps_done=0, steps_total=None, analyzers_failed=[])
    print(f"Analysis started on ae-charlie: {job['id']}", flush=True)


def wait(job: dict) -> bool:
    """Follows the job until it ends. True if it finished; stops at the first connection failure."""
    seen = None
    while True:
        try:
            st = remote_status(job["id"])
        except GatewayClosed:
            print(f"The gateway closed. The job keeps running on ae-charlie. {OPEN_IT} Use `song.py fetch {job['id']}` when it's done.")
            return False
        if st:
            now_at = (st["state"], st.get("step"))
            if now_at != seen:
                print(f"  {st['state']}{': ' + st['step'] if st.get('step') else ''}", flush=True)
                seen = now_at
            write_job(job, step=st.get("step"), steps_done=sum(v["state"] == "done" for v in st["steps"].values()), steps_total=st.get("total"), analyzers_failed=st.get("failed") or [])
            if st["state"] in ("done", "failed"):
                return st["state"] == "done"
        time.sleep(15)


def cmd_add(args) -> int:
    if args.query.startswith("/"):
        listed = args.query
    else:
        hits = rekordbox("--find", args.query)
        if len(hits) != 1:
            print(f"{len(hits)} songs in Rekordbox match {args.query!r}. Be more specific, or give the path:")
            for h in hits[:12]:
                print(f"  {h['path']}")
            return 2
        listed = hits[0]["path"]
    job = pull(listed)
    start(job, args.extra)
    if not args.wait:
        print(f"Follow it with `song.py status {job['id']}`, and bring it back with `song.py fetch {job['id']}` when it's done.")
        return 0
    return fetch(job) if wait(job) else 1


def cmd_analyze(args) -> int:
    job = resolve_job(args.song)
    start(job, args.extra)
    if not args.wait:
        return 0
    return fetch(job) if wait(job) else 1


def cmd_follow(args) -> int:
    job = resolve_job(args.song)
    return fetch(job) if wait(job) else 1


# ---- 3. fetch and compile
def fetch(job: dict) -> int:
    sid = job["id"]
    try:
        st = remote_status(sid)
        if not st or st["state"] != "done":
            print(f"Not finished on ae-charlie: {st['state'] + (', at ' + str(st.get('step')) if st.get('step') else '') if st else 'no job there'}")
            if st and st["state"] == "failed":
                write_job(job, state="failed", failed=st.get("failed"))
                print(f"The logs are in {REMOTE}/logs/songs/{sid}/ on ae-charlie.")
            return 1
        rsync("--include", "*/", "--include", f"{sid}.json", "--exclude", "*", "--prune-empty-dirs", f"{HOST}:{REMOTE}/out/features/", f"{lib.FEATURES}/")
        rsync(f"{HOST}:{REMOTE}/viz/{sid}.json", f"{lib.DATA}/{sid}.json")
    except GatewayClosed:
        print(f"Waiting for ae-charlie: the gateway is closed. {OPEN_IT}")
        return 3
    doc = json.loads((lib.DATA / f"{sid}.json").read_text())
    with lib.shared_lists():
        songs = [s for s in json.loads((lib.DATA / "songs.json").read_text()) if s["id"] != sid]
        songs.append({"id": sid, "name": doc["name"], "path": job["listed"], "duration": round(doc["duration"], 2), "tracks": sorted(doc["notes"])})
        lib.write_atomic(lib.DATA / "songs.json", json.dumps(sorted(songs, key=lambda s: Path(s["path"]).stem.lower()), indent=1, ensure_ascii=False))
    failed = compile_songs([sid], 1)
    write_job(job, state="failed to compile" if failed else "compiled", step=None, analyzers_failed=st.get("failed") or [])
    if failed:
        return 1
    print_song(sid)
    return 0


def cmd_fetch(args) -> int:
    return fetch(resolve_job(args.song))


def compiled_stamp(sid: str) -> dict | None:
    """The `compiled` block of a song's file. It's the last key, so only the tail needs reading."""
    p = lib.COMPILED / f"{sid}.json"
    if not p.exists():
        return None
    with p.open("rb") as f:
        f.seek(max(0, p.stat().st_size - 400))
        tail = f.read().decode("utf-8", "ignore")
    i = tail.rfind('"compiled":')
    return json.loads(tail[i + 11:-1]) if i >= 0 else None


def is_stale(song: dict, meta: dict, code: str) -> bool:
    stamp = compiled_stamp(song["id"])
    return stamp is None or stamp.get("code") != code or stamp.get("inputs") != compile_song.inputs_hash(song["id"], song, meta.get(song["path"], {}))


def read_index() -> dict:
    p = lib.COMPILED / "index.json"
    return {r["id"]: r for r in json.loads(p.read_text())} if p.exists() else {}


def write_index(songs: list[dict], updates: dict) -> None:
    """The song list the viewer reads: every song that has a compiled file, with what compiling it found."""
    old, rows = read_index(), []
    for s in songs:
        if (lib.COMPILED / f"{s['id']}.json").exists():
            known = updates.get(s["id"]) or {k: old[s["id"]][k] for k in ("bpm", "has_beats", "check") if s["id"] in old and k in old[s["id"]]}
            rows.append({**s, **known})
    lib.write_atomic(lib.COMPILED / "index.json", json.dumps(rows))


_shared = {}


def _compile_one(sid: str) -> tuple[str, str | None, dict | None]:
    s = _shared
    try:
        return sid, None, compile_song.compile_to_file(s["by_id"][sid], s["meta"], s["by_id"], s["emb"], s["code"])
    except Exception as e:  # one broken song shouldn't stop the others
        return sid, f"{type(e).__name__}: {e}", None


def compile_songs(ids: list[str], jobs: int) -> list[str]:
    """Compiles these songs and updates the song list. Returns the ids that failed."""
    songs, meta = lib.load_songs()
    _shared.update(by_id={s["id"]: s for s in songs}, meta=meta, emb=compile_song.load_embeddings(), code=compile_song.code_hash())
    t0, failed, updates = time.time(), [], {}
    if jobs > 1 and len(ids) > 1:
        with mp.get_context("fork").Pool(jobs) as pool:
            results = list(pool.imap_unordered(_compile_one, ids))
    else:
        results = [_compile_one(i) for i in ids]
    for sid, err, row in results:
        if err:
            failed.append(sid)
            print(f"  failed: {_shared['by_id'][sid]['name']}: {err}", flush=True)
        else:
            updates[sid] = row
    with lib.shared_lists():
        write_index(songs, updates)
    print(f"compiled {len(ids) - len(failed)} of {len(ids)} songs in {time.time() - t0:.0f} s", flush=True)
    return failed


def resolve(query: str, songs: list[dict]) -> dict:
    q = query.lower()
    hits = [s for s in songs if s["id"].startswith(q)] or [s for s in songs if q in s["name"].lower()]
    if len(hits) != 1:
        sys.exit(f"{len(hits)} songs match {query!r}" + "".join(f"\n  {s['id']}  {s['name']}" for s in hits[:12]))
    return hits[0]


def cmd_compile(args) -> int:
    songs, meta = lib.load_songs()
    if args.all:
        ids = [s["id"] for s in songs]
    elif args.stale:
        code = compile_song.code_hash()
        ids = [s["id"] for s in songs if is_stale(s, meta, code)]
    elif args.song:
        ids = [resolve(args.song, songs)["id"]]
    else:
        sys.exit("name a song, or use --all or --stale")
    return 1 if compile_songs(ids, args.jobs) else 0


# ---- 4. check
def print_song(sid: str) -> None:
    row = read_index().get(sid)
    if not row:
        print(f"{sid}: not compiled")
        return
    c = row.get("check") or {}
    on_kick = f"{c.get('drops_on_kick')} of {c.get('drops')} on a kick" if c.get("drops_on_kick") is not None else "kicks unknown"
    share = f", trackers {round(100 * c['trackers_on_grid'])}% on it" if c.get("trackers_on_grid") is not None else ""
    print(f"{row['name']}  ({sid})\n  grid: {c.get('grid')}{share}, {row.get('bpm')} BPM\n  drops: {c.get('drops')}, {on_kick}\n  hand labels: {c.get('labels') or 'none'}"
          f"{'  hand corrections: ' + ', '.join(c['corrected_by_hand']) if c.get('corrected_by_hand') else ''}{'  LLM reviewed' if c.get('reviewed') else ''}")
    print(f"  to check: {len(c.get('flags', []))}" + "".join(f"\n    {'%7.1f s' % f['at'] if f.get('at') is not None else ' ' * 9}  {f['kind']}: {f['text']}" for f in c.get("flags", [])))


def cmd_status(args) -> int:
    songs, meta = lib.load_songs()
    code = compile_song.code_hash()
    if args.song:
        jobs = {p.stem: json.loads(p.read_text()) for p in lib.JOBS.glob("*.json")} if lib.JOBS.exists() else {}
        q = args.song.lower()
        hits = [s["id"] for s in songs if s["id"].startswith(q) or q in s["name"].lower()] + [j["id"] for j in jobs.values() if j["id"].startswith(q) or q in j["name"].lower()]
        hits = sorted(set(hits))
        if len(hits) != 1:
            sys.exit(f"{len(hits)} songs match {args.song!r}")
        sid = hits[0]
        job = jobs.get(sid)
        if job and job["state"] not in ("compiled",):
            print(f"{job['name']}  ({sid}): {job['state']}")
            try:
                st = remote_status(sid) if job["state"] in ("analysing", "waiting for ae-charlie") else None
                if st:
                    done = sum(v["state"] == "done" for v in st["steps"].values())
                    print(f"  on ae-charlie: {st['state']}{', at ' + st['step'] if st.get('step') else ''}, {done} steps done" + (f", failed: {', '.join(st['failed'])}" if st.get("failed") else ""))
            except GatewayClosed:
                print(f"  the gateway is closed, so I can't see ae-charlie. {OPEN_IT}")
        by_id = {s["id"]: s for s in songs}
        if sid in by_id:
            print_song(sid)
            if is_stale(by_id[sid], meta, code):
                print("  stale: the inputs or the compiler changed. Run `song.py compile --stale`.")
        return 0
    index = read_index()
    rows = [s for s in songs if args.all or s["genre"] == "electronic"]
    stale = [s for s in rows if is_stale(s, meta, code)]
    checks = [index.get(s["id"], {}).get("check") or {} for s in rows]
    count = lambda key: {v: sum(c.get(key) == v for c in checks) for v in sorted({str(c.get(key)) for c in checks} - {"None"})}
    kinds = {}
    for c in checks:
        for k in {f["kind"] for f in c.get("flags", [])}:
            kinds[k] = kinds.get(k, 0) + 1
    print(f"{len(rows)} {'' if args.all else 'electronic '}songs: {len(rows) - len(stale)} compiled and fresh, {len(stale)} stale or not compiled")
    print(f"grid: {count('grid')}")
    print(f"hand labels: {sum(c.get('labels') == 'done' for c in checks)} done, {sum(c.get('labels') == 'partial' for c in checks)} partial")
    print(f"songs to check: {sum(bool(c.get('flags')) for c in checks)}; by kind: {kinds}")
    jobs = [json.loads(p.read_text()) for p in sorted(lib.JOBS.glob('*.json'))] if lib.JOBS.exists() else []
    active = [j for j in jobs if j["state"] != "compiled"]
    if active:
        print("on their way:" + "".join(f"\n  {j['id']}  {j['name']}: {j['state']}" for j in active))
    if args.flags:
        for s, c in zip(rows, checks):
            if c.get("flags"):
                print(f"{s['id']}  {s['name'][:48]}" + "".join(f"\n    {'%7.1f s' % f['at'] if f.get('at') is not None else ' ' * 9}  {f['kind']}: {f['text']}" for f in c["flags"]))
    return 0


def cmd_fix(args) -> int:
    songs, _ = lib.load_songs()
    song = resolve(args.song, songs)
    path = lib.CORRECTIONS / f"{song['id']}.json"
    if args.clear:
        path.unlink(missing_ok=True)
        print("Corrections removed.")
    else:
        fix = json.loads(path.read_text()) if path.exists() else {"id": song["id"], "name": song["name"], "grid": {}}
        for key, value in (("source", args.grid), ("shift_16ths", args.shift_16ths), ("bar_shift", args.bar_shift)):
            if value in ("auto", 0):
                fix["grid"].pop(key, None)
            elif value is not None:
                fix["grid"][key] = value
        fix.update(author="human", updated=now(), note=args.note if args.note is not None else fix.get("note"))
        if fix["grid"]:
            lib.write_atomic(path, json.dumps(fix, ensure_ascii=False, indent=1) + "\n")
        else:
            path.unlink(missing_ok=True)
            print("No corrections left for this song.")
    failed = compile_songs([song["id"]], 1)
    if not failed:
        print_song(song["id"])
    return 1 if failed else 0


def cmd_review(args) -> int:
    import review_llm
    songs, _ = lib.load_songs()
    song = resolve(args.song, songs)
    out = review_llm.review(song["id"], args.model)
    print(f"{args.model}: {len(out['flags'])} flags in {out['seconds']} s, ${out['cost_usd']}. It changed nothing.", flush=True)
    failed = compile_songs([song["id"]], 1)
    if not failed:
        print_song(song["id"])
    return 1 if failed else 0


def main() -> int:
    ap = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    sub = ap.add_subparsers(dest="cmd", required=True)
    c = sub.add_parser("find", help="search Rekordbox")
    c.add_argument("query")
    c.set_defaults(fn=cmd_find)
    c = sub.add_parser("add", help="pull a song and start its analysis")
    c.add_argument("query", help="words from the title or path, or the full path")
    c.add_argument("--wait", action="store_true", help="wait for the analysis, then fetch and compile")
    c.add_argument("--extra", action="store_true", help="also the analyzers that only feed viewer lanes (btc, musicfm, mulan, mt3)")
    c.set_defaults(fn=cmd_add)
    c = sub.add_parser("analyze", help="start or resume the analysis of a pulled song")
    c.add_argument("song")
    c.add_argument("--wait", action="store_true")
    c.add_argument("--extra", action="store_true")
    c.set_defaults(fn=cmd_analyze)
    c = sub.add_parser("follow", help="wait for a running analysis, then fetch and compile it")
    c.add_argument("song")
    c.set_defaults(fn=cmd_follow)
    c = sub.add_parser("fetch", help="bring a finished analysis back and compile it")
    c.add_argument("song")
    c.set_defaults(fn=cmd_fetch)
    c = sub.add_parser("compile", help="build the viewer's file for songs that are already analysed")
    c.add_argument("song", nargs="?")
    c.add_argument("--all", action="store_true")
    c.add_argument("--stale", action="store_true", help="only songs whose inputs or the compiler changed")
    c.add_argument("--jobs", type=int, default=6)
    c.set_defaults(fn=cmd_compile)
    c = sub.add_parser("status", help="one song, or a summary of the library")
    c.add_argument("song", nargs="?")
    c.add_argument("--flags", action="store_true", help="list every song with something to check")
    c.add_argument("--all", action="store_true", help="the whole library, not only electronic songs")
    c.set_defaults(fn=cmd_status)
    c = sub.add_parser("fix", help="a hand correction, kept in its own file and applied by the compiler")
    c.add_argument("song")
    c.add_argument("--grid", choices=("auto", "rekordbox", "trackers"), help="use this grid, whatever the check says; auto goes back to the check")
    c.add_argument("--shift-16ths", type=int, help="move the whole grid by this many 16ths; later is positive, 0 removes it")
    c.add_argument("--bar-shift", type=int, help="start bars this many beats later; 0 removes it")
    c.add_argument("--note")
    c.add_argument("--clear", action="store_true", help="remove the song's corrections")
    c.set_defaults(fn=cmd_fix)
    c = sub.add_parser("review", help="an LLM's flags on a compiled song; costs a few cents")
    c.add_argument("song")
    c.add_argument("--model", default="opus")
    c.set_defaults(fn=cmd_review)
    args = ap.parse_args()
    return args.fn(args)


if __name__ == "__main__":
    sys.exit(main())
