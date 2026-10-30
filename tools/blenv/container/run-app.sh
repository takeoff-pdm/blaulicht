#!/usr/bin/env bash
# Start (or restart) blaulicht-core on display :9 and wait until it is usable.
#   run-app.sh [--desktop] [--showfile PATH] [--keep-config] [-e W,H]...
#   run-app.sh --stop
set -euo pipefail
cd /work

BIN="${CARGO_TARGET_DIR:-/build/target}/debug/blaulicht-core"
CONFIG=/state/config.toml
LOG=/state/app.log
PIDFILE=/state/app.pid
TIMEOUT_S="${BLENV_START_TIMEOUT:-90}"

stop_app() {
    if [[ -f $PIDFILE ]] && kill -0 "$(cat "$PIDFILE")" 2>/dev/null; then
        kill "$(cat "$PIDFILE")" 2>/dev/null || true
        for _ in $(seq 50); do
            kill -0 "$(cat "$PIDFILE")" 2>/dev/null || break
            sleep 0.1
        done
    fi
    pkill -9 -x blaulicht-core 2>/dev/null || true
    rm -f "$PIDFILE"
}

desktop=0 keep_config=0 showfile="" app_args=()
while [[ $# -gt 0 ]]; do
    case "$1" in
        --stop) stop_app; echo "blenv: app stopped"; exit 0 ;;
        --desktop) desktop=1 ;;
        --keep-config) keep_config=1 ;;
        --showfile) showfile="$2"; shift ;;
        -e) app_args+=(-e "$2"); shift ;;
        *) echo "run-app.sh: unknown argument $1" >&2; exit 2 ;;
    esac
    shift
done

[[ -x $BIN ]] || { echo "error: $BIN not built; run 'blenv build' first" >&2; exit 1; }
[[ -f crates/plugins/bundle_debug/inspector.wasm ]] \
    || { echo "error: inspector.wasm missing; run 'blenv build plugins'" >&2; exit 1; }

stop_app

if [[ $keep_config -eq 0 || ! -f $CONFIG ]]; then
    if [[ -n $showfile ]]; then
        line="last_open_showfile = \"$showfile\""
    else
        line=""
    fi
    sed "s|^#@LAST_OPEN_SHOWFILE@\$|$line|" tools/blenv/config.template.toml >"$CONFIG"
fi

[[ $desktop -eq 1 ]] && app_args+=(--desktop-mode)

: >"$LOG"
setsid env RUST_LOG="${RUST_LOG:-info}" "$BIN" -c "$CONFIG" "${app_args[@]}" \
    >>"$LOG" 2>&1 </dev/null &
echo $! >"$PIDFILE"
pid=$(cat "$PIDFILE")

deadline=$((SECONDS + TIMEOUT_S))
until python3 scripts/blctl.py ping >/dev/null 2>&1; do
    if ! kill -0 "$pid" 2>/dev/null; then
        echo "error: blaulicht-core exited during startup; last log lines:" >&2
        tail -n 40 "$LOG" >&2
        exit 1
    fi
    if ((SECONDS > deadline)); then
        echo "error: no inspector ping after ${TIMEOUT_S}s; last log lines:" >&2
        tail -n 40 "$LOG" >&2
        exit 1
    fi
    sleep 0.5
done

wid=$(xdotool search --name '^blaulicht$' | head -n1 || true)
if [[ -n $wid ]]; then
    xdotool windowmove "$wid" 0 0
    [[ $desktop -eq 1 ]] && xdotool windowsize "$wid" "${BLENV_DESKTOP_SIZE_W:-1500}" "${BLENV_DESKTOP_SIZE_H:-950}"
else
    echo "warning: main window 'blaulicht' not found" >&2
fi

echo "blenv: app running (pid $pid, window ${wid:-?})"
python3 scripts/blctl.py ping
