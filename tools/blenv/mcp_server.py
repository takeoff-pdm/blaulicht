#!/usr/bin/env -S uv run --script
# /// script
# requires-python = ">=3.11"
# dependencies = ["mcp>=1.9,<2"]
# ///
"""MCP server exposing the blenv headless blaulicht environment as tools.

Runs with `uv run --script tools/blenv/mcp_server.py` (see .mcp.json). The instance is
the git worktree of the working directory, or BLENV_ROOT. All logic lives in blenv.py.
"""

import sys
from pathlib import Path
from typing import Optional

from mcp.server.fastmcp import FastMCP, Image

sys.path.insert(0, str(Path(__file__).resolve().parent))
import blenv  # noqa: E402

mcp = FastMCP(
    "blaulicht-env",
    instructions=(
        "Headless blaulicht-core (egui) in a per-worktree Docker container with Xvfb. "
        "Loop: blenv_up -> blenv_build -> blenv_run -> screenshot -> click/drag/key -> "
        "screenshot, and check state with blctl/logs. Coordinates are pixels relative to the "
        "main window 'blaulicht' (same as the screenshot) unless `window` names another "
        "window (see windows) or is '' for screen coordinates. Never click Quit or Shutdown."
    ),
)


def _inst() -> blenv.Instance:
    return blenv.resolve_instance()


def _win(window: str) -> Optional[str]:
    return window or None


def _shot(inst: blenv.Instance, window: Optional[str] = blenv.MAIN_WINDOW,
          crop: Optional[str] = None, max_width: Optional[int] = None) -> list:
    path = blenv.screenshot(inst, window=window, crop=crop, max_width=max_width)
    return [Image(data=path.read_bytes(), format="png"), f"saved {path}"]


def _after(inst: blenv.Instance, msg: str, screenshot_after: bool, window: Optional[str]):
    if not screenshot_after:
        return msg
    return [msg, *_shot(inst, window if window else None)]


@mcp.tool()
def blenv_up() -> str:
    """Build the image if needed and start this worktree's container (display + VNC)."""
    return blenv.up(_inst(), log=lambda _m: None)


@mcp.tool()
def blenv_down(volumes: bool = False) -> str:
    """Remove the container. volumes=True also deletes its build cache and state."""
    return blenv.down(_inst(), volumes, log=lambda _m: None)


@mcp.tool()
def blenv_build(what: str = "all", plugins: Optional[list[str]] = None) -> str:
    """Build in the container: what = all | core | headless | web | plugins. First build takes minutes.

    core is the desk (blaulicht-core, engine + egui UI); headless is blaulicht-headless
    (engine only, not part of all); web is the browser UI bundle the engine serves on its
    port (crates/web/build.sh into $CARGO_TARGET_DIR/web; not part of all).

    plugins: plugin dirs under crates/plugins (default: inspector + sample/animation set).
    Returns the exit code and the tail of the build output.
    """
    code, tail = blenv.build(_inst(), what, plugins or [], stream=False)
    return f"exit {code}\n{tail}"


@mcp.tool()
def blenv_run(desktop: bool = False, showfile: Optional[str] = None,
              external_screens: Optional[list[str]] = None, keep_config: bool = False,
              headless: bool = False) -> str:
    """(Re)start the app and wait for the inspector ping.

    desktop: --desktop-mode with a 1500x950 window (default is the 800x480 kiosk layout).
    showfile: host path to a showfile JSON; a copy is loaded so the original stays untouched.
    external_screens: e.g. ["1920,1080"] opens windows bl_ext_0, ...
    headless: run blaulicht-headless instead (engine only, no window; build it with
    blenv_build("headless")). blctl and logs work, screenshots and input don't.
    """
    return blenv.run(_inst(), desktop, showfile, external_screens or [], keep_config, headless)


@mcp.tool()
def blenv_stop() -> str:
    """Stop the app; the container keeps running."""
    return blenv.stop(_inst())


@mcp.tool()
def blenv_status() -> dict:
    """Container state, app pid, inspector ping and VNC URL for this worktree."""
    return blenv.status(_inst())


@mcp.tool()
def screenshot(window: str = blenv.MAIN_WINDOW, crop: Optional[str] = None,
               max_width: Optional[int] = None) -> list:
    """Capture a PNG of a window (default main window; '' = whole 1600x1000 screen).

    crop: ImageMagick geometry WxH+X+Y. max_width: downscale (coordinates then scale too).
    """
    return _shot(_inst(), _win(window), crop, max_width)


@mcp.tool()
def click(x: int, y: int, button: int = 1, double: bool = False,
          window: str = blenv.MAIN_WINDOW, screenshot_after: bool = False):
    """Click at window-relative pixel (x, y). button 1 left, 2 middle, 3 right."""
    inst = _inst()
    blenv.click(inst, x, y, button, double, _win(window))
    return _after(inst, f"clicked ({x}, {y})", screenshot_after, _win(window))


@mcp.tool()
def drag(x1: int, y1: int, x2: int, y2: int, steps: int = 12, hold_ms: int = 60,
         window: str = blenv.MAIN_WINDOW, screenshot_after: bool = False):
    """Press at (x1, y1), move in `steps` steps to (x2, y2), release (sliders, canvases)."""
    inst = _inst()
    blenv.drag(inst, x1, y1, x2, y2, steps, hold_ms, 1, _win(window))
    return _after(inst, f"dragged ({x1}, {y1}) -> ({x2}, {y2})", screenshot_after, _win(window))


@mcp.tool()
def scroll(x: int, y: int, direction: str = "down", amount: int = 3,
           window: str = blenv.MAIN_WINDOW, screenshot_after: bool = False):
    """Scroll wheel at (x, y); direction up | down | left | right."""
    inst = _inst()
    blenv.scroll(inst, x, y, direction, amount, _win(window))
    return _after(inst, f"scrolled {direction} x{amount}", screenshot_after, _win(window))


@mcp.tool()
def key(combos: list[str], window: str = blenv.MAIN_WINDOW, screenshot_after: bool = False):
    """Press keys (xdotool names), e.g. ["Return"], ["ctrl+a", "BackSpace"], ["Escape"]."""
    inst = _inst()
    blenv.key(inst, combos, _win(window))
    return _after(inst, f"pressed {' '.join(combos)}", screenshot_after, _win(window))


@mcp.tool()
def type_text(text: str, window: str = blenv.MAIN_WINDOW, screenshot_after: bool = False):
    """Type text into the focused widget (click the text field first)."""
    inst = _inst()
    blenv.type_text(inst, text, _win(window))
    return _after(inst, f"typed {len(text)} chars", screenshot_after, _win(window))


@mcp.tool()
def windows() -> list:
    """Visible X windows: main 'blaulicht', external screens 'bl_ext_N', plugin windows."""
    return blenv.windows(_inst())


@mcp.tool(description="Switch the main UI page via the inspector. Pages: "
           + ", ".join(blenv.APP_PAGES))
def navigate(page: str, screenshot_after: bool = True):
    inst = _inst()
    out = blenv.navigate(inst, page)
    return _after(inst, out, screenshot_after, blenv.MAIN_WINDOW)


@mcp.tool()
def plugin_ui(plugin_id: int, open: bool = True) -> str:
    """Open/close a plugin's UI. Default blenv ids: 0 inspector, 1 julia, 2 audio reactor,
    3 chaos, 4 sample egui."""
    return blenv.plugin_ui(_inst(), plugin_id, open)


@mcp.tool()
def blctl(args: list[str]) -> str:
    """Run scripts/blctl.py in the container, e.g. ["ping"], ["audio"], ["beat"],
    ["state", "scenes.1.sink.master_speed"], ["event", "<ControlEvent JSON>"],
    ["override", "0", "1", "255"], ["clear", "0", "1"]. (No --watch/tap here.)"""
    return blenv.blctl(_inst(), args)


@mcp.tool()
def logs(lines: int = 200, grep: Optional[str] = None) -> str:
    """Tail the app log (ANSI stripped); grep is a Python regex, e.g. 'ERROR|WARN'."""
    return blenv.logs(_inst(), lines, grep)


if __name__ == "__main__":
    mcp.run()
