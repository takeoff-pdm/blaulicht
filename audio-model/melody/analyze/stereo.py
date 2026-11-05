"""Where each instrument sits in the stereo field over time: pan (-1 left .. +1 right) and width.

The stems are mono, so the stereo mix is split by them: per STFT bin, each stem's share of the
energy (a Wiener-style mask from the stem magnitudes) weights the left and right channels of the
decoded stereo mix. Per stem and frame, pan = (R - L) / (R + L) of its masked energy and width =
side / mid energy (0 = mono, about 1 = fully wide), at 10 frames per second. Silent frames get
pan 0 and width 0. Lights can put a left-panned synth on the left side of the room and spread
wide pads over the room.
"""

import numpy as np

from analyze import common

NAME, VERSION = "stereo", 1
FPS, N_FFT = 10, 4096
HOP = common.SR // FPS
PARTS = ("drums", "bass", "vocals", "other")


def analyse(song, state=None):
    st = common.decode(song.audio)
    S = common.stems(song)
    n = min(st.shape[1], len(S["mix"]))
    L, R = (common.stft(st[c, :n], N_FFT, HOP) for c in (0, 1))
    mags = {k: np.abs(common.stft(S[k][:n], N_FFT, HOP)) ** 2 for k in PARTS}
    total = sum(mags.values()) + 1e-12
    pl, pr = np.abs(L) ** 2, np.abs(R) ** 2
    mid, side = np.abs(L + R) ** 2 / 4, np.abs(L - R) ** 2 / 4
    out = {"fps": FPS, "frames": len(pl), "pan": {}, "width": {}, "summary": {}}
    for k in PARTS:
        m = mags[k] / total
        el, er = (m * pl).sum(1), (m * pr).sum(1)
        em, es = (m * mid).sum(1), (m * side).sum(1)
        level = common.db(el + er)
        active = level > level.max() - 45
        pan = np.where(active, (er - el) / (er + el + 1e-12), 0.0)
        width = np.where(active, np.clip(es / (em + 1e-12), 0, 1.5) / 1.5, 0.0)
        out["pan"][k] = common.track(common.smooth(pan, 3), -1, 1)
        out["width"][k] = common.track(common.smooth(width, 3), 0, 1)
        w = np.maximum(el + er, 0) * active
        wsum = w.sum() + 1e-12
        out["summary"][k] = {"pan": round(float((pan * w).sum() / wsum), 3), "abs_pan": round(float((np.abs(pan) * w).sum() / wsum), 3),
                             "width": round(float((width * w).sum() / wsum), 3)}
    return out
