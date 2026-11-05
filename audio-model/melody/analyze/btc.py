"""Large-vocabulary chords: 170 classes (maj, min, 7, maj7, min7, dim, aug, sus, ... + N, X).

BTC, the Bi-directional Transformer for Chord recognition (Park et al., ISMIR 2019; jayg996/
BTC-ISMIR19, code and weights MIT), large-vocabulary checkpoint, on the song without its drums
(bass + other + vocals), 22.05 kHz, log CQT with the repo's settings (frames of ~93 ms), normalised
with the checkpoint's mean and std. The model sees 108-frame (10 s) windows; scoring them back to
back put chord changes on every 10 s multiple, so the CQT runs over the whole song and the windows
overlap by half, blended with a triangular weight. Output chords [[start, end, label]] ("C:maj7",
"A:min", "N") and how many distinct chords. On sparse club drops it often says N where the bass
clearly holds a root, so the compiler treats N as "no opinion".
"""

import sys

import librosa
import numpy as np
from scipy.signal import resample_poly

from analyze import common

NAME, VERSION = "btc", 2
GPU = True
SRC = common.ROOT / "models/src/BTC-ISMIR19"


def setup():
    import functools

    import torch
    import yaml

    # the repo calls yaml.load(f) without a Loader, which current PyYAML refuses; its config is plain YAML
    yaml.load = functools.partial(yaml.load, Loader=yaml.SafeLoader)
    np.float, np.int, np.bool = float, int, bool  # and it uses the aliases NumPy removed
    sys.path.insert(0, str(SRC))
    from btc_model import BTC_model
    from utils.hparams import HParams
    from utils.mir_eval_modules import idx2voca_chord

    config = HParams.load(str(SRC / "run_config.yaml"))
    config.feature["large_voca"] = True
    config.model["num_chords"] = 170
    model = BTC_model(config=config.model).cuda().eval()
    ck = torch.load(SRC / "test/btc_model_large_voca.pt", map_location="cuda", weights_only=False)
    model.load_state_dict(ck["model"])
    return torch, model, config, ck["mean"], ck["std"], idx2voca_chord()


def analyse(song, state):
    torch, model, config, mean, std, idx2chord = state
    S = common.stems(song)
    sr = config.mp3["song_hz"]
    x = resample_poly(S["mix"] - S["drums"], 1, 2).astype(np.float32)  # 44.1 -> 22.05 kHz
    hop_length = config.feature["hop_length"]
    feat = librosa.cqt(x, sr=sr, n_bins=config.feature["n_bins"], bins_per_octave=config.feature["bins_per_octave"], hop_length=hop_length)
    feat = (np.log(np.abs(feat) + 1e-6).T - mean) / std
    unit = hop_length / sr
    T = config.model["timestep"]
    hop = T // 2
    n = len(feat)
    f = torch.tensor(np.pad(feat, ((hop, T), (0, 0))), dtype=torch.float32).cuda()
    w = (1 - torch.abs(torch.linspace(-1, 1, T)) + 1e-3).cuda()[:, None]
    score = torch.zeros(len(f), config.model["num_chords"]).cuda()
    with torch.no_grad():
        for a in range(0, len(f) - T + 1, hop):
            out, _ = model.self_attn_layers(f[a: a + T][None])
            score[a: a + T] += w * torch.softmax(model.output_layer.output_projection(out)[0], -1)
    preds = score[hop: hop + n].argmax(1).tolist()
    chords, start = [], 0
    for i in range(1, len(preds) + 1):
        if i == len(preds) or preds[i] != preds[start]:
            chords.append([round(start * unit, 2), round(i * unit, 2), idx2chord[preds[start]]])
            start = i
    return {"chords": chords, "summary": {"chords": len(chords), "distinct": len({c[2] for c in chords})}}
