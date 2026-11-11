# /// script
# requires-python = ">=3.11"
# dependencies = ["numpy", "torch"]
# ///
"""Check that the Rust `StemModel` reproduces the PyTorch student.

    uv run audio-model/stems/check_rust_stems.py runs/la20/best.pt song.mp3

Exports the checkpoint, runs `audio-features --stems` on 20 s of the song
and compares with the PyTorch model run offline on the same audio.
"""

import argparse
import subprocess
import sys
import tempfile
from pathlib import Path

import numpy as np
import torch

HERE = Path(__file__).resolve().parent
sys.path.insert(0, str(HERE))
sys.path.insert(0, str(HERE.parent))
from export_stems import export  # noqa: E402
from features import binary, decode  # noqa: E402
from student import SOURCES, Student, StudentConfig  # noqa: E402


def main() -> None:
    ap = argparse.ArgumentParser()
    ap.add_argument("checkpoint", type=Path)
    ap.add_argument("audio", type=Path)
    ap.add_argument("--seconds", type=float, default=20)
    args = ap.parse_args()

    pcm = decode(str(args.audio))
    start = len(pcm) // 3
    x = np.ascontiguousarray(pcm[start : start + int(args.seconds * 44_100)])
    with tempfile.TemporaryDirectory() as tmp:
        model_bin = Path(tmp) / "stems.bin"
        export(args.checkpoint, model_bin)
        run = subprocess.run([binary(), "--sample-rate", "44100", "--stems", str(model_bin)],
                             input=x.tobytes(), capture_output=True, check=True)
    print(run.stderr.decode().strip())
    rust = np.frombuffer(run.stdout, np.float32).reshape(-1, 4).T

    c = torch.load(args.checkpoint, map_location="cpu", weights_only=False)
    cfg = StudentConfig(**{k: tuple(v) if isinstance(v, list) else v for k, v in c["config"].items()})
    m = Student(cfg).eval()
    m.load_state_dict(c["model"])
    # float64 reference: torch's own float32 run differs from the exact result
    # by ~50 dB SDR on trained weights, more than the Rust port does.
    m = m.double()
    # Rust frame g ends at sample (g+1)*hop; torch (center=True) frames line
    # up with that grid on an input delayed by D = n_fft/2 mod hop samples.
    D = (cfg.n_fft // 2) % cfg.hop
    with torch.no_grad():
        ref, _ = m(torch.from_numpy(np.concatenate([np.zeros(D), x.astype(np.float64)]))[None, None])
    ref = ref[0, :, 0].numpy()
    lag = cfg.n_fft - cfg.hop - D  # rust[i] <-> ref[i - lag]
    a = rust[:, 5 * 44_100 : 15 * 44_100]
    b = ref[:, 5 * 44_100 - lag : 15 * 44_100 - lag]
    for s, name in enumerate(SOURCES):
        err = np.abs(a[s] - b[s]).max()
        snr = 10 * np.log10((b[s] ** 2).sum() / ((a[s] - b[s]) ** 2).sum())
        print(f"{name:7s} max |rust - torch| {err:.2e}  (stem rms {np.sqrt((b[s]**2).mean()):.3f}, "
              f"peak {np.abs(b[s]).max():.2f}), signal-to-difference {snr:.1f} dB")


if __name__ == "__main__":
    main()
