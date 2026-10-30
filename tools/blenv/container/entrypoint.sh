#!/usr/bin/env bash
# Container main process: virtual X display :9, openbox, and a VNC server for watching it.
set -euo pipefail

GEOMETRY="${BLENV_GEOMETRY:-1600x1000x24}"
mkdir -p /state

if [[ ! -s /state/vncpass.txt ]]; then
    # VNC passwords are at most 8 characters.
    python3 -c 'import secrets; print(secrets.token_hex(4), end="")' >/state/vncpass.txt
fi
x11vnc -storepasswd "$(cat /state/vncpass.txt)" /state/vncpass >/dev/null 2>&1

rm -f /tmp/.X9-lock /tmp/.X11-unix/X9
# Without GLX, glutin falls back to EGL: Xvfb's GLX visuals have no multisampling, but the
# app asks for 4x MSAA + 24-bit depth, which Mesa's EGL-on-X11 configs do provide.
Xvfb :9 -screen 0 "$GEOMETRY" -nolisten tcp -extension GLX >/state/xvfb.log 2>&1 &
xvfb_pid=$!

for _ in $(seq 50); do
    [[ -S /tmp/.X11-unix/X9 ]] && break
    sleep 0.1
done
[[ -S /tmp/.X11-unix/X9 ]] || { echo "Xvfb did not start" >&2; cat /state/xvfb.log >&2; exit 1; }

openbox --config-file /work/tools/blenv/container/openbox-rc.xml >/state/openbox.log 2>&1 &

x11vnc -display :9 -rfbport 5900 -rfbauth /state/vncpass -forever -shared -noxdamage -quiet \
    >/state/x11vnc.log 2>&1 &

echo "blenv: display :9 ready (${GEOMETRY})"
trap 'kill "$xvfb_pid" 2>/dev/null; exit 0' TERM INT
wait "$xvfb_pid"
