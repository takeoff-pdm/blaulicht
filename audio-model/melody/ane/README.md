# Lyrics (Whisper large-v3) on the Apple Neural Engine

Goal: the lyrics step, and later the whole per-song pipeline, faster than the song itself on the
M2 Pro, so it can run during a show.

The Mac runs faster-whisper's own pipeline, the same code ae-charlie uses. Only the model calls go to
Core ML: `analyze/coreml_whisper.py` (v1) and `analyze/coreml_whisper2.py` (v2). Select a backend with
`MELODY_WHISPER_BACKEND=coreml|coreml2` and point `LYRICS_COREML_MODEL` at the models folder. With the
variable unset, ae-charlie behaves as before (CTranslate2 on CUDA).

## Neural Engine rules (measured on the M2 Pro)

- **1 GiB weight limit:** a program with more than 1 GiB of weights is rejected as a whole, and Core ML
  silently runs every op on the CPU (1022 MiB stays on the NE, 1025 MiB goes to the CPU).
- **64-byte state rows:** Core ML state works on the NE, but each row must be a multiple of 64 bytes.
  1500 fp16 frames fail; padded to 1536 they work.
- **Shared state:** the functions of one multifunction model can share an `MLState` object (`exp/mf_state.py`).
- **Compressed weights:** int8 per-channel weights stay on the NE and halve a bandwidth-bound token
  (300 × 1280² convs: fp16 16.1 ms, int8 8.2 ms). 6-bit palettisation (per tensor) runs on the CPU.
- **Check placement** with `diag/diag plan <model.mlmodelc> ane` (`DIAG_FUNCTION=` picks one function).

## v1 (measured 2026-11-09, Stereo Love, 4.2 min)

- **Lyrics step:** 64.8 s, plus 37 s to load the models.
- **Breakdown:** encoder 10.2 s (14 windows), cross-attention fill 13.0 s, decoder 39.6 s (1007 steps at 39 ms).
- **Against ae-charlie:** 327 vs 331 words, text 91.8%, 88% of word starts within 50 ms. Decoding is
  greedy; faster-whisper greedy on the CPU scores 91.8%, beam 5 scores 93.1%.

## v2 (measured 2026-11-10)

**Lyrics step: 22.1 s for a 4.2 min song (0.09× real time), plus 46 s to load once per worker.**
On the full mix as stand-in vocals (the vocal stem was lost with the scratch copy): encoder 7.4 s
(10 windows), cross-attention fill 0.5 s, 452 token steps 11.7 s (26 ms each), 19 batched calls 1.1 s.
Against faster-whisper greedy on the CPU (CTranslate2 int8, 148 s on the same input): text 92.7%,
91% of word starts within 50 ms. v2 also transcribed a verse the CPU run mostly skipped.
Per call: fill 10 ms, step 12 + 13 ms, multi (64 tokens) 21 + 22 ms; every op of every function on the NE.
Models (int8, weights shared across functions): DecoderA 403 MB, DecoderB 466 MB, in ~/work/ane-lyrics/models/v2.

### Design

`ane_decoder2.py` builds each decoder half as one multifunction model with three functions sharing state:

| function | what it replaces | expected gain |
|---|---|---|
| `fill` | the CrossKV model + Python `write_state` (0.9 s per window) | ~12 s |
| `multi` (64 tokens per call) | one call per token for the prompt and the word-alignment pass (~half of all decoder steps) | ~19 s |
| `step` with int8 weights | fp16 steps | ~40% of the remaining ~20 s |

**Expected:** about 25 s per song.

**Next, if needed:** speculative decoding with distil-large-v3 as the draft model. Its encoder is
large-v3's, frozen, so only a 2-layer decoder is added, and `multi` checks the drafted tokens.

**Open questions for the first build:**
- Does `fill`'s channel concat stay on the NE?
- Are weights shared across functions on save? If not, the model is three times larger.
- Do int8 weights count at their compressed size toward the 1 GiB limit? If so,
  `--split 32` puts all 32 layers in one model: one call per token instead of two.
- Do v2 and v1 produce the same tokens? Compare v2 against v1 on the song with `test_lyrics.py`.

## Rebuild (the scratch copy was lost on 2026-11-10; everything below has to be redone)

1. Venv: Python 3.12 with `coremltools==9.0 torch==2.7.1 safetensors numpy scipy`.
   Add `coremltools` to the venv that runs faster-whisper.
2. Downloads (about 6 GB):
   - `openai/whisper-large-v3` `model.safetensors`, for the decoder;
   - WhisperKit's `openai_whisper-large-v3`, for its AudioEncoder and MelSpectrogram;
   - faster-whisper `large-v3`, for the tokenizer and config.
3. `python split_models.py <whisperkit folder> <models>`, which writes the AudioEncoder in two
   chunks under 1 GiB.
4. `python ane_decoder2.py build <model.safetensors> <models>` (or `ane_decoder.py build` for v1).
5. Check every function: `DIAG_FUNCTION=step diag/diag plan <models>/DecoderA.mlmodelc ane`.
6. Run `test_lyrics.py <song id> <audio> <ae-charlie lyrics.json>`. It needs `stems_vocals/<id>.npy`
   under `MELODY_ROOT`.

The rest of the Mac port's patches were in the lost scratch copy:
- device variables in the other analyzers;
- ADTOF on `mps`;
- BEATs pooling on the CPU;
- `onnxruntime` instead of `onnxruntime-gpu`.

They are listed in the memory notes and have to be reapplied.

## The other slow steps in Core ML (measured 2026-11-11, Stereo Love, 4.2 min)

Same pattern for each: only the network runs in Core ML; STFT, complex math, chunking and
post-processing stay in the original code. Switch them on with `MELODY_COREML=<models folder>`
(`backends.py`; hooks in `analyze/vocals.py`, `analyze/drumparts.py`, `game_batch.py`).

| Step | Before (Mac) | Core ML | Unit | Agreement |
|---|---|---|---|---|
| vocals (`roformer_ane.py`) | 156 s | 68 s | GPU | 52.8 dB |
| stems (`htdemucs_coreml.py`) | 116 s (MPS) | 47.6 s | GPU | 55-61 dB vs PyTorch CPU |
| GAME (`game_coreml.py`) | 94 s (CPU) | 20.7 s | GPU + ORT CPU | note F1 0.892 (CPU vs CPU 0.896) |
| drum parts (`drumsep_coreml.py`) | 35 s (MPS) | 18.2 s | Neural Engine | 23 (crash) to 57 (kick) dB; hit F1 1.0 kick, 0.98 hh/ride, 0.94 crash (GPU: 0.995+, 29.6 s; `DS_UNITS=gpu`) |

All four in one process: 154.5 s plus 13.5 s load. RoFormer on the NE is rejected as a whole
(every op on the CPU); per layer, time attention is slower there than on the GPU.


All five slow steps at once (stems + drum parts + GAME in one process, vocals in a second, lyrics on the NE in a third): **153 s wall time including model loading** for the 252 s song, measured 2026-11-11 (lyrics load 57 s + 27.7 s). The fast analyzers (about 90 s, measured 2026-11-09) were not part of this run: their environments are gone.
