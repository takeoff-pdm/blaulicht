#!/usr/bin/env python3
"""Pretend to be a DJ deck for the setlist plugin.

Sends deck messages (see crates/setlist/src/deck.rs) to 127.0.0.1:9120, the
port the setlist plugin listens on in "Decks" mode, so following, cueing and
track changes can be tested without CDJs.

  setlist_deck_sim.py --map out/02-dj-heartstring-in-der-nacht.json --bar 41
  setlist_deck_sim.py --track "In Der Nacht" --position-ms 60000 --pitch 1.04

With --map, positions can be given in bars (1-based, as the analyzer prints
them) and the track name comes from the map. --jump-at T:BAR seeks to BAR
after T seconds (repeatable), like a hot cue.
"""

import argparse
import json
import socket
import sys
import time


def main() -> int:
    p = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    p.add_argument("--map", help="track map JSON written by setlist-analyze")
    p.add_argument("--track", help="track name to send (default: the map's title)")
    p.add_argument("--index", type=int, help="setlist index to send instead of a name")
    p.add_argument("--deck", type=int, default=1)
    p.add_argument("--bar", type=float, help="start at this bar (needs --map)")
    p.add_argument("--position-ms", type=float, default=0.0)
    p.add_argument("--pitch", type=float, default=1.0, help="playback speed factor")
    p.add_argument("--rate", type=float, default=20.0, help="messages per second")
    p.add_argument("--seconds", type=float, default=60.0, help="how long to play")
    p.add_argument("--jump-at", action="append", default=[], metavar="T:BAR",
                   help="after T seconds jump to BAR (needs --map)")
    p.add_argument("--off-air", action="store_true", help="send on_air: false")
    p.add_argument("--port", type=int, default=9120)
    p.add_argument("--print-start", action="store_true",
                   help="print time.monotonic() at the first message (for latency tests)")
    args = p.parse_args()

    track_map = None
    if args.map:
        with open(args.map) as f:
            track_map = json.load(f)
    if args.track is None and args.index is None:
        if track_map is None:
            p.error("give --track, --index or --map")
        args.track = track_map["title"]

    def bar_ms(bar: float) -> float:
        if track_map is None:
            p.error("bars need --map")
        period = 60000.0 / track_map["bpm"]
        beat = track_map["downbeat"] + (bar - 1.0) * 4.0
        return track_map["first_beat_ms"] + beat * period

    position = bar_ms(args.bar) if args.bar is not None else args.position_ms
    jumps = []
    for spec in args.jump_at:
        t, bar = spec.split(":")
        jumps.append((float(t), bar_ms(float(bar))))
    jumps.sort()

    sock = socket.socket(socket.AF_INET, socket.SOCK_DGRAM)
    start = last = time.monotonic()
    if args.print_start:
        print(f"start {start:.6f}", flush=True)
    shown = -1
    while True:
        now = time.monotonic()
        elapsed = now - start
        if elapsed >= args.seconds:
            break
        position += (now - last) * 1000.0 * args.pitch
        last = now
        while jumps and elapsed >= jumps[0][0]:
            position = jumps.pop(0)[1]
            print(f"\njump to {position / 1000:.2f} s", flush=True)
        msg = {
            "deck": args.deck,
            "position_ms": round(position, 1),
            "pitch": args.pitch,
            "playing": True,
            "on_air": not args.off_air,
        }
        if args.index is not None:
            msg["index"] = args.index
        else:
            msg["track"] = args.track
        sock.sendto(json.dumps(msg).encode(), ("127.0.0.1", args.port))
        if int(elapsed) != shown:
            shown = int(elapsed)
            extra = ""
            if track_map is not None:
                period = 60000.0 / track_map["bpm"]
                beat = (position - track_map["first_beat_ms"]) / period
                extra = f"  bar {(beat - track_map['downbeat']) / 4 + 1:6.1f}"
            print(f"\r{elapsed:6.1f} s  pos {position / 1000:7.2f} s{extra}", end="", flush=True)
        time.sleep(1.0 / args.rate)
    print()
    return 0


if __name__ == "__main__":
    sys.exit(main())
