#!/usr/bin/env python3
from __future__ import annotations

import argparse
import math
import struct
import sys
import wave
from pathlib import Path
from typing import Iterable


def parse_args(argv: Iterable[str]) -> argparse.Namespace:
    parser = argparse.ArgumentParser(
        description="Generate a simple waveform WAV file.",
    )
    parser.add_argument("output", help="Path to output WAV file.")
    parser.add_argument("--seconds", type=float, default=1.0)
    parser.add_argument("--freq", type=float, default=440.0)
    parser.add_argument("--sample-rate", type=int, default=48000)
    parser.add_argument("--amplitude", type=float, default=0.5, help="0.0 to 1.0")
    parser.add_argument(
        "--wave",
        choices=("sine", "saw"),
        default="sine",
        help="Waveform to generate.",
    )
    return parser.parse_args(list(argv))


def main(argv: Iterable[str]) -> int:
    args = parse_args(argv)

    if args.seconds <= 0:
        print("--seconds must be > 0", file=sys.stderr)
        return 2
    if args.freq <= 0:
        print("--freq must be > 0", file=sys.stderr)
        return 2
    if args.sample_rate <= 0:
        print("--sample-rate must be > 0", file=sys.stderr)
        return 2
    if not (0.0 <= args.amplitude <= 1.0):
        print("--amplitude must be between 0.0 and 1.0", file=sys.stderr)
        return 2

    total_frames = int(round(args.seconds * args.sample_rate))
    if total_frames <= 0:
        print("Duration too short for the sample rate.", file=sys.stderr)
        return 2

    amplitude = int(round(args.amplitude * 32767))
    output_path = Path(args.output)

    with wave.open(str(output_path), "wb") as wav:
        wav.setnchannels(1)
        wav.setsampwidth(2)
        wav.setframerate(args.sample_rate)
        for i in range(total_frames):
            if args.wave == "saw":
                phase = (i * args.freq / args.sample_rate) % 1.0
                sample = (2.0 * phase) - 1.0
            else:
                sample = math.sin(2.0 * math.pi * args.freq * (i / args.sample_rate))
            wav.writeframesraw(struct.pack("<h", int(round(sample * amplitude))))
        wav.writeframes(b"")

    return 0


if __name__ == "__main__":
    raise SystemExit(main(sys.argv[1:]))
