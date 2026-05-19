#!/bin/bash
set -euo pipefail

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

while true; do
    configure_monitors_and_get_blaulicht_flags

    log "Launching blaulicht ${BLAULICHT_FLAGS[*]:-<no flags>}"
    set +e
    blaulicht "${BLAULICHT_FLAGS[@]}"
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
