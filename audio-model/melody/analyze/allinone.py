"""Song structure with functional labels, beats and downbeats, from a trained model.

All-In-One (Kim & Nam, WASPAA 2023; code and the taejunkim/allinone weights MIT) through the
maintained fork all-in-one-infer (openmirlab), which replaces the NATTEN dependency with pure-PyTorch
neighbourhood attention. Fed our htdemucs_ft stems, so nothing is separated twice (it was trained on
htdemucs stems, so a small mismatch is possible). Output: segments [[start, end, label]] with
labels intro, outro, break, bridge, inst, solo, verse, chorus, start, end; beats and downbeats; bpm.
It was trained on pop (Harmonix Set) and has no drop or build-up label, so drops mostly come out as
chorus or inst; the compiler uses it as a second opinion next to the club-track section rule.
"""

import tempfile
from pathlib import Path

import numpy as np
import soundfile as sf

from analyze import common

NAME, VERSION = "allinone", 1
GPU = True


def setup():
    from allin1_infer import AllInOneSession, StemsInput

    session = AllInOneSession(model="harmonix-all", device=common.device(mps_ok=False))
    session.load()
    return session, StemsInput


def analyse(song, state):
    session, StemsInput = state
    S = common.stems(song)
    (common.ROOT / "tmp").mkdir(exist_ok=True)
    with tempfile.TemporaryDirectory(dir=str(common.ROOT / "tmp")) as d:
        d = Path(d)
        for k in ("bass", "drums", "other", "vocals"):
            sf.write(d / f"{k}.wav", np.clip(S[k], -1, 1), common.SR, subtype="PCM_16")
        # spec_dir in here: the default ./spec is shared between shards, and each removes it when it looks empty
        out = session.infer(StemsInput(bass=d / "bass.wav", drums=d / "drums.wav", other=d / "other.wav",
                                       vocals=d / "vocals.wav", identifier=song.id), spec_dir=d / "spec", demix_dir=d / "demix")
    r = out[0] if isinstance(out, (list, tuple)) else out
    segs = [[round(float(s.start), 2), round(float(s.end), 2), s.label] for s in r.segments]
    return {"bpm": r.bpm, "beats": [round(float(t), 3) for t in r.beats], "downbeats": [round(float(t), 3) for t in r.downbeats],
            "segments": segs, "summary": {"segments": len(segs), "labels": sorted({s[2] for s in segs})}}
