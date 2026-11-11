# 066 — Lyrics: whole decoder in one Neural Engine model (one call per token)

| | |
|---|---|
| Epic | E12 Mac analysis pipeline |
| Depends on | 065 |
| ABI / showfile | none |

**Touches:** `audio-model/melody/ane/ane_decoder2.py` (`--split 32`), `analyze/coreml_whisper2.py`

## Problem
Each generated token takes two calls, DecoderA (12 ms) and DecoderB (13 ms): 452 steps cost 11.7 s, about half of the 22 s lyrics step. With int8 weights the whole decoder is ~870 MB. If the Neural Engine's 1 GiB limit counts compressed bytes, it fits in one model, which saves one call and one Python round trip per token.

## Scope
- Build with `--split 32` and check with `diag plan` (`DIAG_FUNCTION=step|multi|fill`) that every op is on the NE. If it falls to the CPU, the limit counts uncompressed bytes: note it and stop.
- Measure `step` per token and the whole lyrics step.
- If it helps, measure the Python overhead per call (input building, fp16 logits → fp32) and trim it, for example with preallocated buffers or argmax over fp16.

## Acceptance criteria
- [ ] Placement result written down (limit counts compressed or uncompressed bytes).
- [ ] Token step and song time before and after; same tokens as the two-model build on the test song.

## Verification
`test_lyrics.py` on the 065 songs; Mac only when free.
