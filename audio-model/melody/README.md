# Melody: MIDI for the Rekordbox songs, and an online Melodia

Two things, started 2026-11-03:

1. **Melody MIDI for every song in the local Rekordbox library** (249 songs, the sampler packs
   left out), with offline models. Files are in `audio-model/data/melody/` (gitignored):
   `melodia/`, `game/`, `student/`, `index.json` (song -> files), `auditions/` (40 s clips: the
   song turned down, the extracted melody as a synth on top).
2. **Melodia, online.** Melodia is offline by design. This directory has a streaming network
   distilled from it with a fixed lookahead, and a hand-built online variant of Melodia's own
   pipeline as a baseline.

## Results

**Song MIDI**: all 249 songs, five tracks each (`melodia`, `game`, `lead`, `bass`, `student`, see
below). No ground truth exists for these songs, so judge them by ear: `auditions/` has 40 s
clips of ten songs (deadmau5, Zedd, SHM, Dua Lipa, Ed Sheeran, ATB, EWF, Avicii, Garrix,
Lighthouse Family), one per method.

**Online melody** (frame metrics on the 10 ms grid, MIREX definitions; an estimate counts for
RPA/RCA only when it is voiced). Students: last checkpoint (15k steps), voicing offset 0.5
log-odds picked on 25 training songs. Online Melodia: tuned on 12 training songs (tau 0.8,
jump 2.4, voice 0.5).

| System | Latency | Songs vs Melodia OA / RPA / VFA | MUSDB vs Melodia OA / RPA | MUSDB vs vocal ref RPA / RCA / VFA |
|---|---|---|---|---|
| Melodia, offline (teacher) | whole track | 1 / 1 / 0 | 1 / 1 | **0.489 / 0.577 / 0.391** |
| online Melodia, lag 0 | 23 ms | 0.563 / 0.494 / 0.354 | 0.561 / 0.506 | 0.424 / 0.531 / 0.455 |
| online Melodia, lag 10 | 123 ms | 0.594 / 0.497 / 0.290 | 0.580 / 0.507 | 0.422 / 0.524 / 0.414 |
| online Melodia, lag 30 / 100 | 323 ms / 1.02 s | 0.596 / 0.498 / 0.288 | 0.581 / 0.507 | 0.422 / 0.523 / 0.410 |
| student, lookahead 0 | 23 ms | 0.690 / 0.600 / 0.202 | 0.690 / 0.617 | 0.511 / 0.606 / 0.335 |
| **student, lookahead 10** | **123 ms** | **0.714 / 0.640 / 0.197** | **0.711 / 0.640** | **0.517 / 0.608 / 0.333** |
| student, lookahead 30 | 323 ms | 0.717 / 0.642 / 0.195 | 0.713 / 0.648 | 0.515 / 0.609 / 0.343 |
| student, lookahead 100 | 1.02 s | 0.715 / 0.643 / 0.199 | 0.714 / 0.652 | 0.515 / 0.612 / 0.341 |

What it says:
- **10 frames of lookahead (100 ms) is enough.** 30 and 100 frames add nothing; no lookahead
  costs 2.5 points of OA.
- **The student agrees with Melodia on 71% of frames**, and gets Melodia's pitch within 50
  cents on 64% of its voiced frames. The rest is mostly Melodia's own track-level decisions
  (which instrument is "the melody" this second, octave choices fixed with seconds of context).
- **Against the vocal reference the student beats its teacher**: RPA 0.517 vs 0.489, RCA 0.608
  vs 0.577, fewer false alarms (0.333 vs 0.391). Imitating Melodia across 250 h of crops averages
  out some of its per-track mistakes. (The vocal reference marks instrumental leads as unvoiced,
  so its OA and VFA penalise every system for following a lead synth; compare RPA/RCA.)
- **The hand-built online Melodia is clearly worse** (OA 0.60 at best, vocal RPA 0.42). Most of
  Melodia's quality is in its offline contour stages; a salience function plus a tracker does
  not recover it, while the network does.
- Cost of the student: 0.34M parameters, 19.9M MAC per 10 ms frame (2 GMAC/s, about the stem
  student's 1.85). No Rust port yet.

Caveats: the "vs Melodia" numbers measure imitation, not truth; the only ground truth here is
the vocal reference, which is itself a monophonic tracker (SwiftF0) on MUSDB's real vocal stems
and covers vocals only. Training used 50 h of FMA; the student shared
GPUs with the stem sweep, so no hyperparameter search was done (one config, four lookaheads).

## Methods used for the song MIDI

| Method | What it transcribes | How |
|---|---|---|
| `melodia` | the predominant melody of the full mix, any instrument | Essentia `PredominantPitchMelodia` (Salamon & Gomez 2012), defaults, f0 segmented into semitone notes (`melodia_batch.py`) |
| `game` | the vocal line | separated vocals, then [GAME](https://github.com/openvpi/GAME) 1.0.3 large (openvpi, MIT, 2026) through its ONNX release, run by `game_batch.py` (the repo ships no ONNX pipeline; this follows its ONNX.md) |
| `lead` | the instrumental lead | Melodia on the "other" stem (mix - drums - bass - vocals), so a lead synth is followed even under vocals (`melodia_stems.py`) |
| `bass` | the bass line | Melodia on the bass stem with the salience grid moved down an octave (27.5-880 Hz, peaks 30-440 Hz) |
| `student` | the predominant melody, streaming | the lookahead-30 student below, run over each song |

Stems: `htdemucs_ft` for the first 51 songs (`logs/stems_htdemucs_ft_ids.txt` on ae-charlie),
plain `htdemucs` (4x faster, slightly worse) for the rest, because the GPUs were shared with the
stem sweep. The `student` MIDI comes from the lookahead-30 run (written before the lookahead-10
evaluation finished; the two score the same). Its notes use the same segmentation as Melodia's
(semitone rounding, median filter, 80 ms minimum).

Not used: **Sheet Sage** (melody of any instrument + chords, beat aligned). Its only supported
path is a 2022 Docker image (CUDA 10.1, torch 1.4: its Jukebox mode cannot use the Ampere GPUs
on ae-charlie; the CPU mode can), and pulling and running that image was blocked by the
session's permission check. Its trained models are CC BY-NC-SA 3.0.

## Is there an online Melodia?

Not as such. Melodia is offline in three places: contours are tracked forwards and backwards in
time, contour salience is thresholded against statistics of the whole track, and octave errors
are fixed against a 5 s moving average of the melody pitch. Essentia's implementation takes the
whole signal. Published real-time predominant-melody systems are different algorithms:
Goto's PreFEst (melody and bass lines, real time, 2004) and Arora & Behera's online harmonic
cluster tracking (IEEE TASLP 21(3), 2013, vocal melody). Learned streaming pitch trackers
(PESTO, SwiftF0's stream API) are monophonic.

So both online variants here are built:

- `online_melodia.py`: Melodia's causal front end (equal-loudness filter, 2048-sample frames,
  spectral peaks, harmonic-summation salience over 10-cent bins, Essentia's own algorithms) and
  a fixed-lag Viterbi tracker over 300 pitch states + "no melody" with a causal running
  voicing reference instead of the offline contour stages. Parameters tuned on 24 training songs
  against offline Melodia.
- `student_melody.py`: a network trained to reproduce offline Melodia from a bounded window.

## Student

Input: the stem student's front end, mono STFT n_fft 2048, hop 441 (100 frames/s at 44.1 kHz).
Magnitude is resampled to a log-frequency grid (20 cents per bin) and stacked by harmonic: for
each output pitch bin k (55 Hz * 2^(k/60), 300 bins up to 1745 Hz) the log magnitude at
h * f_k, h in {1/2, 1, ..., 8}. Five neighbouring bins share one network position (one
semitone, 60 positions, 64 channels); 8 blocks of [conv across positions (kernel 5) | dilated
conv over time per position (dilations 1..64, 1)], the time convs padded asymmetrically so the
future context sums to exactly `lookahead` frames (the stem student's scheme, checked by
perturbation). Head: 5 logits per position (back to 20-cent bins) + one "no melody" logit from
pooled features, softmax over 301 classes. 0.34M parameters, 19.9M MAC per frame (2 GMAC/s).
Latency: 1024 samples (half the analysis window, 23 ms) + lookahead x 10 ms.

Targets: offline Melodia on the same audio, resampled to the 100 fps grid; Gaussian soft
targets (sigma 20 cents) over pitch bins, one-hot "no melody" on unvoiced frames.

Data (all mono 44.1 kHz): FMA-medium clips (labelled while training ran; the loader rescans),
the dance tracks, MUSDB18-HQ train mixes (the stem work's teacher files), and the Rekordbox
songs except the held-out ones (song id % 6 == 0: 55 songs, also removed from the dance set).
6 s crops, batch 8, gain -9..+3 dB, AdamW 1e-3, cosine over 15k steps, bf16.

Evaluation (`train_melody.py`, `eval_melody.py`): MIREX frame metrics on the 10 ms grid,
overall accuracy (OA), raw pitch / raw chroma accuracy within 50 cents (RPA / RCA), voicing
recall and false alarm (VR / VFA), on
- the 55 held-out songs against offline Melodia (how well the student imitates it);
- the 50 MUSDB18 test mixes against offline Melodia, and against a vocal reference (SwiftF0
  on the isolated vocal stem, `vocal_reference.py`). The vocal reference marks instrumental
  leads as unvoiced, so compare RPA/RCA there; offline Melodia is scored the same way.

## Viewer (`viz/`)

A local research player: pick a song, play it, and watch the analysis and a simulated club light
show run with it.

    python3 audio-model/melody/viz/serve.py --open      # http://127.0.0.1:8765, stdlib only

Left: piano roll of the five note tracks (Melodia, GAME vocals, lead, bass, student), each with
optional pitch contour and its own synth voice; Rekordbox beat grid with numbered bars; a band
with the bar sections (crates/setlist kinds: Intro, Groove, Breakdown, Build, Drop, Outro) and
the Rekordbox phrases (fill-ins shaded); drops as red lines; stem loudness lanes; a whole-song
overview to click or drag. The audio offset against the analysis is measured on load (0 ms in
Chrome on the files checked) and can be changed with a slider.

Right: `club.js`, a three.js club in a 16:9 view (Fullscreen button for a 16:9 fullscreen view),
with a "Coming up" cue list below it (phrase changes, music changes, sections, build starts, blackouts,
drops, fill-ins, with bars to go). The room is 10 m wide, 17 m long and 4.5 m high: the stage (3 m deep) on a short
wall with uplights and a bass LED strip (the LED wall behind the DJ is gone); a tangled spider web of five flexible
5 m LED ropes (100 pixels each, 5 cm apart, behind a frosted diffuser: the shader blurs
neighbouring pixels so single LEDs never show) round a hub on the room's centre line between two
VP1s, each fixed to the ceiling at three points and hanging in two catenary loops of different
depth (0.34-0.75 m, lowest point 3.72 m). Roughly mirror-symmetric but messy: two mirrored pairs
that cross in an X (one towards the stage, one towards the back, with slightly different anchors
and sags left and right) and one rope across the hub, five crossings seen from below; every
anchor clears the VP1s and the downlight cans; acoustic-panel walls with cove LEDs, a bar,
a door; on the ceiling a grid of 32 downlights (4 across, 8 along) and four VaryTech VP1 LED panels
(warm/cold white, the engine's blinder; see `crates/shared/src/fixture/light.rs`) in a line along
the long axis down the middle. The truss moving heads and truss strobes are commented out for now.
Walk with WASD at eye height (Shift runs, the stage is a step up, walls and the bar block), drag to
look, wheel to step; Audience / DJ / Top presets. The show is a pure function of song time, so
seeking and pausing work; while playing, the visuals subtract the audio output latency
(`AudioContext.outputLatency`) so they match what is heard.

| What | Drives |
|---|---|
| Rekordbox beat grid, aligned to the drum hits, or replaced where the beat trackers show it's wrong (`grid.py`, 14 of the 175 electronic songs) | all pacing; pattern cycles are 1, 2, 4 or 8 beats and only change on 2-bar boundaries, so motion stays in phase; the crowd bounces a little on the beat and sways over two beats, about one in six hops in drops. Rekordbox's grid sits about 29 ms after the kicks in the audio we play (median over the 249 mp3s, likely the 25 ms mp3 encoder + decoder delay that ffmpeg and the browser trim), 40-60 ms in a few songs, and live-played songs drift up to 100 ms against its constant tempo. `grid.py` moves every beat onto the nearby strong drum hits (local median over +-8 beats where they agree, smoothed, three passes): median offset 0 ms in every song afterwards, the spread in Mr. Brightside from 80 to 11 ms. Phrases move with it. The club also renders each frame for the moment it reaches the screen (one frame ahead) |
| note grid | patterns don't glide: they jump 8 steps per cycle (sixteenths for 1-2 beat cycles, eighths for 4, quarters for 8), and every step starts a 60 ms flash; grooves jump a row per beat. Checked on a drop: every step lands on its sixteenth (within the 2 ms sampling), 64 of 68 strong drum hits within 15 ms of a step |
| drum hits (onsets in the drums stem) | every pulse, decaying in about 70 ms and weighted towards kicks and snares (hats only sparkle): uplights, tube segments, coves and downlights go from near dark to full on the real hits, so a kick that drops out stops the pulse and a snare roll flickers; in builds the downlight row steps towards the stage on every hit, so rolls speed it up |
| loudness (followers on the stems, fast attack, slow release) | the level inside a section: quiet moments dim; in grooves each spoke of the LED rope web meters one stem (bass, drums, other, vocals, mix) as a bar out from the hub, so you see which instruments play |
| music changes | drops, Rekordbox phrase starts, and bar lines where the stem levels of the two bars before and after differ by more than 0.2 (an instrument comes in or drops out; about one every 9 bars, most inside a phrase). Each starts a segment (median 6 bars) with a new downlight pattern, swaps the two palette colours, and swells the coves for a beat; the cue list shows them |
| key (Rekordbox) | one matched colour pair per song, plus white, no other hues: red + orange, pink + purple, magenta + cyan, blue + violet, cyan + blue, green + teal, picked by the key's place on the circle of fifths (minor keys by their relative major; neighbouring keys share or neighbour a pair) |
| clean melody line (`melody_line.py`) | the melody plays across the ceiling in every section but builds: each note lights two lamps of the row its pitch maps to (the line's low notes near the stage), alternating sides, flashing on the onset and holding at its velocity while it sounds; on the LED rope web each note is a soft blob on the next spoke round the hub, as far out as its pitch is high; notes take the pair's second colour, whiter at their onset |
| chord root (clean bass line) | chord changes swap which colour of the pair leads (roots an odd number of fifths from the tonic lead with the other one), so colour follows the harmony without new hues |
| hooks (recurring 2-bar melody blocks) | the same hook always gets the same pattern and colours, so a drop's call and response looks the same when it comes back (checked on Acraze - Do It To It: every hook keeps its pattern across both drops) |
| melody activity per 2 bars | pattern speed (note onsets per beat -> cycle length) |
| segment melody | the downlight pattern in drops: wide range -> rings, stepwise -> diagonal waves, leaps -> sparse checkers, sparse -> fans / sweeps along the room, never twice in a row. All are narrow: at most about a quarter of the 32 downlights are lit at once |
| sections | level: breakdowns dim and slow, grooves chase half a row away from the stage, drops full and fast; Rekordbox's club phrases nudge it (Chorus up, Down down). Tuned for electronic music only: the stem rule (kick and bass return after a breakdown) is a club-track rule |
| LED rope web (5 spokes x 100 pixels, pixel 0 at the hub) | builds close in from the outer anchors towards the hub, whiter towards the drop, flashing on the drum hits; drops play the hook's pattern on the note grid like the ceiling (a ring running out from the hub, the same a fifth later per spoke as a spiral, stepping 5 cm blocks, a ring closing in on the hub, one spoke after another round the hub), flashing on each step and on the kick; grooves meter one stem per spoke out from the hub; calm parts a faint slow gradient; melody blobs on top, as far out as the note is high. Each rope lights the room with a point light in its average colour (area lights per rope cost a third of the frame rate) |
| drops (known in advance) | strobes only on the drop, when the bass hits, in one of four types per drop so drops differ: VP1 blinder, rope spin (each flash a 30% segment of one rope, rope by rope round the hub, the segment wandering along it), rope pairs (each flash two 30% segments on two different ropes), and VP1 + rope pairs for the biggest drop (the longest breakdown before it, 8 bars or more); the ropes never flash all at once, and consecutive drops never share a type. Timing: sixteenths (cold + warm white) from its downbeat for up to one bar, easing from full power, for as long as the low end stays within 6 dB of the first beat (a full bar for all 71 drops in the first 40 songs); while they fire every other light, the ambient light and the bar are off, so the strobes are the only light; nothing strobes before it. Over the build one downlight row runs towards the stage and the rope web closes in on its hub; blackout on the last half beat. The grid downbeat is the hit: the drums + bass stems jump within 40 ms of it for 370 of 490 drops, and offbeat basslines (hard dance) follow the kick by half a beat |
| smoke machine (stage edge, below the booth) | fires a long soft fill (up to 6 s) at the start of every breakdown of 4 bars or more and a short punchy blast (1 s) on every drop's downbeat; the selector under the club picks breakdowns, drops, both or off. The Fog button (or holding F) fires it by hand, whatever the selector says. Its particles leave the nozzle with the jet's speed and slow down, then within a few seconds spread out through the whole room, growing and thinning into an even haze, all computed from the song time; they are additive in the room's current light colour (white in a strobe flash, invisible in a blackout), so smoke only shows where light hits it. The haze it leaves (rising while it fires, clearing with a 30 s time constant) makes the downlight beams up to twice as strong |
| stem loudness | uplights (drums; two of four per beat), bass strip (bass, coloured by the bass note), DJ spot (vocals) |

Melody for the lights (`melody_line.py`, server side): the raw transcriptions are honest but poor
light cues. On the 124 electronic songs, 4-15% of their notes are shorter than a sixteenth, GAME
splits 23% of held vocal notes, and only 5-10% of onsets sit on the beat grid. Per source, the
pitch is re-read from the pitch contour with the song's tuning taken out (most songs are within
8 cents of A = 440 Hz, but 5 are a quarter tone off, e.g. L'Amour Toujours -47 cents, Modjo +42,
where every semitone rounding was a coin toss; their lines went from 43-74% to 89-100% in key),
same-pitch pieces are merged unless the stem has a new onset at the joint, octave jumps are
folded, onsets and ends snap to the sixteenth grid, and blips (shorter than 60% of a sixteenth,
out-of-key and short, or silent) are dropped; the key filter only applies to a source that fits
the key (75% in key), so a doubtful Rekordbox key doesn't eat real notes. The main line takes the
vocal where it covers 30% of a bar and the lead otherwise. Result: 0% blips, 87% of melody and
84% of bass onsets on the sixteenth grid, the remaining same-pitch repeats (14%) have a real onset.
The viewer shows the clean "Melody line" and "Bass line" by default (tick synth to hear them
against the song); the raw tracks and the online student are off by default.

The song list shows electronic tracks only (124 of the 249; tick Pop to include the rest). Only 13 songs carry a Rekordbox genre tag, and Rekordbox's phrase mood is a poor proxy (78% agreement; "high" includes Ed Sheeran and Katy Perry, "mid" includes ATB, Darude and Paul van Dyk), and an audio rule (vocal share, instrumental stretches, tempo drift) reached 85%; so the songs are labelled by hand in `data/melody/viz/genres.json` (artist and track; dance-pop by DJs and producers such as Calvin Harris, David Guetta, Avicii counts as electronic, singer-led pop as pop). Edit that file to move a song; songs missing from it fall back to their genre tag, then to the mood.

Strobes can be switched off under the club. Sections and drops come from `sections.py`: the
hindsight drop rule of `audio-model/drops.py` on the drums + bass stem loudness per bar.

Data: the viewer only reads compiled songs, and `pipeline/README.md` has the whole path from Rekordbox to there:
`song.py add` pulls one song and runs its analysis, including `pipeline/build_viz_data.py` on ae-charlie, which packs its
notes (read back from the exported MIDI), contours and loudness (~0.5 MB per song);
`pipeline/rekordbox_show.py` reads beat grid, BPM, key and phrases from a copy of the Rekordbox database;
`pipeline/compile_song.py` merges them with the analyzers' features into `audio-model/data/melody/compiled/<id>.json`.
`serve.py` serves that file and the mp3 from its Rekordbox path (only compiled songs, bound to 127.0.0.1), and
picks up newly compiled songs without a restart. three.js 0.160 loads from jsDelivr. The page keeps the song
in the URL (`#song=<id>`). Keys: Space, arrows (Shift for 1 s), +/-, 1-5.

## Show compiler

`analyze/` holds 22 offline analyzers (drum pieces and drum transcription, two sound event models and a
trained effect detector, beats, style / mood tags, two chord trackers, four takes on structure, polyphonic and
multi-instrument transcription, cleaner vocals, lyrics, two text-audio embeddings, stereo, timbre, risers /
impacts / silences); see `analyze/README.md` for models, licences, timings and how each compares.
`pipeline/show_compile.py` merges them per song into one cue list (builds with their real start and
intensity curve, the audio's own pre-drop silences, drum hits per piece checked against a second
model, impacts, sound events, chord stabs, chords, structure parts on phrase lines, lyric lines,
pump, stereo, a style preset, a beat-grid check), the viewer shows each as a lane, and the club
plays the cues (kick + snare pulses, hi-hat sparkle, impact and stab flashes, rope looks for air
horns, sirens and whooshes, smoke on booms, looks by structure letter, uplights leaning towards
the synths' side, faster patterns for energetic songs). BTC's chords, MusicFM's structure, All-In-One's
function labels and YourMT3+'s instruments are viewer lanes only, and so is the effect head except for
risers running into a drop; the README in `analyze/` says why.

## Looks

The menu under the club sets what the room listens to; three toggles work with any look (remembered per browser). They live in `viz/club_looks.mjs`,
which has no three.js in it: a look writes one frame of fixture values onto a plain desk and `club.js` draws the desk. Blackouts and strobes are handled before any look runs, so
strobes stay on the drop, up to a bar, as the only light. Every look uses the song's colour pair plus white.

| Look | Follows | What it does | Beat depth | With the kick | Lamps lit, 95th pct | Drop / breakdown |
|---|---|---|---|---|---|---|
| **Club** (default) | everything | the original show, unchanged | .081 | +.69 | 31% | 1.8 |
| **Club 2** | whichever part leads | Club without the clutter, one job per fixture: ceiling and walls the leading part, ropes the next one, uplights kick and snare, floor strip the bass, DJ spot the vocal. Patterns and colours hold through a section | see below | | | |
| **Mixing Desk** | stems | drums the ceiling, bass the floor strip and uplights, synths the ropes, vocals the DJ spot and walls; no section logic | .040 | +.17 | 34% | 1.4 |
| **Score** | notes, ahead of time | piano roll: notes cross the ceiling a row per eighth and land on the stage row as they sound; on the ropes the hub is now and the anchor a bar ahead | .035 | +.13 | 19% | 1.4 |
| **Shockwave** | drum hits | a kick crosses the room in one beat, a snare rings out from the hub, chord stabs ring in, crashes and impacts roll back from the far wall | .151 | +.75 | 25% | 2.5 |
| **Breathing Room** | sidechain | glows between kicks and ducks on each one, by the measured pump depth; wide and dim | .084 | -.67 | 100% (at most .21 brightness) | 2.2 |
| **Kick Room** | drums only | one colour plus white; no drums, no light (black 24% of the time, up to 62%) | .495 | +.86 | 31% | 5.3 |
| **Stained Glass** | chords | each root owns an area of ceiling on a circle-of-fifths ring; minor or major picks the colour, far from the key goes white; no pulses | .004 | -.05 | 28% | 2.1 |
| **Spotlight** | voice | Club steps back, starting just before someone sings; warm spot, rope glow on the pitch, chops flicker. Uses when a voice sounds, not Whisper's text | .062 | +.65 | 34% | 2.0 |
| **Weather** | timbre | no beat: brightness lifts light from floor to ceiling, noise whitens it and fires the smoke (6 times a song), stereo moves it sideways | .001 | +.04 | 25% | 3.2 |
| **Human Lighter** | a person's map | a community Beat Saber map's lighting replayed: back lasers on the stage wall and its spill, rings on a quarter of the ceiling (spin steps the rows, zoom swaps inner and outer lamps), lasers as 30% bands running along the ropes at the laser speed, track lights on the walls and floor strip. 25 songs, 38 maps; other songs stay on Club. Numbers from those 25 | .080 | +.21 | 25% | 1.3 |
| **Director** | all of them | a look per section: a home look from the song's style on drops and grooves, Shockwave and Countdown in builds, Spotlight, Stained Glass or Weather in breakdowns. Time: Spotlight 26%, Shockwave 19, Breathing Room 14, Weather 14, Score 11, Kick Room 9, Stained Glass 4, Mixing Desk 2 | .056 | +.21 | 28% | 1.5 |

Club 2 came from watching Human Lighter on Lean On: the human show looked calmer and followed the notes better. Map `29a78` follows the synth's chord stabs, while our melody line takes the vocal (all of it in both drops) and Club's flashes follow the snare and hi-hats. So each section's lead is the part (vocal, stabs or synth line) with enough notes and the steadiest rhythm from bar to bar; loudness can't tell, because on Lean On the vocal is at least as loud in every section. This is a heuristic tuned by eye on a few songs, and the lighters themselves disagree about what to follow. Over all songs it picks the synth line for about half the bars, the vocal for a third.

On-screen brightness from the audience camera, in drops and grooves, correlated with each part per 16th (a rendered measurement in the browser, on three songs):

| Song, look | Mean brightness | Leading part | Kick | Snare |
|---|---|---|---|---|
| Lean On, Club | .097 | stabs .35 | .44 | .65 |
| Lean On, **Club 2** | .094 | stabs .66 | .10 | .28 |
| Lean On, Human `29a78` | .148 | stabs .63 | .25 | .15 |
| Ride It, Club | .074 | melody .26 | .68 | .52 |
| Ride It, **Club 2** | .074 | melody .53 | -.09 | -.13 |
| Ride It, Human | .192 | melody .26 | .39 | .21 |
| Sandstorm, Club | .041 | melody .33 | .43 | .12 |
| Sandstorm, **Club 2** | .024 | melody .55 | .20 | .04 |

These were measured before Lean On's grid was corrected: Rekordbox's sat a 16th early, so Club's grid-stepped patterns ran off the kick on that song, and its row needs measuring again.

Sandstorm shows the weak spot: with a single synth line and no stabs Club 2 is about half as bright as Club. Human Lighter stays brighter everywhere, because its lights stay on between events.

Human Lighter is a reference to compare the other looks with, not a show to keep: it ignores the music and only plays what a person programmed. Colours become the song's pair, your strobe and blackout at each drop still apply, and Slow Burn and Countdown leave it alone. Parts of our version the map doesn't cover stay dark (16.7% of frames on average, up to 50% on a song). The flash and fade times (0.6 s, 1.5 s) are from memory of the game, and the fixture mapping is a choice. Lightshows are built from `decision/bs/maps/` by `decision/bs_lightshow.py` (`python3 audio-model/melody/decision/bs_lightshow.py`, then restart `serve.py`) into the gitignored `data/melody/viz/lightshows/`. Maps are other people's work, so they stay out of the repo; the viewer credits each by its BeatSaver link.

Maps are aligned with one constant offset, so one made for a shorter edit can stay on the beat and lose the song's structure. The map menu's tooltips show how each follows our loudness in its first and second half, and marks a negative number as "may have drifted". That can also just be a lighter working against the loudness; nobody has checked by ear.

`check_looks.mjs` also tests the replay against each map's raw events, working out the expected state separately from the look: 68,000 events right after they fire, 3,210 later states (a flash settling, a fade half gone and gone), 880 ring spins, 812 zooms and 186 laser speeds, with no mismatch. It skipped 2,173 events inside a blackout or strobe, and events with another of their group within 10 ms.

Toggles:

- **Déjà Vu:** a repeat of a part replays its first appearance's pattern, step and colour lead (same in 99.6% of 8,491 compared moments; repeats cover a median 37% of a song). Every drop counts as the same music and all of a song's drops get one strobe type, its own, because on the Beat Saber maps lighters treat a song's drops alike (`decision/notes.md`). So it gives up variety between a song's drops.
- **Slow Burn:** one fixture group in the intro, another with each section, all of them only on the biggest drop (longest breakdown before it), then they retire. Other drops strobe at most 2 beats and never on the VP1s.
- **Countdown:** in the last 8 bars of a build a spine of lamps goes out a row at a time, the last bar counts its beats on the front row, and the ropes fill towards the hub.

`decision/check_looks.mjs` runs all of this headless. On the 175 electronic songs at 25 fps, every look alone, every toggle on Club and every look with all toggles (25.4 million frames): 0 violations of the strobe window, a dark room while strobing, rope flashes of at most two ropes and 30%, the colour pair, or finite values, and no crash on a song with no analysis at all. Notes reach the stage row on their onset (82,335 notes); the back row peaks 0.876 beats after the front (1,652 isolated kicks, 7/8 by design). Beat depth is how much of the light level moves with the beat. Lamps lit counts brightness of at least 0.1. Drop / breakdown is the median ratio of total light. The cached cues it reads predate the effect head in the builds.

Club is unchanged: every fixture value matches the code before the split on 3 songs (5,094 frames, every drop and blackout), and `decision/harness.cjs check rules` is identical on all 175. Nothing here says a look is good: I checked still frames by eye, and nobody has watched them in motion with the music.

    python3 audio-model/melody/decision/build_club.py                             # after changing club.js
    node audio-model/melody/decision/check_looks.mjs --fps 25 --out a.json        # about 5 minutes with --shard i/8 in parallel
    node audio-model/melody/decision/check_looks.mjs --report a.json

## Labelling

`http://127.0.0.1:8766/#label=1` is the viewer's labelling mode, for the first ground truth: sections and effects per song,
from `audio-model/data/melody/labels/queue.txt` (175 electronic songs, shuffled), saved to `labels/<id>.json`. About 50 songs give a first score of
the analyzers; 100 is the target, and enough to try `analyze/edm_finetune/` with `--labels hand`.

Sections: I intro, U buildup, P drop, K breakdown, O outro, L silence. Q / E jump between boundaries with a 2-bar pre-roll (Shift: none), G replays,
C splits (Shift: on a beat, Alt: free), X deletes, `,` `.` nudge a bar (Shift: a beat), drag moves, ⌘Z undoes. Effects: R riser, T downlifter,
H air horn, N siren, V other (press twice to open and close), M impact, Shift+X removes. Enter saves as done and goes on (a second Enter if some boundaries weren't heard yet), Shift+Enter leaves it for later.

The suggestions come from `pipeline/sections.py`, whose long outros are a known weak spot (`analyze/README.md`). Correct them rather than trust them.

## Decision models

`decision/` asks whether a model should decide the show, not just analyse the song: `decision/README.md` has the verdict and `decision/notes.md` the evidence.

## Files

| File | What |
|---|---|
| `melodia_batch.py` | Melodia f0 + notes MIDI for a song list |
| `game_batch.py`, `run_game_follow.sh` | GAME vocal transcription on separated vocals |
| `render_audition.py` | audition clips |
| `export_midi.py` | collect MIDI under readable names |
| `melodia_labels.py` | Melodia teacher labels on the 100 fps grid |
| `vocal_reference.py` | MUSDB18 vocal reference pitch |
| `student_melody.py` | the streaming student |
| `train_melody.py`, `melody_common.py` | training, data, metrics |
| `online_melodia.py` | online Melodia baseline: salience + fixed-lag Viterbi, tuning, eval |
| `eval_melody.py` | final evaluation + student MIDI |
| `pipeline/` | from Rekordbox to a compiled song, with its own page (`ui.py`) and command (`song.py`), checks and corrections (`pipeline/README.md`) |
| `viz/serve.py`, `viz/viewer.html`, `viz/club.js` | the viewer and club simulator |
| `viz/club_looks.mjs` | the club's looks and toggles |
| `viz/label.js` | the labelling mode |
| `analyze/` | the analyzers, pilots (`analyze/pilots/`), effect head training (`analyze/fx_train/`), SongFormer fine-tune (`analyze/edm_finetune/`) |
| `decision/` | plan layer, Beat Saber comparison and lightshow build, and the A/B listening test |
| `../charlie/` | package versions, clone commits and run scripts from ae-charlie, and what's still only there |

Everything ran on ae-charlie in `/localhome/sokrates/melody` (`code/`, `data/`, `out/`, `runs/`,
`logs/`; venvs `.venv-ess` Essentia, `.venv-game` GAME, `.venv-ort` SwiftF0, `.venv-train` torch).
