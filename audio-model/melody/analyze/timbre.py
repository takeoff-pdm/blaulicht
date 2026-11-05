"""Tone colour per stem over time, and the sidechain pump.

Per stem (mix, drums, bass, other, vocals) at 20 frames per second:
  brightness  spectral centroid, log scale: 0 = 50 Hz .. 1 = 12.8 kHz;
  noisiness   spectral flatness from 100 Hz to 16 kHz: 0 tonal .. 1 white noise;
  level       dB (-80..0) of the stem.
Pump: the kick onsets of the drums stem (low band below 150 Hz rising by 6 dB or more) mark the
beats; between two kicks (0.25-0.8 s apart) the depth is how far the other stem dips below its
peak in that interval (dB, 18 dB = 1), i.e. how hard the sidechain ducks them. Lights can breathe
with the pump, take white from the brightness and sparkle on noisy textures.
"""

import numpy as np

from analyze import common

NAME, VERSION = "timbre", 1
FPS, N_FFT = 20, 2048
HOP = common.SR // FPS


def analyse(song, state=None):
    S = common.stems(song)
    freqs = np.fft.rfftfreq(N_FFT, 1 / common.SR)
    band = (freqs >= 100) & (freqs <= 16000)
    out = {"fps": FPS, "brightness": {}, "noisiness": {}, "level": {}, "summary": {}}
    for k in common.STEM_NAMES:
        P = np.abs(common.stft(S[k], N_FFT, HOP)) ** 2
        e = P.sum(1)
        level = common.db(e / N_FFT * 4)
        active = level > max(level.max() - 50, -80)
        cen = (P * freqs).sum(1) / (e + 1e-12)
        bright = np.where(active, np.clip(np.log2(np.maximum(cen, 50) / 50) / 8, 0, 1), 0.0)
        Pb = P[:, band] + 1e-12
        flat = np.where(active, np.exp(np.log(Pb).mean(1)) / Pb.mean(1), 0.0)
        out["brightness"][k] = common.track(bright)
        out["noisiness"][k] = common.track(np.clip(flat * 4, 0, 1))  # flatness of music rarely exceeds 0.25
        out["level"][k] = common.track(level, -80, 0)
        out["summary"][k] = {"brightness": round(float(bright[active].mean()) if active.any() else 0.0, 3),
                             "noisiness": round(float(np.clip(flat * 4, 0, 1)[active].mean()) if active.any() else 0.0, 3)}
    # kicks from the drums stem's low band, at 100 fps
    hop = common.SR // 100
    D = np.abs(common.stft(S["drums"], 2048, hop)) ** 2
    low = common.db(D[:, freqs[: D.shape[1]] < 150].sum(1))
    rise = np.maximum(0, low - np.concatenate([[low[0]] * 3, low[:-3]]))
    kicks = [i for i in common.peaks(rise, 6.0, 20) if low[i] > low.max() - 30]
    env = common.smooth(common.db((np.abs(common.stft(S["other"], 2048, hop)) ** 2).sum(1)), 3)  # pads and synths duck
    pump = []
    for a, b in zip(kicks, kicks[1:]):
        if not 25 <= b - a <= 80:
            continue
        seg = env[a:b]
        if seg.max() < env.max() - 40:
            continue
        depth = float(seg.max() - seg[: max(2, int(0.6 * len(seg)))].min())
        pump.append([round(a / 100, 3), round(min(depth / 18, 1.0), 3)])
    out["kicks"] = [round(i / 100, 3) for i in kicks]
    out["pump"] = pump  # [kick time, depth 0..1]
    depths = [d for _, d in pump]
    out["summary"]["pump"] = round(float(np.median(depths)), 3) if depths else 0.0
    return out
