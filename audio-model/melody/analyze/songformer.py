"""Song structure with functional labels, from a newer model than All-In-One: SongFormer as it comes.

SongFormer (Hao et al., arXiv 2510.02797; ASLP-lab/SongFormer on Hugging Face). MuQ and MusicFM
embeddings at two time scales (30 s and 420 s windows), then a small transformer with a boundary
head and a label head, on the mix at 24 kHz. Output: segments [[start, end, label]] with the pop
labels intro, verse, pre-chorus, chorus, bridge, inst, silence, outro. Trained on pop like All-In-One, so
it has no drop or build-up label; `edm_finetune/` retrains it on the EDM labels.

Decoded with ffmpeg like the stems, so the times are on the viewer's timeline. The model's own
stitching of 420 s pieces has a bug on songs over 7 minutes (see `_stitching_guard`).

On the 175 electronic songs it stays behind All-In-One: 80% of drops on a boundary against 84% (chance by
sliding the boundaries: 12% and 11%), 55% of boundaries on a bar line against 76%. It has no beat tracker
behind it and runs at 8.33 frames a second: within half a beat 72% are on a bar line, and its boundaries
sit about 50 ms early. Drops mostly come out as chorus (44%) or inst (32%). About 4 s a song, but MusicFM's
420 s window takes about 25 GB of GPU memory, so give it the GPU alone.
"""

import os
import sys

from analyze import common

NAME, VERSION = "songformer", 1
GPU = True
LOCAL = common.ROOT / "models/songformer_hf"
SR = 24000


def _stitching_guard(model):
    """The stitching counts one frame too many on every 420 s seam, so the last frame of a long song
    divides 0 by 0. That NaN averages the last segment's label scores to NaN, and it comes out
    "intro" whatever it is. Drop that frame before the model's own post-processing sees it."""
    forward_globals = sys.modules[type(model).__module__].__dict__
    original = forward_globals["postprocess_functional_structure"]

    def guarded(logits, config):
        import torch

        for key, x in logits.items():
            bad = torch.isnan(x).reshape(x.shape[0], x.shape[1], -1).any(-1)[0]
            if bad.any():
                keep = (~bad).nonzero()[-1].item()
                logits[key] = x[:, : keep + 1]
        return original(logits, config)

    forward_globals["postprocess_functional_structure"] = guarded


def setup():
    # Before transformers is imported: its module and config caches default to the nearly full NFS home.
    os.environ.setdefault("HF_HOME", str(common.ROOT / "models/hf_cache_sf"))
    os.environ["SONGFORMER_LOCAL_DIR"] = str(LOCAL)
    sys.path.append(str(LOCAL))
    from transformers import AutoModel

    model = AutoModel.from_pretrained(str(LOCAL), trust_remote_code=True, low_cpu_mem_usage=False)
    model.to("cuda").eval()
    _stitching_guard(model)
    return model


def analyse(song, model):
    x = common.decode(song.audio, SR, 1)[0]
    duration = len(x) / SR
    segs = []
    for s in model(x):
        a, b = float(s["start"]), min(float(s["end"]), duration)
        if b > a:
            segs.append([round(a, 2), round(b, 2), s["label"]])
    if segs:  # the model's clock runs in whole frames: close the small gap to the end of the song
        segs[-1][1] = round(duration, 2)
    seconds = {}
    for a, b, label in segs:
        seconds[label] = round(seconds.get(label, 0.0) + b - a, 2)
    return {"segments": segs, "summary": {"segments": len(segs), "labels": sorted(seconds), "seconds": seconds}}
