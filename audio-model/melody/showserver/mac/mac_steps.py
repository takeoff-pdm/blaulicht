"""One stage of the Mac analysis, run by mac_song.py inside the right venv (from ane-lyrics/scripts/song_mac.py).

    <venv>/bin/python mac_steps.py <stage> <song id> <audio> [analyzer]

Stages: stems (venv-sep), game (venv-sep), melodia (venv-ana), lead_bass (venv-ana), viewer_data (venv-ana,
needs <out dir>), analyzer <name> (venv per analyzer). Everything is read from and written to $MELODY_ROOT
the way ae-charlie lays it out, keyed by the pipeline's song id.
"""

import importlib
import json
import os
import sys
from pathlib import Path

import numpy as np

M = Path(__file__).resolve().parents[2]  # audio-model/melody
sys.path[:0] = [str(M), str(M / "ane"), str(M / "pipeline")]
from analyze import common  # noqa: E402

stage, sid, audio = sys.argv[1], sys.argv[2], Path(sys.argv[3])
song = common.Song(sid, audio.stem, audio, str(audio))
ROOT = common.ROOT
STEMS = ROOT / "stems_songs" / f"{sid}.npy"


def stems():
    import backends
    import htdemucs_coreml as H
    bag = backends.demucs(H.bag())
    wav = H.load_audio(audio)
    s = H.separate(bag, wav, "cpu").numpy()  # drums, bass, other, vocals
    # ae-charlie's teacher layout: mix, drums, bass, vocals (+ other)
    out = np.stack([wav.numpy().mean(0), s[0].mean(0), s[1].mean(0), s[3].mean(0), s[2].mean(0)]).astype(np.float32)
    STEMS.parent.mkdir(parents=True, exist_ok=True)
    tmp = STEMS.with_suffix(".tmp.npy")
    np.save(tmp, out)
    os.replace(tmp, STEMS)


def game():
    import backends
    import game_batch as G
    g = backends.game(G.Game(Path(os.environ["GAME_MODEL"]), "cpu"))
    v = np.load(STEMS, mmap_mode="r")[3].astype(np.float32)
    notes = []
    for a, b in G.chunks(v):
        notes += [(a / G.SR + s, a / G.SR + e, p) for s, e, p in g.transcribe(v[a:b])]
    out = ROOT / "out/game"
    out.mkdir(parents=True, exist_ok=True)
    G.write_midi(notes, out / f"{sid}.mid")
    (out / f"{sid}.json").write_text(json.dumps([[round(s, 3), round(e, 3), round(p, 2)] for s, e, p in notes]))
    print("game notes", len(notes))


def melodia():
    import melodia_batch as MB
    out = ROOT / "out/melodia"
    out.mkdir(parents=True, exist_ok=True)
    f0, conf = MB.melodia(str(audio))
    np.savez_compressed(out / f"{sid}.npz", f0=f0, conf=conf, hop=MB.HOP, sr=MB.SR)  # keyed by the song id on the Mac
    MB.write_midi(MB.f0_to_notes(f0, MB.HOP / MB.SR), out / f"{sid}.mid")


def lead_bass():
    import melodia_stems as MS
    for name in MS.SETTINGS:
        (ROOT / "out" / name).mkdir(parents=True, exist_ok=True)
    print(MS.work((STEMS, ROOT / "out")))


def viewer_data():
    import build_viz_data as V
    doc = V.song_doc(ROOT, sid, sid, audio.stem, STEMS)
    out = Path(sys.argv[4])
    out.mkdir(parents=True, exist_ok=True)
    tmp = out / f".{sid}.tmp"
    tmp.write_text(json.dumps(doc, separators=(",", ":")))
    os.replace(tmp, out / f"{sid}.json")


def analyzer(name):
    m = importlib.import_module(f"analyze.{name}")
    st = m.setup() if hasattr(m, "setup") else None
    common.save(m.NAME, m.VERSION, song, m.analyse(song, st))


if __name__ == "__main__":
    if stage == "analyzer":
        analyzer(sys.argv[4])
    else:
        {"stems": stems, "game": game, "melodia": melodia, "lead_bass": lead_bass, "viewer_data": viewer_data}[stage]()
