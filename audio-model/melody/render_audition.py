"""Audition clips: the song quietly underneath, the extracted melody as a synth on top.

For each requested song id writes `<out>/<name> - <method>.mp3`: a 40 s mono excerpt from just
before the middle of the track, the mix turned down and a sine-plus-harmonics synth of the melody
on top, so the ear can follow the extracted line against the song.

Methods: `melodia` (the f0 contour of out/melodia/<id>.npz, continuous pitch) and any directory
under out/ keyed by stem ids: note JSON files ([start, end, midi] lists, out/game), Melodia
contours (out/lead, out/bass) or 100 fps f0 arrays (out/student).

    python render_audition.py --ids 6a216b41f62b,... --out auditions
"""

import argparse
import hashlib
import json
import subprocess
from pathlib import Path

import numpy as np

SR = 44100
ROOT = Path("/localhome/sokrates/melody")


def decode(path, start, dur):
    pcm = subprocess.run(["ffmpeg", "-v", "error", "-ss", str(start), "-t", str(dur), "-i", str(path),
                          "-ac", "1", "-ar", str(SR), "-f", "f32le", "-"], check=True, capture_output=True).stdout
    return np.frombuffer(pcm, np.float32)


def synth_f0(f0_hz, frame_s, n, start):
    """Continuous-pitch synth for an f0 track sampled every frame_s seconds."""
    t = start + np.arange(n) / SR
    f = np.interp(t, np.arange(len(f0_hz)) * frame_s, f0_hz)
    on = (f > 0).astype(np.float32)
    env = np.convolve(on, np.ones(441) / 441, mode="same")  # 10 ms fades
    phase = 2 * np.pi * np.cumsum(np.where(f > 0, f, 0)) / SR
    return env * (np.sin(phase) + 0.3 * np.sin(2 * phase) + 0.15 * np.sin(3 * phase))


def synth_notes(notes, n, start):
    y = np.zeros(n, np.float32)
    for s, e, p in notes:
        a, b = int((s - start) * SR), int((e - start) * SR)
        if b <= 0 or a >= n:
            continue
        a, b = max(a, 0), min(b, n)
        f = 440 * 2 ** ((round(p) - 69) / 12)
        t = np.arange(b - a) / SR
        env = np.minimum(1, np.minimum(t / 0.01, (t[::-1]) / 0.03))
        ph = 2 * np.pi * f * t
        y[a:b] += env * (np.sin(ph) + 0.3 * np.sin(2 * ph) + 0.15 * np.sin(3 * ph))
    return y


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--ids", required=True)
    ap.add_argument("--methods", default="melodia,game,lead,bass,student")
    ap.add_argument("--out", type=Path, required=True)
    ap.add_argument("--seconds", type=float, default=40)
    args = ap.parse_args()
    args.out.mkdir(parents=True, exist_ok=True)
    index = json.loads((ROOT / "out/melodia/index.json").read_text())
    for sid in args.ids.split(","):
        path = ROOT / index[sid]["path"]
        dur = float(subprocess.run(["ffprobe", "-v", "error", "-show_entries", "format=duration", "-of",
                                    "csv=p=0", str(path)], capture_output=True, text=True).stdout)
        start = max(0.0, dur * 0.45 - args.seconds / 2)
        mix = decode(path, start, args.seconds)
        n = len(mix)
        for m in args.methods.split(","):
            if m == "melodia":
                z = np.load(ROOT / f"out/melodia/{sid}.npz")
                syn = synth_f0(z["f0"], int(z["hop"]) / int(z["sr"]), n, start)
            else:  # stem-based methods use ids of the absolute path (songs_remote.txt)
                gid = hashlib.sha1(str(ROOT / index[sid]["path"]).encode()).hexdigest()[:12]
                notes, contour = ROOT / f"out/{m}/{gid}.json", ROOT / f"out/{m}/{gid}.npz"
                if notes.exists():
                    syn = synth_notes(json.loads(notes.read_text()), n, start)
                elif contour.exists():
                    z = np.load(contour)
                    syn = synth_f0(z["f0"], int(z["hop"]) / int(z["sr"]), n, start)
                elif (ROOT / f"out/{m}/{gid}.npy").exists():  # student f0 at 100 fps
                    syn = synth_f0(np.load(ROOT / f"out/{m}/{gid}.npy"), 0.01, n, start)
                else:
                    continue
            mix_n = mix / (np.abs(mix).max() + 1e-6) * 0.25
            y = mix_n + 0.35 * syn / (np.abs(syn).max() + 1e-6)
            name = args.out / f"{path.stem} - {m}.mp3"
            subprocess.run(["ffmpeg", "-v", "error", "-y", "-f", "f32le", "-ar", str(SR), "-ac", "1", "-i", "-",
                            "-b:a", "160k", str(name)], input=y.astype(np.float32).tobytes(), check=True)
            print(name, flush=True)


if __name__ == "__main__":
    main()
