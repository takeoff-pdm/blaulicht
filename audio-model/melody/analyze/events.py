"""Sound events over time, from an AudioSet tagger: air horns, sirens, whooshes, explosions,
crowds, white noise and the like (and the tagger's view of synths, singing and electronic styles).

PANNs Cnn14_DecisionLevelMax (Kong et al. 2020; code MIT, weights CC BY 4.0, via panns_inference)
on the song without its drums and bass (other + vocals, so kicks don't read as thumps), 32 kHz,
in 30 s chunks. It outputs 527 classes at 100 fps, but its real resolution is about 0.32 s (it
predicts at 1/32 of that rate and upsamples). Kept: a probability track at 10 fps for each class
below, events where a class stays at 0.2 or more for 0.3 s or more ([start, end, peak]), and the
song's 15 most present classes.
"""

import numpy as np
from scipy.signal import resample_poly

from analyze import common

NAME, VERSION = "events", 1
GPU = True
WANT = ["Air horn, truck horn", "Siren", "Civil defense siren", "Whoosh, swoosh, swish", "Explosion", "Boom",
        "Thump, thud", "Rumble", "Cheering", "Applause", "Crowd", "Clapping", "Zing", "White noise", "Pink noise",
        "Beep, bleep", "Gunshot, gunfire", "Whistle", "Whistling", "Laughter", "Speech", "Shout", "Singing", "Choir",
        "Synthesizer", "Drum roll", "Scratching (performance technique)", "Chirp tone", "Sine wave", "Theremin",
        "Electronic music", "Techno", "Dubstep", "House music", "Trance music", "Drum and bass",
        "Electronic dance music", "Ambient music"]
CHUNK = 30 * 32000


def setup():
    from panns_inference import SoundEventDetection, labels

    sed = SoundEventDetection(checkpoint_path=None, device="cuda")
    idx = {name: labels.index(name) for name in WANT if name in labels}
    return sed, idx, labels


def analyse(song, state):
    sed, idx, labels = state
    S = common.stems(song)
    x = resample_poly(S["other"] + S["vocals"], 320, 441).astype(np.float32)
    parts = []
    for a in range(0, len(x), CHUNK):
        seg = x[a: a + CHUNK]
        if len(seg) < 32000:
            seg = np.pad(seg, (0, 32000 - len(seg)))
        fw = sed.inference(seg[None, :])[0]  # (frames, 527) at 100 fps
        parts.append(fw[: int(round(len(x[a: a + CHUNK]) / 320))])
    fw = np.concatenate(parts)
    n10 = len(fw) // 10
    p10 = fw[: n10 * 10].reshape(n10, 10, -1).max(1)
    tracks, events = {}, []
    for name, j in idx.items():
        tracks[name] = common.track(p10[:, j])
        for a, b in common.runs(p10[:, j] >= 0.2, 3, 1):
            events.append([round(a / 10, 1), round(b / 10, 1), name, round(float(p10[a:b, j].max()), 2)])
    mean = fw.mean(0)
    top = [[labels[j], round(float(mean[j]), 3)] for j in np.argsort(-mean)[:15]]
    return {"fps": 10, "tracks": tracks, "events": sorted(events), "top": top}
