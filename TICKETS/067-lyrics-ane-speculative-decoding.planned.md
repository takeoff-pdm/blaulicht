# 067 — Lyrics: speculative decoding with distil-large-v3 as the draft model

| | |
|---|---|
| Epic | E12 Mac analysis pipeline |
| Depends on | 065, 066 |
| ABI / showfile | none |

**Touches:** new `audio-model/melody/ane/draft_decoder.py`, `analyze/coreml_whisper2.py`

## Problem
Generation is still one large-v3 token per step (about 12 s per song). distil-large-v3 keeps large-v3's encoder frozen and has a 2-layer decoder, so it can draft several tokens from the same encoder output. The existing `multi` function then checks them in one call (43 ms for 64 tokens). Greedy speculative decoding gives the same tokens as plain greedy decoding.

## Scope
- Build the distil-large-v3 decoder as its own stateful Neural Engine model (fill/step like `ane_decoder2.py`, int8).
- Draft k tokens (try 4–8), verify with `multi` and keep the agreeing prefix plus the correction. Rewind the caches by position: the masks already allow overwriting positions.
- Apply faster-whisper's timestamp rules and suppression to the target model's logits only, so the output stays the same.

## Acceptance criteria
- [ ] Same tokens as plain v2 greedy on the 065 songs (fp16 noise excepted, with any differences listed).
- [ ] Acceptance rate and lyrics-step time per song; worth keeping if the step drops by at least 30%.

## Verification
`test_lyrics.py`; Mac only when free.
