"""Drum transcription: onset events per drum class, a second opinion next to DrumSep's hits.

ADTOF Frame-RNN (Zehren et al. 2021/2023) through the PyTorch port (xavriley/ADTOF-pytorch, which
states no licence; its bundled weights are converted from ADTOF's, CC BY-NC-SA 4.0, so research only)
on the htdemucs_ft drums stem, 100 fps, the authors' per-class thresholds. Classes: kick (BD), snare (SD), toms (TT,
merged), hi-hat (HH, open and closed merged), cymbals (crash and ride merged). On electronic
music the authors report F 0.79 for kick, 0.57 snare, 0.56 hi-hat, 0.20 cymbals. Each hit is
[time, velocity], but the port writes every note at the same velocity, so only the time means anything.
"""

import tempfile

import numpy as np
import soundfile as sf

from analyze import common

NAME, VERSION = "adtof", 1
GPU = True
PITCH = {35: "kick", 36: "kick", 38: "snare", 40: "snare", 47: "toms", 45: "toms", 48: "toms", 50: "toms",
         42: "hh", 44: "hh", 46: "hh", 49: "cymbals", 51: "cymbals", 52: "cymbals", 55: "cymbals", 57: "cymbals"}


def setup():
    from adtof_pytorch import transcribe_to_midi
    import pretty_midi

    return transcribe_to_midi, pretty_midi


def analyse(song, state):
    transcribe_to_midi, pretty_midi = state
    drums = common.stems(song)["drums"]
    with tempfile.TemporaryDirectory(dir=str(common.ROOT / "tmp")) as d:
        wav, mid = f"{d}/drums.wav", f"{d}/drums.mid"
        sf.write(wav, drums, common.SR, subtype="FLOAT")
        transcribe_to_midi(wav, mid, thresholds=[0.22, 0.24, 0.32, 0.22, 0.30], fps=100, device=common.device(mps_ok=False))
        pm = pretty_midi.PrettyMIDI(mid)
    hits = {k: [] for k in ("kick", "snare", "toms", "hh", "cymbals")}
    for inst in pm.instruments:
        for n in inst.notes:
            k = PITCH.get(n.pitch)
            if k:
                hits[k].append([round(n.start, 3), round(n.velocity / 127, 3)])
    for k in hits:
        hits[k].sort()
    return {"hits": hits, "summary": {k: len(v) for k, v in hits.items()}}
