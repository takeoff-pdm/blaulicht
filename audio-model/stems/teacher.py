"""Separate audio with a large offline teacher and store distillation targets.

Each input becomes `<out>/<id>.npy`: float16, shape (4, samples), mono at
44.1 kHz, rows = mix, drums, bass, vocals. "other" is mix - drums - bass -
vocals, so the four stems always sum to the mix. The teacher runs on the
stereo signal; its outputs are averaged to mono afterwards.

MUSDB18-HQ ground truth is converted to the same layout with --musdb.

    python teacher.py --list files.txt --out data/teacher/edm --model htdemucs_ft
    python teacher.py --musdb data/musdb18hq/train --out data/teacher/musdb_train
"""

import argparse
import hashlib
import subprocess
import sys
import time
from concurrent.futures import ThreadPoolExecutor
from pathlib import Path

import numpy as np
import soundfile as sf
import torch

SR = 44_100
STEMS = ("drums", "bass", "other", "vocals")


def decode(path: Path) -> np.ndarray:
    """(2, n) float32 at 44.1 kHz."""
    pcm = subprocess.run(
        ["ffmpeg", "-v", "error", "-i", str(path), "-ac", "2", "-ar", str(SR), "-f", "f32le", "-"],
        check=True, capture_output=True,
    ).stdout
    return np.frombuffer(pcm, np.float32).reshape(-1, 2).T.copy()


def clip_id(path: Path) -> str:
    return hashlib.sha1(str(path).encode()).hexdigest()[:12]


def save(out: Path, mix: np.ndarray, drums: np.ndarray, bass: np.ndarray, vocals: np.ndarray) -> None:
    arr = np.stack([mix, drums, bass, vocals]).astype(np.float16)
    tmp = out.with_suffix(".tmp.npy")
    np.save(tmp, arr)
    tmp.rename(out)


def run_teacher(args) -> None:
    from demucs.apply import apply_model
    from demucs.pretrained import get_model

    model = get_model(args.model).to("cuda").eval()
    assert list(model.sources) == list(STEMS), model.sources
    files = [Path(l.strip()) for l in Path(args.list).read_text().splitlines() if l.strip()]
    files = files[args.shard :: args.num_shards]
    if args.reverse:
        files = files[::-1]
    out_dir = Path(args.out)
    out_dir.mkdir(parents=True, exist_ok=True)
    index = open(out_dir / f"index.{args.shard}.tsv", "a")
    todo = [f for f in files if not (out_dir / f"{clip_id(f)}.npy").exists()]
    print(f"{len(todo)} of {len(files)} to do", flush=True)

    def load(f):
        try:
            return f, decode(f)
        except Exception as e:  # corrupt files exist in FMA
            return f, e

    t0, seconds = time.time(), 0.0
    with ThreadPoolExecutor(4) as pool:
        for i, (f, wav) in enumerate(pool.map(load, todo)):
            if (out_dir / f"{clip_id(f)}.npy").exists():
                continue  # another worker got here first
            if isinstance(wav, Exception) or wav.shape[1] < SR * 5:
                print(f"skip {f}: {wav if isinstance(wav, Exception) else 'too short'}", flush=True)
                continue
            x = torch.from_numpy(wav).cuda()
            ref = x.mean(0)
            mean, std = ref.mean(), ref.std() + 1e-8
            with torch.no_grad():
                src = apply_model(
                    model, ((x - mean) / std)[None], device="cuda", shifts=args.shifts,
                    split=True, overlap=0.25, progress=False,
                )[0]
            src = (src * std + mean).mean(1).float().cpu().numpy()  # (4, n) mono
            mix = wav.mean(0)
            save(out_dir / f"{clip_id(f)}.npy", mix, src[0], src[1], src[3])
            index.write(f"{clip_id(f)}\t{f}\n")
            index.flush()
            seconds += wav.shape[1] / SR
            if i % 20 == 0:
                el = time.time() - t0
                print(f"{i}/{len(todo)} audio {seconds/3600:.2f} h, {seconds/el:.1f}x realtime", flush=True)


def run_musdb(args) -> None:
    out_dir = Path(args.out)
    out_dir.mkdir(parents=True, exist_ok=True)
    with open(out_dir / "index.0.tsv", "w") as index:
        for track in sorted(Path(args.musdb).iterdir()):
            stems = {s: sf.read(track / f"{s}.wav", dtype="float32")[0].mean(1) for s in STEMS}
            mix = sum(stems.values())
            save(out_dir / f"{clip_id(track)}.npy", mix, stems["drums"], stems["bass"], stems["vocals"])
            index.write(f"{clip_id(track)}\t{track}\n")
            print(track.name, flush=True)


def main() -> None:
    ap = argparse.ArgumentParser()
    ap.add_argument("--list")
    ap.add_argument("--musdb")
    ap.add_argument("--out", required=True)
    ap.add_argument("--model", default="htdemucs_ft")
    ap.add_argument("--shifts", type=int, default=1)
    ap.add_argument("--shard", type=int, default=0)
    ap.add_argument("--num-shards", type=int, default=1)
    ap.add_argument("--reverse", action="store_true", help="walk the list backwards (a helper worker)")
    args = ap.parse_args()
    if args.musdb:
        run_musdb(args)
    elif args.list:
        run_teacher(args)
    else:
        sys.exit("need --list or --musdb")


if __name__ == "__main__":
    main()
