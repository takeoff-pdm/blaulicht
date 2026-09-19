#!/usr/bin/env bash
# Headless X display for UI verification, viewable live over VNC.
#
#   scripts/headless_x.sh start    # Xvfb on :9 + x11vnc on localhost:5909
#   scripts/headless_x.sh stop
#   scripts/headless_x.sh status
#
# Then run the app with DISPLAY=:9 (x11 feature) and watch it with
#   vncviewer localhost:5909
# The VNC session accepts input, so you can click inside it as well.
set -euo pipefail

DISPLAY_NUM="${DISPLAY_NUM:-9}"
GEOMETRY="${GEOMETRY:-1600x1000}"
VNC_PORT="${VNC_PORT:-5909}"
LOG_DIR="${LOG_DIR:-/tmp/blaulicht-headless-x}"
mkdir -p "$LOG_DIR"

find_tool() {
    # $1 = binary name, $2 = nixpkgs attribute
    if command -v "$1" >/dev/null 2>&1; then
        command -v "$1"
        return
    fi
    local out
    out="$(nix build "nixpkgs#$2" --no-link --print-out-paths 2>/dev/null)" || {
        echo "error: $1 not installed and nix build nixpkgs#$2 failed" >&2
        exit 1
    }
    echo "$out/bin/$1"
}

xvfb_running() { pgrep -f "Xvfb :$DISPLAY_NUM( |$)" >/dev/null; }
vnc_running() { pgrep -f "x11vnc .*-rfbport $VNC_PORT" >/dev/null; }

case "${1:-status}" in
start)
    X11VNC="$(find_tool x11vnc x11vnc)"
    if ! xvfb_running; then
        setsid nohup Xvfb ":$DISPLAY_NUM" -screen 0 "${GEOMETRY}x24" -nolisten tcp \
            >"$LOG_DIR/xvfb.log" 2>&1 < /dev/null &
        for _ in $(seq 1 50); do
            [ -e "/tmp/.X11-unix/X$DISPLAY_NUM" ] && break
            sleep 0.1
        done
    fi
    if ! vnc_running; then
        # x11vnc refuses to start when it sees a Wayland session; it only
        # needs the Xvfb display, so hide the session variables from it.
        setsid nohup env -u WAYLAND_DISPLAY -u XDG_SESSION_TYPE \
            "$X11VNC" -display ":$DISPLAY_NUM" -localhost -nopw -forever -shared \
            -rfbport "$VNC_PORT" -noxdamage -o "$LOG_DIR/x11vnc.log" \
            >/dev/null 2>&1 < /dev/null &
        sleep 1
    fi
    echo "display  DISPLAY=:$DISPLAY_NUM ($GEOMETRY)"
    echo "watch    vncviewer localhost:$VNC_PORT"
    echo "xdotool  $(find_tool xdotool xdotool)"
    ;;
stop)
    pkill -f "x11vnc .*-rfbport $VNC_PORT" || true
    pkill -f "Xvfb :$DISPLAY_NUM( |$)" || true
    echo "stopped"
    ;;
status)
    xvfb_running && echo "Xvfb :$DISPLAY_NUM running" || echo "Xvfb :$DISPLAY_NUM not running"
    vnc_running && echo "x11vnc on localhost:$VNC_PORT running" || echo "x11vnc not running"
    ;;
*)
    echo "usage: $0 start|stop|status" >&2
    exit 2
    ;;
esac
