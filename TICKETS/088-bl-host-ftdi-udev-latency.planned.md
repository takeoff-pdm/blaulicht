# 088 — Show host `bl`: udev rule for FTDI latency and USB autosuspend

| | |
|---|---|
| Epic | Ops / show host |
| Depends on | none |
| ABI / showfile | none |

**Touches:** host `bl` only: `/etc/udev/rules.d/99-blaulicht-dmx.rules` (new). No repo code, unless we also ship the rule
in the repo (for example `tools/host/99-blaulicht-dmx.rules`) so it survives a reinstall.

## Problem
Audit of the show laptop `bl` on 2026-11-12 (`ssh bl`, Debian 13, kernel 6.12.116+deb13-amd64, i5-4310M). The host is
mostly tuned: `performance` governor, `cpuidle.off=1`, `rtprio 99` for the `blaulicht` user, user in `dialout`, little
running. Two USB settings still add DMX delay and jitter:

1. **FTDI latency timer is 16 ms.** Both DMX interfaces are FTDI FT232 (`0403:6001`, `/dev/ttyUSB0` and `ttyUSB1`):
   ```
   /sys/bus/usb-serial/devices/ttyUSB0/latency_timer 16
   /sys/bus/usb-serial/devices/ttyUSB1/latency_timer 16
   ```
   The FTDI chip holds small packets for up to `latency_timer` ms before it sends them to the host. 16 ms is close to a
   whole DMX frame (~23 ms at full universe) and makes frame timing uneven. The usual fix for DMX is 1 ms. No udev rule
   sets it today, so it goes back to 16 after every reboot or replug.
2. **USB autosuspend is on.** `/sys/module/usbcore/parameters/autosuspend` is `2` and 10 devices have
   `power/control = auto`. Devices on the bus: the two FTDI interfaces, AKAI MIDI Mix, AKAI APC mini mk2, KORG
   nanoKONTROL Studio, an HD USB audio interface, an eGalax touchscreen, and VIA Labs hubs (`2109:2817`/`2109:0817`). A
   suspended device or hub costs a wake-up delay or a dropout.

The engine opens the ports with `serialport` at 250 000 baud, 8N2, 10 ms timeout (`crates/engine/src/dmx/mod.rs:167`
and `:307`). It doesn't touch `latency_timer`, so this has to be done by the host.

Since 2026-11-12, `install/tune.sh` can set `latency_timer=1` and `power/control=on` for FTDI ports at Blaulicht start
when `blaulicht.sh --experimental-dmx-tuning` is used (off by default). This ticket is the permanent version (udev also
covers replugged adapters).

### Expected gain is probably small (correction, 2026-11-12)
The latency timer only delays data going **from the FTDI chip to the host** (RX): the chip holds partial RX buffers
for up to `latency_timer` ms. DMX output is TX only, and host-to-chip bulk writes are sent right away. So the 16 ms
setting likely adds little delay to DMX output. Do it because it's cheap, but measure before and after instead of
expecting a big change.

### The real per-frame cost: break and write ordering
The send path for each universe (`output_worker`, `crates/engine/src/dmx/mod.rs:~250`):

```rust
port.set_break()?;                               // USB control transfer (FTDI SET_DATA with break bit)
spin_sleep::sleep(Duration::from_micros(100));   // intended break length
port.clear_break()?;                             // second control transfer
spin_sleep::sleep(Duration::from_micros(12));    // intended mark-after-break
port.write_all(&frame.universes[universe_no])?;  // bulk OUT, returns when the data is in the kernel buffer
```

Open questions to check while working on this ticket:
1. **Break length isn't the 100 µs sleep.** Each `set_break`/`clear_break` is a separate USB control transfer that
   blocks until the device acknowledges it (one or more USB frames, typically ~125 µs–1 ms each). The break on the
   wire is "100 µs + control-transfer turnaround", so it's longer and varies with USB load. DMX allows long breaks
   (≥ 88 µs, no hard maximum), so this is a jitter/timing issue rather than a spec violation. Measure the actual
   break and MAB with a scope or DMX analyzer.
2. **Break can overtake the previous frame's data.** `write_all` returns once the 513 bytes are queued in the kernel/
   driver, not when they're on the wire (~22.7 ms at 250 kbaud with 8N2). Control transfers (break) and bulk transfers
   (data) are separate USB pipes, so the next frame's `set_break` is not guaranteed to wait for the previous frame's
   data to finish. At a 25 ms tick there's ~2 ms of margin; at a faster tick (ticket 089) or under scheduling jitter the
   break could cut off the tail of the previous frame (last channels flicker or drop). Fix to try: `port.flush()`
   (tcdrain) after `write_all`, or before `set_break`, and measure what it costs per frame.
3. **Two universes run one after the other** in the same thread, so universe 1's frame starts only after universe 0's
   break, MAB and write calls return. Note the per-universe timing when measuring.

## Scope
Draft rule (untested, it's why this is a ticket and not applied directly):

```udev
# /etc/udev/rules.d/99-blaulicht-dmx.rules
# FTDI DMX interfaces: 1 ms latency timer, never autosuspend.
ACTION=="add", SUBSYSTEM=="usb-serial", DRIVERS=="ftdi_sio", ATTR{latency_timer}="1"
ACTION=="add", SUBSYSTEM=="usb", ATTR{idVendor}=="0403", ATTR{idProduct}=="6001", TEST=="power/control", ATTR{power/control}="on"

# Hubs and show controllers: keep awake too.
ACTION=="add", SUBSYSTEM=="usb", ATTR{idVendor}=="2109", TEST=="power/control", ATTR{power/control}="on"
ACTION=="add", SUBSYSTEM=="usb", ATTR{idVendor}=="09e8", TEST=="power/control", ATTR{power/control}="on"
ACTION=="add", SUBSYSTEM=="usb", ATTR{idVendor}=="0944", TEST=="power/control", ATTR{power/control}="on"
```

Things to check while testing:
- The `latency_timer` line matches on the `usb-serial` device (`ttyUSB*` parent). Confirm with
  `udevadm info -a -p /sys/bus/usb-serial/devices/ttyUSB0` that `DRIVERS=="ftdi_sio"` is in the chain. If it doesn't
  match, an alternative is `SUBSYSTEM=="tty", KERNEL=="ttyUSB*", ATTRS{idVendor}=="0403", RUN+="/bin/sh -c 'echo 1 > /sys/bus/usb-serial/devices/%k/latency_timer'"`.
- The simpler, broader alternative to the per-vendor `power/control` lines is the boot parameter
  `usbcore.autosuspend=-1` in `GRUB_CMDLINE_LINUX_DEFAULT` (needs `update-grub` and a reboot). Pick one and note why.

Apply:
```sh
sudo install -m644 99-blaulicht-dmx.rules /etc/udev/rules.d/
sudo udevadm control --reload
sudo udevadm trigger --action=add --subsystem-match=usb --subsystem-match=usb-serial
```
Undo: delete the file, reload, replug (or reboot).

## Out of scope
Also found in the audit; separate tickets if wanted:
- Kernel preemption is in `voluntary` mode (`/sys/kernel/debug/sched/preempt`); `preempt=full` boot param or an RT kernel.
- Lid close suspends (logind defaults, sleep targets not masked).
- `no_turbo=1`, `vm.swappiness=60`.
- No systemd unit that starts blaulicht with RT scheduling.

## Acceptance criteria
- [ ] After a reboot, both `ttyUSB*` show `latency_timer` = 1.
- [ ] After a reboot and after replugging an interface, the FTDI devices, hubs and MIDI controllers show
      `power/control` = `on`.
- [ ] DMX output works on both universes and the MIDI controllers still respond.
- [ ] Notes record the measured break length, MAB and frame interval (before and after), and whether the break ever
      cuts into the previous frame; if it does, a follow-up ticket for `flush()`/tcdrain in `output_worker`.

## Verification
- `cat /sys/bus/usb-serial/devices/*/latency_timer`
- `for d in /sys/bus/usb/devices/*/; do echo "$d $(cat $d/idVendor 2>/dev/null) $(cat $d/power/control)"; done`
- Replug one DMX interface and check both again.
- Run a show and watch the fixtures; optionally compare frame timing before and after with a DMX analyzer.

## Notes
