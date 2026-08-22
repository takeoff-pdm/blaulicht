#!/usr/bin/env python3
"""blctl — control/inspect a running blaulicht engine via the inspector plugin.

The inspector plugin (crates/plugins/inspector) listens on 127.0.0.1:9099.
Requests are single JSON datagrams; replies come back chunked as
{"id", "seq", "total", "data"} frames whose concatenated `data` is the
reply JSON.

Examples:
  blctl.py ping
  blctl.py audio
  blctl.py audio --watch
  blctl.py beat --watch
  blctl.py state scenes.1.sink.master_speed
  blctl.py event '{"SetSceneFocus": 1}'
  blctl.py override 0 1 255
  blctl.py clear 0 1
  blctl.py tap
"""

import argparse
import json
import socket
import sys
import time

INSPECTOR_ADDR = ("127.0.0.1", 9099)
TIMEOUT_S = 3.0


class Client:
    def __init__(self):
        self.sock = socket.socket(socket.AF_INET, socket.SOCK_DGRAM)
        self.sock.bind(("127.0.0.1", 0))  # known local addr so replies reach us
        self.sock.settimeout(TIMEOUT_S)
        self.next_id = 1

    def request(self, cmd: str, **fields):
        request_id = self.next_id
        self.next_id += 1
        payload = {"id": request_id, "cmd": cmd, **fields}
        self.sock.sendto(json.dumps(payload).encode(), INSPECTOR_ADDR)

        chunks = {}
        total = None
        deadline = time.monotonic() + TIMEOUT_S
        while time.monotonic() < deadline:
            try:
                data, _ = self.sock.recvfrom(65535)
            except socket.timeout:
                break
            frame = json.loads(data.decode())
            if frame.get("tap"):
                continue  # stray tap frame while a request is in flight
            if frame.get("id") != request_id:
                continue
            chunks[frame["seq"]] = frame["data"]
            total = frame["total"]
            if len(chunks) == total:
                body = "".join(chunks[i] for i in range(total))
                return json.loads(body)
        raise TimeoutError(
            f"no complete reply for '{cmd}' "
            f"({len(chunks)}/{total if total is not None else '?'} chunks) — "
            "is the app running with the inspector plugin enabled?"
        )

    def stream_taps(self):
        """Print tap frames forever (Ctrl+C to stop)."""
        self.sock.settimeout(None)
        while True:
            data, _ = self.sock.recvfrom(65535)
            frame = json.loads(data.decode())
            print(json.dumps(frame), flush=True)


def emit(value):
    print(json.dumps(value, indent=2))


def main():
    parser = argparse.ArgumentParser(description=__doc__.split("\n")[0])
    parser.add_argument(
        "--watch",
        action="store_true",
        help="repeat the command every 0.5s (audio/beat)",
    )
    sub = parser.add_subparsers(dest="command", required=True)
    sub.add_parser("ping")
    sub.add_parser("audio")
    sub.add_parser("beat")
    state = sub.add_parser("state")
    state.add_argument("path", nargs="?", default="")
    event = sub.add_parser("event")
    event.add_argument("body", help="ControlEvent as JSON, e.g. '{\"SetSceneFocus\": 1}'")
    override = sub.add_parser("override")
    override.add_argument("universe", type=int)
    override.add_argument("channel", type=int)
    override.add_argument("value", type=int)
    clear = sub.add_parser("clear")
    clear.add_argument("universe", type=int)
    clear.add_argument("channel", type=int)
    sub.add_parser("tap", help="stream all ControlEvents on the bus")

    args = parser.parse_args()
    client = Client()

    try:
        if args.command == "tap":
            client.request("tap", on=True)
            print("# tapping control events — Ctrl+C to stop", file=sys.stderr)
            try:
                client.stream_taps()
            except KeyboardInterrupt:
                client.sock.settimeout(TIMEOUT_S)
                client.request("tap", on=False)
            return

        while True:
            if args.command == "ping":
                emit(client.request("ping"))
            elif args.command == "audio":
                reply = client.request("audio")
                emit(reply["current"] if args.watch else reply)
            elif args.command == "beat":
                emit(client.request("beat"))
            elif args.command == "state":
                emit(client.request("state", path=args.path))
            elif args.command == "event":
                emit(client.request("event", body=json.loads(args.body)))
            elif args.command == "override":
                emit(
                    client.request(
                        "override",
                        universe=args.universe,
                        channel=args.channel,
                        value=args.value,
                    )
                )
            elif args.command == "clear":
                emit(client.request("clear", universe=args.universe, channel=args.channel))

            if not args.watch:
                break
            time.sleep(0.5)
    except TimeoutError as e:
        print(f"error: {e}", file=sys.stderr)
        sys.exit(1)
    except KeyboardInterrupt:
        pass


if __name__ == "__main__":
    main()
