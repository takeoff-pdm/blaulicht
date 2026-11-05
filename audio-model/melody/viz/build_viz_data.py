"""Pack every song's analysis into one JSON file for the visualizer (runs on ae-charlie).

Per song `<out>/<id>.json` (id = the stem id, sha1 of the absolute path on ae-charlie):
  notes:    {track: [[start s, end s, midi], ...]} read back from the generated MIDI files
            (melodia, game, lead, bass, student), so the viewer shows exactly what was exported;
  contours: {track: base64 int16 little-endian, MIDI cents (midi * 100), 0 = no pitch} at
            100 fps (frame g at g * 10 ms): melodia (mix), lead, bass (Melodia on stems), student;
  energy:   {stem: base64 uint8} at 100 fps, RMS over 20 ms centred on the frame, mapped
            linearly from -60..0 dBFS to 0..255, for mix, drums, bass, other, vocals.
`<out>/songs.json` lists the songs (id, name, Rekordbox path, duration, tracks present).

    python build_viz_data.py /localhome/sokrates/melody/viz
"""

import base64
import hashlib
import json
import sys
from pathlib import Path

import numpy as np
import pretty_midi

ROOT = Path("/localhome/sokrates/melody")
SR, HOP = 44100, 441
TRACKS = ("melodia", "game", "lead", "bass", "student")


def b64(a: np.ndarray) -> str:
    return base64.b64encode(np.ascontiguousarray(a).tobytes()).decode()


def midi_notes(path: Path):
    if not path.exists():
        return None
    pm = pretty_midi.PrettyMIDI(str(path))
    return [[round(n.start, 3), round(n.end, 3), n.pitch] for inst in pm.instruments for n in inst.notes]


def cents_100fps(f0: np.ndarray, frame_s: float, n_frames: int) -> np.ndarray:
    idx = np.clip(np.round(np.arange(n_frames) * 0.01 / frame_s).astype(int), 0, len(f0) - 1)
    f = f0[idx].astype(np.float64)
    out = np.zeros(n_frames, np.int16)
    v = f > 0
    out[v] = np.round(100 * (69 + 12 * np.log2(f[v] / 440))).astype(np.int16)
    return out


def energy_100fps(x: np.ndarray, n_frames: int) -> np.ndarray:
    x = np.pad(x.astype(np.float32), (HOP, HOP + HOP * 2))
    sq = np.cumsum(np.concatenate([[0.0], x.astype(np.float64) ** 2]))
    starts = np.arange(n_frames) * HOP  # window [g*HOP - HOP, g*HOP + HOP) in unpadded samples
    rms = np.sqrt((sq[starts + 2 * HOP] - sq[starts]) / (2 * HOP) + 1e-12)
    db = 20 * np.log10(rms + 1e-9)
    return np.clip((db + 60) / 60 * 255, 0, 255).astype(np.uint8)


def main():
    out = Path(sys.argv[1])
    out.mkdir(parents=True, exist_ok=True)
    index = json.loads((ROOT / "out/melodia/index.json").read_text())
    songs = []
    for sid, v in sorted(index.items(), key=lambda kv: Path(kv[1]["path"]).stem.lower()):
        gid = hashlib.sha1(str(ROOT / v["path"]).encode()).hexdigest()[:12]
        stems_path = ROOT / "stems_songs" / f"{gid}.npy"
        if not stems_path.exists():
            print("no stems", v["path"], flush=True)
            continue
        stems = np.load(stems_path, mmap_mode="r")
        n = stems.shape[1]
        n_frames = n // HOP + 1
        keys = {"melodia": sid, "game": gid, "lead": gid, "bass": gid, "student": gid}
        notes = {t: midi_notes(ROOT / "out" / t / f"{keys[t]}.mid") for t in TRACKS}
        notes = {t: v_ for t, v_ in notes.items() if v_ is not None}
        contours = {}
        mel = np.load(ROOT / "out/melodia" / f"{sid}.npz")
        contours["melodia"] = b64(cents_100fps(mel["f0"], int(mel["hop"]) / int(mel["sr"]), n_frames))
        for t in ("lead", "bass"):
            p = ROOT / "out" / t / f"{gid}.npz"
            if p.exists():
                z = np.load(p)
                contours[t] = b64(cents_100fps(z["f0"], int(z["hop"]) / int(z["sr"]), n_frames))
        p = ROOT / "out/student" / f"{gid}.npy"
        if p.exists():
            contours["student"] = b64(cents_100fps(np.load(p), 0.01, n_frames))
        mix, drums, bass, vocals = (np.asarray(stems[i], np.float32) for i in range(4))
        sources = {"mix": mix, "drums": drums, "bass": bass, "other": mix - drums - bass - vocals, "vocals": vocals}
        energy = {k: b64(energy_100fps(x, n_frames)) for k, x in sources.items()}
        doc = {"id": gid, "name": Path(v["path"]).stem, "duration": n / SR, "frames": n_frames, "fps": 100,
               "notes": notes, "contours": contours, "energy": energy}
        (out / f"{gid}.json").write_text(json.dumps(doc, separators=(",", ":")))
        songs.append({"id": gid, "name": Path(v["path"]).stem, "path": v["listed"], "duration": round(n / SR, 2),
                      "tracks": sorted(notes)})
        print(len(songs), doc["name"], {t: len(x) for t, x in notes.items()}, flush=True)
    (out / "songs.json").write_text(json.dumps(songs, indent=1, ensure_ascii=False))


if __name__ == "__main__":
    main()
