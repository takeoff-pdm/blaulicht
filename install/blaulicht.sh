#!/bin/bash
set -euo pipefail

log() {
    printf '[blaulicht.sh] %s\n' "$*"
}

setup_xserver() {
    # Disable screensaver and power management
    xset s off || true
    xset -dpms || true
    xset s noblank || true

    # TODO: do something to relays?
    #

    xrandr --newmode "1024x600_60.00" 49.00  1024 1064 1168 1312  600 603 613 624 -hsync +vsync || true
    xrandr --newmode "800x480_60.00" 29.50  800 824 896 992  480 483 493 500 -hsync +vsync || true

    xrandr --addmode VGA-1 "1024x600_60.00" || true
    xrandr --addmode VGA-1 "800x480_60.00" || true

    # Keep the touchscreen panel as the primary display at the bottom,
    # with the HDMI display above it when present.
    xrandr --output HDMI-1 --mode 1920x1080 --pos 0x0 \
           --output VGA-1 --primary --mode "800x480_60.00" --pos 0x1080 || \
    xrandr --output VGA-1 --primary --mode "800x480_60.00" || true

    # The touchscreen should only address the VGA panel, not the combined desktop.
    xinput map-to-output "eGalaxTouch Virtual Device for Single" VGA-1 || true
    xinput set-prop "eGalaxTouch Virtual Device for Single" \
        "Coordinate Transformation Matrix" \
        0.416667 0 0 \
        0 0.307692 0.692308 \
        0 0 1 || true

    log "X server setup complete."
}

# Returns monitor count
determine_monitor_count() {
    # Wait briefly for monitor info to become available.
    for _ in {1..20}; do
    if xrandr --listmonitors >/dev/null 2>&1; then
        break
    fi
    sleep 0.5
    done

    local monitors
    monitors="$(xrandr --listmonitors 2>/dev/null | awk 'NR==1 {print $2}')"
    MONITORS="${monitors:-1}"
    log "Detected monitors: ${MONITORS}"
}

setup_devilspie2_and_get_blaulicht_flags() {
    #
    # Determine Blaulicht launch flags, dependent on monitor count.
    # If we have an external HDMI monitor attached, tell blaulicht that it needs to attach a second screen.
    #
    script_dir="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
    local geom=""

    BLAULICHT_FLAGS=()
    if (( MONITORS >= 2 )); then
        log "External monitor detected, generating devilspie2 config."
        "${script_dir}/generate-devilspie2.sh" --output-name HDMI-1

        geom="$(xrandr --query | awk '$1=="HDMI-1" && $2=="connected" {print $0}' | sed -nE 's/.* ([0-9]+x[0-9]+\+[0-9]+\+[0-9]+).*/\1/p' | head -n1)"
        if [[ -z "${geom}" ]]; then
            geom="$(xrandr --listmonitors | awk '$1=="1:" {print $3}' | head -n1)"
            geom="$(echo "${geom}" | sed -E 's#/[^x+]+##g')"
        fi

        if [[ "${geom}" =~ ^([0-9]+)x([0-9]+)\+([0-9]+)\+([0-9]+)$ ]]; then
            w="${BASH_REMATCH[1]}"
            h="${BASH_REMATCH[2]}"
            BLAULICHT_FLAGS=(-e "${w},${h}")
            log "Using external screen size: ${w},${h}"
        else
            log "Failed to parse monitor geometry '${geom}', launching without -e."
        fi
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

setup_xserver

while true; do
    determine_monitor_count
    setup_devilspie2_and_get_blaulicht_flags

    log "Starting devilspie2."
    pkill -x devilspie2 >/dev/null 2>&1 || true
    devilspie2 &

    log "Launching blaulicht ${BLAULICHT_FLAGS[*]:-<no flags>}"
    set +e
    blaulicht "${BLAULICHT_FLAGS[@]}"
    exit_code=$?
    set -e

    log "blaulicht exited with code ${exit_code}"
    log "Stopping devilspie2."
    pkill -x devilspie2 >/dev/null 2>&1 || true

    if [[ "${exit_code}" -eq 42 ]]; then
        log "Restart requested. Relaunching."
        continue
    fi

    log "Quit requested. Dropping into rescue session."
    launch_rescue_session
    break
done
