"""Sound events at 40 ms resolution: the sharper counterpart to the `events` analyzer.

PretrainedSED (Schmid et al., ICASSP 2025; repo MIT) with its BEATs checkpoint, trained on AudioSet
Strong: 447 classes, 25 frames per second, against about 0.32 s real resolution for PANNs. On the
song without its drums and bass (other + vocals, so kicks don't read as thumps), 16 kHz, in the
10 s pieces the model was trained on, overlapping by half and blended with a triangular weight
(back-to-back pieces made events start and stop on 10 s multiples). Kept: a probability track at
25 fps for each class below, events where a class stays at 0.2 or more for 120 ms or more ([start,
end, class, peak], gaps up to 80 ms bridged), and the song's 15 most present classes.

What it's good for, over 300 songs: crowd sounds, applause, air horns, shouts and laughter, in the
few songs that have them. Whoosh, Boom, Explosion and Zing stay near zero on club music, so risers
and impacts still come from the `risers` analyzer. AudioSet comes from YouTube audio, which is worth
keeping in mind for anything that ships.
"""

import os
import sys

import numpy as np
from scipy.signal import resample_poly

from analyze import common

NAME, VERSION = "sed", 2
GPU = True
SRC = common.ROOT / "models/src/PretrainedSED"
SR, PIECE = 16000, 10
WANT = ["Air horn, truck horn", "Siren", "Whoosh, swoosh, swish", "Explosion", "Boom", "Thump, thud", "Rumble",
        "Cheering", "Applause", "Crowd", "Clapping", "Zing", "White noise, pink noise", "Beep, bleep", "Gunshot, gunfire",
        "Whistle", "Laughter", "Shout", "Singing", "Choir", "Speech"]


def setup():
    import torch

    sys.path.insert(0, str(SRC))
    os.chdir(SRC)  # its checkpoint folder is relative ("resources")
    from data_util import audioset_classes
    from models.beats.BEATs_wrapper import BEATsWrapper
    from models.prediction_wrapper import PredictionsWrapper

    model = PredictionsWrapper(BEATsWrapper(), checkpoint="BEATs_strong_1").eval().cuda()
    names = list(audioset_classes.as_strong_train_classes)
    return torch, model, names, {n: names.index(n) for n in WANT if n in names}


def analyse(song, state):
    torch, model, names, idx = state
    S = common.stems(song)
    x = resample_poly(S["other"] + S["vocals"], 160, 441).astype(np.float32)
    seg, hop = PIECE * SR, PIECE * SR // 2
    y = np.pad(x, (hop, seg))  # every real sample sits in the middle of some piece
    pieces = [y[a: a + seg] for a in range(0, len(y) - seg + 1, hop)]
    scores = []
    with torch.no_grad():
        for i in range(0, len(pieces), 16):
            batch = torch.from_numpy(np.stack(pieces[i: i + 16])).cuda()
            out, _ = model(model.mel_forward(batch))  # (batch, classes, frames)
            scores.append(torch.sigmoid(out).float().cpu().numpy())
    scores = np.concatenate(scores)
    frames = scores.shape[2]
    step, fps = frames // 2, frames / PIECE
    w = 1 - np.abs(np.linspace(-1, 1, frames)) + 1e-3
    total, weight = np.zeros((step * (len(scores) - 1) + frames, scores.shape[1])), np.zeros(step * (len(scores) - 1) + frames)
    for i, s in enumerate(scores):
        total[i * step: i * step + frames] += (s * w).T
        weight[i * step: i * step + frames] += w
    P = (total / weight[:, None])[step: step + int(len(x) / SR * fps)]
    tracks, events = {}, []
    for name, j in idx.items():
        tracks[name] = common.track(P[:, j])
        for a, b in common.runs(P[:, j] >= 0.2, max(1, round(0.12 * fps)), round(0.08 * fps)):
            events.append([round(a / fps, 2), round(b / fps, 2), name, round(float(P[a:b, j].max()), 2)])
    mean = P.mean(0)
    top = [[names[j], round(float(mean[j]), 3)] for j in np.argsort(-mean)[:15]]
    return {"fps": round(fps, 3), "tracks": tracks, "events": sorted(events), "top": top}
