"""Cleaner vocals: Kim's Mel-Band RoFormer (vocals_mel_band_roformer.ckpt via audio-separator;
Multisong vocals SDR 10.98 against 8.38 for htdemucs_ft) on the stereo mix, decoded with ffmpeg
like the stems so it shares their timeline. Writes the vocals as `stems_vocals/<id>.npy` (float16
mono 44.1 kHz) for the lyrics analyzer, and as features the vocal regions (50 ms frames within 30
dB of the loudest, gaps under 0.5 s bridged, at least 0.3 s) and how much of the song has vocals.
The checkpoint states no licence: research use only.
"""

import shutil
import tempfile
from pathlib import Path

import numpy as np
import soundfile as sf

from analyze import common

NAME, VERSION = "vocals", 1
GPU = True
MODEL = "vocals_mel_band_roformer.ckpt"
OUT = common.ROOT / "stems_vocals"
MODELS_DIR = common.ROOT / "models/audio-separator"
TMP = common.ROOT / "tmp"


def setup():
    from audio_separator.separator import Separator

    TMP.mkdir(parents=True, exist_ok=True)
    OUT.mkdir(parents=True, exist_ok=True)
    sep = Separator(output_dir=str(TMP), model_file_dir=str(MODELS_DIR), output_format="WAV", log_level=30,
                    mdxc_params={"segment_size": 256, "override_model_segment_size": False, "batch_size": 4, "overlap": 2, "pitch_shift": 0})
    sep.load_model(model_filename=MODEL)
    return sep


def regions(v, sr=common.SR):
    hop = sr // 20
    n = len(v) // hop
    lev = common.db((v[: n * hop].reshape(n, hop) ** 2).mean(1))
    on = lev > lev.max() - 30
    return [[round(a / 20, 2), round(b / 20, 2)] for a, b in common.runs(on, 6, 10)], lev


def analyse(song, sep):
    work = Path(tempfile.mkdtemp(dir=TMP))
    try:
        st = common.decode(song.audio)
        src = work / f"{song.id}_mix.wav"
        sf.write(src, st.T, common.SR, subtype="FLOAT")
        sep.output_dir = str(work)
        if getattr(sep, "model_instance", None) is not None:
            sep.model_instance.output_dir = str(work)
        files = sep.separate(str(src))
        voc = next(Path(f) if Path(f).is_absolute() else work / f for f in files if "(vocals)" in Path(f).name.lower())
        x, _ = sf.read(voc, dtype="float32", always_2d=True)
        v = x.mean(1)
        np.save(OUT / f"{song.id}.npy", v.astype(np.float16))
        reg, lev = regions(v)
        dur = len(v) / common.SR
        return {"regions": reg, "share": round(sum(b - a for a, b in reg) / max(dur, 1e-3), 3),
                "level": common.track(lev[::1], -80, 0), "fps_level": 20}
    finally:
        shutil.rmtree(work, ignore_errors=True)
