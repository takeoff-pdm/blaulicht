# 068 — Lyrics: long-running worker for shows (no 46 s load per song)

| | |
|---|---|
| Epic | E12 Mac analysis pipeline |
| Depends on | 065 |
| ABI / showfile | none |

**Touches:** `audio-model/melody/analyze/run.py` or the compiler service's Mac backend, `analyze/coreml_whisper2.py`

## Problem
Loading the v2 models takes 46 s, twice the lyrics step itself. During a show, songs arrive one by one, so the models have to stay loaded. The encoder (7.4 s) also runs while the decoder waits, although both use the Neural Engine.

## Scope
- A worker that loads the models once and processes songs from a queue, as part of the Mac backend of the planned compiler service.
- Check whether a second load is faster because Core ML caches the compiled programs. If not, look into keeping the worker alive between shows.
- Try overlapping the encoder of the next window with decoding of the current one: encoder on the GPU while the decoder runs on the NE, but only if the GPU isn't needed by the other steps at that time.

## Acceptance criteria
- [ ] Second and later songs pay no model load.
- [ ] First and warm load times written down; overlap measured or ruled out.

## Verification
Two songs in a row through the worker; Mac only when free.
