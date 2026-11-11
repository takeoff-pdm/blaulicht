"""Decode audio and run it through the Rust mel front end.

The live engine computes features with `blaulicht-audio-features`; training
pipes PCM through the same code via its `audio-features` binary, so both see
identical input. Augmentation happens on PCM before `extract`.
"""

import functools
import json
import subprocess
from pathlib import Path

import numpy as np

REPO = Path(__file__).resolve().parent.parent
SAMPLE_RATE = 44_100


@functools.cache
def binary() -> str:
    """Build the front end (a no-op when current) and return its path."""
    out = subprocess.run(
        [
            "cargo", "build", "--release", "-q",
            "-p", "blaulicht-audio-features",
            "--message-format=json",
        ],
        cwd=REPO,
        check=True,
        capture_output=True,
        text=True,
    ).stdout
    for line in out.splitlines():
        msg = json.loads(line)
        if msg.get("executable"):
            return msg["executable"]
    raise RuntimeError("cargo did not report an audio-features executable")


@functools.cache
def info(sample_rate: int = SAMPLE_RATE) -> dict:
    out = subprocess.run(
        [binary(), "--sample-rate", str(sample_rate), "--info"],
        check=True, capture_output=True, text=True,
    ).stdout
    return json.loads(out)


def decode(path: str, sample_rate: int = SAMPLE_RATE) -> np.ndarray:
    """Mono mix at `sample_rate`. ffmpeg trims the MP3 encoder delay, which
    matches Rekordbox's grid timing (checked by check_alignment.py)."""
    pcm = subprocess.run(
        ["ffmpeg", "-v", "error", "-i", path, "-ac", "1", "-ar", str(sample_rate),
         "-f", "f32le", "-"],
        check=True, capture_output=True,
    ).stdout
    return np.frombuffer(pcm, np.float32)


def extract(pcm: np.ndarray, sample_rate: int = SAMPLE_RATE) -> np.ndarray:
    """(frames, n_mels) float32; frame k covers audio up to (k + 1) / frame_rate s."""
    out = subprocess.run(
        [binary(), "--sample-rate", str(sample_rate)],
        input=np.ascontiguousarray(pcm, np.float32).tobytes(),
        check=True, capture_output=True,
    ).stdout
    return np.frombuffer(out, np.float32).reshape(-1, info(sample_rate)["n_mels"])


def beat_activations(
    pcm: np.ndarray, model: Path, sample_rate: int = SAMPLE_RATE
) -> np.ndarray:
    """(2, frames) beat and downbeat probabilities from the Rust front end and
    Rust `BeatModel`, exactly as the engine would compute them."""
    out = subprocess.run(
        [binary(), "--sample-rate", str(sample_rate), "--model", str(model)],
        input=np.ascontiguousarray(pcm, np.float32).tobytes(),
        check=True, capture_output=True,
    ).stdout
    return np.frombuffer(out, np.float32).reshape(-1, 2).T


def targets(
    beats_s: list[float], beat_in_bar: list[int], n_frames: int, frame_rate: int = 100
) -> tuple[np.ndarray, np.ndarray]:
    """Beat and downbeat targets: 1 in the first frame where each beat is
    audible, `floor(t * frame_rate)`."""
    beats = np.asarray(beats_s)
    idx = np.floor(beats * frame_rate).astype(int)
    keep = (idx >= 0) & (idx < n_frames)
    beat = np.zeros(n_frames, np.uint8)
    downbeat = np.zeros(n_frames, np.uint8)
    beat[idx[keep]] = 1
    downbeat[idx[keep & (np.asarray(beat_in_bar) == 1)]] = 1
    return beat, downbeat
