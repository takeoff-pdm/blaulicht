#!/usr/bin/env python3
"""One song through the analysis on ae-charlie: stems, melody, the analyzers, the viewer's data. Started by song.py.

    nohup python3 code/pipeline/remote_song.py "/Users/mik/Music/x.mp3" [--extra] > logs/songs/<id>.log 2>&1 &

The audio must already be in audio/<that path>. Writes out/songs/<id>.json after every step, which is what song.py reads.
Songs run one at a time, so later ones wait as "queued". A step that's already done is skipped, so running this
again resumes it. Stems, Melodia and the viewer's data are required; a failed analyzer is noted and the rest go on, since
every analyzer is optional for the compiler. Standard library only.
"""

import fcntl
import hashlib
import json
import os
import re
import subprocess
import sys
import time
from pathlib import Path

ROOT = Path("/localhome/sokrates/melody")
LISTS = ROOT / "songs_all.txt"
# analyzer -> (venv, needs a GPU). In run order: `lyrics` reads what `vocals` writes.
CORE = {"stereo": ("ana", 0), "timbre": ("ana", 0), "risers": ("ana", 0), "chords": ("ana", 0), "structure": ("ana", 0),
        "beats": ("ml", 1), "allinone": ("aio", 1), "drumparts": ("sep", 1), "adtof": ("ml", 1), "events": ("ml", 1), "sed": ("sed", 1),
        "fxhead": ("fx", 1), "tags": ("esstf", 0), "transcribe": ("bp", 0), "vocals": ("sep", 1), "lyrics": ("asr", 1), "embed": ("ml", 1)}
EXTRA = {"btc": ("ml", 1), "musicfm": ("ml", 1), "mulan": ("mulan", 1), "mt3": ("mt3", 1)}  # viewer lanes and small extras


def free_gpu() -> str:
    """The GPU with the least memory in use, read fresh per step so a job that started meanwhile is left alone."""
    out = subprocess.run(["nvidia-smi", "--query-gpu=index,memory.used", "--format=csv,noheader,nounits"], capture_output=True, text=True, check=True).stdout
    rows = [tuple(int(x) for x in line.split(",")) for line in out.strip().splitlines()]
    return str(min(rows, key=lambda r: r[1])[0])


class Job:
    def __init__(self, listed: str):
        self.listed, self.audio = listed, ROOT / ("audio" + listed)
        self.id = hashlib.sha1(str(self.audio).encode()).hexdigest()[:12]
        self.melodia_id = hashlib.sha1(("audio" + listed).encode()).hexdigest()[:12]
        self.logs = ROOT / "logs/songs" / self.id
        self.logs.mkdir(parents=True, exist_ok=True)
        self.file = ROOT / "out/songs" / f"{self.id}.json"
        self.file.parent.mkdir(parents=True, exist_ok=True)
        self.status = {"id": self.id, "listed": listed, "state": "queued", "step": None, "steps": {}, "started": time.time(), "failed": []}
        self.save()

    def save(self) -> None:
        tmp = self.file.with_suffix(".tmp")
        tmp.write_text(json.dumps(self.status, ensure_ascii=False, indent=1))
        os.replace(tmp, self.file)

    def step(self, name: str, cmd: list[str], cwd: Path, done: bool = False, gpu: bool = False, env: dict | None = None, required: bool = False) -> bool:
        if done:
            self.status["steps"][name] = {"state": "done", "skipped": True}
            self.save()
            return True
        e = {**os.environ, **(env or {}), "CUDA_DEVICE_ORDER": "PCI_BUS_ID", "CUDA_VISIBLE_DEVICES": free_gpu() if gpu else ""}
        self.status["step"] = name
        self.status["steps"][name] = {"state": "running", "gpu": e["CUDA_VISIBLE_DEVICES"]}
        self.save()
        t0 = time.time()
        with (self.logs / f"{name}.log").open("w") as log:
            rc = subprocess.run(cmd, cwd=cwd, env=e, stdout=log, stderr=subprocess.STDOUT).returncode
        ok = rc == 0
        self.status["steps"][name].update(state="done" if ok else "failed", seconds=round(time.time() - t0, 1))
        if not ok:
            self.status["failed"].append(name)
            if required:
                self.status["state"] = "failed"
        self.save()
        return ok or not required


def analyzer_done(name: str, sid: str) -> bool:
    """Output there, at the analyzer's current version. run.py knows this too, but only after loading the venv."""
    out = ROOT / "out/features" / name / f"{sid}.json"
    if not out.exists():
        return False
    m = re.search(r'^NAME, VERSION = "[^"]+", (\d+)', (ROOT / "code/analyze" / f"{name}.py").read_text(), re.M)
    return bool(m) and json.loads(out.read_text()).get("version") == int(m.group(1))


def main() -> int:
    listed = sys.argv[1]
    job = Job(listed)
    if not job.audio.exists():
        job.status.update(state="failed", error=f"no audio at {job.audio}")
        job.save()
        return 1
    lock = (ROOT / "tmp/run_song.lock").open("w")
    fcntl.flock(lock, fcntl.LOCK_EX)  # one song at a time, the rest wait here as "queued"
    job.status.update(state="running", running_since=time.time())
    job.save()
    with LISTS.open("a+") as f:  # melodia_batch.py rewrites its index from this list
        f.seek(0)
        if listed not in f.read().splitlines():
            f.write(listed + "\n")
    sid, code = job.id, ROOT / "code"
    steps = [("stems", ["/localhome/sokrates/stems/.venv/bin/python", "code/teacher.py", "--list", str(job.logs / "song.txt"), "--out", "stems_songs", "--model", "htdemucs_ft"],
              ROOT, (ROOT / f"stems_songs/{sid}.npy").exists(), True, True),
             ("melodia", [".venv-ess/bin/python", "code/melodia_batch.py", str(LISTS), "out/melodia", "--root", "audio", "--jobs", "4"],
              ROOT, (ROOT / f"out/melodia/{job.melodia_id}.mid").exists() and job.melodia_id in json.loads((ROOT / "out/melodia/index.json").read_text()), False, True),
             ("game", [".venv-game/bin/python", "code/game_batch.py", "stems_songs", "out/game", "--model", "models/game-large/GAME-1.0.3-large-onnx"],
              ROOT, (ROOT / f"out/game/{sid}.mid").exists(), True, False),
             ("lead_bass", ["../.venv-ess/bin/python", "melodia_stems.py", "../stems_songs", "../out", "--jobs", "4"],
              code, (ROOT / f"out/lead/{sid}.mid").exists() and (ROOT / f"out/bass/{sid}.mid").exists(), False, False)]
    (job.logs / "song.txt").write_text(str(job.audio) + "\n")
    analyzers = {**CORE, **(EXTRA if "--extra" in sys.argv else {})}
    job.status["total"] = len(steps) + len(analyzers) + 1
    for name, cmd, cwd, done, gpu, required in steps:
        if not job.step(name, cmd, cwd, done=done, gpu=gpu, required=required):
            return 1
    for name, (venv, gpu) in analyzers.items():
        env = None
        if name == "lyrics":  # faster-whisper finds CUDA's libraries inside its venv
            nv = ROOT / ".venv-asr/lib/python3.12/site-packages/nvidia"
            env = {"LD_LIBRARY_PATH": f"{nv}/cublas/lib:{nv}/cudnn/lib:" + os.environ.get("LD_LIBRARY_PATH", "")}
        job.step(name, [f"../.venv-{venv}/bin/python", "-m", "analyze.run", name, "--only", sid], code, done=analyzer_done(name, sid), gpu=bool(gpu), env=env)
    if not job.step("viewer_data", [".venv-ess/bin/python", "code/pipeline/build_viz_data.py", str(ROOT / "viz"), "--only", sid], ROOT, required=True):
        return 1
    job.status.update(state="done", step=None, finished=time.time())
    job.save()
    return 0


if __name__ == "__main__":
    sys.exit(main())
