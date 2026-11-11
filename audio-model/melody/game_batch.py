"""Singing voice -> MIDI notes with GAME (openvpi, MIT) through its released ONNX models.

Reads teacher stem files (`<id>.npy`, float16 (4, n) mono 44.1 kHz: mix, drums, bass, vocals),
transcribes the vocal row and writes `<out>/<id>.mid` plus `<out>/<id>.json` (notes as
[start s, end s, midi pitch float]). The pipeline follows GAME's ONNX.md: encoder ->
dur2bd -> D3PM loop over the segmenter -> bd2dur -> estimator, on chunks cut at silences.

    python game_batch.py stems_dir out/game --model models/game-large/GAME-1.0.3-large-onnx
"""

import argparse
import json
import os
import sys
from pathlib import Path

import numpy as np
import onnxruntime as ort

ort.preload_dlls()

SR = 44100
MAX_CHUNK_S = 30.0


def chunks(wav: np.ndarray, sr=SR, thresh_db=-40.0, min_sil_s=0.2, max_len_s=MAX_CHUNK_S):
    """(start, end) sample ranges of non-silent regions, each at most max_len_s long."""
    hop = sr // 100
    n = len(wav) // hop
    rms = np.sqrt(np.mean(wav[: n * hop].reshape(n, hop).astype(np.float32) ** 2, 1) + 1e-12)
    loud = 20 * np.log10(rms / (rms.max() + 1e-12)) > thresh_db
    spans, start, quiet = [], None, 0
    for i, l in enumerate(loud):
        if l:
            if start is None:
                start = i
            quiet = 0
        elif start is not None:
            quiet += 1
            if quiet * hop >= min_sil_s * sr:
                spans.append((start, i - quiet + 1))
                start = None
    if start is not None:
        spans.append((start, n))
    out = []
    max_f = int(max_len_s * 100)
    for s, e in spans:
        s, e = max(0, s - 5), min(n, e + 5)  # 50 ms of context on both sides
        while e - s > max_f:  # cut long regions at their quietest frame in the last third
            lo = s + max_f * 2 // 3
            cut = lo + int(np.argmin(rms[lo : s + max_f]))
            out.append((s * hop, cut * hop))
            s = cut
        if e - s >= 30:  # skip regions shorter than 0.3 s
            out.append((s * hop, e * hop))
    return out


class Game:
    def __init__(self, model_dir: Path, device: str):
        prov = ["CUDAExecutionProvider", "CPUExecutionProvider"] if device == "cuda" else ["CPUExecutionProvider"]
        so = ort.SessionOptions()
        so.intra_op_num_threads = 4
        self.s = {n: ort.InferenceSession(str(model_dir / f"{n}.onnx"), so, providers=prov)
                  for n in ("encoder", "dur2bd", "segmenter", "bd2dur", "estimator")}
        self.cfg = json.loads((model_dir / "config.json").read_text())

    def transcribe(self, wav: np.ndarray, nsteps=8, seg_threshold=0.2, radius=2, est_threshold=0.2):
        dur = np.array([len(wav) / SR], np.float32)
        x_seg, x_est, mask_t = self.s["encoder"].run(None, {"waveform": wav[None].astype(np.float32), "duration": dur})
        # No known boundaries inside a chunk (dur2bd with a single duration yields none, and its
        # CUDA kernel fails on the empty reduction).
        known = np.zeros_like(mask_t)
        bd = known
        for i in range(nsteps):
            bd = self.s["segmenter"].run(None, {
                "x_seg": x_seg, "language": np.zeros(1, np.int64), "known_boundaries": known,
                "prev_boundaries": bd, "t": np.array([i / nsteps], np.float32), "maskT": mask_t,
                "threshold": np.array(seg_threshold, np.float32), "radius": np.array(radius, np.int64),
            })[0]
        durs, mask_n = self.s["bd2dur"].run(None, {"boundaries": bd, "maskT": mask_t})
        presence, scores = self.s["estimator"].run(None, {
            "x_est": x_est, "boundaries": bd, "maskT": mask_t, "maskN": mask_n,
            "threshold": np.array(est_threshold, np.float32),
        })
        notes, t = [], 0.0
        for d, m, p, s in zip(durs[0], mask_n[0], presence[0], scores[0]):
            if not m:
                break
            if p:
                notes.append((t, t + float(d), float(s)))
            t += float(d)
        return notes


def write_midi(notes, path, program=52):
    import pretty_midi

    pm = pretty_midi.PrettyMIDI(initial_tempo=120)
    inst = pretty_midi.Instrument(program=program, name="vocal melody")
    for s, e, p in notes:
        if e - s >= 0.03:
            inst.notes.append(pretty_midi.Note(velocity=100, pitch=int(round(p)), start=s, end=e))
    pm.instruments.append(inst)
    pm.write(str(path))


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("stems", type=Path)
    ap.add_argument("out", type=Path)
    ap.add_argument("--model", type=Path, required=True)
    ap.add_argument("--device", default="cuda")
    ap.add_argument("--row", type=int, default=3, help="3 = vocals")
    ap.add_argument("--limit", type=int, default=0)
    ap.add_argument("--shard", type=int, default=0)
    ap.add_argument("--num-shards", type=int, default=1)
    args = ap.parse_args()
    args.out.mkdir(parents=True, exist_ok=True)
    game = Game(args.model, args.device)
    if os.environ.get("MELODY_COREML"):  # Mac backend: the segmenter's network in Core ML (ane/backends.py)
        sys.path.insert(0, str(Path(__file__).resolve().parent / "ane"))
        import backends
        backends.game(game)
    files = sorted(args.stems.glob("*.npy"))
    files = [f for f in files if not f.name.endswith(".tmp.npy")][: args.limit or None]
    files = [f for f in files if int(f.stem, 16) % args.num_shards == args.shard]
    for k, f in enumerate(files):
        out = args.out / f"{f.stem}.json"
        if out.exists():
            continue
        wav = np.load(f, mmap_mode="r")[args.row].astype(np.float32)
        notes = []
        for a, b in chunks(wav):
            notes += [(a / SR + s, a / SR + e, p) for s, e, p in game.transcribe(wav[a:b])]
        write_midi(notes, args.out / f"{f.stem}.mid")
        out.write_text(json.dumps([[round(s, 3), round(e, 3), round(p, 2)] for s, e, p in notes]))
        print(f"[{k + 1}/{len(files)}] {f.stem}: {len(notes)} notes", flush=True)


if __name__ == "__main__":
    main()
