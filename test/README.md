# End-to-end latency test

This directory contains a simple end-to-end harness that:
- plays an audio file on the system output, and
- listens for Art-Net DMX output to detect when the audio-driven DMX changes.

## Prerequisites
- Configure an Art-Net receiver in the app (e.g. `127.0.0.1:6454`).
- Load a showfile with a single fixture using the `audiovolume` animation.
- Ensure the audio input is wired (manual loopback is fine).
- Install libmpv + python bindings and pyartnet.

## Usage
Run the system as normal, then execute:

```bash
./test/artnet_latency.py path/to/test.mp3 --universe 0
```

Optional flags:
- `--channel <n>` and `--channel-count <n>` to monitor a specific channel range.
- `--runs <n>` to repeat and get a min/avg/max summary.

## Notes
- The script preloads the audio into memory (memfd on Linux) before calling libmpv.
- The script measures from audio playback start to first detected DMX change.
