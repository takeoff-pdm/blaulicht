# Offline analyzers for the show compiler

Everything about a song that the light show can use, computed offline on ae-charlie
(`/localhome/sokrates/melody`), one JSON per song and analyzer in `out/features/<analyzer>/<id>.json`.
`viz/show_compile.py` merges them into one cue list per song (`doc.cues`), the viewer shows them as
lanes under the stem lanes, and the club (`viz/club.js`) plays the cues.

    sh audio-model/melody/analyze/sync.sh push      # code to ae-charlie
    # on ae-charlie, in code/: <venv>/bin/python -m analyze.run <analyzer> [--jobs N | --shard i --num-shards n]
    sh audio-model/melody/analyze/sync.sh pull      # features to audio-model/data/melody/features, then restart serve.py

`run.py` skips songs already done at the analyzer's current `VERSION`, so a new playlist only costs
its own songs. Every analyzer is optional for the compiler.

| Analyzer | What it gives the show | Method | Weights licence | Env (ae-charlie) | 300 songs |
|---|---|---|---|---|---|
| `stereo` | pan and width per stem, 10 fps | stem masks on the stereo mix (own DSP) | none needed | `.venv-ana` | 50 s (16 jobs) |
| `timbre` | brightness, noisiness per stem; kicks and sidechain pump depth | own DSP | none needed | `.venv-ana` | 62 s |
| `risers` | noise sweeps, snare rolls, impacts, pre-drop silences, drum density | own DSP | none needed | `.venv-ana` | 67 s |
| `drumparts` | hits of kick, snare, toms, hi-hat, ride, crash + levels | MDX23C DrumSep (aufr33/jarredou) via audio-separator, onsets per piece | **none stated: research only** | `.venv-sep` | ~10 s/song GPU |
| `events` | air horn, siren, whoosh, boom, crowd, white noise ... (38 AudioSet classes, 10 fps) | PANNs Cnn14_DecisionLevelMax on other + vocals | CC BY 4.0 | `.venv-ml` | 218 s (1 GPU) |
| `beats` | beats and downbeats, a check on the Rekordbox grid | Beat This! final0 (CPJKU) | MIT | `.venv-ml` | 453 s (1 GPU) |
| `tags` | 400 Discogs styles, moods (aggressive, happy, party, relaxed, sad), danceability | Essentia Discogs-EffNet + heads | **CC BY-NC-SA 4.0: non-commercial** | `.venv-esstf` | 244 s (8 jobs CPU) |
| `chords` | chords (24 triads + N) and key over time, tuning | own chroma + Viterbi | none needed | `.venv-ana` | 61 s |
| `structure` | parts A, B, C ... (same letter = the music came back), boundaries, repetition | own self-similarity / novelty / clustering | none needed | `.venv-ana` | 42 s |
| `transcribe` | polyphonic synth notes, chord stabs, polyphony | Basic Pitch (Spotify) on the other stem | Apache-2.0 | `.venv-bp` (Python 3.11, TF 2.14, numpy<2) | 326 s (8 jobs CPU) |
| `vocals` | cleaner vocals (`stems_vocals/<id>.npy`) and vocal regions | Kim Mel-Band RoFormer via audio-separator | **none stated: research only** | `.venv-sep` | ~22 s/song GPU |
| `lyrics` | words with timestamps, sung lines, vocal chops, language | faster-whisper large-v3 on the RoFormer vocals, own vocal chunks, hallucination filter | MIT | `.venv-asr` | ~7 s/song GPU |
| `embed` | song embedding (similar songs), zero-shot style prompts | LAION-CLAP clap-htsat-unfused | Apache-2.0 | `.venv-ml` | ~2 s/song GPU |
| `adtof` | drum transcription; confirms DrumSep's kicks, snares and hats | ADTOF Frame-RNN (PyTorch port) on the drums stem | **port states none, ADTOF's weights CC BY-NC-SA 4.0: non-commercial** | `.venv-ml` | 270 s (1 GPU) |
| `allinone` | part boundaries, labels (intro, verse, chorus, inst, break, outro), beats, downbeats | All-In-One through the all-in-one-infer fork (no NATTEN needed), on our stems | MIT | `.venv-aio` | ~35 min (2 GPUs) |
| `sed` | sound events at 40 ms; merged with `events` | PretrainedSED, BEATs checkpoint (AudioSet Strong, 447 classes), on other + vocals | code MIT, trained on AudioSet (YouTube audio) | `.venv-sed` | 347 s (1 GPU) |
| `btc` | large-vocabulary chords (7ths, sus, dim ...); viewer only | BTC large-vocab checkpoint on the song without drums | MIT | `.venv-ml` | 394 s (1 GPU) |
| `musicfm` | structure from learned embeddings, song embedding; viewer only | MusicFM 25 Hz MSD checkpoint, layer 7, with the `structure` segmenter | MIT (one code file Apache-2.0) | `.venv-ml` | 287 s (1 GPU) |
| `mt3` | which instruments play when, their notes; viewer only | YourMT3+ "YPTF.MoE+Multi (noPS)" on the mix | **unclear: the Space says Apache-2.0, the GitHub repo GPL-3.0** | `.venv-mt3` (Python 3.11, numpy 1.26, transformers 4.45) | ~1 h (2 GPUs) |
| `mulan` | the song's style prompts; its embedding isn't used | MuQ-MuLan-large on the mix, CLAP's prompts | not checked | `.venv-mulan` (transformers 4.48.3) | 447 s (1 GPU) |
| `songformer` | part boundaries and pop labels; not used by the compiler or shown | SongFormer as it comes, on the mix | not checked | `.venv-sf` (Python 3.10) | ~4 s/song (needs the GPU alone) |
| `fxhead` | risers, downlifters, impacts, air horns, sirens, reverse cymbals at 40 ms; risers feed the builds, all are a lane | own conv + BiGRU head on frozen BEATs and a log-mel, trained on synthetic mixtures (`fx_train/README.md`) | own weights; trained on YouTube uploads of mixed licence (`fx_train/yt_manifest.json`) | `.venv-fx` | 276 s (1 GPU) |

Checked on known songs: DrumSep's kicks land on the drop (Acraze, 61.52 s) and every 0.48 s
after it (125 BPM); Beat This! agrees with the aligned Rekordbox grid (125.0 BPM, 100% of
downbeats); the tags call Acraze house / bassline and Spaceman electro / progressive house; the
structure gives all three Acraze drops the same letter, both breakdowns another and both builds a
third; the risers analyzer finds Acraze's real pre-drop silence (138.23-138.31 s, drop at 138.32);
lyrics give the real hook lines once Whisper's phantom subtitle lines are filtered.

Weak spots: CLAP's zero-shot prompts are right about half the time (Charlotte de Witte comes out
"vocal, groovy"), so the Essentia tags stay the main style signal and the prompts now come from MuLan; the cymbal stems of DrumSep
carry hi-hat bleed (stricter thresholds for ride and crash); the chord tracker knows triads only;
the DSP risers are sparse (hence `fxhead`) and the snare-band rolls also fire on busy percussion (the compiler uses
DrumSep's snare rate instead); PANNs finds few events in most songs (and nothing in Acraze), and SED adds only a few.

## How the second opinions compare

There is no ground truth for these songs, so this compares the analyzers with each other, the beat
grid and the drops; it can show one is cleaner or steadier, not that it is right. On the 175
electronic songs, medians unless noted:

| Question | Result | What the compiler does |
|---|---|---|
| Kicks: DrumSep or ADTOF? | ADTOF has 99% of its kicks in DrumSep's, DrumSep 89% in ADTOF's. DrumSep's unconfirmed kicks land on a 16th only 63% of the time (about 50% by chance; confirmed ones 97%). Same share of drop beats covered (94%) | keeps a DrumSep kick, snare or hat if ADTOF has it too, or it is strong (0.75) and on a 16th: kicks on a 16th go from 97.7% to 99.7%, drop beats covered from 93.8% to 93.4% |
| Snares before a drop | DrumSep hears about twice ADTOF's in the last 4 bars (19 against 9), 84% of its unconfirmed ones on a 16th, so probably real rolls | build curves count all of DrumSep's snares |
| Crash | DrumSep's crashes and ADTOF's cymbals barely overlap (5% and 29%); the authors report F 0.20 for cymbals on electronic music | keeps DrumSep's strong crashes; not settled |
| Chords: own or BTC? | 15 changes a minute against 28, 0% against 15% of them under a beat, the same root 54% of the time; both about 100% diatonic to the Rekordbox key, so that doesn't separate them | the colours follow the chords, so they stay on the own tracker; BTC is a lane |
| Part boundaries | on a bar line: own 19%, MusicFM 27%, SongFormer 55%, All-In-One 76%. Drops on a boundary (mean): own 72%, SongFormer 80%, All-In-One 84% | All-In-One's boundaries plus a cut at every drop (100%); SongFormer beat it on pop, not here |
| Which parts repeat | drop pairs with the same letter: own 26%, MusicFM 12%. All-In-One's 46% counts only a few labels, so chorus = chorus says little | letters from the own structure |
| All-In-One's labels at drops | chorus, inst and sometimes verse or intro: trained on pop, it has no drop or build label | a lane; the section rule keeps finding drops |
| Similar songs | nearest neighbour shares the top style: CLAP 41%, MusicFM 42%, MuLan 48% (against CLAP p = 0.10), chance 11%. MuLan's neighbours lean on tempo: within 3 BPM 72% against 55%, and with tempo held to 3 BPM its style edge goes (44% against 48%) | stays on CLAP |
| Style prompts | agree with Essentia's top style (balanced top 1): MuLan 37%, CLAP 5%, chance 9%. Function prompts fail: "drop" follows the rule's drops at AUC 0.65, CLAP 0.71, loudness 0.85 | song prompts from MuLan, CLAP as the fallback; no section prompts |
| Risers before a drop (61 held-out songs, 167 drops) | a riser ends in the last bar: DSP 4.8% at a 1.0% null, `fxhead` 17.4% at 2.6% (lift 4.8 and 6.7, intervals overlap; a tie at a 1% null). Only 27 of its 209 risers overlap the DSP ones. Empty fake drops fire 4-6%, and reversing the audio takes its lift to 1.03. Not checked by ear | its risers count only when they run into a drop: audio-based builds go from 94 to 163 of 501 drops, 56 start earlier (median 4 bars, at most 16), no other cue changes |
| Impacts on a drop | `fxhead` 6.6% at a 1.1% null, but empty fake drops score higher at every threshold: it may only hear kick and bass come back | a lane only, no flashes |
| Sound events | SED and PANNs rarely agree (10% and 31% of events overlap). SED finds the applause in a live recording and Scooter's crowd, but also long "air horns" that may be horn-like synths; Whoosh, Boom and Explosion stay near zero on club music | both feed `events`: where both hear it SED gives the start; SED alone must score 0.4, and its air horns must last 4 s or less. Not checked by ear |
| Instruments | only 65 of 175 songs get 20+ bass notes from YourMT3+ (median 10, Acraze's bassline 1), 155 get piano, 150 guitar; its singing is 93% inside the RoFormer's vocal regions and covers 62% of them | a lane |

Two things to watch when adding models. Scoring 10 s pieces back to back made BTC and SED change on
10 s multiples, so both overlap their windows by half now (BTC's chord changes on such a multiple
are down to 3.8%, about chance). All-In-One writes to `./spec` and removes it when it looks empty,
which breaks a second process in the same folder, so `allinone.py` gives each song its own folder.

Old repositories need patches, kept inside the analyzers rather than in the clones: BTC's
`yaml.load` without a Loader and NumPy's removed aliases, MusicFM's old `transformers.deepspeed`
import. `timm` pulls in the newest torch, so pin torch and torchaudio to the same build last when
installing (`.venv-sed` is separate for that reason).

## Tried and dropped

- **Music Flamingo** (`flamingo.py`, 5 songs, `pilots/results/flamingo.txt`): names the same synths on silence and on drums alone,
  and its section function agrees with the rule 20% of the time where a constant answer gets 27%. Its timestamps follow a regular grid.
- **Skip-BART** (`pilots/skip_bart.py`, 5 songs, `pilots/results/skip_bart.txt`, clips in `pilots/results/skip_bart/`): not beat-locked, and most of its
  variance comes from the seed. Our wrapper's fidelity to the authors' setup isn't proven, so this rejects it for club songs here, not in general.

## Waiting for hand labels

Nothing above has ground truth yet. The labelling view (`http://127.0.0.1:8766/#label=1`) writes `audio-model/data/melody/labels/<id>.json`:

- **Scoring:** `python -m analyze.fx_train.evaluate labels <fxhead features> audio-model/data/melody/labels` scores the effect head. The section analyzers still need a scorer.
- **`edm_finetune/`:** SongFormer retrained on EDM labels. On the rule's own labels it agrees with the rule 81.6% of the time on 33 held-out songs (All-In-One mapped 55.4%, always "drop" 39.3%), which only shows the pipeline runs: it learned the rule. The `hand` path has only run on fake files.
- **The rule's outros:** `viz/sections.py` calls everything after the last detected drop an outro, so a missed drop turns the rest of the song into one (Robin S 93%, Brejcha's Gravity 65%, Sandstorm 61%; 28 of 175 songs at 25% or more, median 9%). Real extended mixes have long outros too, so the labels should settle this before the rule changes. The labelling suggestions and the fine-tune's weak labels inherit it.

## Not run

lv-chordia (its torch pin looked unresolvable, BTC stands in) and Sheet Sage (a Docker
image). Licences of anything that ships: Essentia's models and ADTOF's are non-commercial, DrumSep
and the RoFormer state none, YourMT3+ is unclear, SED was trained on AudioSet, `fxhead` on mixed YouTube uploads, MuLan's and SongFormer's weren't checked; Whisper, Beat This!,
Basic Pitch, PANNs, CLAP, All-In-One, BTC and MusicFM are permissive.
