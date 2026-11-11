# /// script
# requires-python = ">=3.11"
# dependencies = ["numpy", "torch"]
# ///
"""Write a student checkpoint in the format `StemModel` (Rust) loads.

    uv run audio-model/stems/export_stems.py runs/la20/best.pt [--out stems.bin]

The layout is documented in crates/audio_features/src/stem_model.rs.
"""

import argparse
import struct
import sys
from pathlib import Path

import numpy as np
import torch

sys.path.insert(0, str(Path(__file__).parent))
from student import Student, StudentConfig  # noqa: E402

FORMAT_VERSION = 1


def export(ckpt_path: Path, out: Path) -> None:
    c = torch.load(ckpt_path, map_location="cpu", weights_only=False)
    cfg = StudentConfig(**{k: tuple(v) if isinstance(v, list) else v for k, v in c["config"].items()})
    edges = Student(cfg).edges
    state = {k: v.float().numpy() for k, v in c["model"].items()}
    n = len(cfg.dilations)
    ints = [FORMAT_VERSION, cfg.n_fft, cfg.hop, cfg.channels, cfg.kernel, cfg.mix_channels,
            cfg.sources, n, *cfg.dilations, *cfg.right_pads(), len(edges)]
    ints += [e for lo_hi in edges for e in lo_hi]
    header = b"STEM" + struct.pack(f"<{len(ints)}I", *ints)

    names = ["feat_mean", "feat_std"]
    for i in range(len(edges)):
        names += [f"encode.{i}.weight", f"encode.{i}.bias"]
    for i in range(n):
        t, m = f"blocks.{2*i}.", f"blocks.{2*i+1}."
        names += [t + p for p in ("norm.weight", "norm.bias", "conv.weight", "conv.bias", "mix.weight", "mix.bias")]
        names += [m + p for p in ("norm.weight", "norm.bias", "down.weight", "down.bias",
                                  "across.weight", "across.bias", "up.weight", "up.bias")]
    names += ["out_norm.weight", "out_norm.bias"]
    for i in range(len(edges)):
        names += [f"decode.{i}.weight", f"decode.{i}.bias"]
    if missing := set(state) - set(names):
        raise SystemExit(f"checkpoint has weights the export does not know: {sorted(missing)}")
    body = b"".join(np.ascontiguousarray(state[k], "<f4").tobytes() for k in names)
    out.write_bytes(header + body)
    print(f"wrote {out} ({(len(header) + len(body)) / 1e6:.1f} MB, lookahead {cfg.lookahead} frames)")


def main() -> None:
    ap = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    ap.add_argument("checkpoint", type=Path)
    ap.add_argument("--out", type=Path)
    args = ap.parse_args()
    export(args.checkpoint, args.out or args.checkpoint.with_name("stems.bin"))


if __name__ == "__main__":
    main()
