"""Structure from MusicFM embeddings: the same segmentation as `structure` (novelty boundaries,
lettered parts, repetition), but on a learned music representation instead of hand-made features.

MusicFM (Won et al. 2024, minzwon/musicfm; code MIT with one Apache-2.0 file, the MSD checkpoint MIT
on Hugging Face), 25 Hz model on the mix at 24 kHz in 30 s chunks, layer 7 (the layer the authors use
for downstream tasks), averaged to 2 frames per second. Also kept: the song's mean embedding
(normalised) for cross-song similarity.

The viewer shows it and the lights don't use it. On the 175 electronic songs its parts give 12% of
drop pairs the same letter against 26% for the own features, and its nearest neighbours share a
song's style no more often than CLAP's (42% against 41%).
"""

import base64
import sys
import types

import numpy as np
from scipy.signal import resample_poly

from analyze import common
from analyze.structure import segment

NAME, VERSION = "musicfm", 1
GPU = True
SRC = common.ROOT / "models/src/musicfm"
FPS = 2


def setup():
    import torch
    import transformers.integrations.deepspeed as ds

    sys.modules.setdefault("transformers.deepspeed", ds)  # the repo imports the pre-4.40 module path
    sys.path.insert(0, str(SRC.parent))
    from musicfm.model.musicfm_25hz import MusicFM25Hz

    model = MusicFM25Hz(is_flash=False, stat_path=str(SRC / "data/msd_stats.json"), model_path=str(SRC / "data/pretrained_msd.pt"))
    return torch, model.cuda().eval()


def analyse(song, state):
    torch, model = state
    x = resample_poly(common.stems(song)["mix"], 160, 294).astype(np.float32)  # 44.1 -> 24 kHz
    chunk = 30 * 24000
    feats = []
    with torch.no_grad():
        for a in range(0, len(x), chunk):
            seg = x[a: a + chunk]
            if len(seg) < 24000:
                break
            emb = model.get_latent(torch.from_numpy(seg)[None].cuda(), layer_ix=7)  # (1, T, 1024) at 25 Hz
            feats.append(emb[0].float().cpu().numpy())
    E = np.concatenate(feats)
    n = len(E) // 12  # 25 Hz -> about 2 per second
    F = E[: n * 12].reshape(n, 12, -1).mean(1)
    F = (F - F.mean(0)) / (F.std(0) + 1e-6)
    out = segment(F, 25 / 12)
    mean = E.mean(0)
    mean = mean / (np.linalg.norm(mean) + 1e-9)
    out["embedding"] = base64.b64encode(mean.astype(np.float16).tobytes()).decode()
    return out
