# 096 — Capture audio at the device's native sample rate (no PipeWire resampling)

| | |
|---|---|
| Epic | Ops / show host (latency) |
| Depends on | none |
| ABI / showfile | none |

**Touches:** `crates/audio_engine/src/audio_source/microphone.rs`, `crates/audio_engine/src/audio_source/stream_frames.rs`,
`crates/engine/src/mainloop/audio.rs`, possibly the `audioviz` input init (crate `audioviz` 0.6.0)

## Problem
`pw-top` on the show host `bl` (2026-11-12, Blaulicht running, USB interface "Soundprese HD USB Audio Device"):

```
S  ID  QUANT   RATE    WAIT    BUSY  ERR  FORMAT
R 129    128  48000  70.6us   2.4us   13  S24LE 2 48000 alsa_input.usb-Soundprese_HD_USB_Audio_Device-00.analog-stereo
R 114    120  44100  11.4us  41.1us    0  F32LE 2 44100  + alsa_capture.blaulicht
R 151    120  44100  15.5us  46.1us    0  F32LE 2 44100  + alsa_capture.blaulicht
```

1. **Rate mismatch:** the device and the PipeWire graph run at 48 kHz (`clock.allowed-rates = [48000]`), Blaulicht's
   capture streams at 44.1 kHz, so PipeWire resamples continuously (CPU on a 2-core machine, plus resampler delay).
   The rate comes from `audioviz`'s `Input::init` (`microphone.rs:75`), presumably the ALSA `pipewire`/`default`
   device's default config. The buffer size is computed from `config.processor.sampling_rate`
   (`microphone.rs:66-68`, "10 ms at 48k"), which may not match the rate actually opened.
2. **Two capture streams:** Blaulicht has two `alsa_capture.blaulicht` nodes on the same input. Find out why
   (spectrum and PCM tracker opening separately? a leftover stream after a device switch/reload? the device probe in
   `resolve_audioviz_device`?). If both read the same device, one stream should feed both.
3. The input logged 13 errors (xruns) in 55 minutes with a 128-frame quantum. `install/tune.sh` now forces a 256-frame
   quantum on the host; this ticket shouldn't depend on that, but note whether xruns remain.

## Scope
- Open the capture stream at the device's native/graph rate: query the supported configs and prefer the device's
  default or 48 kHz when supported, instead of 44.1 kHz. Pass the actual rate through to `StreamFrames`, the
  collector, the beat tracker/model and `loop_tempo_estimator` (they already take `sample_rate`; check every place
  that assumes 44.1 kHz, e.g. `collector.rs:1147,1200` test sources, the beat model's expected input rate).
- If the beat model needs a fixed rate (e.g. trained at 44.1 kHz or 22.05 kHz), resample inside Blaulicht once, on the
  analysis path, and note it.
- Compute the requested buffer size from the opened rate, not the configured one.
- Find and remove the second capture stream if it's redundant.
- Log the opened device, rate, channels and buffer size at startup.

## Out of scope
- Real-time priority for the audio thread (091 covers engine threads; add the capture thread there if useful).
- The host PipeWire quantum (handled in `install/tune.sh`).

## Acceptance criteria
- [ ] On `bl`, `pw-top` shows Blaulicht capturing at 48000 Hz (same as the device), with one capture node.
- [ ] Spectrum, beat detection (legacy, tracker and model) and tempo estimation give the same results as before on
      the same music (compare BPM and beat timing on a test track).
- [ ] On macOS (CoreAudio) capture still works and uses the device's rate.

## Verification
- `cargo clippy --all-targets --all-features`, `cargo fmt`, `cargo test --all-features`.
- On `bl`: `pw-top -b -n 2`, the startup log line, and the ERR column after 30 minutes of a show.

## Notes
