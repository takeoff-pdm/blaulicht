"""Shared bits of the offline analyzers for the show compiler (they run on ae-charlie).

Every analyzer is a module in this package with
    NAME, VERSION        its output folder and version (bump VERSION to recompute)
    GPU = False          True: one process per GPU, the model loaded once (setup)
    setup() -> state     optional; loads models
    analyse(song, state) -> dict   the song's features, JSON-serialisable
and writes `out/features/<NAME>/<id>.json` with {"analyzer", "version", "id", ...}. `run.py`
walks the songs and skips the ones already analysed at the current VERSION, so a new playlist only
costs its own songs.

Songs are the ones with stems (`stems_songs/<id>.npy`: float16 (4, n) mono 44.1 kHz, rows mix,
drums, bass, vocals; "other" = mix - drums - bass - vocals), listed in `out/melodia/index.json`.
The id is the viewer's song id (sha1 of the absolute audio path on ae-charlie). Times in the
features are seconds of the decoded audio, the same timeline as the viewer.
"""

import base64
import hashlib
import json
import os
import subprocess
from dataclasses import dataclass
from pathlib import Path

import numpy as np

ROOT = Path(os.environ.get("MELODY_ROOT", "/localhome/sokrates/melody"))
SR = 44100
FEATURES = ROOT / "out/features"
STEM_NAMES = ("mix", "drums", "bass", "vocals", "other")


@dataclass
class Song:
    id: str  # stem id = viewer song id
    name: str
    audio: Path  # the audio file on ae-charlie
    listed: str  # its Rekordbox path on the Mac


def songs() -> list[Song]:
    index = json.loads((ROOT / "out/melodia/index.json").read_text())
    out = []
    for v in index.values():
        audio = ROOT / v["path"]
        gid = hashlib.sha1(str(audio).encode()).hexdigest()[:12]
        if (ROOT / "stems_songs" / f"{gid}.npy").exists():
            out.append(Song(gid, audio.stem, audio, v["listed"]))
    return sorted(out, key=lambda s: s.name.lower())


def stems(song: Song) -> dict[str, np.ndarray]:
    """Mono float32 stems at 44.1 kHz: mix, drums, bass, vocals, other."""
    a = np.load(ROOT / "stems_songs" / f"{song.id}.npy", mmap_mode="r")
    mix, drums, bass, vocals = (np.asarray(a[i], np.float32) for i in range(4))
    return {"mix": mix, "drums": drums, "bass": bass, "vocals": vocals, "other": mix - drums - bass - vocals}


def decode(path: Path, sr: int = SR, channels: int = 2) -> np.ndarray:
    """(channels, n) float32, decoded with ffmpeg like the stems (same timeline)."""
    pcm = subprocess.run(["ffmpeg", "-v", "error", "-i", str(path), "-ac", str(channels), "-ar", str(sr), "-f", "f32le", "-"],
                         check=True, capture_output=True).stdout
    return np.frombuffer(pcm, np.float32).reshape(-1, channels).T.copy()


def frames(x: np.ndarray, n_fft: int, hop: int) -> np.ndarray:
    """(n_frames, n_fft) Hann-windowed frames, frame i centred on sample i * hop."""
    x = np.pad(x, (n_fft // 2, n_fft // 2 + hop))
    n = (len(x) - n_fft) // hop + 1
    idx = np.lib.stride_tricks.sliding_window_view(x, n_fft)[::hop][:n]
    return idx * np.hanning(n_fft).astype(np.float32)


def stft(x: np.ndarray, n_fft: int, hop: int) -> np.ndarray:
    return np.fft.rfft(frames(x, n_fft, hop), axis=1).astype(np.complex64)


def db(x, floor=-100.0):
    return np.maximum(10 * np.log10(np.maximum(x, 1e-12)), floor)


def smooth(x: np.ndarray, n: int) -> np.ndarray:
    """Centred moving average over n frames."""
    if n <= 1:
        return x
    k = np.ones(n) / n
    return np.convolve(np.pad(x, (n // 2, n - 1 - n // 2), mode="edge"), k, mode="valid")


def peaks(x: np.ndarray, threshold: float, min_gap: int) -> list[int]:
    """Local maxima above threshold, at least min_gap frames apart (the larger one wins)."""
    cand = [i for i in range(1, len(x) - 1) if x[i] >= threshold and x[i] >= x[i - 1] and x[i] > x[i + 1]]
    cand.sort(key=lambda i: -x[i])
    taken, out = np.zeros(len(x), bool), []
    for i in cand:
        if not taken[max(0, i - min_gap): i + min_gap + 1].any():
            out.append(i)
            taken[i] = True
    return sorted(out)


def runs(mask: np.ndarray, min_len: int, max_gap: int = 0) -> list[tuple[int, int]]:
    """[start, end) runs of True of at least min_len frames, bridging gaps up to max_gap."""
    out, i, n = [], 0, len(mask)
    while i < n:
        if not mask[i]:
            i += 1
            continue
        j = i
        while j < n:
            if mask[j]:
                j += 1
                continue
            k = j
            while k < n and not mask[k] and k - j < max_gap + 1:
                k += 1
            if k < n and mask[k] and k - j <= max_gap:
                j = k
            else:
                break
        if j - i >= min_len:
            out.append((i, j))
        i = j + 1
    return out


def track(x: np.ndarray, lo: float = 0.0, hi: float = 1.0) -> str:
    """A frame track as base64 uint8 (lo..hi mapped to 0..255)."""
    q = np.clip((np.asarray(x, np.float64) - lo) / (hi - lo) * 255, 0, 255).round().astype(np.uint8)
    return base64.b64encode(q.tobytes()).decode()


def out_path(name: str, song: Song) -> Path:
    return FEATURES / name / f"{song.id}.json"


def done(name: str, version: int, song: Song) -> bool:
    p = out_path(name, song)
    if not p.exists():
        return False
    try:
        return json.loads(p.read_text()).get("version") == version
    except (OSError, json.JSONDecodeError):
        return False


def save(name: str, version: int, song: Song, doc: dict) -> None:
    p = out_path(name, song)
    p.parent.mkdir(parents=True, exist_ok=True)
    tmp = p.with_suffix(".tmp")
    tmp.write_text(json.dumps({"analyzer": name, "version": version, "id": song.id, "name": song.name, **doc},
                              separators=(",", ":"), ensure_ascii=False))
    tmp.replace(p)
