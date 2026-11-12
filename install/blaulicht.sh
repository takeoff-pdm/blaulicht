#!/bin/bash
set -euo pipefail

# Options (passed on to blaulicht-tune, see install/tune.sh):
#   --experimental-dmx-tuning  tune the FTDI DMX interfaces at start.
#   --experimental-no-smt      turn Hyper-Threading off.
#   --experimental-no-avahi    stop mDNS (avahi-daemon).
TUNE_ARGS=()
for arg in "$@"; do
    case "$arg" in
        --experimental-dmx-tuning | --experimental-no-smt | --experimental-no-avahi) TUNE_ARGS+=("$arg") ;;
        *) printf '[blaulicht.sh] unknown argument: %s\n' "$arg" >&2; exit 2 ;;
    esac
done

log() {
    printf '[blaulicht.sh] %s\n' "$*"
}

setup_xserver_basics() {
    # Disable screensaver and power management
    xset s off || true
    xset -dpms || true
    xset s noblank || true

    log "X server setup complete."
}

apply_low_latency_tuning() {
    # Runtime-only host tuning (CPU clock, preemption, USB autosuspend, swappiness).
    # Needs the NOPASSWD sudoers entry from install.sh; never blocks the start.
    sudo -n /usr/bin/blaulicht-tune "${TUNE_ARGS[@]}" || log "blaulicht-tune failed or not permitted, continuing untuned."
}

configure_monitors_and_get_blaulicht_flags() {
    local script_dir
    local watcher_json
    local width
    local height

    script_dir="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"

    BLAULICHT_FLAGS=()

    if ! watcher_json="$(bash "${script_dir}/monitor_watcher.sh" apply)"; then
        log "monitor_watcher.sh apply failed, launching without external screen flags."
        return
    fi


    if [[ "$(echo "${watcher_json}" | jq -r '.external_should_exist')" == "true" ]]; then
        width="$(echo "${watcher_json}" | jq -r '.external_output.width')"
        height="$(echo "${watcher_json}" | jq -r '.external_output.height')"

        if [[ "${width}" != "null" && "${height}" != "null" ]]; then
            BLAULICHT_FLAGS=(-e "${width},${height}")
            log "Using external screen size: ${width},${height}"
            return
        fi
    fi

    if [[ "$(echo "${watcher_json}" | jq -r '.external_connected')" == "true" ]]; then
        log "External monitor present, but watcher output did not contain usable geometry."
    else
        log "Single monitor detected, launching without -e."
    fi
}

launch_rescue_session() {
    if tmux has-session -t rescue 2>/dev/null; then
        log "Rescue tmux session already exists, attaching."
        xterm -e 'tmux attach -t rescue' &
    else
        log "Starting rescue tmux session."
        xterm -e 'tmux new-session -s rescue "rescue.sh; exec bash"' &
    fi
}

#
# Main start order
#

setup_xserver_basics
apply_low_latency_tuning

while true; do
    configure_monitors_and_get_blaulicht_flags

    log "Launching blaulicht ${BLAULICHT_FLAGS[*]:-<no flags>}"
    set +e
    # Hold a logind inhibitor while running: closing the lid or idle doesn't suspend.
    systemd-inhibit --what=handle-lid-switch:sleep:idle --who=blaulicht \
        --why="Show running" --mode=block blaulicht "${BLAULICHT_FLAGS[@]}"
    exit_code=$?
    set -e

    log "blaulicht exited with code ${exit_code}"

    if [[ "${exit_code}" -eq 42 ]]; then
        log "Restart requested. Relaunching."
        continue
    fi

    log "Quit requested. Dropping into rescue session."
    launch_rescue_session
    break
done
