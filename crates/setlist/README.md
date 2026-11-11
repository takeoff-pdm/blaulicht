# Setlist: lights that know the track

If the setlist is known before the show, every track can be analyzed offline:
beat grid, bars and sections. The light engine then knows what comes next and
can do what a reactive engine cannot: ramp a build so it peaks exactly on the
drop, black out on the beat before it, and hit the drop look on its downbeat,
a few milliseconds early to make up for DMX and fixture latency.

Three pieces:

| Piece | Where | Does |
|---|---|---|
| Analyzer | `crates/setlist_analyzer` (`setlist-analyze`) | audio files to track maps + `setlist.json` |
| Format and logic | `crates/setlist` (`blaulicht-setlist`) | track map format, cue plan, cue runner, beat-clock and deck followers |
| Plugin | `crates/plugins/setlist` | loads the setlist, follows the DJ, sends the cues |

## Workflow

1. Analyze the setlist (files, folders or an `.m3u8` from rekordbox, in
   playing order):

   ```sh
   direnv exec . cargo run --release -p blaulicht-setlist-analyzer -- \
       --out ~/shows/club/setlist ~/Music/set.m3u8
   ```

   It prints what it found per track. Check it before the show:

   ```text
   [2/18] .../DJ Heartstring - In Der Nacht.mp3
     152.00 BPM, 6:08.7, 233 bars, grid fit on 933 beats ±1.9 ms
     downbeat on grid beat 0 (margin 1.29)
       0:00.0  bar   1  Intro      44.0 bars
       1:09.5  bar  45  Build       4.0 bars
       1:15.8  bar  49  Drop       48.0 bars  <<
       2:31.6  bar  97  Breakdown  40.0 bars
       3:34.7  bar 137  Build       8.0 bars
       3:47.4  bar 145  Drop       56.0 bars  <<
       5:15.8  bar 201  Outro      33.8 bars
   ```

   Fixes when something is off:
   - `--phase-hints FILE`: file-name substrings (one per line) of tracks whose
     grid sits on the off-beat; they move by half a beat. Same format as
     `crates/audio_engine/beat_eval_phase_hints.txt`.
   - `--downbeat 'In Der Nacht=2'`: bar 1 starts on grid beat 2 of that track
     (repeatable). Worth checking when the output says "unsure: check bar 1".
   - `-v` prints the per-bar features (kick, energy, low, high).

2. In blaulicht, add the plugin (`crates/plugins/bundle_debug/setlist.wasm`,
   built by `cd crates/plugins && make build_noopt`), open it, enter the path
   of `setlist.json` and press **Load**.

3. **Looks** tab: pick a scene per section kind (`-` keeps the previous look),
   the blackout length before drops (in half beats), an optional scene to
   show during the blackout (dimmers at 0, movers aimed at their drop
   positions: they travel in the dark), a hit scene added on the drop's first
   beats, the build ramp (master alpha from N % to 100 %, speed x1 to x4) and
   how early to fire.

4. **Follow** tab: switch on **Drive lights** and pick a source:
   - **Beat clock**: choose the track, press **Sync bar 1** on its first
     downbeat (or **Drop!** on a drop, or click the timeline). From then on
     the engine's beats keep it in time. With "audio sync" on in the Pro DJ
     Link plugin those beats come from the CDJs; "Take bar phase from the
     tempo source" also takes their bar phase.
   - **Decks**: deck messages on `127.0.0.1:9120` say what plays where; the
     plugin follows track changes, seeks and loops on its own.

While Drive is on, the plugin owns the overlay stack: each look replaces it
(`SetOverlays`), and an empty stack is a blackout (the render base is BLANK).

## Deck messages

One JSON object per UDP datagram to `127.0.0.1:9120` (see `src/deck.rs`):

```json
{"deck": 2, "track": "Artist - Title", "position_ms": 61234.5,
 "pitch": 1.023, "playing": true, "on_air": true}
```

`position_ms` is in the track's own time (independent of pitch), so it maps
onto the analyzer's grid whatever grid the sender uses. `track` is matched
loosely against title, tag title and file name; `index` (setlist position)
works too. Among live decks, one past its intro beats one in its intro beats
one in its outro, so the lights move to the incoming track once the outgoing
one reaches its outro; otherwise the current deck stays.

`scripts/setlist_deck_sim.py` plays a fake deck:

```sh
scripts/setlist_deck_sim.py --map ~/shows/club/setlist/02-dj-heartstring-in-der-nacht.json \
    --bar 43 --pitch 1.04 --jump-at 5:135
```

### Pro DJ Link (once the `prolink` crate is in)

The Pro DJ Link plugin already sees `TrackLoaded { track_id }`, beats, pitch
and (when joined) status with the on-air flag. To drive the setlist plugin it
needs to send one deck message per player and status/position update with
`bpf::send_udp("127.0.0.1:9120", json)`:

- `track`: title (and artist) from the CDJ's metadata (dbserver), or the
  setlist `index` from a rekordbox-id-to-setlist mapping;
- `position_ms`: CDJ-3000 absolute position packets, or beat number on the
  rekordbox grid times its beat length plus its first beat;
- `pitch`, `playing` and `on_air` from the status packets.

Nothing in the setlist crate or plugin has to change for that.

## Cues

`plan()` turns a map and the look settings into cues on grid beats:

| Where | Cue |
|---|---|
| each section start | its look |
| a build before a drop | ramp of the build scene (alpha, speed) until the blackout |
| drop − blackout | blackout, or the blackout scene |
| drop | drop look + hit scene |
| drop + hit length | drop look |

The runner fires cues as the position passes them (lead time added), resets
ramped scenes when a ramp ends, and after a jump (seek, hot cue, loop,
re-sync, track change) sets the look in effect instead of replaying
everything in between.

## How the analyzer works

- **Grid**: the beat model the live engine runs (`audio-model/`) over the
  whole track; the beat activation is folded over 100..200 BPM, the sharpest
  fold wins, and a least-squares line through the activation peaks gives
  tempo and phase. Constant tempo, like DJ software assumes.
- **Downbeat**: the model's downbeat output hardly separates the beats of a
  bar on club music, so the downbeat is the beat phase where the features
  change most (breakdowns and drops start on bar 1), with a 1.2x prior for
  the phase where the music starts.
- **Sections**: per bar, kick presence (bass level or a clear low-band
  attack) and energy; a Viterbi pass labels bars quiet / groove / peak with
  switches cheapest on 16-, 8- and 4-bar phrase boundaries; runs then become
  intro, groove, breakdown, build (up to 8 bars before a drop), drop
  (at least 8 bars) and outro.

## Status (2026-11-03, MVP)

Checked on the 18 club tracks in `~/Documents/cdj2000nxs-mp3`:

- Tempo agrees with `beat_eval`'s reference grid on all 18 (to 0.01 BPM).
- Beat phase: 16 agree within a constant 20..28 ms, now calibrated out
  (`ATTACK_CALIBRATION_MS`). "Take Me To The Sky" lands on the off-beat
  (known from `beat_eval_phase_hints.txt`, needs `--phase-hints`); "Another
  Year Alone" differs from the reference by half a beat, not checked by ear.
- Downbeats and sections look like club structure (drops on bars 25, 33, 49,
  97, 129, 145, ...), but have not been checked by ear yet.
- Plugin, end to end in a headless engine with simulated decks: cues reach the
  bus within -5..+20 ms of their beat with 30 ms lead; seeks catch up; a
  two-deck transition moves to the incoming track. Release build:
  ~0.15 ms per tick.

Not done yet: tempo changes within a track, per-track hand edits of cues,
listening tests, the Pro DJ Link sender, audio-only tracking (matching the
master output against the known tracks).
