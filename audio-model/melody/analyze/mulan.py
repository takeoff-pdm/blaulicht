"""A second text-audio model next to `embed`: MuQ-MuLan (OpenMuQ/MuQ-MuLan-large, about 700M
parameters, 24 kHz), the same shape as `embed.py` so the two compare directly.

On the mix, one 512-d embedding per 10 s window with a 5 s hop (MuLan's own clip length, and CLAP's
windows). Kept: the song's mean embedding (normalised, float16), per window the similarity to CLAP's
style prompts (imported from `embed.py`, so they stay the same) and to a few prompts about the
music's function (`SECTIONS`), and the song's best-matching prompts. A window at index i covers
i * 5 s to i * 5 s + 10 s. Unlike `embed`, it keeps the scale of the tracks (`lo`, `hi`), so the raw
cosines can be recovered.

On the 175 electronic songs, with no ground truth: its song-level prompts agree with Essentia far more
often than CLAP's (style-name prompts, balanced top 1: 37% against 5%, chance 9%). Its neighbours lean
on tempo (within 3 BPM 72% against 55%, pair cosine against BPM difference r 0.61 against 0.23); with
tempo held to 3 BPM they share the top style no more often than CLAP's (44% against 48%). The section
prompts don't work: "drop" follows the rule's drops less well than CLAP on the same prompt (AUC 0.65
against 0.71, loudness 0.85), and the others stay near chance.

Env: `.venv-mulan` needs transformers 4.48.3 (5.22 crashes in the audio tower's conformer) and
torchvision 0.26.0 to match torch 2.11.0. 300 songs in 447 s on one GPU.
"""

import base64
import os

import numpy as np
from scipy.signal import resample_poly

from analyze import common
from analyze.embed import PROMPTS

NAME, VERSION = "mulan", 1
MODEL = "OpenMuQ/MuQ-MuLan-large"
GPU = True
WIN, HOP, SR = 10.0, 5.0, 24000
BATCH = 32
# Chosen on three songs only (Acraze, Spaceman, Charlotte de Witte) from 38 candidates, by how well each
# separates the rule's sections. That choice didn't carry over: "the intro of a song" went from AUC 0.89
# on those three to 0.51 on the other 172
SECTIONS = {
    "intro": "the intro of a song",
    "build": "a build-up before a drop",
    "drop": "a big EDM drop with a pounding kick and heavy bass",
    "breakdown": "a quiet breakdown",
    "groove": "a verse with a steady beat",
    "outro": "This part of the song is the outro.",
}


def setup():
    # /home on ae-charlie is a nearly full NFS, so the 3 GB of weights stay on the local disk
    os.environ.setdefault("HF_HOME", str(common.ROOT / "models/hf-mulan"))
    import torch
    from muq import MuQMuLan

    model = MuQMuLan.from_pretrained(MODEL).to("cuda").eval()
    with torch.no_grad():
        styles = model(texts=list(PROMPTS.values()))
        sections = model(texts=list(SECTIONS.values()))
    return torch, model, styles, sections


def _tracks(sim, names):
    lo, hi = float(sim.min()), float(sim.max())
    return lo, hi, {names[j]: common.track(sim[:, j], lo, hi) for j in range(len(names))}


def analyse(song, state):
    torch, model, styles, sections = state
    x = resample_poly(common.stems(song)["mix"], 160, 294).astype(np.float32)  # 44.1 -> 24 kHz
    win, hop = int(WIN * SR), int(HOP * SR)
    x = np.pad(x, (0, max(0, win - len(x))))
    starts = list(range(0, len(x) - win + 1, hop))
    embs = []
    with torch.no_grad():
        for i in range(0, len(starts), BATCH):
            batch = np.stack([x[s: s + win] for s in starts[i: i + BATCH]])
            embs.append(model.mulan_module.get_audio_latents(torch.from_numpy(batch).cuda()))  # normalised
    E = torch.cat(embs)
    sim = (E @ styles.T).float().cpu().numpy()  # windows x prompts, cosine
    sec = (E @ sections.T).float().cpu().numpy()
    mean = torch.nn.functional.normalize(E.mean(0), dim=-1).float().cpu().numpy()
    names, sec_names = list(PROMPTS), list(SECTIONS)
    song_sim = sim.mean(0)
    order = np.argsort(-song_sim)
    lo, hi, tracks = _tracks(sim, names)
    sec_lo, sec_hi, sec_tracks = _tracks(sec, sec_names)
    return {"fps": 1 / HOP, "window": WIN, "prompts": PROMPTS,
            "song": [[names[j], round(float(song_sim[j]), 3)] for j in order],
            "lo": round(lo, 4), "hi": round(hi, 4), "tracks": tracks,
            "section_prompts": SECTIONS, "section_lo": round(sec_lo, 4), "section_hi": round(sec_hi, 4),
            "section_tracks": sec_tracks,
            "embedding": base64.b64encode(mean.astype(np.float16).tobytes()).decode()}
