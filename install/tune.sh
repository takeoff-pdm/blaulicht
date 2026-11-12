#!/usr/bin/env bash
# Low-latency tuning for the show host. Runtime only: everything here is lost on reboot
# and re-applied by blaulicht.sh on every start (via `sudo -n /usr/bin/blaulicht-tune`).
# Each step is best-effort; a missing knob is logged and skipped.
# Usage: blaulicht-tune [--experimental-dmx-tuning] [--experimental-no-smt] [--experimental-no-avahi]
#   --experimental-dmx-tuning  also tune the FTDI DMX interfaces (latency timer 1 ms,
#                              no USB autosuspend). Experimental, see TICKETS/088.
#   --experimental-no-smt      turn Hyper-Threading off (4 -> 2 CPUs). Less jitter between
#                              sibling threads, less total CPU. Benchmark before using at a show.
#   --experimental-no-avahi    stop avahi-daemon (mDNS) until reboot. Hostnames like `bl.local`
#                              stop resolving for other machines; Blaulicht itself doesn't use mDNS.
set -uo pipefail

DMX_TUNING=0
NO_SMT=0
NO_AVAHI=0
for arg in "$@"; do
    case "$arg" in
        --experimental-dmx-tuning) DMX_TUNING=1 ;;
        --experimental-no-smt) NO_SMT=1 ;;
        --experimental-no-avahi) NO_AVAHI=1 ;;
        *) printf '[blaulicht-tune] unknown argument: %s\n' "$arg" >&2; exit 2 ;;
    esac
done

log() { printf '[blaulicht-tune] %s\n' "$*"; }

if [[ "$(id -u)" -ne 0 ]]; then
    log "must run as root" >&2
    exit 1
fi

write() { # write <value> <file...>
    local value="$1" f
    shift
    for f in "$@"; do
        if [[ -w "$f" ]]; then
            echo "$value" >"$f" 2>/dev/null || log "failed: $value > $f"
        else
            log "skip (missing): $f"
        fi
    done
}

cpu=/sys/devices/system/cpu

# CPU: pin the clock at its maximum. The performance governor alone still lets
# intel_pstate (passive) and the energy bias drop the clock to ~1.5 GHz.
for c in "$cpu"/cpu[0-9]*/cpufreq; do
    write performance "$c/scaling_governor"
    write "$(cat "$c/cpuinfo_max_freq")" "$c/scaling_max_freq" "$c/scaling_min_freq"
done
write 100 "$cpu/intel_pstate/max_perf_pct" "$cpu/intel_pstate/min_perf_pct"
write 0 "$cpu"/cpu[0-9]*/power/energy_perf_bias
for c in "$cpu"/cpu[0-9]*/cpufreq/energy_performance_preference; do
    [[ -e "$c" ]] && write performance "$c"
done
# Turbo stays as configured (no_turbo=1 keeps the clock constant at 2.7 GHz).

# CPU: no C1E (enhanced halt). With C1E a halted core drops to its lowest clock and voltage
# and has to ramp back up after every wakeup; `cpuidle.off=1` doesn't stop it (it's a
# hardware setting). Clears bit 1 of MSR_POWER_CTL (0x1FC) on every CPU, like the BIOS
# "C1E" option. Measured on `bl`: 0x4005f -> busy clock dipped to ~1.8 GHz.
modprobe msr 2>/dev/null
msr_c1e_off() {
    local dev val new
    for dev in /dev/cpu/[0-9]*/msr; do
        [[ -e "$dev" ]] || continue
        val=$((0x$(dd if="$dev" bs=8 count=1 skip=$((0x1FC)) iflag=skip_bytes 2>/dev/null | od -An -tx8 | tr -d ' ')))
        new=$((val & ~0x2))
        [[ "$new" -eq "$val" ]] && continue
        # MSRs are written as 8 little-endian bytes at offset 0x1FC.
        printf "$(printf '%016x' "$new" | sed -E 's/(..)(..)(..)(..)(..)(..)(..)(..)/\\x\8\\x\7\\x\6\\x\5\\x\4\\x\3\\x\2\\x\1/')" |
            dd of="$dev" bs=8 count=1 seek=$((0x1FC)) oflag=seek_bytes conv=notrunc 2>/dev/null ||
            log "failed: C1E off on $dev"
    done
}
msr_c1e_off

# Scheduler: full preemption instead of voluntary (PREEMPT_DYNAMIC kernel).
mountpoint -q /sys/kernel/debug || mount -t debugfs none /sys/kernel/debug 2>/dev/null
write full /sys/kernel/debug/sched/preempt

# USB: no autosuspend for devices present now and for devices plugged in later.
write -1 /sys/module/usbcore/parameters/autosuspend
write on /sys/bus/usb/devices/*/power/control

# Memory: avoid swapping out the show process.
write 10 /proc/sys/vm/swappiness

# Kernel background work that can interrupt the show threads:
# - Transparent huge pages only on request: with "always", page faults and khugepaged can
#   stall on memory compaction.
# - No NMI watchdog: it fires a perf-counter NMI on every CPU periodically.
# - vmstat updates every 10 s instead of every 1 s on each CPU.
# - Timers stay on the CPU that armed them instead of migrating to busy CPUs.
# RT throttling (sched_rt_runtime_us = 950000) stays on as a safety net for a spinning RT thread.
write madvise /sys/kernel/mm/transparent_hugepage/enabled
write 0 /proc/sys/kernel/nmi_watchdog
write 10 /proc/sys/vm/stat_interval
write 0 /proc/sys/kernel/timer_migration

# Interrupt layout. On a 2-core/4-thread CPU (siblings 0+2 and 1+3 on `bl`), keep USB
# (DMX adapters, MIDI, audio interface) alone on CPU1 and move graphics and network to the
# other core, so UI redraws don't interrupt the CPU that handles DMX traffic.
# TICKETS/091 can pin dmx-output to the USB CPU. Values are CPU lists; IRQs are found by name.
USB_IRQ_CPUS=1
OTHER_IRQ_CPUS=0,2
set_irq_cpus() { # set_irq_cpus <cpu list> <name regex>
    local irq
    for irq in $(awk -v re="$2" '$NF ~ re { sub(":", "", $1); print $1 }' /proc/interrupts); do
        write "$1" "/proc/irq/$irq/smp_affinity_list"
    done
}
set_irq_cpus "$USB_IRQ_CPUS" 'xhci_hcd|ehci_hcd'
set_irq_cpus "$OTHER_IRQ_CPUS" 'i915|eno[0-9]|enp[0-9]s[0-9]|eth[0-9]'

# Network (ArtNet): no PCIe link power saving and no Energy Efficient Ethernet, so the NIC
# never has to wake its link before sending. ArtNet goes out every 25 ms, so the link sits
# idle in between, which is exactly when EEE/ASPM kick in. EEE needs ethtool (install.sh).
write performance /sys/module/pcie_aspm/parameters/policy
if command -v ethtool >/dev/null; then
    for nic in /sys/class/net/*/device; do
        nic="$(basename "$(dirname "$nic")")"
        ethtool --set-eee "$nic" eee off 2>/dev/null && log "network: $nic eee off"
        # No Ethernet flow control: a congested switch sends pause frames that stop our
        # transmit for up to ~33 ms at 1 Gbit/s (eno1 on `bl` had received 657). For ArtNet a
        # dropped frame beats a late one; the next frame follows 25 ms later anyway.
        ethtool -A "$nic" autoneg off rx off tx off 2>/dev/null && log "network: $nic pause off"
    done
else
    log "skip: EEE (ethtool missing)"
fi

# Interrupt thread priorities. Only with the `threadirqs` boot option (install.sh), where each
# handler runs as a SCHED_FIFO thread `irq/<n>-<name>` at priority 50. USB carries DMX, MIDI
# and audio, so it goes above the rest; graphics goes below. Engine RT threads (TICKETS/091,
# dmx-output at 80) still outrank all of these.
IRQ_PRIO_USB=60
IRQ_PRIO_NET=55
IRQ_PRIO_GPU=40
set_irq_thread_prio() { # set_irq_thread_prio <prio> <name regex>
    local pid
    for pid in $(ps -eo pid=,comm= | awk -v re="$2" '$2 ~ "^irq/[0-9]+-(" re ")" { print $1 }'); do
        chrt -f -p "$1" "$pid" >/dev/null 2>&1 || log "failed: chrt $1 $pid"
    done
}
if grep -qw threadirqs /proc/cmdline; then
    set_irq_thread_prio "$IRQ_PRIO_USB" 'xhci_hcd|ehci_hcd'
    set_irq_thread_prio "$IRQ_PRIO_NET" 'eno[0-9]|enp[0-9]s[0-9]|eth[0-9]'
    set_irq_thread_prio "$IRQ_PRIO_GPU" 'i915'
    log "irq threads: usb=$IRQ_PRIO_USB net=$IRQ_PRIO_NET gpu=$IRQ_PRIO_GPU"
else
    log "skip: irq thread priorities (no threadirqs boot option)"
fi

# Audio: fix the PipeWire graph buffer at 256 frames (~5.3 ms at 48 kHz). The dynamic
# quantum dropped to 128 on `bl` and the USB input logged xruns (13 in 55 min). Runtime
# only: PipeWire forgets it on restart/reboot; `clock.force-quantum 0` undoes it.
# PipeWire runs per user, so this runs as the user who called sudo, not as root.
PW_QUANTUM=256
if [[ -n "${SUDO_USER:-}" && -n "${SUDO_UID:-}" ]] && command -v pw-metadata >/dev/null; then
    if timeout 5 runuser -u "$SUDO_USER" -- env XDG_RUNTIME_DIR="/run/user/$SUDO_UID" \
        pw-metadata -n settings 0 clock.force-quantum "$PW_QUANTUM" >/dev/null 2>&1; then
        log "pipewire: force-quantum=$PW_QUANTUM"
    else
        log "failed: pipewire force-quantum (PipeWire not running for $SUDO_USER?)"
    fi
else
    log "skip: pipewire quantum (not run via sudo, or pw-metadata missing)"
fi

# Optional: Hyper-Threading off. Runtime only; `echo on` restores it.
if [[ "$NO_SMT" -eq 1 ]]; then
    write off "$cpu/smt/control"
fi

# Optional: stop mDNS until reboot (socket first, or it restarts on demand).
if [[ "$NO_AVAHI" -eq 1 ]]; then
    systemctl stop avahi-daemon.socket avahi-daemon.service 2>/dev/null || log "failed: stop avahi"
fi

# DMX output chips (FTDI FT232, 0403:6001): the chip holds partial data going to the host
# (RX) for up to latency_timer ms (default 16). DMX output is TX, so the gain is probably
# small (see TICKETS/088); also keeps the adapter out of USB autosuspend. Only applies to
# interfaces plugged in now; a replugged interface is back at 16 until the next start.
tune_dmx_chips() {
    local tty dev usbdev found=0
    for tty in /sys/bus/usb-serial/devices/ttyUSB*; do
        [[ -e "$tty" ]] || continue
        dev="$(readlink -f "$tty")"
        [[ "$(basename "$(readlink -f "$dev/driver")")" == ftdi_sio ]] || continue
        found=1
        write 1 "$tty/latency_timer"
        # Walk up to the USB device that owns this port.
        usbdev="$dev"
        while [[ "$usbdev" != / && ! -e "$usbdev/idVendor" ]]; do
            usbdev="$(dirname "$usbdev")"
        done
        [[ -e "$usbdev/power/control" ]] && write on "$usbdev/power/control"
        log "dmx: $(basename "$tty") latency_timer=$(cat "$tty/latency_timer" 2>/dev/null)" \
            "power=$(cat "$usbdev/power/control" 2>/dev/null)"
    done
    [[ "$found" -eq 1 ]] || log "dmx: no FTDI interfaces found"
}

if [[ "$DMX_TUNING" -eq 1 ]]; then
    tune_dmx_chips
fi

log "applied: governor=$(cat "$cpu/cpu0/cpufreq/scaling_governor" 2>/dev/null)" \
    "min=$(cat "$cpu/cpu0/cpufreq/scaling_min_freq" 2>/dev/null)" \
    "preempt=$(grep -o '([a-z]*)' /sys/kernel/debug/sched/preempt 2>/dev/null)" \
    "usb_autosuspend=$(cat /sys/module/usbcore/parameters/autosuspend 2>/dev/null)" \
    "swappiness=$(cat /proc/sys/vm/swappiness)" \
    "smt=$(cat "$cpu/smt/control" 2>/dev/null)" \
    "boot=$(grep -ow -e threadirqs -e idle=poll /proc/cmdline | tr '\n' ' ')" \
    "power_ctl=$(dd if=/dev/cpu/0/msr bs=8 count=1 skip=$((0x1FC)) iflag=skip_bytes 2>/dev/null | od -An -tx8 | tr -d ' ')" \
    "temps_C=$(awk '{ printf "%d ", $1 / 1000 }' /sys/class/thermal/thermal_zone*/temp 2>/dev/null)"
