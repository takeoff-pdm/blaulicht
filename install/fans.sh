#!/usr/bin/env bash
set -euo pipefail

# Locate the correct hidraw device
find_relay_device() {
  for dev in /dev/hidraw*; do
    if udevadm info -q all -n "$dev" | grep -q "ID_VENDOR_FROM_DATABASE=Van Ooijen" && \
       udevadm info -q all -n "$dev" | grep -q "ID_MODEL_FROM_DATABASE=HID device except mice, keyboards, and joysticks"; then
      echo "$dev"
      return 0
    fi
  done
  return 1
}

DEVICE=$(find_relay_device) || {
  echo "Error: relay device not found" >&2
  exit 1
}

case "${1:-}" in
  on)
    echo "Turning fan ON via $DEVICE"
    echo -ne '\x00\xFE\x01' | dd of="$DEVICE" bs=3 count=1 status=none
    ;;
  off)
    echo "Turning fan OFF via $DEVICE"
    echo -ne '\x00\xFD\x01' | dd of="$DEVICE" bs=3 count=1 status=none
    ;;
  *)
    echo "Usage: $0 {on|off}" >&2
    exit 1
    ;;
esac

