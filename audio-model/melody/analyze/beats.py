"""Beats and downbeats from the audio, as a cross-check on the Rekordbox grid.

Beat This! (Foscarin et al., ISMIR 2024; code and weights MIT; checkpoint final0) on the mix,
mono 22.05 kHz, without the DBN post-processing (it follows tempo changes and needs no fixed
meter). The compiler compares it with the aligned Rekordbox grid: where they agree the grid is
trusted; where Rekordbox's downbeat phase or tempo is off, these beats can replace it.
"""

import numpy as np

from analyze import common

NAME, VERSION = "beats", 1
GPU = True


def setup():
    from beat_this.inference import Audio2Beats

    return Audio2Beats(checkpoint_path="final0", device=common.device(), dbn=False)


def analyse(song, a2b):
    x = common.decode(song.audio, 22050, 1)[0]
    beats, downbeats = a2b(x, 22050)
    beats, downbeats = np.asarray(beats, float), np.asarray(downbeats, float)
    ibi = np.diff(beats)
    return {"beats": [round(float(t), 3) for t in beats], "downbeats": [round(float(t), 3) for t in downbeats],
            "bpm": round(float(60 / np.median(ibi)), 2) if len(ibi) else None,
            "tempo_spread": round(float(np.std(60 / ibi[ibi > 0.1])), 2) if len(ibi) > 4 else None}
