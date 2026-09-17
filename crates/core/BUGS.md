# Known Bugs

Findings from a full-codebase audit on 2026-09-16. Paths are relative to the
workspace root unless they start with `src/` (then relative to `crates/core`).
Checked items have fixes in the working tree; unchecked items remain open.

## Engine and output (loses light output or hangs)

- [x] **Art-Net output dies permanently on one send error.** `src/dmx/mod.rs:99-116`
  sets `artnet.socket = None` after a single failed `send_to`; nothing recreates it.
  A network flap or one unreachable receiver darkens every universe to every receiver
  until restart. Serial DMX has the same no-recovery path at `src/dmx/mod.rs:118-150`
  (that one at least logs and shows in health).
- [x] **Supervisor can hang forever on reload + device change.** `src/mainloop/mod.rs:236-240`
  stores `CONTINUE` after `plugin_manager.reload()`. If the supervisor stored `ABORT`
  during the reload and is blocked in `join()` (`src/mainloop/supervisor.rs:238`),
  the abort is overwritten and the join never returns.
- [x] **Panics in the engine thread are never detected.** Restart is keyed only on
  `CRASHED`, set only on the `Err` path in `src/mainloop/supervisor.rs:275-285`.
  No `catch_unwind` / panic hook / `panic = "abort"`. Any unwrap in the tick loop
  leaves DMX and plugins dead while the UI reports "running".
- [x] **Audio device failure takes DMX and plugins with it.** `src/mainloop/mod.rs:103-122`
  compiles and inits all wasm plugins before `audio::open_stream`. If the stream
  fails, `run()` errors and the supervisor retries every 2 s, recompiling every
  plugin each time and never producing output.
- [x] **Hot-reload of a half-written `.wasm` restarts the whole engine.**
  `src/plugin/mod.rs:271-289`, `src/plugin/wasm.rs:376-387`, `src/mainloop/mod.rs:231`.
  The watcher fires per `write()`; a truncated module makes `instantiate_plugins`
  fail, `init()` marks every plugin errored, and `reload()?` propagates to `CRASHED`.
  Same effect for one corrupt plugin at startup.
- [x] **Errored animation plugins keep ticking.** `src/plugin/tick.rs:216-225` builds
  the animation task list from `plugin_runtime_kinds` without the `has_errored()`
  filter the normal path uses (`tick.rs:356`). A trapping animation plugin traps and
  logs "Disabling plugin" ~80×/s forever.
- [x] **Reload never clears old instances.** `reload()` at `src/plugin/mod.rs:216-232`
  only re-inserts into `self.plugins`. A rebuilt plugin rejected by the ABI check
  (`src/plugin/wasm.rs:400`) leaves the old instance running with its error flag reset.
- [x] **Animation output deserialize panics outside `catch_unwind`.**
  `src/plugin/tick.rs:726` calls `AnimationTickOutput::deserialize`, which panics on
  bad bytes (`crates/shared/src/abi.rs:212-216`). With the missing panic detection
  above, one bad plugin write kills the engine. `drop_animation_instance`
  (`tick.rs:495-503`) is also unguarded.
- [x] **Scene Graph page holds the engine write lock for the whole frame.**
  `src/app/pages/scene_graph.rs:332` locks `dmx_engine.write()` until the function
  returns, blocking the DMX tick at UI frame rate while the page is visible.

Critical-fix verification (2026-09-17): core library tests pass, including
regressions for output recovery, reload shutdown, missing audio, malformed animation
output, plugin failure isolation/reload, and scene-graph edit merging. All-feature
workspace Clippy and the no-Wasmtime audio-mock build pass (existing warnings remain).
Scene Graph visual verification passed: screenshots of the empty page and a
populated two-node graph show the canvas rendering and timed transitions continuing
while the page is open. Physical serial-device reconnection has not been exercised. Original findings above are
retained as descriptions of the fixed failures.

## Wrong behaviour (silent)

- [ ] **SDK `SerialConnection::send` transmits MIDI.**
  `crates/plugin_framework/src/serial.rs:82-84` calls `bl_transmit_midi_safe`.
  Nothing ever reaches the serial port.
- [ ] **`ControlEvent::SetEnabled` and `ControlEvent::AddToProperty` are dropped.**
  No handler exists in core or shared; they fall into the "unsupported fixture event"
  arm at `crates/shared/src/fixture/state.rs:360`, and the test at line 388 codifies
  this. The console plugin emits both (`crates/plugins/console/src/lib.rs:624,639`),
  so those buttons do nothing.
- [ ] **`AssignPaletteToProperty` / `UnassignPaletteFromProperty` bypass
  `palette_assignments`.** `crates/shared/src/fixture/state.rs:1144-1151` rewrites the
  slot without updating `EngineSink::palette_assignments`; the integrity check then
  reports a stale pointer (`debug_assert!` at `src/dmx/mod.rs:330`) and unassign snaps
  the slot to `Literal(0)` instead of the resolved value.
- [ ] **Fixture swap breaks animation control.** `src/dmx/management.rs:504-520`
  rewrites selection keys without `sorted()` (unlike `move_fixture_to_group`,
  line 433). Lookups use sorted keys, so pause/remove/speed report
  "No animations for this selection".
- [ ] **Showfile with `start_addr == 0` corrupts the DMX start code.** `load_showfile`
  (`src/dmx/state.rs:106-114`) clamps only the upper bound (the UI paths at
  `src/dmx/management.rs:221,323` reject 0). Channel 0 is transmitted as the start
  code, so compliant receivers drop the whole serial frame. `universe_no >= 10` is
  also not rejected on load.
- [ ] **Deleting scene 0 causes a per-tick event loop.** `delete_scene`
  (`crates/shared/src/state/engine.rs:125-152`) allows removing the reserved scene 0
  and never prunes `views`. `SetSceneFocus` on a missing id answers with
  `SetSceneFocus(0)` (`src/dmx/mod.rs:856`), which is re-sent to the bus every tick.
- [ ] **Plugin tab selection keyed by `(0, tabs_id)` for every plugin.**
  `src/app/plugin_ui.rs:591` ignores the plugin id, so two plugins sharing a tabs id
  fight over one selection and one renders no content.
- [ ] **Nested plugin tab groups deadlock the UI thread.** Same block
  (`src/app/plugin_ui.rs:590-633`) holds the `plugin_ui_tabs_selected` write lock
  while recursing into `render_plugin_ops`; a nested `BeginTabs` re-locks it.
- [ ] **Pointer-palette depth off by one between `properties()` and `resolve()`.**
  `crates/shared/src/palette.rs:105-107` counts from the target,
  `crates/shared/src/fixture/value.rs:75-80` from the pointer. A four-deep chain
  shows as bound in the UI and renders black; `apply_to` uses a third depth.
- [ ] **Config file written non-atomically.** `src/config.rs:417-418` truncates then
  writes, on every autosave and showfile open. A crash mid-write leaves a config the
  app refuses to boot from. `write_atomic` already exists at `src/config.rs:293`.
- [ ] **Screen layout written back from a stale clone.** `src/app/ui.rs:190-193` and
  `src/app/external_screen.rs:672-686` clone the screen, render, then overwrite.
  Loading a showfile from the System page discards the loaded layout; removing
  screen 0 from its own tab writes its layout over former screen 1.
- [ ] **Startup showfile load races plugin init.** `src/main.rs:183-199` vs `244-253`:
  the supervisor spawns plugins before `read_showfile` fills `plugin_state_storage`,
  so a plugin loading state in `initialize()` may see an empty store.
- [ ] **Scene-graph pending transition to a missing node starves other edges.**
  `crates/shared/src/scene_graph.rs:987-998, 1032-1034`. Only reachable from a
  hand-edited showfile.

## Audio analysis

- [ ] **Band ceiling cached from the first frame.**
  `crates/audio_engine/src/signals.rs:414-427` caches `max_freq` once and never
  invalidates it. Silent bins carry `freq = 0`, so a silent first frame leaves mid and
  high bands at zero for the session, and silent bins always count toward the bass
  average (`signals.rs:432-441`).
- [ ] **FFT uses configured channel count / sample rate, not the device's.**
  `crates/audio_engine/src/audio_source/microphone.rs:143-176` keeps
  `channel_count = 2` and `sampling_rate = 48000` from `config.toml`. A mono or
  44.1 kHz device shifts every bin frequency and the bass band. Core side:
  `src/mainloop/audio.rs:332-336`.
- [ ] **Rayleigh tempo prior is unnormalised.** `signals.rs:1009-1029`: the prior
  peaks at ~0.02, so `bpm_confidence` never reaches the 0.5 fallback gate in
  `src/dmx/animation/audio.rs:19` and threshold sensitivity depends on capture
  period. Fix: multiply by `σ·e^0.5`.
- [ ] **`Signal::Volume` truncates instead of saturating.** `signals.rs:1165` casts
  `usize as u8`, wrapping at 256. Use `.min(255)`.
- [ ] **Onset period EMA absorbs capture gaps.** `signals.rs:575-583` feeds a whole
  stall into the EMA, giving wrong BPM for several seconds after any hiccup; the
  disconnect reset at `crates/audio_engine/src/collector.rs:524-543` does not clear it.
- [ ] **Capture stream errors are never recovered.** A dead cpal stream looks like
  permanent silence; `RELOAD` does not reopen it (`src/mainloop/mod.rs:171`).
- [ ] **File source stops decoding on the first `DecodeError`.**
  `crates/audio_engine/src/audio_source/file.rs:393-406` breaks instead of skipping
  the packet.
- [ ] **Noise mock diverges from the real source.** `has_new` always true, bass gate
  always saturated (`audio_source/noise.rs:257-259,308`); `src/mainloop/audio.rs:35`
  passes sample rate `41100` (typo for 44100).

## UI (crash or dead control)

- [ ] **Animations page panics after showfile load/close.** `selected_animation_id`
  is never cleared on load or close; Apply hits `get_mut(&id).unwrap()` at
  `src/app/pages/animations.rs:566-572` (also `:502-507`).
- [ ] **Overlay picker cannot be closed.** `src/app/pages/view.rs:190` passes
  `base_picker_open` instead of `overlay_picker_open`.
- [ ] **Numberpad ignores physical Enter / Backspace / Escape / digit keys.**
  `src/app/components/numberpad.rs:239` takes all events out before the
  `consume_key` calls at lines 281-301, which then scan an empty list.
- [ ] **Log "Auto-Scroll" toggle is overwritten every frame.**
  `src/app/components/log.rs:127-129` vs `:296-302`.
- [ ] **View creation panics at 257 views.** `src/app/pages/view.rs:245-247`
  `.expect("view id overflow")`; `animations.rs:121-139` handles the same case with a popup.
- [ ] **Doctest in `src/app/theme.rs:193` fails to compile** (references
  `catppuccin_egui`, which is no longer a dependency). `cargo test --doc` is red.

## Plugin host / SDK (lower priority)

- [ ] **MIDI clock / active-sense / SysEx dropped with a `warn!` each.**
  `src/plugin/midi.rs:142,177-186`: `Ignore::None` delivers them but only lengths 2
  and 3 are accepted; floods the log at up to 50 lines/s.
- [ ] **Tick input written to hard-coded guest address `0x10000`.**
  `src/plugin/tick.rs:524-542`: inside the wasm shadow stack; works only with the
  default stack size and tick inputs under ~960 KiB. Use a guest-exported buffer like
  MIDI/serial/UDP do.
- [ ] **SDK imports `controls_log/controls_set/controls_config` that the host never
  provides.** `crates/plugin_framework/src/blaulicht.rs:54-56`, used by `ui.rs:44-45`.
  Any plugin touching `plugin_framework::ui::Button` or `printc!` fails to instantiate.
- [ ] **`get_dmx()` panics when called from a plugin's `initialize()`.**
  `crates/plugin_framework/src/state.rs:26-33` has no `curr_len == 0` guard (unlike
  `get_udp`); `EngineState::deserialize(&[])` panics.
- [ ] **`sys` / `udp` host calls block the tick thread.** `src/plugin/wasm.rs:636-640`
  runs `bash -c` synchronously (5 s timeout); `wasm.rs:553-554` does synchronous DNS
  if given a hostname.
- [ ] **Dropped `Result`s from `system_sender.send` in `src/util.rs:18,31`.**
  Harmless today (unbounded channel), but hides a closed-channel error.


## Visual UI audit (2026-09-17, 800×480)

Viewed screenshots of all 11 main pages using audio-mock and an isolated showfile
with one fixture, two scenes, one animation, and a two-node timed scene graph.
Screenshots are in `/tmp/blaulicht-ui-audit/` (session artifacts, not committed).
Dialog interactions, System subtabs, and selected animation/node editors were not
inspected: inspector navigation cannot click those widgets.

- [x] **Fixtures Setup toolbar runs off the window; most DMX simulator buttons
  are hidden.** `src/app/pages/fixtures_setup.rs:912-923` emits every universe
  button into the same toolbar row. At 800×480 only Sim. DMX 0 and 1 are fully
  visible; the next button is cut off at the right edge. No horizontal scrollbar
  is shown. Evidence: `FixturesSetup-selected.png`. Use a universe selector or
  a bounded scrolling/wrapping toolbar.
- [x] **Fixture Performance clips scene-change details and hides Delete.**
  `src/app/pages/fixtures_perf.rs:486-493` places a long group/fixture/property
  label and Delete in one horizontal row in an already narrow right column.
  Even the default names are cut off after “Fixture”; Delete is outside the
  visible window. Evidence: `FixturesPerformance-selected.png`. Reserve width
  for the action and wrap/truncate the description inside the available column.
- [x] **Audio fader labels overlap neighbouring controls.**
  `src/app/components/fader.rs:163-165` allocates only the 120 px track width,
  then paints value/label beyond its right edge (`:246-267`). The Audio page's
  fixed 50 px spacers (`src/app/pages/audio.rs:903-930`) do not cover the label
  width: the Gate thumb overlaps the end of “Volume”. Evidence:
  `Audio-selected.png`. Allocate the complete widget bounds, including text.
- [x] **Audio auto-calibration switch has no label.**
  `src/app/pages/audio.rs:932-938` renders a bare switch after Boost, with no
  visible indication that it controls auto-calibration. Evidence:
  `Audio-selected.png`. Add an explicit “Auto-calibrate” label.
- [x] **Scene Graph countdown makes nodes and connections jump and clips text.**
  `src/app/pages/scene_graph.rs:148-153` appends a countdown only to the active
  node. Its header grows when activated, moving the output connector and edges;
  the right-hand node's countdown is clipped when it grows at the canvas edge.
  The fixed layout also reserves half the workspace for an empty editor
  (`:569-574`). Evidence: `SceneGraph-selected.png` versus `SceneGraph-next.png`.
  Reserve stable header/countdown width and consider collapsing the empty editor
  or fitting the graph using its expanded node bounds.


Visual fixes verified (2026-09-17): fresh 800×480 screenshots in
`/tmp/blaulicht-ui-fixed/` show all ten simulator buttons, wrapped scene-change
text with a fully visible 32 px Delete button, separated fader labels, an explicit
Auto-calibrate checkbox, and a fitted graph with stable node/pin bounds across
transitions. The empty node editor collapses until a node is selected; fitting
also runs when the canvas changes size. Before screenshots remain in
`/tmp/blaulicht-ui-audit/`. Core regression tests cover fader dragging to both
range endpoints, label spacing, stable countdown header bounds, and graph fitting.

## Audio page visual audit (2026-09-17, real capture)

Viewed the Audio page with real PipeWire capture (a track played into a null
sink whose monitor was the default source), because the noise mock saturates
every bin and hides these. Screenshots are session artifacts, not committed.

- [x] **Spectrogram top half is always black.** The input processor returns
  its bounded, interpolated spectrum (~900 entries for a 2048-slot buffer);
  `microphone.rs` pads the rest with defaults and `bin_spectrum_to_u8` binned
  the padding as dead bins. The spectrogram collector output now uses
  `bin_spectrum_to_u8_fitted` (`fit_spectrum: true`), which bins only the
  populated prefix and spreads it over exactly 128 bins.
- [x] **Black rows inside the spectrum.** The processor's cubic interpolation
  leaves empty slots wherever it overshoots below zero; a bin made of such
  holes averaged to 0. Fitted binning ignores holes and repeats the lower
  neighbour for an all-hole bin.
- [x] **Live spectrogram is aliased and drops beat markers.** With a 120 s
  window at 60 Hz (~10 columns per pixel) `advance_spectrogram_ring` drew only
  the column that crossed a pixel boundary and discarded the rest, so the
  image was vertical static and most beats never showed. Columns are now
  accumulated and averaged per pixel; beat/onset flags are OR-ed, matching
  the full redraw in `create_spectrogram_image`.
- [x] **Bins left a remainder strip and beat lines hid the spectrum.** Bins
  used `floor(height / bins)` rows (7 px unused at 160 px); `bin_rows` now
  distributes rows proportionally. Beat markers are blended (`draw_markers`)
  instead of painting opaque red over a whole pixel column; onset markers are
  a 4 px bar in the bottom padding instead of a 1 px dot.
- [x] **Audio page cloned the whole spectrogram every frame.**
  `audio.rs` cloned up to 7200 columns × 128 buckets per frame; it now holds
  the read guard while updating the texture.
- [x] **Time-series graphs render as dashed lines.** `TimeSeriesGraph::draw`
  emitted each Catmull-Rom sub-segment as its own `line_segment`; the uncapped
  ends left gaps on flat traces. It now adds one `Shape::line` polyline.
- [ ] **DMX/plugin audio column has the same dead-top-half and hole issue.**
  `COLLECTOR_DMX` still uses plain `bin_spectrum_to_u8` over the padded
  buffer, so plugins see ~55% empty bins. Left unchanged because bucket
  indexes are plugin-visible; switch it to `fit_spectrum: true` deliberately.
