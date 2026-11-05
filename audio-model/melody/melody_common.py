"""Shared paths, splits and MIREX-style melody metrics (no torch, so the Essentia venv can use it)."""

from pathlib import Path

import numpy as np

ROOT = Path("/localhome/sokrates/melody")
TEACHER = Path("/localhome/sokrates/stems/data/teacher")
SR, HOP = 44100, 441


def is_test_song(sid: str) -> bool:
    """Held-out Rekordbox songs: every song whose id (sha1 of its path, 12 hex digits) % 6 == 0."""
    return int(sid, 16) % 6 == 0


def metrics(ref: np.ndarray, est: np.ndarray) -> dict:
    """MIREX-style frame metrics on a common grid; both in Hz, 0 = unvoiced."""
    n = min(len(ref), len(est))
    ref, est = ref[:n], est[:n]
    rv, ev = ref > 0, est > 0
    cents = np.zeros(n)
    both = rv & ev
    cents[both] = 1200 * np.abs(np.log2(est[both] / ref[both]))
    chroma = np.abs((cents + 600) % 1200 - 600)
    good = both & (cents <= 50)
    good_c = both & (chroma <= 50)
    return {
        "frames": n, "ref_voiced": int(rv.sum()), "ref_unvoiced": int((~rv).sum()),
        "pitch_ok": int(good.sum()), "chroma_ok": int(good_c.sum()),
        "est_voiced_on_voiced": int(both.sum()), "est_voiced_on_unvoiced": int((ev & ~rv).sum()),
        "oa_ok": int(good.sum() + (~rv & ~ev).sum()),
    }


def summarize(ms: list[dict]) -> dict:
    s = {k: sum(m[k] for m in ms) for k in ms[0]}
    return {
        "OA": s["oa_ok"] / s["frames"], "RPA": s["pitch_ok"] / max(s["ref_voiced"], 1),
        "RCA": s["chroma_ok"] / max(s["ref_voiced"], 1), "VR": s["est_voiced_on_voiced"] / max(s["ref_voiced"], 1),
        "VFA": s["est_voiced_on_unvoiced"] / max(s["ref_unvoiced"], 1),
    }
