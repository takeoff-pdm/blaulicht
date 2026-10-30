#!/usr/bin/env python3
"""blenv — run and drive blaulicht-core in a headless X Docker container.

One container per git worktree (bind-mounted at /work), with Xvfb on :9, VNC for
watching, and xdotool/ImageMagick for input and screenshots. Usable as a CLI
(`scripts/blenv ...`) and as a library (tools/blenv/mcp_server.py imports it).

Typical loop:
  blenv up && blenv build && blenv run
  blenv screenshot            # -> .blenv/shots/<ts>.png, read it
  blenv click 120 40          # window-relative coordinates
  blenv nav Audio
  blenv blctl audio
  blenv stop                  # or: blenv down [--volumes]
"""

import argparse
import hashlib
import json
import os
import re
import shlex
import subprocess
import sys
import time
from dataclasses import dataclass
from pathlib import Path
from typing import List, Optional, Sequence, Tuple

TOOLS_DIR = Path(__file__).resolve().parent
CARGO_VOLUME = "blenv-cargo"
LABEL = "blenv"
MAIN_WINDOW = "blaulicht"
DEFAULT_SETTLE_MS = 400
APP_PAGES = (
    "Logs System Audio FixturesSetup View ViewPerformance FixturesPerformance "
    "Animations Palettes SceneGraph Visualizer"
).split()


class BlenvError(RuntimeError):
    pass


@dataclass
class Instance:
    root: Path

    @property
    def id(self) -> str:
        digest = hashlib.sha1(str(self.root).encode()).hexdigest()[:8]
        base = re.sub(r"[^a-zA-Z0-9_.-]", "-", self.root.name).lower()
        return f"blenv-{base}-{digest}"

    @property
    def container(self) -> str:
        return self.id

    @property
    def target_volume(self) -> str:
        return f"{self.id}-target"

    @property
    def state_volume(self) -> str:
        return f"{self.id}-state"

    @property
    def shots_dir(self) -> Path:
        return self.root / ".blenv" / "shots"


def resolve_instance(root: Optional[str] = None) -> Instance:
    """Instance for BLENV_ROOT, an explicit root, or the git worktree around cwd."""
    root = root or os.environ.get("BLENV_ROOT")
    if root is None:
        proc = subprocess.run(
            ["git", "rev-parse", "--show-toplevel"], capture_output=True, text=True
        )
        if proc.returncode != 0:
            raise BlenvError("not inside a git worktree; set BLENV_ROOT")
        root = proc.stdout.strip()
    path = Path(root).resolve()
    if not (path / "crates" / "core" / "Cargo.toml").is_file():
        raise BlenvError(f"{path} does not look like a blaulicht v3 checkout")
    return Instance(path)


def image_tag(inst: Instance) -> str:
    dockerfile = inst.root / "tools" / "blenv" / "Dockerfile"
    digest = hashlib.sha256(dockerfile.read_bytes()).hexdigest()[:12]
    return f"blenv:{digest}"


#
# Docker plumbing.
#


def docker(*args: str, check: bool = True, capture: bool = True, data: Optional[bytes] = None):
    proc = subprocess.run(
        ["docker", *args],
        input=data,
        stdout=subprocess.PIPE if capture else None,
        stderr=subprocess.PIPE if capture else None,
    )
    if check and proc.returncode != 0:
        err = (proc.stderr or b"").decode(errors="replace").strip()
        raise BlenvError(f"docker {' '.join(args[:2])} failed: {err}")
    return proc


def container_state(inst: Instance) -> Optional[dict]:
    proc = docker("inspect", inst.container, check=False)
    if proc.returncode != 0:
        return None
    return json.loads(proc.stdout)[0]


def require_running(inst: Instance) -> None:
    state = container_state(inst)
    if state is None or not state["State"]["Running"]:
        raise BlenvError(f"container {inst.container} is not running; run 'blenv up'")


def cexec(
    inst: Instance,
    cmd: Sequence[str],
    check: bool = True,
    capture: bool = True,
    tty: bool = False,
    data: Optional[bytes] = None,
) -> subprocess.CompletedProcess:
    """Run a command inside the container, in /work."""
    flags = ["-i"] if (tty or data is not None) else []
    if tty:
        flags.append("-t")
    proc = subprocess.run(
        ["docker", "exec", *flags, "-w", "/work", inst.container, *cmd],
        input=data,
        stdout=subprocess.PIPE if capture else None,
        stderr=subprocess.PIPE if capture else None,
    )
    if check and proc.returncode != 0:
        detail = ""
        if capture:
            detail = (proc.stderr or b"").decode(errors="replace").strip() or (
                proc.stdout or b""
            ).decode(errors="replace").strip()
        raise BlenvError(f"{shlex.join(cmd)} failed (exit {proc.returncode}): {detail}")
    return proc


def cexec_text(inst: Instance, cmd: Sequence[str], check: bool = True) -> str:
    return cexec(inst, cmd, check=check).stdout.decode(errors="replace")


#
# Lifecycle.
#


def ensure_image(inst: Instance, log=print) -> str:
    tag = image_tag(inst)
    if docker("image", "inspect", tag, check=False).returncode != 0:
        log(f"building image {tag} (first time takes a few minutes)...")
        proc = subprocess.run(
            ["docker", "build", "-t", tag, "-f", str(inst.root / "tools/blenv/Dockerfile"),
             str(inst.root / "tools/blenv")]
        )
        if proc.returncode != 0:
            raise BlenvError("docker build failed")
    return tag


def up(inst: Instance, log=print) -> str:
    tag = ensure_image(inst, log)
    state = container_state(inst)
    if state is not None and state["Config"]["Image"] != tag:
        log("image changed; recreating container (volumes are kept)")
        docker("rm", "-f", inst.container)
        state = None
    if state is None:
        docker(
            "run", "-d", "--init",
            "--name", inst.container,
            "--hostname", "blenv",
            "--label", f"{LABEL}=1",
            "--label", f"{LABEL}.root={inst.root}",
            "-v", f"{inst.root}:/work",
            "-v", f"{CARGO_VOLUME}:/cargo",
            "-v", f"{inst.target_volume}:/build",
            "-v", f"{inst.state_volume}:/state",
            "-p", "127.0.0.1::5900",
            tag,
        )
    elif not state["State"]["Running"]:
        docker("start", inst.container)
    for _ in range(100):
        if cexec(inst, ["test", "-S", "/tmp/.X11-unix/X9"], check=False).returncode == 0:
            break
        time.sleep(0.1)
    else:
        logs = docker("logs", inst.container, check=False).stdout.decode(errors="replace")
        raise BlenvError(f"display :9 did not come up:\n{logs}")
    msg = f"{inst.container} up (display :9, {vnc_url(inst)})"
    log(msg)
    return msg


def down(inst: Instance, volumes: bool = False, log=print) -> str:
    docker("rm", "-f", inst.container, check=False)
    removed = [inst.container]
    if volumes:
        for vol in (inst.target_volume, inst.state_volume):
            if docker("volume", "rm", vol, check=False).returncode == 0:
                removed.append(vol)
    msg = "removed " + ", ".join(removed)
    log(msg)
    return msg


def build(inst: Instance, what: str = "all", plugins: Sequence[str] = (), stream: bool = True) -> Tuple[int, str]:
    """Build inside the container. Streams output when `stream`, else returns its tail."""
    require_running(inst)
    forwarded = [f"{k}={os.environ[k]}" for k in ("BLENV_FEATURES", "CARGO_BUILD_JOBS")
                 if k in os.environ]
    cmd = ["env", *forwarded, "tools/blenv/container/build.sh", what, *plugins]
    if stream:
        proc = cexec(inst, cmd, check=False, capture=False)
        return proc.returncode, ""
    proc = subprocess.run(
        ["docker", "exec", "-w", "/work", inst.container, *cmd],
        stdout=subprocess.PIPE, stderr=subprocess.STDOUT,
    )
    out = proc.stdout.decode(errors="replace")
    return proc.returncode, "\n".join(out.splitlines()[-60:])


def _container_showfile(inst: Instance, showfile: str) -> str:
    """Copy a host showfile into /state so the app never writes to the original."""
    src = Path(showfile).expanduser().resolve()
    if not src.is_file():
        raise BlenvError(f"showfile not found: {src}")
    cexec(inst, ["mkdir", "-p", "/state/showfiles"])
    dest = f"/state/showfiles/{src.name}"
    docker("cp", str(src), f"{inst.container}:{dest}")
    return dest


def run(
    inst: Instance,
    desktop: bool = False,
    showfile: Optional[str] = None,
    external_screens: Sequence[str] = (),
    keep_config: bool = False,
) -> str:
    require_running(inst)
    args: List[str] = []
    if desktop:
        args.append("--desktop")
    if showfile:
        args += ["--showfile", _container_showfile(inst, showfile)]
    if keep_config:
        args.append("--keep-config")
    for screen in external_screens:
        args += ["-e", screen]
    cexec(inst, ["sh", "-c", "cat > /state/run-args.json"], data=json.dumps(args).encode())
    return _run_app(inst, args)


def restart(inst: Instance) -> str:
    require_running(inst)
    proc = cexec(inst, ["cat", "/state/run-args.json"], check=False)
    args = json.loads(proc.stdout) if proc.returncode == 0 else []
    return _run_app(inst, args)


def _run_app(inst: Instance, args: Sequence[str]) -> str:
    proc = subprocess.run(
        ["docker", "exec", "-w", "/work", inst.container, "tools/blenv/container/run-app.sh", *args],
        stdout=subprocess.PIPE, stderr=subprocess.STDOUT,
    )
    out = proc.stdout.decode(errors="replace").strip()
    if proc.returncode != 0:
        raise BlenvError(out)
    return out


def stop(inst: Instance) -> str:
    require_running(inst)
    return cexec_text(inst, ["tools/blenv/container/run-app.sh", "--stop"]).strip()


def status(inst: Instance) -> dict:
    state = container_state(inst)
    info = {
        "instance": inst.id,
        "root": str(inst.root),
        "container": "absent" if state is None else state["State"]["Status"],
    }
    if state is None or not state["State"]["Running"]:
        return info
    pid = cexec_text(inst, ["sh", "-c", "cat /state/app.pid 2>/dev/null"], check=False).strip()
    alive = bool(pid) and cexec(inst, ["kill", "-0", pid], check=False).returncode == 0
    info["app_pid"] = int(pid) if alive else None
    if alive:
        ping = cexec(inst, ["python3", "scripts/blctl.py", "ping"], check=False)
        info["ping"] = (
            json.loads(ping.stdout) if ping.returncode == 0 else "no reply"
        )
    info["vnc"] = vnc_url(inst)
    return info


def vnc_url(inst: Instance) -> str:
    port = docker("port", inst.container, "5900", check=False).stdout.decode().strip()
    if not port:
        return "vnc: not published"
    port = port.splitlines()[0].rsplit(":", 1)[-1]
    password = cexec_text(inst, ["cat", "/state/vncpass.txt"], check=False).strip()
    return f"vnc://:{password}@127.0.0.1:{port}"


def list_instances() -> List[dict]:
    proc = docker(
        "ps", "-a", "--filter", f"label={LABEL}=1",
        "--format", '{{.Names}}\t{{.Status}}\t{{.Label "blenv.root"}}',
    )
    rows = []
    for line in proc.stdout.decode().splitlines():
        name, status_, root = (line.split("\t") + ["", ""])[:3]
        rows.append({"name": name, "status": status_, "root": root,
                     "root_exists": bool(root) and Path(root).is_dir()})
    return rows


def gc(remove: bool = False, log=print) -> List[str]:
    """Stopped instances and instances whose worktree is gone, with their volumes."""
    victims = [
        r for r in list_instances()
        if not r["root_exists"] or not r["status"].startswith("Up")
    ]
    names = []
    for row in victims:
        names.append(row["name"])
        log(f"{'removing' if remove else 'would remove'} {row['name']} ({row['root']})")
        if remove:
            docker("rm", "-f", row["name"], check=False)
            if not row["root_exists"]:
                for suffix in ("-target", "-state"):
                    docker("volume", "rm", row["name"] + suffix, check=False)
    if not victims:
        log("nothing to clean up")
    elif not remove:
        log("re-run with --yes to remove (volumes are only removed for deleted worktrees)")
    return names


#
# Screen and input.
#


def window_id(inst: Instance, name: str = MAIN_WINDOW) -> str:
    out = cexec_text(inst, ["xdotool", "search", "--name", f"^{re.escape(name)}$"], check=False)
    ids = out.split()
    if not ids:
        raise BlenvError(f"no window named {name!r}; is the app running? (blenv windows)")
    return ids[0]


def window_geometry(inst: Instance, wid: str) -> Tuple[int, int, int, int]:
    out = cexec_text(inst, ["xdotool", "getwindowgeometry", "--shell", wid])
    vals = dict(line.split("=", 1) for line in out.split() if "=" in line)
    return int(vals["X"]), int(vals["Y"]), int(vals["WIDTH"]), int(vals["HEIGHT"])


def windows(inst: Instance) -> List[dict]:
    require_running(inst)
    out = cexec_text(inst, ["xdotool", "search", "--onlyvisible", "--name", "."], check=False)
    result = []
    for wid in out.split():
        name = cexec_text(inst, ["xdotool", "getwindowname", wid], check=False).strip()
        try:
            x, y, w, h = window_geometry(inst, wid)
        except BlenvError:
            continue
        result.append({"id": wid, "name": name, "x": x, "y": y, "width": w, "height": h})
    return result


def screenshot(
    inst: Instance,
    out: Optional[str] = None,
    window: Optional[str] = MAIN_WINDOW,
    crop: Optional[str] = None,
    max_width: Optional[int] = None,
) -> Path:
    """Capture a window (default: main) or the whole screen (window=None) as PNG.

    `max_width` shrinks wide captures; click coordinates then need scaling back up.
    """
    require_running(inst)
    target = "root" if window is None else raise_window(inst, window)
    # import blocks forever on some vanishing windows; never let that hang the caller.
    cmd = ["timeout", "20", "import", "-display", ":9", "-window", target]
    if crop:
        cmd += ["-crop", crop, "+repage"]
    if max_width:
        cmd += ["-resize", f"{max_width}x>"]
    png = cexec(inst, [*cmd, "png:-"]).stdout
    if out is None:
        inst.shots_dir.mkdir(parents=True, exist_ok=True)
        stamp = time.strftime("%Y%m%d-%H%M%S") + f"-{int(time.time() * 1000) % 1000:03d}"
        path = inst.shots_dir / f"{stamp}.png"
    else:
        path = Path(out).expanduser().resolve()
        path.parent.mkdir(parents=True, exist_ok=True)
    path.write_bytes(png)
    return path


def raise_window(inst: Instance, window: str) -> str:
    """Bring a window to the front so clicks and window captures hit it, not an overlap."""
    wid = window_id(inst, window)
    cexec(inst, ["xdotool", "windowraise", wid])
    return wid


def _abs(inst: Instance, x: int, y: int, window: Optional[str]) -> Tuple[int, int]:
    if window is None:
        return x, y
    wx, wy, _, _ = window_geometry(inst, raise_window(inst, window))
    return wx + x, wy + y


def _xdotool(inst: Instance, args: Sequence[str], settle_ms: int) -> None:
    require_running(inst)
    cexec(inst, ["xdotool", *args])
    time.sleep(settle_ms / 1000)


def click(inst: Instance, x: int, y: int, button: int = 1, double: bool = False,
          window: Optional[str] = MAIN_WINDOW, settle_ms: int = DEFAULT_SETTLE_MS) -> None:
    ax, ay = _abs(inst, x, y, window)
    args = ["mousemove", "--sync", str(ax), str(ay), "click"]
    if double:
        args += ["--repeat", "2", "--delay", "80"]
    _xdotool(inst, [*args, str(button)], settle_ms)


def move(inst: Instance, x: int, y: int, window: Optional[str] = MAIN_WINDOW,
         settle_ms: int = DEFAULT_SETTLE_MS) -> None:
    ax, ay = _abs(inst, x, y, window)
    _xdotool(inst, ["mousemove", "--sync", str(ax), str(ay)], settle_ms)


def drag(inst: Instance, x1: int, y1: int, x2: int, y2: int, steps: int = 12,
         hold_ms: int = 60, button: int = 1, window: Optional[str] = MAIN_WINDOW,
         settle_ms: int = DEFAULT_SETTLE_MS) -> None:
    ax1, ay1 = _abs(inst, x1, y1, window)
    ax2, ay2 = _abs(inst, x2, y2, window)
    steps = max(1, steps)
    step_s = f"{hold_ms / 1000 / steps:.3f}"
    args = ["mousemove", "--sync", str(ax1), str(ay1), "mousedown", str(button),
            "sleep", f"{hold_ms / 1000:.3f}"]
    for i in range(1, steps + 1):
        px = round(ax1 + (ax2 - ax1) * i / steps)
        py = round(ay1 + (ay2 - ay1) * i / steps)
        args += ["mousemove", str(px), str(py), "sleep", step_s]
    args += ["mouseup", str(button)]
    _xdotool(inst, args, settle_ms)


def scroll(inst: Instance, x: int, y: int, direction: str = "down", amount: int = 3,
           window: Optional[str] = MAIN_WINDOW, settle_ms: int = DEFAULT_SETTLE_MS) -> None:
    buttons = {"up": "4", "down": "5", "left": "6", "right": "7"}
    if direction not in buttons:
        raise BlenvError(f"direction must be one of {', '.join(buttons)}")
    ax, ay = _abs(inst, x, y, window)
    _xdotool(inst, ["mousemove", "--sync", str(ax), str(ay), "click", "--repeat",
                    str(max(1, amount)), "--delay", "40", buttons[direction]], settle_ms)


def _focus_args(inst: Instance, window: Optional[str]) -> List[str]:
    return [] if window is None else ["windowfocus", "--sync", raise_window(inst, window)]


def key(inst: Instance, combos: Sequence[str], window: Optional[str] = MAIN_WINDOW,
        settle_ms: int = DEFAULT_SETTLE_MS) -> None:
    """xdotool key names, e.g. Return, Escape, ctrl+s, Tab."""
    _xdotool(inst, [*_focus_args(inst, window), "key", "--delay", "40", *combos], settle_ms)


def type_text(inst: Instance, text: str, window: Optional[str] = MAIN_WINDOW,
              settle_ms: int = DEFAULT_SETTLE_MS) -> None:
    _xdotool(inst, [*_focus_args(inst, window), "type", "--delay", "25", "--", text], settle_ms)


#
# Inspector (blctl) and logs.
#


def blctl(inst: Instance, args: Sequence[str]) -> str:
    require_running(inst)
    proc = cexec(inst, ["python3", "scripts/blctl.py", *args], check=False)
    out = proc.stdout.decode(errors="replace")
    if proc.returncode != 0:
        raise BlenvError((proc.stderr.decode(errors="replace") + out).strip())
    return out


def send_event(inst: Instance, event: dict, settle_ms: int = DEFAULT_SETTLE_MS) -> str:
    out = blctl(inst, ["event", json.dumps(event)])
    time.sleep(settle_ms / 1000)
    return out


def navigate(inst: Instance, page: str, settle_ms: int = DEFAULT_SETTLE_MS) -> str:
    if page not in APP_PAGES:
        raise BlenvError(f"unknown page {page!r}; one of: {' '.join(APP_PAGES)}")
    return send_event(inst, {"MainUi": {"NavigatePage": page}}, settle_ms)


def plugin_ui(inst: Instance, plugin_id: int, open_: bool = True,
              settle_ms: int = DEFAULT_SETTLE_MS) -> str:
    return send_event(
        inst, {"MainUi": {"SetPluginUIOpen": {"plugin_id": plugin_id, "open": open_}}}, settle_ms
    )


def logs(inst: Instance, lines: int = 200, grep: Optional[str] = None) -> str:
    require_running(inst)
    text = cexec_text(inst, ["tail", "-n", str(lines), "/state/app.log"], check=False)
    text = re.sub(r"\x1b\[[0-9;]*m", "", text)
    if grep:
        pattern = re.compile(grep)
        text = "\n".join(l for l in text.splitlines() if pattern.search(l))
    return text


#
# CLI.
#


def _window_arg(args) -> Optional[str]:
    return None if getattr(args, "screen", False) else args.window


def main(argv: Optional[Sequence[str]] = None) -> int:
    parser = argparse.ArgumentParser(
        prog="blenv", description=__doc__.split("\n")[0],
        epilog="Coordinates are relative to the main window unless --window/--screen is given.",
    )
    parser.add_argument("--root", help="worktree to use (default: git toplevel of cwd)")
    sub = parser.add_subparsers(dest="cmd", required=True)

    sub.add_parser("up", help="build image if needed, start container")
    p = sub.add_parser("down", help="remove container")
    p.add_argument("--volumes", action="store_true", help="also remove target/state volumes")
    sub.add_parser("status")
    sub.add_parser("shell", help="interactive shell in the container")

    p = sub.add_parser("build", help="build core and/or plugins in the container")
    p.add_argument("what", nargs="?", default="all", choices=["all", "core", "plugins"])
    p.add_argument("plugins", nargs="*", help="plugin dirs (default: blenv set)")

    p = sub.add_parser("run", help="(re)start the app and wait for the inspector")
    p.add_argument("--desktop", action="store_true", help="--desktop-mode, window 1500x950")
    p.add_argument("--showfile", help="host showfile to load (copied into the container)")
    p.add_argument("-e", "--external-screen", action="append", default=[], metavar="W,H")
    p.add_argument("--keep-config", action="store_true", help="keep /state/config.toml")
    sub.add_parser("restart", help="restart the app with the last run arguments")
    sub.add_parser("stop", help="stop the app (container keeps running)")

    def add_target(p, settle=True):
        g = p.add_mutually_exclusive_group()
        g.add_argument("--window", default=MAIN_WINDOW, help="window title (default: blaulicht)")
        g.add_argument("--screen", action="store_true", help="screen coordinates")
        if settle:
            p.add_argument("--settle", type=int, default=DEFAULT_SETTLE_MS, metavar="MS")

    p = sub.add_parser("screenshot", help="capture PNG, prints its path")
    p.add_argument("-o", "--out")
    p.add_argument("--crop", metavar="WxH+X+Y")
    add_target(p, settle=False)

    p = sub.add_parser("click")
    p.add_argument("x", type=int)
    p.add_argument("y", type=int)
    p.add_argument("--button", type=int, default=1)
    p.add_argument("--double", action="store_true")
    add_target(p)

    p = sub.add_parser("move")
    p.add_argument("x", type=int)
    p.add_argument("y", type=int)
    add_target(p)

    p = sub.add_parser("drag")
    for name in ("x1", "y1", "x2", "y2"):
        p.add_argument(name, type=int)
    p.add_argument("--steps", type=int, default=12)
    p.add_argument("--hold-ms", type=int, default=60)
    p.add_argument("--button", type=int, default=1)
    add_target(p)

    p = sub.add_parser("scroll")
    p.add_argument("x", type=int)
    p.add_argument("y", type=int)
    p.add_argument("direction", choices=["up", "down", "left", "right"])
    p.add_argument("amount", nargs="?", type=int, default=3)
    add_target(p)

    p = sub.add_parser("key", help="xdotool key names, e.g. Return ctrl+s")
    p.add_argument("combos", nargs="+")
    add_target(p)

    p = sub.add_parser("type", help="type text into the focused widget")
    p.add_argument("text")
    add_target(p)

    sub.add_parser("windows", help="list visible X windows")

    p = sub.add_parser("blctl", help="scripts/blctl.py inside the container")
    p.add_argument("args", nargs=argparse.REMAINDER)

    p = sub.add_parser("nav", help="navigate to an app page")
    p.add_argument("page", choices=APP_PAGES)
    p = sub.add_parser("plugin-ui", help="open/close a plugin window")
    p.add_argument("plugin_id", type=int)
    p.add_argument("--close", action="store_true")

    p = sub.add_parser("logs", help="app log (/state/app.log)")
    p.add_argument("-n", "--lines", type=int, default=200)
    p.add_argument("-f", "--follow", action="store_true")
    p.add_argument("--grep")

    p = sub.add_parser("vnc", help="print VNC URL and open it")
    p.add_argument("--no-open", action="store_true")

    sub.add_parser("list", help="all blenv instances")
    p = sub.add_parser("gc", help="remove stopped/orphaned instances")
    p.add_argument("--yes", action="store_true")

    args = parser.parse_args(argv)

    try:
        if args.cmd == "list":
            for row in list_instances():
                gone = "" if row["root_exists"] else "  [worktree missing]"
                print(f"{row['name']}\t{row['status']}\t{row['root']}{gone}")
            return 0
        if args.cmd == "gc":
            gc(remove=args.yes)
            return 0

        inst = resolve_instance(args.root)
        cmd = args.cmd
        if cmd == "up":
            up(inst)
        elif cmd == "down":
            down(inst, volumes=args.volumes)
        elif cmd == "status":
            print(json.dumps(status(inst), indent=2))
        elif cmd == "shell":
            require_running(inst)
            return cexec(inst, ["bash"], check=False, capture=False, tty=True).returncode
        elif cmd == "build":
            code, _ = build(inst, args.what, args.plugins)
            return code
        elif cmd == "run":
            print(run(inst, args.desktop, args.showfile, args.external_screen, args.keep_config))
        elif cmd == "restart":
            print(restart(inst))
        elif cmd == "stop":
            print(stop(inst))
        elif cmd == "screenshot":
            print(screenshot(inst, args.out, _window_arg(args), args.crop))
        elif cmd == "click":
            click(inst, args.x, args.y, args.button, args.double, _window_arg(args), args.settle)
        elif cmd == "move":
            move(inst, args.x, args.y, _window_arg(args), args.settle)
        elif cmd == "drag":
            drag(inst, args.x1, args.y1, args.x2, args.y2, args.steps, args.hold_ms,
                 args.button, _window_arg(args), args.settle)
        elif cmd == "scroll":
            scroll(inst, args.x, args.y, args.direction, args.amount, _window_arg(args), args.settle)
        elif cmd == "key":
            key(inst, args.combos, _window_arg(args), args.settle)
        elif cmd == "type":
            type_text(inst, args.text, _window_arg(args), args.settle)
        elif cmd == "windows":
            for w in windows(inst):
                print(f"{w['id']}\t{w['width']}x{w['height']}+{w['x']}+{w['y']}\t{w['name']}")
        elif cmd == "blctl":
            require_running(inst)
            tty = sys.stdout.isatty() and any(a in ("tap", "--watch") for a in args.args)
            return cexec(inst, ["python3", "scripts/blctl.py", *args.args],
                         check=False, capture=False, tty=tty).returncode
        elif cmd == "nav":
            print(navigate(inst, args.page), end="")
        elif cmd == "plugin-ui":
            print(plugin_ui(inst, args.plugin_id, not args.close), end="")
        elif cmd == "logs":
            if args.follow:
                require_running(inst)
                shell = f"tail -n {args.lines} -F /state/app.log"
                if args.grep:
                    shell += f" | grep --line-buffered -E {shlex.quote(args.grep)}"
                return cexec(inst, ["sh", "-c", shell], check=False, capture=False,
                             tty=sys.stdout.isatty()).returncode
            print(logs(inst, args.lines, args.grep))
        elif cmd == "vnc":
            url = vnc_url(inst)
            print(url)
            if not args.no_open and sys.platform == "darwin":
                subprocess.run(["open", url])
    except BlenvError as e:
        print(f"blenv: {e}", file=sys.stderr)
        return 1
    except KeyboardInterrupt:
        return 130
    return 0


if __name__ == "__main__":
    sys.exit(main())
