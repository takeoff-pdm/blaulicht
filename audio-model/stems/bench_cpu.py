# /// script
# requires-python = ">=3.11"
# dependencies = ["numpy", "scipy", "torch"]
# ///
"""CPU cost of the student on this machine, single thread.

    uv run audio-model/stems/bench_cpu.py --ckpt runs/la20/best.pt

1. torch, 10 s of audio as one batch of frames: the throughput an
   optimised implementation can reach (BLAS over many frames at once).
2. numpy streaming, one 441-sample hop at a time: the frame-by-frame path
   with Python overhead, an upper bound.
"""

import argparse
import sys
import time
from pathlib import Path

import numpy as np
import torch

sys.path.insert(0, str(Path(__file__).parent))
from stream import StreamingStudent  # noqa: E402
from student import Student, StudentConfig  # noqa: E402


def main() -> None:
    ap = argparse.ArgumentParser()
    ap.add_argument("--ckpt", type=Path, required=True)
    args = ap.parse_args()
    torch.set_num_threads(1)
    c = torch.load(args.ckpt, map_location="cpu", weights_only=False)
    cfg = StudentConfig(**{k: tuple(v) if isinstance(v, list) else v for k, v in c["config"].items()})
    m = Student(cfg).eval()
    m.load_state_dict(c["model"])
    x = torch.randn(1, 1, 44_100 * 10) * 0.1
    with torch.no_grad():
        m(x)
        t = time.perf_counter()
        for _ in range(3):
            m(x)
        dt = (time.perf_counter() - t) / 3
    print(f"torch 1 thread: {dt:.3f} s per 10 s audio -> real-time factor {dt/10:.3f} "
          f"({m.macs_per_frame()*100/1e9:.2f} GMAC/s of audio)")

    s = StreamingStudent(args.ckpt)
    hop = np.random.randn(s.hop).astype(np.float32) * 0.1
    for _ in range(cfg.lookahead + 10):
        s.push(hop)
    t = time.perf_counter()
    n = 300
    for _ in range(n):
        s.push(hop)
    dt = (time.perf_counter() - t) / n
    print(f"numpy streaming: {1e3*dt:.2f} ms per hop (budget 10.0 ms) -> real-time factor {dt/0.01:.2f}")


if __name__ == "__main__":
    main()
