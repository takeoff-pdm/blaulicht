"""The drums stem split into its pieces, and the hits of each piece.

MDX23C DrumSep (aufr33 / jarredou, via audio-separator; 6 stems: kick, snare, toms, hh, ride,
crash; kick SDR 14.5, snare 9.8 on ZFTurbo's table) separates the htdemucs_ft drums stem (mono,
fed as two identical channels). Per piece at 100 fps: the hits are the frames where its level
(10 ms RMS, dB) rises within 20 ms by 6 dB (kick, snare), 5 dB (hi-hat), 8 dB (toms, ride) or
10 dB (crash; the cymbal stems carry hi-hat bleed) and comes within 35 dB of the piece's loudest,
at least 80 ms apart (kick), 60 ms (snare, toms, ride), 40 ms (hi-hat) or 0.5 s (crash);
strength is the hit's level in the piece's own range (20th percentile .. 99.5th). Also a 50 fps
level track per piece. The DrumSep weights carry no licence: research use only.
"""

import os
import shutil
import sys
import tempfile
from pathlib import Path

import numpy as np
import soundfile as sf

from analyze import common

NAME, VERSION = "drumparts", 1
GPU = True
MODEL = "MDX23C-DrumSep-aufr33-jarredou.ckpt"
PIECES = ("kick", "snare", "toms", "hh", "ride", "crash")
GAP = {"kick": 8, "snare": 6, "toms": 6, "hh": 4, "ride": 6, "crash": 50}
RISE = {"kick": 6, "snare": 6, "toms": 8, "hh": 5, "ride": 8, "crash": 10}  # dB within 20 ms; the cymbal stems carry hi-hat bleed
MODELS_DIR = common.ROOT / "models/audio-separator"
TMP = common.ROOT / "tmp"


def setup():
    from audio_separator.separator import Separator

    TMP.mkdir(parents=True, exist_ok=True)
    # overlap 2 instead of the default 8: about 4x faster, the hits barely move
    sep = Separator(output_dir=str(TMP), model_file_dir=str(MODELS_DIR), output_format="WAV", log_level=30,
                    mdxc_params={"segment_size": 256, "override_model_segment_size": False, "batch_size": 4, "overlap": 2, "pitch_shift": 0})
    sep.load_model(model_filename=MODEL)
    if os.environ.get("MELODY_COREML"):  # Mac backend: the network in Core ML (ane/backends.py)
        sys.path.insert(0, str(Path(__file__).resolve().parents[1] / "ane"))
        import backends
        backends.drumsep(sep)
    return sep


def _hits(x, gap, min_rise):
    hop = common.SR // 100
    n = len(x) // hop
    rms = np.sqrt((x[: n * hop].reshape(n, hop) ** 2).mean(1) + 1e-12)
    lev = common.db(rms ** 2, -100)
    rise = lev - np.concatenate([[lev[0]] * 2, lev[:-2]])
    lo, hi = np.percentile(lev, 20), np.percentile(lev, 99.5)
    out = []
    for i in common.peaks(rise, min_rise, gap):
        peak = lev[i: i + 3].max()
        if peak > lev.max() - 35:
            out.append([round(i / 100, 3), round(float(np.clip((peak - lo) / max(1.0, hi - lo), 0, 1)), 3)])
    return out, lev


def analyse(song, sep):
    work = Path(tempfile.mkdtemp(dir=TMP))
    try:
        drums = common.stems(song)["drums"]
        src = work / f"{song.id}_drums.wav"
        sf.write(src, np.stack([drums, drums], 1), common.SR, subtype="FLOAT")
        sep.output_dir = str(work)
        if getattr(sep, "model_instance", None) is not None:
            sep.model_instance.output_dir = str(work)
        files = sep.separate(str(src))
        out = {"fps_level": 50, "pieces": {}, "summary": {}}
        for f in files:
            p = Path(f) if Path(f).is_absolute() else work / f
            name = next((k for k in PIECES if f"({k})" in p.name.lower()), None)
            if name is None:
                continue
            x, _ = sf.read(p, dtype="float32", always_2d=True)
            hits, lev = _hits(x.mean(1), GAP[name], RISE[name])
            out["pieces"][name] = {"hits": hits, "level": common.track(lev[::2], -80, 0)}
            out["summary"][name] = len(hits)
        return out
    finally:
        shutil.rmtree(work, ignore_errors=True)
