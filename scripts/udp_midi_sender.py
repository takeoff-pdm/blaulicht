#!/usr/bin/env python3
"""
Simulates MIDI-like inputs over UDP for testing the UdpPort plugin subsystem.

Usage:
    python3 scripts/udp_midi_sender.py [host:port]

    Default target: 127.0.0.1:9000

Commands (type into stdin):
    cc <controller> <value>     - Control Change (e.g. "cc 1 127")
    note <note> <velocity>      - Note On (e.g. "note 60 127")
    off <note>                  - Note Off (e.g. "off 60")
    raw <hex bytes>             - Raw bytes (e.g. "raw b0 01 7f")
    fader <id> <value 0-255>    - Shortcut for cc (e.g. "fader 0 200")
    scene <id>                  - Trigger scene (note on, vel 127) (e.g. "scene 5")
    q                           - Quit
"""

import socket
import sys

DEFAULT_TARGET = "127.0.0.1:9000"

MIDI_CC = 0xB0
MIDI_NOTE_ON = 0x90
MIDI_NOTE_OFF = 0x80


def parse_target(arg: str) -> tuple[str, int]:
    if ":" in arg:
        host, port = arg.rsplit(":", 1)
        return host, int(port)
    return "127.0.0.1", int(arg)


def main():
    target = DEFAULT_TARGET
    if len(sys.argv) > 1:
        target = sys.argv[1]

    host, port = parse_target(target)
    sock = socket.socket(socket.AF_INET, socket.SOCK_DGRAM)

    print(f"UDP MIDI sender -> {host}:{port}")
    print("Commands: cc, note, off, raw, fader, scene, q")
    print()

    while True:
        try:
            line = input("> ").strip()
        except (EOFError, KeyboardInterrupt):
            break

        if not line:
            continue

        parts = line.split()
        cmd = parts[0].lower()

        try:
            if cmd == "q":
                break
            elif cmd == "cc" and len(parts) == 3:
                controller = int(parts[1]) & 0x7F
                value = int(parts[2]) & 0x7F
                data = bytes([MIDI_CC, controller, value])
            elif cmd == "note" and len(parts) == 3:
                note = int(parts[1]) & 0x7F
                velocity = int(parts[2]) & 0x7F
                data = bytes([MIDI_NOTE_ON, note, velocity])
            elif cmd == "off" and len(parts) >= 2:
                note = int(parts[1]) & 0x7F
                data = bytes([MIDI_NOTE_OFF, note, 0])
            elif cmd == "raw" and len(parts) > 1:
                data = bytes(int(b, 16) for b in parts[1:])
            elif cmd == "fader" and len(parts) == 3:
                fader_id = int(parts[1]) & 0x7F
                value = int(parts[2]) & 0x7F
                data = bytes([MIDI_CC, fader_id, value])
            elif cmd == "scene" and len(parts) == 2:
                scene_id = int(parts[1]) & 0x7F
                data = bytes([MIDI_NOTE_ON, scene_id, 127])
            else:
                print(f"  Unknown: {line}")
                continue

            sock.sendto(data, (host, port))
            print(f"  sent {len(data)}B: {data.hex(' ')}")

        except (ValueError, IndexError) as e:
            print(f"  Error: {e}")

    sock.close()
    print("bye")


if __name__ == "__main__":
    main()
