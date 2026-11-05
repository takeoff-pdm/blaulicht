# A frame-level detector for club-music effects

AudioSet models score whooshes and booms near zero on club music, and the DSP `risers` analyzer is sparse. So this
trains a small head (conv + BiGRU, frozen BEATs plus a log-mel) on synthetic mixtures: effects pasted into real song bars
with exact labels. `analyze/fxhead.py` is the analyzer; weights are `models/fx_head/best.pt` on ae-charlie.

**Verdict.** Risers: yes, worth trying. On held-out songs the head agrees with drops well above several nulls and
reaches many more drops than the DSP analyzer. Impacts: no evidence it hears them; it reacts at least as much to
kick and bass returning alone. Downlifter, air horn, siren and reverse cymbal: unverified on real music. **Nobody
has listened to any of this and there are no hand labels yet, so all of it is agreement, not accuracy.**

## What it trained on

- **Generated** (`effects.py`): filtered noise sweeps, pitch rises, detuned chords, sine rises, accelerating pulses,
  reverse swells, Shepard tones, filter sweeps over the song itself; sub drops and falling versions; layered impacts;
  horn stacks; five siren types; reverse cymbals; plus non-effects that must stay silent (snare rolls, steady pads, wobbles,
  arpeggios).
- **YouTube** (`yt_sources.py`, `cuts.py`): isolated effect uploads, weakly labelled by query and title. 1,451 candidates, 240
  chosen, 230 downloaded (8 unavailable, 2 single-video 403s, no bot check), 215 kept after dropping tutorials, songs and spoken
  watermarks: 234 MB, 251 min, 135 channels. Licence field Creative Commons on 108; 67 more say free in the title; 40 say neither,
  taken for the classes short of candidates (downlifter, reverse cymbal). All listed in `yt_manifest.json`. Audio stays
  in `models/fx_head/sources/`. Cut on silence, then duration, spectral and speech checks: 972 usable cuts (634 train, 121 val, 217 test;
  22 near-copies across splits and 32 over a per-video cap dropped), split so no channel or near-identical title crosses.
- **Purity by eye**, 24 random cuts per class, spectrogram shape only: riser about 80%, downlifter 70%, impact 85%, horn
  90%, siren 60%, reverse cymbal 80%. Sirens are the weak class.
- **Rekordbox** (`~/Music/rekordbox/Sampler`, not the empty `PioneerDJ` folder): 30 WAVs, 22 used (13 effects, 9 non-effects). Kept out
  of all training as the one set of real, professionally made sounds no head has heard. It has no riser or reverse
  cymbal, so those are its downlifters and cymbals reversed. No other effects were found on the Mac.
- **Mixtures** (`mixtures.py`): 6,000 generated and 6,000 YouTube for training, 300 + 300 validation, 400 each for test (generated, YouTube,
  Rekordbox); 40 s kept of 50 s. Songs are split by artist: 120 train, 20 val, 21 dev, 61 test.
- **Backgrounds and label noise** (`spans.py`): only bars away from drops (16 before, 2 after), outside Build sections and section
  edges, and a bar away from anything `risers`, `sed` or `events` marks. About half of each song qualifies. Sub-threshold real
  effects can remain.
- **Fake drops**: half the mixtures mute drums and bass and bring them back on a bar line. Effects are largely independent of this
  and many fake drops stay empty, so "no kick means riser" doesn't pay.

## Results on the 61 test electronic songs (167 drops)

The head was chosen on the 20 dev songs before the test songs were opened. Thresholds for risers and impacts come from firing rate
at bar lines away from drops on dev songs, which never looks at drops. Null: bar lines at least 8 bars before or 4 after any drop.
Quiet null: those that follow a Breakdown bar, so the same kind of context with no drop coming. CIs resample whole songs.

Riser ending in the last bar before the line:

| | Drops | Null | Quiet null | Lift [95%] | Lift, quiet | AUC | Drops at a 3 / 5 / 10% null rate |
|---|---|---|---|---|---|---|---|
| DSP `risers` | 4.8% | 1.0% | 1.5% | 4.8 [1.7, 8.9] | 3.2 | 0.52 | can't fire that often (3.6% at 1%) |
| Generated only | 17.4% | 2.4% | 3.6% | 7.3 [4.8, 10.5] | 4.8 | 0.73 | 7.2 / 12.0 / 20.4% |
| YouTube only | 15.6% | 2.6% | 3.7% | 6.0 [3.2, 9.9] | 4.2 | 0.72 | 8.4 / 13.8 / 24.6% |
| **Both (chosen)** | 17.4% | 2.6% | 3.8% | 6.7 [4.3, 9.9] | 4.6 | 0.72 | 10.8 / 16.2 / 27.5% |

The sources don't separate on real songs: the CIs overlap heavily (both at 10%: [19.5, 35.8]; generated: [13.9, 28.7]). At a 1% null rate the
chosen head and DSP tie at 3.6%, so the gain is in reach at higher rates, not in precision.

Controls for the riser:

- **Fake drops with nothing pasted**, same measure and threshold: 4-6% against 17.4% on real drops. Context alone doesn't explain it.
- **Reversed audio**: the context stays but a rising sweep falls. Any-frame lift goes from 2.4 to 1.03 (quiet null 1.9 to 1.09), and the
  downlifter class gains lift 4.6 there. That's what hearing the sweep should look like.
- **Seams**: 9.4% of event starts within 0.2 s of a 5 s multiple, chance 8%.

Impact on the downbeat (within 0.16 s):

| | Drops | Null | Lift [95%] | Empty fake drops, same threshold |
|---|---|---|---|---|
| DSP `risers` | 1.2% | 0.4% | 3.0 [0, 9.2] | not measured |
| Generated only | 1.8% | 0.3% | 6.0 [0, 11.3] | 2-4% |
| YouTube only | 6.0% | 0.7% | 8.6 [4.1, 14.8] | 6-8% |
| Both (chosen) | 6.6% | 1.1% | 6.0 [2.2, 10.9] | 11-16% |

The lift over the null looks fine, but real drops never clearly beat empty fake drops: for the chosen head real drops stay below them at
every threshold from 0.02 to 0.9 (`results/curve_test.json`). One retry weighted empty fake drops as hard negatives
(`both_both_hn`). It is inconclusive: at t = 0.2 real drops beat empty ones on test (13.8% against 7.6%) but not on dev (10.7% against 17.0%), and it
lost riser lift on dev, so it wasn't chosen. Fine-tuning the BEATs layers wasn't tried.

Synthetic hold-out, mean event F1 over classes, on generated / YouTube / Rekordbox test sets:

| Trained on | Generated | YouTube | Rekordbox (never trained on) |
|---|---|---|---|
| Generated only | 0.610 | 0.368 | 0.336 |
| YouTube only | 0.298 | 0.559 | 0.574 |
| Both (chosen) | 0.553 | 0.529 | 0.523 |

This is where real sounds pay off: generated effects alone transfer poorly to real ones. Chosen head, frame/event F1 on the Rekordbox
set: riser 0.43/0.53, downlifter 0.21/0.30, impact 0.25/0.42, horn 0.89/0.87, siren 0.34/0.42, reverse cymbal 0.48/0.60. All 10 heads, including
log-mel-only and BEATs-only, are in `results/`. `both_mel` had the top test riser lift (9.7) but wasn't the dev choice, and picking it now would be
choosing on the test set.

How often it fires on the test songs (medians): riser 0.57 a minute, 6 s long, active 7% of the time, silent in 10 of 61 songs; impact 0.69 a minute.
Downlifter, horn, siren and reverse cymbal are silent in most songs (52, 53, 42, 58 of 61). Over all 300: riser 0.59 a minute on electronic, 0.30 on pop.
Horns and sirens barely agree with SED: of 14 horn events 1 overlaps a SED air horn, of 35 sirens none, and SED has only 6 such events on these songs. Horns land
on songs like RÜFÜS DU SOL's Innerbloom, which may just be synths.

## For a human to check

`results/cases_best.json` has the full lists. Riser, top confidence (the DSP analyzer misses all of these unless noted):

- Swedish House Mafia, Greyhound (`0f39a31104e7`) 0:24.5-1:01.4, before a drop
- Zedd, Clarity (`ce281bd15e8a`) 4:06.8-4:20.8, before a drop; also 0:37.9-1:01.0
- deadmau5, Strobe (`90916f976b65`) 9:48.4-10:12.4
- Dimitri Vegas & Like Mike, Mammoth (`80f1fdffb40a`) 3:54.0-4:30.0
- Netsky, Rio (`f0811c996e8b`) 0:15.3-0:22.6
- Boris Brejcha, Gravity (`1f6236562632`) 3:13.9-3:46.5 (DSP overlaps this one)

Named songs:

- Hardwell, Spaceman (`fe0b3c607b1b`), drops 0:30.0 and 2:00.0: riser 1:50.6-1:59.5 ending just before the second, DSP agrees; riser 1:05.2-1:10.4; a "siren"
  at 1:54.3-1:59.5. Nothing before the first drop.
- Acraze, Do It To It (`58e556af6855`), drops 0:15.4, 1:01.5, 2:18.3: impacts on all three (treat as unproven, see above), no riser before any of them;
  risers at 1:28.8-1:31.7, 1:44.9-1:47.6, 2:43.8-2:48.5, 3:25.7-3:27.4, which the DSP analyzer doesn't have.

Hand labels: `python -m analyze.fx_train.evaluate labels <fxhead features> audio-model/data/melody/labels` reads files with status "done" (kinds riser, downlifter,
impact with start == end, airhorn, siren, other; `other` scores against reverse cymbal). Checked with a throwaway file only; no labels exist yet.

## Running it

Env `.venv-fx`: Python 3.12, torch and torchaudio 2.11.0+cu128, torchvision 0.26.0+cu128 (PyPI's 0.29.2, pulled by timm, broke `import timm`), timm, scipy, pandas, yt-dlp.

    python -m analyze.fx_train.yt_sources search; python -m analyze.fx_train.yt_sources download   # about 1.5 h, polite rate
    python -m analyze.fx_train.cuts; python -m analyze.fx_train.cuts audit <dir>                   # about 5 min
    python -m analyze.fx_train.mixtures val_gen test_gen test_rb train_gen val_yt test_yt train_yt  # 3 mixtures/s: about 1.2 h, 22 GB
    sh analyze/fx_train/run_all.sh gen yt both                                                     # heads, 15-25 min each
    python -m analyze.fx_train.evaluate cache dev test                                             # real songs, forward and reversed, 5 min
    sh analyze/fx_train/run_eval.sh calibrate; sh analyze/fx_train/run_eval.sh test                # then: python -m analyze.fx_train.tabulate test
    python -m analyze.run fxhead --force                                                           # 300 songs in 276 s on an A100

`tmp/fx/` (mixtures and song embeddings) was removed afterwards; these commands rebuild it. Set `CUDA_DEVICE_ORDER=PCI_BUS_ID CUDA_VISIBLE_DEVICES=<n>` for anything using the GPU.
