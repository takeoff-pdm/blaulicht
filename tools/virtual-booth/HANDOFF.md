# Handoff: virtual DJ booth that speaks Pro DJ Link

Status: plan only, nothing built yet (2026-11-04). Written for the agent who builds it.

## Goal

A small desktop app on macOS (and Linux) that acts like a DJ booth with two CDJs and a
simple two-channel mixer. It plays tracks from a real, already analyzed rekordbox USB
export, and while it plays it puts realistic Pro DJ Link traffic on the network: the
same announcements, beat, status, position and mixer packets that real CDJ-3000s and a
DJM would send.

Why: blaulicht's setlist feature (`crates/setlist`, `crates/plugins/setlist`) needs to
know which track is playing and where, and that information comes from the CDJs over
Pro DJ Link. The `prolink` Rust crate that parses this traffic is coming soon. There is
no CDJ hardware at the desk, and rekordbox can't stand in: it only uses Pro DJ Link in
Export mode, as a library server for real players. In Performance mode its decks never
appear on the network. This booth is the test bed: you DJ in it (or replay a script),
and blaulicht sees what it would see in a club.

It's also the ground truth. The booth knows exactly what it played and when, so it can
log that and blaulicht's cue timing can be measured against it.

## Read first (blaulicht side)

- `crates/setlist/README.md`: what the setlist plugin needs from a deck, the deck
  message format, and the "Pro DJ Link" hand-off section.
- `crates/setlist/src/deck.rs`: the deck message (`deck`, `track`, `index`,
  `position_ms`, `pitch`, `bpm`, `playing`, `on_air`) and how the active deck is chosen.
- `scripts/setlist_deck_sim.py`: the current fake deck. It sends deck messages only and
  has no audio. The booth replaces it.
- `crates/plugins/pioneer_pro_dj_link/src/lib.rs`: the existing Pro DJ Link plugin and
  which `prolink` events it uses: `DeviceAppeared` (model, type, IP), `DeviceStatus`,
  `MasterChanged`, `TrackLoaded { track_id }`, and `Beat { beat, bpm, pitch,
  next_beat_ms }`. It listens passively for beats and joins as a virtual CDJ to get
  status. The `prolink` dependency points at a path that only exists on the Linux box,
  so this plugin does not build on the Mac yet.

## Scope

In v1:

- Two decks (CDJ-3000 by default, CDJ-2000NXS2 as a second profile) and one mixer
  (DJM-900NXS2 identity):
  - **Decks:** load, play/pause, CUE (CDJ behavior), hot cues A-H from the library,
    needle search on the waveform, 4- and 8-beat loops (with exit and reloop), beat jump,
    pitch fader with ±6/10/16/WIDE ranges, nudge, SYNC, and MASTER.
  - **Mixer:** trim, 3-band EQ, channel faders, crossfader and master level.
- Library: a rekordbox USB export (legacy Device Library: `PIONEER/rekordbox/export.pdb`
  and `PIONEER/USBANLZ/...`), with:
  - its playlists and tracks;
  - beat grids, cues and loops;
  - preview waveforms;
  - phrases (shown on the waveform).
- Audio: Web Audio, played to any output device (speakers, BlackHole or the existing
  Multi-Output Device). That way blaulicht's live beat tracking can hear the same audio.
- Pro DJ Link output, realistic enough that independent parsers can't tell it from real
  gear (packet list below), including TCP dbserver metadata in milestone 4.
- Headless script mode in plain Node. It needs no audio and no window, replays a JSON
  script, and runs on the Linux show box and in tests.
- Debug outputs:
  - blaulicht deck messages to `127.0.0.1:9120` (optional; usable before the `prolink`
    crate exists);
  - a ground-truth JSONL log.

Not in v1: master tempo/key lock (time-stretch), scratching audio, effects, headphone cue,
NFS (CDJs serve USB media over NFSv2; skip it), Device Library Plus / OneLibrary,
rekordbox's own encrypted `master.db`, artwork, MIDI controllers, or a look that copies
Pioneer's. Use plain, generic UI with no Pioneer logos or artwork. The model strings in
packets have to be real for protocol realism. That is fine for a local test tool; don't
distribute it.

## The test library (needs the user, do this first)

The agent can't drive rekordbox. Ask the user to:

1. In rekordbox 7, put the 18 test tracks from `~/Documents/cdj2000nxs-mp3` in a
   playlist. These are the tracks the setlist analyzer was checked on.
2. Analyze them with phrase analysis switched on.
3. Export the playlist to a USB stick, or to a mounted FAT32/exFAT disk image if
   rekordbox lists it as a device: `hdiutil create -size 4g -fs ExFAT -volname RBTEST
   ...`. The export has to include the legacy Device Library (`export.pdb`). rekordbox 7
   can also write Device Library Plus, which is out of scope here.
4. Copy the volume to a folder, for example `~/Documents/rb-booth-library/`. The booth
   takes that folder (or the mounted volume) as its library root.

Check the export before building on it: `PIONEER/rekordbox/export.pdb` exists, and each
track has `ANLZ0000.DAT` and `.EXT` files.

## Architecture

```
tools/virtual-booth/              (Node/TypeScript, own package.json, not in the Cargo workspace)
  packages/core/   pure TS, no Electron: library parsing, deck + mixer models, timeline,
                   packet encoders, network emitter, dbserver, script runner, ground-truth log
  packages/cli/    `vbooth run <script.json>`, `vbooth dump-library <root>`, `vbooth listen`
  app/             Electron: main (network via core), preload (narrow API), renderer (UI + Web Audio)
  fixtures/        golden packets, example scripts
```

- Electron, because a browser can't send UDP. Keep `contextIsolation` on; the preload
  exposes a narrow API. For the UI, Svelte fits the repo (`web/` uses it) but anything
  simple works. Use `vitest` for tests.
- **One timeline model drives everything.**
  - A deck's state is a list of anchors: `{ epoch_ms, pos_ms, rate, playing, loop? }`.
    A new anchor is set on every change (play, pause, seek, pitch, loop, nudge, hot cue)
    and once per second while playing, to correct drift between the audio clock and the
    system clock.
  - From the anchors, core extrapolates the playhead at any moment and schedules every
    packet: the next beat, the next position packet, status.
  - In the app, the renderer owns the audio clock and sends anchors to main with
    *audible* timestamps (`AudioContext.getOutputTimestamp()` plus `outputLatency`). IPC
    jitter then doesn't matter.
  - In headless mode, the script runner sets the anchors.
- **Audio.**
  - Decode each track fully with `decodeAudioData` (about 130 MB for a 6-minute stereo
    track; two decks is fine) and play it with `AudioBufferSourceNode.playbackRate`
    (pitch changes speed and key, like a CDJ with master tempo off).
  - Mixer chain per channel: trim, then EQ (low shelf, peaking, high shelf), then
    channel fader, then crossfader. Then master, then `AudioContext.setSinkId(device)`.
- **Precise sending.** Main sets a timer about 2 ms early, then spins with `setImmediate`
  until the target time. Measure the error and log percentiles.
- **Time bases.** rekordbox's grid is in rekordbox's decoded time. Chromium's mp3 decoder
  and symphonia (the setlist analyzer) may trim encoder delay differently, which shifts a
  track by tens of milliseconds. Measure the offset between the decoded audio and the
  PQTZ grid per track (kick onsets against grid beats). If Chromium differs, add a
  per-file offset so the audio matches rekordbox's grid. Report what you find: the
  setlist analyzer has the same question.

## Library parsing

- `export.pdb`: a DeviceSQL file. Read these tables: tracks, artists, albums, keys,
  genres, playlist tree, playlist entries.
- ANLZ files, at the path stored on each track row. The tags used:
  - PQTZ: beat grid. Each beat has a beat-in-bar number (1-4), tempo ×100 and time in ms.
  - PCOB / PCO2: memory cues, hot cues and loops.
  - PWAV, PWV3 or PWV5: waveforms.
  - PSSI (in `.EXT`): phrases. This tag may be XOR-scrambled in rekordbox 6+ exports.
- Use the Kaitai Struct definitions from Deep Symmetry's crate-digger
  (`rekordbox_pdb.ksy`, `rekordbox_anlz.ksy`), compiled to JavaScript, or the already
  compiled ones in `prolink-connect` (TypeScript, check its license). Don't hand-write
  the PDB parser.
- `vbooth dump-library` prints the playlists, then for each track: title, artist, BPM,
  length, beat count and first beat. That's milestone 0's check against what rekordbox
  shows.

## Network

Devices:

| Device | Number | Model string | Notes |
|---|---|---|---|
| deck 1 | 1 | `CDJ-3000` (or `CDJ-2000NXS2`) | |
| deck 2 | 2 | same as deck 1 | |
| mixer | 33 (`0x21`) | `DJM-900NXS2` | |

- **Interface.** Pick the interface explicitly (IP, prefix, MAC) and broadcast to its
  *directed* broadcast address (for example `192.168.1.255`), never to
  `255.255.255.255`. The blaulicht plugin's `transport.rs` explains why: the global
  broadcast leaves through the default route, which is often the wrong interface.
- **IP per device.** All three devices share one IP by default. Real gear has one IP per
  device, and some parsers key devices by IP. Optionally, allow a source IP per device
  through temporary interface aliases (`sudo ifconfig en0 alias A.B.C.D/32` on macOS).
  Test both. A permanent alias would be system config, which belongs in `~/nixos` (see
  Constraints).
- **blaulicht on the same Mac.** Its plugin host binds UDP 50000-50002 with a plain
  `UdpSocket::bind` and no `SO_REUSEPORT` (`crates/engine/src/plugin/udp.rs`). On macOS
  that means two processes can't share those ports. So:
  - **Two machines** (most realistic): booth on the Mac, blaulicht on the Linux box,
    with a cable or switch between them.
  - **Same host:** the booth binds nothing on 50000-50002 and sends from ephemeral ports.
    It can't hear blaulicht's announcements, so it sends status to its own IP on 50002
    as well as to any configured targets. Check early that macOS delivers your own
    broadcasts to local sockets bound on `0.0.0.0`.
  - Adding `SO_REUSEPORT` on the blaulicht side is a separate blaulicht change. Don't
    make it from this task; list it as a follow-up.
- **Safety.** Default to an interface with no real players on it. Before claiming 1, 2
  or 33, listen for 3 seconds. If a real device already uses the number, refuse to start
  (or move up) and say so loudly. Never run on a show network with real CDJs during a
  show.

### Packets

The protocol reference is Deep Symmetry's DJ Link Ecosystem Analysis
(https://djl-analysis.deepsymmetry.org/), especially its startup, vcdj, beats,
mixer-integration and track-metadata pages. The offsets below come from it. Check each
one against the page before encoding, and pin it with a golden test.

All packets start with the magic `51 73 70 74 31 57 6d 4a 4f 4c` ("Qspt1WmJOL"). The type
byte is at `0x0a`, and the device name is padded with zeros in the header.

| Port | Type | Sender | When | Notes |
|---|---|---|---|---|
| 50000 | `0a` | all | startup, 3× ~300 ms | initial announcement; CDJ-3000 is one byte longer, `0x21`=`04` |
| 50000 | `00`, `02`, `04` | all | startup, 3× each ~300 ms | number claim; CDJ-3000 uses `0x21`=`03` |
| 50000 | `06` | all | CDJ ~2.0 s, DJM ~1.5 s | keep-alive (`0x36` bytes); D `0x24`, MAC, IP, peer count, type `0x34` (`01` CDJ, `02` mixer); CDJ-3000 `0x35`=`64` |
| 50000 | `08` | owner | on conflict | defend 1, 2 and 33 when someone else claims them |
| 50001 | `28` | CDJ | each grid beat, playing, analyzed track only | beat (`0x60` bytes; next beat/2nd/next bar/4th/2nd bar/8th beat ms at 0% pitch from `0x24`; pitch `0x54`; BPM×100 `0x5a`; Bb `0x5c`; D `0x21`, `0x5f`) |
| 50001 | `28` | DJM | constantly | backup metronome at the master tempo, pitch +0% (blaulicht's plugin has tests for this) |
| 50001 | `0b` | CDJ-3000 | every 30 ms while a track is loaded (paused too) | absolute position: track length s, playhead ms, pitch ×100 signed, effective BPM ×10 |
| 50001 | see spec | DJM | on change + periodic | channels on-air (`0x2d` bytes) |
| 50001 | see spec | CDJ | on MASTER press | master handoff request/response; optional, after milestone 3 |
| 50002 | `0a` | CDJ | ~200 ms, unicast to every announced device | CDJ status, `0x200` bytes (CDJ-3000) or `0x11c` (NXS2) |
| 50002 | `29` | DJM | ~200 ms, unicast | mixer status (`0x38` bytes); F `0x27` `f0` master / `d0` not |
| TCP 12523 | | CDJ | on request | dbserver port query (`RemoteDBServer`), then dbserver on the returned port |

CDJ status fields:

| Field | Offset | Value |
|---|---|---|
| D | `0x21`, `0x24` | device number |
| A | `0x27` | activity |
| Dr / Sr / Tr | `0x28` / `0x29` / `0x2a` | source player / slot (`03` USB) / track type (`01` rekordbox) |
| rekordbox ID | `0x2c..2f` | track id from `export.pdb` |
| P1 | `0x7b` | `03` playing, `04` looping, `05` paused, `06` at cue, `09` searching, `11` end |
| firmware | `0x7c..7f` | firmware string |
| Syncn | `0x84..87` | sync counter |
| F | `0x89` | bit 6 play, bit 5 master, bit 4 sync, bit 3 on-air |
| P2 | `0x8b` | play state |
| pitch1..4 | `0x8c`, `0x98`, `0xc0`, `0xc4` | `0x100000` = 0 % |
| Mv | `0x90` | `8000` with a rekordbox track |
| BPM×100 | `0x92` | track BPM ×100 |
| P3 | `0x9d` | play mode detail |
| Mm / Mh | `0x9e` / `0x9f` | master meaning / master handoff |
| beat | `0xa0..a3` | beat counter |
| cue countdown | `0xa4..a5` | `01ff` = none |
| Bb | `0xa6` | beat in bar from the grid |
| packet counter | `0xc8..cb` | stays 0 on CDJ-3000 |

Behavior that real parsers rely on, all of which the booth must get right:

- Beat packets only while playing an analyzed track, sent on the beat as heard.
- Bb comes from the grid's own beat-in-bar, never counted locally.
- The beat counter matches the grid index. Check whether it is 1-based against a
  capture or beat-link.
- Status keeps flowing while paused.
- The on-air flag follows the mixer: channel fader above 0 and crossfader not fully away.
- Exactly one master. Syncn changes on a master handoff.
- The peer count follows the devices actually seen.
- Position packets keep coming while a CDJ-3000 is paused.

Make two realism knobs configurable:

- `beat_packet_offset_ms`, default 0, so blaulicht's lead-time compensation can be
  tested.
- a jitter model, default off.

### dbserver (milestone 4)

Real metadata lookup works like this:

1. A client connects to TCP 12523 and sends the `RemoteDBServer` query.
2. The booth answers with a port.
3. On that port, the client speaks the dbserver protocol: messages with magic
   `872349ae` and typed fields (numbers and UTF-16BE strings), for example a metadata
   request followed by a menu render.

Serve at least:

- title, artist, album, duration, tempo and key for the rekordbox IDs on each deck;
- the beat grid and cue lists;
- waveforms, optionally.

Accept requests from any device number. Real players only answer numbers 1-4 (1-6 on
CDJ-3000), which matters for blaulicht's virtual CDJ later. `prolink-connect` contains a
complete client for this protocol: mirror it as the server and use it to test.

Ask the user, or check the `prolink` crate once it lands, how blaulicht gets track
titles: through dbserver, or through NFS and `export.pdb`. If it's NFS, write that down
as the next gap rather than building NFS.

## Headless scripts and ground truth

`vbooth run session.json [--audio] [--log out.jsonl] [--deck-messages]`:

```json
{
  "library": "~/Documents/rb-booth-library",
  "profile": "cdj3000",
  "steps": [
    { "at": 0,   "deck": 1, "load": { "title": "In Der Nacht" } },
    { "at": 0.5, "deck": 1, "cue": { "bar": 41 } },
    { "at": 1,   "mixer": { "ch1": 1, "ch2": 0, "xfader": 0 } },
    { "at": 1,   "deck": 1, "play": true },
    { "at": 20,  "deck": 2, "load": { "playlist_index": 3 } },
    { "at": 21,  "deck": 2, "sync": true },
    { "at": 22,  "deck": 2, "play": { "on": "next_bar_of_master" } },
    { "at": 40,  "mixer": { "ch2": { "to": 1, "over_s": 8 } } },
    { "at": 60,  "deck": 1, "hotcue": "B" },
    { "at": 75,  "deck": 2, "pitch": 4.0 },
    { "at": 90,  "deck": 2, "loop": { "beats": 4, "for_s": 6 } },
    { "at": 120, "end": true }
  ]
}
```

- `--audio` plays through a hidden Electron window. Without it, the timeline runs on the
  system clock.
- **Ground-truth log.** One JSON line per thing that happened at its audible time
  (epoch ms):
  - load, play, pause, seek, loop, hot cue, pitch, sync and master;
  - on-air changes;
  - every grid beat (deck, track id, title, `pos_ms`, beat, Bb, effective BPM).
  
  With `--log-packets`, add every packet sent, with its type and send-time error.
- **`--deck-messages`** also sends blaulicht's deck JSON to `127.0.0.1:9120`. It uses the
  same timeline, so the setlist plugin can be driven by the booth before the `prolink`
  crate exists.

## Milestones

Each milestone ends with tests passing and a short note in this folder's README on what
works and how it was checked.

0. **Library and timeline, headless.**
   - Parse the export. `dump-library` matches rekordbox's track list, BPMs and lengths.
   - The deck model handles play, pause, cue, hot cue, loop, beat jump, pitch and sync
     against the grid, with unit tests on the timeline (beat crossings, loops, rate
     changes).
   - The script runner and the ground-truth log work without network or audio.
1. **Pro DJ Link presence, beats and status, headless.**
   - Encoders with golden byte tests.
   - Startup and claim sequence, keep-alive, conflict defense.
   - Beat, CDJ status, CDJ-3000 position, mixer status, on-air and mixer beat packets.
   - Check with an independent parser: a small `vbooth listen` built on `prolink-connect`
     shows CDJ-3000 #1, CDJ-3000 #2 and DJM-900NXS2 #33, and shows correct BPM, pitch,
     beat, Bb, play state, master, sync and on-air while a script runs.
   - Optional: beat-link-trigger (Java GUI) or Wireshark by hand.
   - Measure the send-time error. Target: p99 within ±3 ms of the scheduled time.
2. **Electron app with audio.**
   - Two deck panels:
     - waveform overview with the playhead;
     - zoomed waveform with grid ticks and phrase colors;
     - time, BPM, pitch and bar.beat;
     - the buttons from Scope.
   - Mixer in the middle, library browser at the bottom.
   - A network panel: interface choice, devices seen, packet counters, profile and
     realism toggles, and the deck-message toggle.
   - Measure that a beat packet leaves within ±5 ms of the audible beat (record the
     BlackHole output against packet timestamps, or compare against the anchors).
   - Report the per-track decoder offset (see Time bases).
3. **Master handoff and edge cases.**
   - MASTER button handoff packets.
   - End of track (P1 `11`).
   - Loading while playing.
   - A track with no grid (Tr `02`, no beat packets, BPM `ffff`).
   - Pitch at WIDE.
   - The NXS2 profile (no position packets, `0x11c` status).
4. **dbserver.** `prolink-connect` fetches title, artist, BPM, duration and beat grid for
   both decks from the booth.
5. **blaulicht end to end** (needs the `prolink` crate).
   - The Pro DJ Link plugin sees the booth's devices, beats and status, and can join as a
     virtual CDJ.
   - The setlist plugin follows track changes, seeks and loops in deck mode.
   - Cue times from blaulicht (inspector `tap`, see `crates/setlist/README.md` and the
     project memory's e2e recipe) compared against the ground-truth log. Write the
     numbers down.
   - If a blaulicht change is needed (for example sending deck messages from the Pro DJ
     Link plugin, or the macOS interface detection, which uses Linux-only `ip -j addr`),
     list it. Don't make it unless asked.

## Constraints

- Stay inside `tools/virtual-booth/`. The blaulicht Rust code and other sessions'
  uncommitted work (`crates/audio_features/...`) are not part of this task.
- Commit only when the user asks.
- No keep-awake daemons or `caffeinate` LaunchAgents. A short `caffeinate -i <cmd>`
  around one long run is fine.
- System-level changes (persistent IP aliases, network services) go through `~/nixos`
  (`sudo /run/current-system/sw/bin/darwin-rebuild switch --flake ~/nixos`), never as
  hand-made dotfiles or LaunchAgents. Temporary `sudo ifconfig ... alias` for one test is
  fine, after asking the user.
- The booth transmits on a real network. Follow the safety rules under Network.

## Open questions for the user

1. Any real CDJ or DJM captures (pcap) around, for example next to the `prolink` crate
   in `~/Downloads/prolink-test` on the Linux box? They would anchor the golden tests
   better than the spec alone.
2. Default profile: is CDJ-3000 + DJM-900NXS2 right, or should it match a specific club
   booth (the test folder is named after the CDJ-2000NXS)?
3. Metadata: does the `prolink` crate use dbserver or NFS?
4. Keep the booth in this repo (`tools/virtual-booth/`) or give it its own repository?

## References

- DJ Link Ecosystem Analysis, Deep Symmetry: https://djl-analysis.deepsymmetry.org/
- crate-digger (export.pdb and ANLZ Kaitai definitions):
  https://github.com/Deep-Symmetry/crate-digger
- prolink-connect (TypeScript Pro DJ Link client: status, dbserver, PDB):
  https://github.com/EvanPurkhiser/prolink-connect
- beat-link / beat-link-trigger (reference parser and GUI for manual checks):
  https://github.com/Deep-Symmetry/beat-link
