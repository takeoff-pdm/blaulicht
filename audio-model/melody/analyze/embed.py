"""Audio embeddings: what the song sounds like, as vectors, plus zero-shot style prompts.

LAION-CLAP (laion/clap-htsat-unfused, Apache-2.0, via transformers) on the mix at 48 kHz, one
512-d embedding per 10 s window with a 5 s hop. Kept: the song's mean embedding (normalised, for
"which songs sound alike", so a set gets consistent looks across similar tracks), per window the
similarity to a small set of lighting-relevant text prompts (a commercially clean counterpart to
the Essentia tags: dark / euphoric / aggressive / calm ...), and the song's best-matching prompts.
"""

import base64

import numpy as np
from scipy.signal import resample_poly

from analyze import common

NAME, VERSION = "embed", 2
# laion/larger_clap_music's text tower collapses (every prompt embeds at cosine 0.999 to every other
# in transformers 4.57 and 5.22), so zero-shot prompts would be noise; the general checkpoint works
MODEL = "laion/clap-htsat-unfused"
GPU = True
WIN, HOP, SR = 10.0, 5.0, 48000
PROMPTS = {
    "dark": "dark, aggressive, hard techno with a pounding kick",
    "euphoric": "euphoric, uplifting trance anthem with big supersaw chords",
    "happy": "happy, sunny summer house music",
    "melancholic": "emotional, melancholic melodic music",
    "heavy_drop": "a heavy bass drop at a festival",
    "calm": "calm, atmospheric ambient breakdown",
    "vocal": "a catchy vocal pop hook",
    "hardstyle": "hardstyle with distorted kicks",
    "acid": "acid techno with a squelchy 303 bassline",
    "groovy": "groovy, funky disco house",
    "rave": "fast, energetic rave music",
}


def _text(model, t):
    """Text embedding in CLAP's joint space: text tower, pooled, then its projection (spelled out:
    newer transformers return the unprojected pooled output from get_text_features)."""
    return model.text_projection(model.text_model(input_ids=t["input_ids"], attention_mask=t["attention_mask"]).pooler_output)


def _audio(model, inp):
    kw = {k: inp[k] for k in ("input_features", "is_longer") if k in inp}
    return model.audio_projection(model.audio_model(**kw).pooler_output)


def setup():
    import torch
    from transformers import ClapModel, ClapProcessor

    model = ClapModel.from_pretrained(MODEL).to(common.device()).eval()
    proc = ClapProcessor.from_pretrained(MODEL)
    with torch.no_grad():
        t = proc(text=list(PROMPTS.values()), return_tensors="pt", padding=True).to(common.device())
        te = _text(model, t)
        te = torch.nn.functional.normalize(te, dim=-1)
    return torch, model, proc, te


def analyse(song, state):
    torch, model, proc, te = state
    x = resample_poly(common.stems(song)["mix"], 160, 147).astype(np.float32)  # 44.1 -> 48 kHz
    win, hop = int(WIN * SR), int(HOP * SR)
    starts = list(range(0, max(1, len(x) - win + 1), hop))
    embs = []
    with torch.no_grad():
        for i in range(0, len(starts), 16):
            batch = [x[s: s + win] for s in starts[i: i + 16]]
            inp = proc(audio=batch, sampling_rate=SR, return_tensors="pt").to(common.device())
            e = _audio(model, inp)
            embs.append(torch.nn.functional.normalize(e, dim=-1))
    E = torch.cat(embs)
    sim = (E @ te.T).float().cpu().numpy()  # windows x prompts, cosine
    mean = torch.nn.functional.normalize(E.mean(0), dim=-1).float().cpu().numpy()
    names = list(PROMPTS)
    song_sim = sim.mean(0)
    order = np.argsort(-song_sim)
    lo, hi = float(sim.min()), float(sim.max())
    return {"fps": 1 / HOP, "window": WIN, "prompts": PROMPTS,
            "song": [[names[j], round(float(song_sim[j]), 3)] for j in order],
            "tracks": {names[j]: common.track(sim[:, j], lo, hi) for j in range(len(names))},
            "embedding": base64.b64encode(mean.astype(np.float16).tobytes()).decode()}
