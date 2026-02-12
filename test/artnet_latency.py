#!/usr/bin/env python3
from __future__ import annotations

import argparse
import os
import socket
import struct
import sys
import tempfile
import time
from dataclasses import dataclass
from pathlib import Path
from typing import Iterable, Optional, Tuple

try:
    import mpv
except ImportError as exc:
    raise SystemExit(
        "python-mpv is required. Install the mpv bindings (python-mpv) and libmpv."
    ) from exc

try:
    import pyartnet  # noqa: F401
except ImportError as exc:
    raise SystemExit("pyartnet is required. Install the pyartnet package.") from exc

ARTNET_HEADER = b"Art-Net\x00"
ARTNET_PORT = 6454
ARTDMX_OPCODE = 0x5000


def parse_artdmx(packet: bytes) -> Optional[Tuple[int, bytes]]:
    if len(packet) < 18:
        return None

    if packet[:8] != ARTNET_HEADER:
        return None

    opcode = struct.unpack("<H", packet[8:10])[0]
    if opcode != ARTDMX_OPCODE:
        return None

    universe = struct.unpack("<H", packet[14:16])[0]
    length = struct.unpack(">H", packet[16:18])[0]
    if length == 0:
        return None
    if len(packet) < 18 + length:
        return None
    data_bytes = packet[18 : 18 + length]

    return universe, data_bytes


@dataclass(frozen=True)
class ChannelWindow:
    start: int
    count: int

    def slice(self, data: bytes) -> bytes:
        if self.start >= len(data):
            return b""
        end = min(len(data), self.start + self.count)
        return data[self.start : end]


@dataclass
class AudioBuffer:
    fd: int
    source: str
    backing: Optional[object] = None

    def rewind(self) -> None:
        os.lseek(self.fd, 0, os.SEEK_SET)


def load_audio_buffer(path: str) -> AudioBuffer:
    data = Path(path).read_bytes()
    if hasattr(os, "memfd_create"):
        fd = os.memfd_create("blaulicht-audio")
        os.write(fd, data)
        os.lseek(fd, 0, os.SEEK_SET)
        return AudioBuffer(fd=fd, source=f"fd://{fd}")

    temp_file = tempfile.TemporaryFile()
    temp_file.write(data)
    temp_file.flush()
    temp_file.seek(0)
    return AudioBuffer(fd=temp_file.fileno(), source=f"fd://{temp_file.fileno()}", backing=temp_file)


def recv_artdmx(
    sock: socket.socket,
    universe_filter: Optional[int],
    timeout_s: float,
) -> Optional[Tuple[int, bytes, int]]:
    sock.settimeout(timeout_s)
    try:
        packet, _addr = sock.recvfrom(1024)
    except socket.timeout:
        return None
    parsed = parse_artdmx(packet)
    if not parsed:
        return None
    universe, data = parsed
    if universe_filter is not None and universe != universe_filter:
        return None
    return universe, data, time.monotonic_ns()


def capture_baseline(
    sock: socket.socket,
    universe_filter: Optional[int],
    frames: int,
    timeout_s: float,
) -> Optional[bytes]:
    baseline = None
    remaining = frames
    deadline = time.monotonic() + timeout_s
    while remaining > 0:
        if time.monotonic() > deadline:
            return baseline
        result = recv_artdmx(sock, universe_filter, timeout_s=0.5)
        if result is None:
            continue
        _universe, data, _ts = result
        baseline = data
        remaining -= 1
    return baseline


def max_delta(a: bytes, b: bytes) -> int:
    if not a or not b:
        return 0
    max_diff = 0
    for av, bv in zip(a, b):
        diff = abs(av - bv)
        if diff > max_diff:
            max_diff = diff
    return max_diff


def wait_for_change(
    sock: socket.socket,
    universe_filter: Optional[int],
    baseline: bytes,
    window: Optional[ChannelWindow],
    min_delta: int,
    timeout_s: float,
) -> Optional[Tuple[int, bytes, int, int]]:
    deadline = time.monotonic() + timeout_s
    while time.monotonic() < deadline:
        result = recv_artdmx(sock, universe_filter, timeout_s=0.5)
        if result is None:
            continue
        universe, data, ts = result
        baseline_window = window.slice(baseline) if window else baseline
        data_window = window.slice(data) if window else data
        delta = max_delta(baseline_window, data_window)
        if delta >= min_delta:
            return universe, data, ts, delta
    return None


def format_ms(ns: int) -> str:
    return f"{ns / 1_000_000:.2f} ms"


def parse_args(argv: Iterable[str]) -> argparse.Namespace:
    parser = argparse.ArgumentParser(
        description="Play audio and measure time until Art-Net DMX changes.",
    )
    parser.add_argument("audio", help="Path to audio file to play.")
    parser.add_argument("--listen-host", default="0.0.0.0")
    parser.add_argument("--listen-port", type=int, default=ARTNET_PORT)
    parser.add_argument("--universe", type=int, default=0)
    parser.add_argument("--channel", type=int, default=None, help="1-based channel start.")
    parser.add_argument(
        "--channel-count",
        type=int,
        default=None,
        help="Number of channels to monitor (default 1 when --channel is set).",
    )
    parser.add_argument("--baseline-frames", type=int, default=3)
    parser.add_argument("--baseline-timeout", type=float, default=5.0)
    parser.add_argument("--timeout", type=float, default=8.0)
    parser.add_argument("--min-delta", type=int, default=1)
    parser.add_argument("--runs", type=int, default=1)
    parser.add_argument("--cooldown-ms", type=int, default=500)
    parser.add_argument(
        "--no-stop",
        action="store_true",
        help="Do not stop playback after detection.",
    )
    parser.add_argument("--verbose", action="store_true")
    return parser.parse_args(list(argv))


def main(argv: Iterable[str]) -> int:
    args = parse_args(argv)

    if args.channel is not None:
        if args.channel < 1:
            print("--channel must be 1-based", file=sys.stderr)
            return 2
        count = args.channel_count if args.channel_count is not None else 1
        if count < 1:
            print("--channel-count must be >= 1", file=sys.stderr)
            return 2
        window = ChannelWindow(start=args.channel - 1, count=count)
    else:
        window = None

    audio_buffer = load_audio_buffer(args.audio)
    player = mpv.MPV(ytdl=False, video=False)
    player.pause = True

    sock = socket.socket(socket.AF_INET, socket.SOCK_DGRAM)
    sock.setsockopt(socket.SOL_SOCKET, socket.SO_REUSEADDR, 1)
    sock.bind((args.listen_host, args.listen_port))

    latencies = []

    for run in range(1, args.runs + 1):
        if args.verbose:
            print(f"Run {run}/{args.runs}: waiting for baseline Art-Net frames...")
        baseline = capture_baseline(
            sock,
            universe_filter=args.universe,
            frames=max(args.baseline_frames, 1),
            timeout_s=args.baseline_timeout,
        )
        if baseline is None:
            print("No Art-Net DMX received during baseline window.", file=sys.stderr)
            return 1

        audio_buffer.rewind()
        if args.verbose:
            print(f"Run {run}: starting playback via libmpv")
        start_ns = time.monotonic_ns()
        player.command("loadfile", audio_buffer.source, "replace")
        player.pause = False

        change = wait_for_change(
            sock,
            universe_filter=args.universe,
            baseline=baseline,
            window=window,
            min_delta=max(args.min_delta, 1),
            timeout_s=args.timeout,
        )
        if change is None:
            print("Timed out waiting for DMX change.", file=sys.stderr)
            if not args.no_stop:
                player.command("stop")
            return 1

        _universe, _data, change_ns, delta = change
        latency_ns = change_ns - start_ns
        latencies.append(latency_ns)

        print(
            f"Run {run}: change detected (delta {delta}); latency {format_ms(latency_ns)}",
        )

        if not args.no_stop:
            player.command("stop")

        if run < args.runs and args.cooldown_ms > 0:
            time.sleep(args.cooldown_ms / 1000.0)

    if len(latencies) > 1:
        avg_ns = sum(latencies) // len(latencies)
        min_ns = min(latencies)
        max_ns = max(latencies)
        print(
            "Summary: "
            f"avg {format_ms(avg_ns)}, min {format_ms(min_ns)}, max {format_ms(max_ns)}"
        )

    return 0


if __name__ == "__main__":
    raise SystemExit(main(sys.argv[1:]))
