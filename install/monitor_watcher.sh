#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
STATE_FILE="/tmp/blaulicht-monitor-state.json"
OUTPUT_FILE="${HOME}/.config/devilspie2/blaulicht.lua"

VGA_OUTPUT="VGA-1"
HDMI_OUTPUT="HDMI-1"
TOUCH_DEVICE="eGalaxTouch Virtual Device for Single"
VGA_MODE="800x480_60.00"
HDMI_MODE="1920x1080"

usage() {
    cat <<'EOF'
Usage:
  monitor_watcher.sh probe
  monitor_watcher.sh apply
EOF
}

log() {
    printf '[monitor_watcher] %s\n' "$*" >&2
}

require_commands() {
    local subcommand="$1"
    local cmd
    for cmd in jq xrandr; do
        if ! command -v "$cmd" >/dev/null 2>&1; then
            log "Missing required command: $cmd"
            exit 1
        fi
    done

    if [[ "${subcommand}" == "apply" ]]; then
        for cmd in devilspie2 pkill xinput; do
            if ! command -v "$cmd" >/dev/null 2>&1; then
                log "Missing required command: $cmd"
                exit 1
            fi
        done
    fi
}

is_output_connected() {
    local output="$1"
    xrandr --query | awk -v output="$output" '$1 == output {print ($2 == "connected") ? "true" : "false"; exit}'
}

ensure_vga_modes() {
    xrandr --newmode "1024x600_60.00" 49.00 1024 1064 1168 1312 600 603 613 624 -hsync +vsync || true
    xrandr --newmode "${VGA_MODE}" 29.50 800 824 896 992 480 483 493 500 -hsync +vsync || true
    xrandr --addmode "${VGA_OUTPUT}" "1024x600_60.00" || true
    xrandr --addmode "${VGA_OUTPUT}" "${VGA_MODE}" || true
}

desired_signature() {
    local external_connected="$1"
    if [[ "${external_connected}" == "true" ]]; then
        printf 'layout=hdmi_above_vga;main=%s:800x480+0+1080:primary;external=%s:1920x1080+0+0\n' "${VGA_OUTPUT}" "${HDMI_OUTPUT}"
    else
        printf 'layout=vga_only;main=%s:800x480+0+0:primary;external=none\n' "${VGA_OUTPUT}"
    fi
}

last_signature() {
    jq -r '.signature // ""' "${STATE_FILE}" 2>/dev/null || true
}

write_single_screen_fallback() {
    mkdir -p "$(dirname "${OUTPUT_FILE}")"
    cat > "${OUTPUT_FILE}" <<'EOF'
local name = get_window_name() or ""
local ext_prefix = "bl_ext_"

if name:sub(1, #ext_prefix) == ext_prefix then
    debug_print("[DS2] Blaulicht external screen ignored in VGA-only mode")
elseif name == "blaulicht" then
    debug_print("[DS2] Blaulicht main screen")
    maximize()
    undecorate_window()
else
    debug_print("[DS2] unknown window: " .. tostring(name))
end
EOF
}

restart_devilspie2() {
    pkill -x devilspie2 >/dev/null 2>&1 || true
    devilspie2 &
}

apply_touchscreen_dual() {
    xinput map-to-output "${TOUCH_DEVICE}" "${VGA_OUTPUT}" || true
    xinput set-prop "${TOUCH_DEVICE}" \
        "Coordinate Transformation Matrix" \
        0.416667 0 0 \
        0 0.307692 0.692308 \
        0 0 1 || true
}

apply_touchscreen_single() {
    xinput map-to-output "${TOUCH_DEVICE}" "${VGA_OUTPUT}" || true
    xinput set-prop "${TOUCH_DEVICE}" \
        "Coordinate Transformation Matrix" \
        1 0 0 \
        0 1 0 \
        0 0 1 || true
}

emit_state_json() {
    local changed="$1"
    local applied="$2"
    local external_connected="$3"
    local signature="$4"

    if [[ "${external_connected}" == "true" ]]; then
        jq -nc \
            --argjson changed "${changed}" \
            --argjson applied "${applied}" \
            --arg signature "${signature}" \
            '{
                changed: $changed,
                external_connected: true,
                external_should_exist: true,
                applied: $applied,
                layout: "hdmi_above_vga",
                main_output: {
                    name: "VGA-1",
                    width: 800,
                    height: 480,
                    x: 0,
                    y: 1080,
                    primary: true
                },
                external_output: {
                    name: "HDMI-1",
                    width: 1920,
                    height: 1080,
                    x: 0,
                    y: 0
                },
                signature: $signature
            }'
    else
        jq -nc \
            --argjson changed "${changed}" \
            --argjson applied "${applied}" \
            --arg signature "${signature}" \
            '{
                changed: $changed,
                external_connected: false,
                external_should_exist: false,
                applied: $applied,
                layout: "vga_only",
                main_output: {
                    name: "VGA-1",
                    width: 800,
                    height: 480,
                    x: 0,
                    y: 0,
                    primary: true
                },
                external_output: null,
                signature: $signature
            }'
    fi
}

write_state_file() {
    local signature="$1"
    local external_connected="$2"
    local tmpfile

    tmpfile="$(mktemp)"
    emit_state_json false true "${external_connected}" "${signature}" > "${tmpfile}"
    mv "${tmpfile}" "${STATE_FILE}"
}

main() {
    local subcommand="${1:-}"
    local external_connected
    local signature
    local previous_signature
    local changed="false"

    case "${subcommand}" in
        probe|apply)
            ;;
        -h|--help|"")
            usage
            exit 0
            ;;
        *)
            usage >&2
            exit 1
            ;;
    esac

    require_commands "${subcommand}"

    if [[ "$(is_output_connected "${VGA_OUTPUT}")" != "true" ]]; then
        log "Primary output ${VGA_OUTPUT} is not connected."
        exit 1
    fi

    external_connected="$(is_output_connected "${HDMI_OUTPUT}")"
    signature="$(desired_signature "${external_connected}")"
    previous_signature="$(last_signature)"
    if [[ "${signature}" != "${previous_signature}" ]]; then
        changed="true"
    fi

    if [[ "${subcommand}" == "probe" ]]; then
        emit_state_json "${changed}" false "${external_connected}" "${signature}"
        exit 0
    fi

    ensure_vga_modes

    if [[ "${external_connected}" == "true" ]]; then
        xrandr --output "${HDMI_OUTPUT}" --mode "${HDMI_MODE}" --pos 0x0 \
               --output "${VGA_OUTPUT}" --primary --mode "${VGA_MODE}" --pos 0x1080
        apply_touchscreen_dual
        bash "${SCRIPT_DIR}/generate-devilspie2.sh" --output-name "${HDMI_OUTPUT}" --output "${OUTPUT_FILE}" >&2
    else
        xrandr --output "${HDMI_OUTPUT}" --off \
               --output "${VGA_OUTPUT}" --primary --mode "${VGA_MODE}" --pos 0x0
        apply_touchscreen_single
        write_single_screen_fallback
    fi

    restart_devilspie2
    write_state_file "${signature}" "${external_connected}"
    emit_state_json "${changed}" true "${external_connected}" "${signature}"
}

main "$@"
