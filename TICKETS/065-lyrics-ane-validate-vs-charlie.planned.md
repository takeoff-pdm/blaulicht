# 065 — Validate Neural Engine lyrics (v2) against ae-charlie on real vocal stems

| | |
|---|---|
| Epic | E12 Mac analysis pipeline |
| Depends on | — |
| ABI / showfile | none |

**Touches:** `audio-model/melody/ane/test_lyrics.py`, `analyze/coreml_whisper2.py` (only if a bug turns up)

## Problem
v2 (`MELODY_WHISPER_BACKEND=coreml2`, models in `~/work/ane-lyrics/models/v2`) takes 22 s on Stereo Love. It was only checked on the full mix against faster-whisper greedy on the CPU (92.7% of the text, 91% of word starts within 50 ms). The vocal stem and ae-charlie's reference lyrics were lost with the scratch folder, so v2 hasn't been compared with what production produces.

## Scope
- Fetch `stems_vocals/<id>.npy` and `features/lyrics/<id>.json` from ae-charlie for 3–5 songs: Stereo Love, one German song, one dense rap or fast vocal, and one mostly instrumental track. Only the user reopens the gateway.
- Run v2 and compare with ae-charlie using `test_lyrics.py`: word count, text match, and word starts within 50 ms.
- Check the hallucination filters (no-speech, compression ratio, phantom lines) on the instrumental track.

## Acceptance criteria
- [ ] Table per song: v2 vs ae-charlie (beam 5, CUDA), with v1's Stereo Love numbers (91.8%, 88%) as the baseline.
- [ ] Text ≥ 90% and timing ≥ 85% on average, or the cause is found and written down.
- [ ] Results in `audio-model/melody/ane/README.md`.

## Verification
Run on the Mac only when the user says it is free.
