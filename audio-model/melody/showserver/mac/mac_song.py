#!/usr/bin/env python3
"""One song through the whole analysis on this Mac, the steps one process at a time. Started by the show server.

    python3 mac_song.py <song id> <audio> <status.json> <viz out dir>

Mirrors remote_song.py: stems, melodia, game, lead/bass, the analyzers, the viewer data; required steps stop
the song, a failed analyzer is noted and the rest go on. A step that's done already is skipped, so running it
again resumes. Writes <status.json> after every step in remote_song.py's format.

Memory guard for the 16 GB Mac: a step starts only with >= 35% memory free and load < 8; it's killed when free
memory drops under 12% or swap grows by more than 3 GB during the step, which fails the song. Standard library only.
"""

import json
import os
import re
import signal
import subprocess
import sys
import time
from pathlib import Path

HERE = Path(__file__).resolve().parent
MELODY = HERE.parents[1]
BASE = Path(os.environ.get("MAC_ANALYSIS_HOME", Path.home() / "work/ane-lyrics"))  # venvs, models, root/
ROOT = BASE / "root"
STEPS = HERE / "mac_steps.py"
VENV = {"sep": BASE / "venv-sep/bin/python", "asr": BASE / "venv/bin/python", "ana": BASE / "venv-ana/bin/python"}
ENV = {"MELODY_ROOT": str(ROOT), "MELODY_COREML": str(BASE / "models"), "DEMUCS_SHIFTS": "1",
       "GAME_MODEL": str(BASE / "models/game/GAME-1.0.3-large-onnx"), "PYTORCH_ENABLE_MPS_FALLBACK": "1", "MELODY_DEVICE": "mps"}
LYRICS_ENV = {"HF_HUB_OFFLINE": "1", "HF_HOME": str(BASE / "hf"), "MELODY_WHISPER_BACKEND": "coreml2",
              "LYRICS_COREML_MODEL": str(BASE / "models/v2")}
# analyzer -> venv, in remote_song.py's order (lyrics reads what vocals writes)
ANALYZERS = {"stereo": "ana", "timbre": "ana", "risers": "ana", "chords": "ana", "structure": "ana", "beats": "ana",
             "allinone": "ana", "drumparts": "sep", "adtof": "ana", "events": "ana", "sed": "ana", "fxhead": "ana",
             "tags": "ana", "transcribe": "ana", "vocals": "sep", "lyrics": "asr", "embed": "ana"}
_e = lambda k, d: float(os.environ.get(k, d))
MIN_FREE_START, MIN_FREE_RUN = _e("MAC_MIN_FREE_START", 35), _e("MAC_MIN_FREE_RUN", 12)
MAX_SWAP_GROWTH_MB, MAX_LOAD = _e("MAC_MAX_SWAP_GROWTH_MB", 3072), _e("MAC_MAX_LOAD", 8)  # swap growth during one step


def missing() -> list[str]:
    """What this Mac lacks to run the pipeline (empty = ready)."""
    need = [*VENV.values(), ROOT, BASE / "models/htdemucs", Path(ENV["GAME_MODEL"]), BASE / "models/v2"]
    return [str(p) for p in need if not p.exists()]


def free_pct() -> int:
    out = subprocess.run(["memory_pressure"], capture_output=True, text=True).stdout
    m = re.search(r"free percentage:\s*(\d+)%", out)
    return int(m.group(1)) if m else 100


def swap_mb() -> float:
    out = subprocess.run(["sysctl", "-n", "vm.swapusage"], capture_output=True, text=True).stdout
    m = re.search(r"used = ([\d.]+)M", out)
    return float(m.group(1)) if m else 0.0


def load1() -> float:
    return os.getloadavg()[0]


class Song:
    def __init__(self, sid: str, audio: Path, status: Path, viz: Path):
        self.sid, self.audio, self.file, self.viz = sid, audio, status, viz
        self.logs = status.parent / f"{sid}.logs"
        self.logs.mkdir(parents=True, exist_ok=True)
        self.status = {"id": sid, "state": "running", "step": None, "steps": {}, "started": time.time(), "failed": [], "total": 5 + len(ANALYZERS)}
        self.save()

    def save(self):
        tmp = self.file.with_suffix(".tmp")
        tmp.write_text(json.dumps(self.status, indent=1))
        os.replace(tmp, self.file)

    def step(self, name: str, venv: str, args: list[str], done: bool, required: bool, env: dict | None = None) -> bool:
        if done:
            self.status["steps"][name] = {"state": "done", "skipped": True}
            self.save()
            return True
        self.status["step"] = name
        while free_pct() < MIN_FREE_START or load1() >= MAX_LOAD:
            self.status["steps"][name] = {"state": "waiting", "why": f"free {free_pct()}%, load {load1():.0f}"}
            self.save()
            time.sleep(10)
        self.status["steps"][name] = {"state": "running"}
        self.save()
        t0 = time.time()
        with (self.logs / f"{name}.log").open("w") as log:
            p = subprocess.Popen([str(VENV[venv]), str(STEPS), *args], env={**os.environ, **ENV, **(env or {})},
                                 stdout=log, stderr=subprocess.STDOUT, start_new_session=True)
            killed, swap0 = None, swap_mb()
            while p.poll() is None:
                if free_pct() < MIN_FREE_RUN or swap_mb() - swap0 > MAX_SWAP_GROWTH_MB:
                    killed = f"killed: free {free_pct()}%, swap grew {swap_mb() - swap0:.0f} MB"
                    os.killpg(p.pid, signal.SIGKILL)
                    p.wait()
                    break
                time.sleep(2)
        ok = p.returncode == 0 and not killed
        self.status["steps"][name] = {"state": "done" if ok else "failed", "seconds": round(time.time() - t0, 1), **({"why": killed} if killed else {})}
        if not ok:
            self.status["failed"].append(name)
            if required or killed:  # a kill means the Mac is short of memory: stop the song, don't push on
                self.status.update(state="failed", error=killed or f"{name} failed, see {self.logs / (name + '.log')}")
        self.save()
        return ok or not (required or killed)


def analyzer_done(name: str, sid: str) -> bool:
    out = ROOT / "out/features" / name / f"{sid}.json"
    if not out.exists():
        return False
    m = re.search(r'^NAME, VERSION = "[^"]+", (\d+)', (MELODY / "analyze" / f"{name}.py").read_text(), re.M)
    try:
        return bool(m) and json.loads(out.read_text()).get("version") == int(m.group(1))
    except ValueError:
        return False


def main() -> int:
    sid, audio, status, viz = sys.argv[1], Path(sys.argv[2]), Path(sys.argv[3]), Path(sys.argv[4])
    status.parent.mkdir(parents=True, exist_ok=True)
    s = Song(sid, audio, status, viz)
    if gaps := missing():
        s.status.update(state="failed", error="missing on this Mac: " + ", ".join(gaps))
        s.save()
        return 1
    os.nice(5)
    a = [sid, str(audio)]
    o = ROOT / "out"
    if not (s.step("stems", "sep", ["stems", *a], (ROOT / f"stems_songs/{sid}.npy").exists(), True)
            and s.step("melodia", "ana", ["melodia", *a], (o / f"melodia/{sid}.npz").exists(), True)
            and s.step("game", "sep", ["game", *a], (o / f"game/{sid}.mid").exists(), False)
            and s.step("lead_bass", "ana", ["lead_bass", *a], (o / f"lead/{sid}.mid").exists() and (o / f"bass/{sid}.mid").exists(), False)):
        return 1
    for name, venv in ANALYZERS.items():
        if not s.step(name, venv, ["analyzer", *a, name], analyzer_done(name, sid), False, LYRICS_ENV if name == "lyrics" else None):
            return 1
    if not s.step("viewer_data", "ana", ["viewer_data", *a, str(viz)], False, True):
        return 1
    s.status.update(state="done", step=None, finished=time.time())
    s.save()
    return 0


if __name__ == "__main__":
    sys.exit(main())
