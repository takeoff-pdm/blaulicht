"""Where songs get analysed. A worker runs one song in a thread: analysis, then fetch and compile.

CharlieWorker drives ae-charlie through the same functions as `song.py add` (pipeline/song.py), so
a song looks the same whichever way it was started. MacWorker runs the same steps on this Mac
(mac/mac_song.py) and finishes through the same register-and-compile step.
"""

import contextlib
import io
import json
import os
import shutil
import signal
import subprocess
import sys
import threading
import time
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent.parent / "pipeline"))
import library as lib  # noqa: E402
import song  # noqa: E402

MAC = Path(__file__).resolve().parent / "mac"
COMPILE_LOCK = threading.Lock()  # song.compile_songs keeps module state; one compile at a time


class Retry(Exception):
    """Not the song's fault (connection gone): put it back in the queue."""


class Worker:
    name = "worker"
    enabled = True
    concurrency = 1
    prefer_others = False  # True: only takes a song when no other enabled worker has room

    def available(self) -> tuple[bool, str]:
        return True, ""

    def run(self, job: dict, progress, resume: bool = False) -> None:
        """Blocks until the song is compiled. Raises Retry or another exception on failure."""
        raise NotImplementedError

    def info(self) -> dict:
        ok, why = self.available()
        return {"name": self.name, "enabled": self.enabled, "concurrency": self.concurrency, "available": ok, "reason": why}


def quiet(fn, *args):
    """song.py prints for a terminal and exits on errors; capture both."""
    out = io.StringIO()
    try:
        with contextlib.redirect_stdout(out):
            rc = fn(*args)
    except SystemExit as e:
        code = e.code
        msg = out.getvalue().strip().splitlines()[-1:] or [str(code)]
        if code == 3:
            raise Retry("ae-charlie gateway closed") from None
        raise RuntimeError(code if isinstance(code, str) else msg[0]) from None
    return rc, out.getvalue()


class CharlieWorker(Worker):
    name = "ae-charlie"
    concurrency = 2  # songs at once on ae-charlie; it picks the least busy GPU per step
    _gateway = (0.0, False)

    def available(self):
        t, ok = self._gateway
        if time.time() - t > 10:
            ok = song.gateway_open()
            self._gateway = (time.time(), ok)
        return (True, "") if ok else (False, "gateway closed: run `ssh -fN ssh-proxy-gorilla`")

    def run(self, job, progress, resume=False):
        sid = job["song_id"]
        pjob = song.read_job(sid) or song.write_job({"id": sid, "name": Path(job["listed"]).stem, "listed": job["listed"]}, state="pulled")
        try:
            st = song.remote_status(sid) if resume else None
        except song.GatewayClosed:
            raise Retry("ae-charlie gateway closed") from None
        if not (st and st["state"] in ("queued", "running", "done")):  # else: still running there, follow it
            progress(step="uploading")
            quiet(song.start, pjob, False, self.concurrency)
        seen = None
        while True:
            try:
                st = song.remote_status(sid)
            except song.GatewayClosed:
                raise Retry("ae-charlie gateway closed while analysing (the job keeps running there)") from None
            if st:
                done = sum(v["state"] == "done" for v in st["steps"].values())
                at = (st["state"], st.get("step"), done)
                if at != seen:
                    seen = at
                    progress(step=st.get("step") or st["state"], steps_done=done, steps_total=st.get("total"))
                if st["state"] == "failed":
                    raise RuntimeError(f"failed on ae-charlie at {st.get('step')}: {', '.join(st.get('failed') or [])}")
                if st["state"] == "done":
                    break
            time.sleep(10)
        progress(state="compiling", step="fetch + compile")
        with COMPILE_LOCK:
            rc, out = quiet(song.fetch, pjob)
        if rc:
            raise RuntimeError(out.strip().splitlines()[-1] if out.strip() else "compile failed")
        if not (lib.COMPILED / f"{sid}.json").exists():
            raise RuntimeError("no compiled file after compiling")


class MacWorker(Worker):
    """Local analysis on this Mac: mac/mac_song.py as a child process, one song at a time, with a memory guard.
    Off unless the server runs with --mac-worker. Takes a song only when no other worker has room (see server.py)."""
    name = "mac"
    enabled = False
    concurrency = 1
    prefer_others = True
    procs: set = set()

    def available(self):
        sys.path.insert(0, str(MAC))
        import mac_song
        gaps = mac_song.missing()
        return (False, "missing on this Mac: " + ", ".join(gaps)) if gaps else (True, "")

    def run(self, job, progress, resume=False):
        sid = job["song_id"]
        work = lib.MODEL_DIR / "data/melody/mac_jobs"
        status, viz = work / f"{sid}.json", work / "viz"
        progress(step="starting")
        p = subprocess.Popen([sys.executable, str(MAC / "mac_song.py"), sid, job["listed"], str(status), str(viz)],
                             stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL, start_new_session=True)
        self.procs.add(p)
        seen = None
        try:
            while p.poll() is None:
                time.sleep(2)
                st = read_json(status)
                if st:
                    done = sum(v["state"] == "done" for v in st["steps"].values())
                    step = st.get("step")
                    why = (st["steps"].get(step) or {}).get("why") if step else None
                    at = (step, done, why)
                    if at != seen:
                        seen = at
                        progress(step=f"{step} (waiting: {why})" if why and st["steps"][step]["state"] == "waiting" else step,
                                 steps_done=done, steps_total=st.get("total"))
        except BaseException:
            os.killpg(p.pid, signal.SIGKILL)  # cancelled, or the server is stopping
            raise
        finally:
            self.procs.discard(p)
        st = read_json(status) or {}
        if p.returncode or st.get("state") != "done":
            raise RuntimeError(st.get("error") or f"mac_song.py exited with {p.returncode}")
        progress(state="compiling", step="compile")
        # the results into the pipeline's folders, as `song.py fetch` does for ae-charlie
        root = Path(os.environ.get("MAC_ANALYSIS_HOME", Path.home() / "work/ane-lyrics")) / "root/out/features"
        for f in root.glob(f"*/{sid}.json"):
            dst = lib.FEATURES / f.parent.name / f.name
            dst.parent.mkdir(parents=True, exist_ok=True)
            shutil.copyfile(f, dst)
        shutil.copyfile(viz / f"{sid}.json", lib.DATA / f"{sid}.json")
        pjob = song.read_job(sid) or song.write_job({"id": sid, "name": Path(job["listed"]).stem, "listed": job["listed"]}, state="pulled")
        with COMPILE_LOCK:
            rc, out = quiet(song.register_and_compile, pjob, st.get("failed") or [])
        if rc:
            raise RuntimeError(out.strip().splitlines()[-1] if out.strip() else "compile failed")

    @classmethod
    def shutdown(cls):
        for p in list(cls.procs):
            with contextlib.suppress(ProcessLookupError):
                os.killpg(p.pid, signal.SIGKILL)


def read_json(p: Path) -> dict | None:
    try:
        return json.loads(p.read_text())
    except (OSError, ValueError):
        return None


def all_workers() -> list[Worker]:
    return [CharlieWorker(), MacWorker()]
