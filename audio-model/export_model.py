# /// script
# requires-python = ">=3.11"
# dependencies = ["numpy", "torch"]
# ///
"""Write a trained checkpoint in the format `BeatModel` (Rust) loads.

    uv run audio-model/export_model.py audio-model/runs/baseline/model.pt

Writes `beat_model.bin` next to the checkpoint. The layout is documented in
crates/audio_features/src/beat_model.rs.
"""

import argparse
import struct
from pathlib import Path

import numpy as np
import torch

FORMAT_VERSION = 1


def export(ckpt_path: Path, out: Path) -> None:
    ckpt = torch.load(ckpt_path, map_location="cpu", weights_only=False)
    config = ckpt["model_config"]
    state = {k: v.numpy() for k, v in ckpt["model"].items()}
    dilations = list(config["dilations"])

    header = struct.pack(
        f"<4s{7 + len(dilations)}I3f",
        b"BTCN",
        FORMAT_VERSION,
        ckpt["frontend"]["version"],
        config["n_mels"],
        config["channels"],
        state["input.weight"].shape[2],
        config["kernel"],
        len(dilations),
        *dilations,
        ckpt["lead_ms"],
        ckpt["thresholds"]["beat"],
        ckpt["thresholds"]["downbeat"],
    )
    arrays = [ckpt["mean"], ckpt["std"], state["input.weight"], state["input.bias"]]
    for i in range(len(dilations)):
        for name in ("norm.weight", "norm.bias", "conv.weight", "conv.bias", "mix.weight", "mix.bias"):
            arrays.append(state[f"blocks.{i}.{name}"])
    arrays += [state["norm.weight"], state["norm.bias"], state["head.weight"], state["head.bias"]]

    used = {"input.weight", "input.bias", "norm.weight", "norm.bias", "head.weight", "head.bias"}
    used |= {k for k in state if k.startswith("blocks.")}
    if missing := set(state) - used:
        raise SystemExit(f"checkpoint has weights the export does not know: {sorted(missing)}")

    body = b"".join(np.ascontiguousarray(a, "<f4").tobytes() for a in arrays)
    out.write_bytes(header + body)
    print(f"wrote {out} ({len(header) + len(body)} bytes)")


def main() -> None:
    ap = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    ap.add_argument("checkpoint", type=Path)
    ap.add_argument("--out", type=Path)
    args = ap.parse_args()
    export(args.checkpoint, args.out or args.checkpoint.with_name("beat_model.bin"))


if __name__ == "__main__":
    main()
