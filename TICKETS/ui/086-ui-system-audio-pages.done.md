# 086 — Plugins, MIDI & Serial, Remote UI and Audio pages

| | |
|---|---|
| Epic | E13 UI port |
| Depends on | 069, 071 |
| ABI / showfile | none expected; check while implementing |

**Touches:** `app/pages/system/{misc,midi,serial,remote}.rs`, `app/pages/audio.rs`

## Problem
The rest of 038: these pages exist in the mockup (`system.html#plugins`, `#devices`, `#remote`, `pages.html#audio`) but not yet in the studio layout. Gaps listed in `design/studio/UI_GAP.md`.

## Scope
- Plugins: list and detail (status, file, reload on change), Show UI / Pop out, embedded plugin interface.
- MIDI & Serial: one table (device, kind, status, last activity, Reconnect).
- Remote UI: address, Access (switch, passcode with Show and Remove, warning without passcode), connected clients.
- Audio: Current input with Change device and Info, spectrogram, Input faders (horizontal, 082) and Auto-calibrate.

## Out of scope
- DMX & Art-Net (062).
- The kiosk layout (`Density::Compact`) stays unchanged.

## Acceptance criteria
- [x] Each page matches its mockup at the same window size, both themes.
- [x] No ComboBox left on these pages.

## Verification
- `cargo clippy --all-targets --all-features`, `cargo fmt`, `cargo test --all-features`.
- UI: screenshots per `crates/core/AGENTS.md` next to the mockup of each page.

## Notes
- Desktop only, every new path gated on `layout.is_desktop()`: `render_plugins_desktop` (new `system/plugins.rs`), `render_devices_desktop` (`midi.rs`), `render_remote_desktop` (`remote.rs`), `render_audio_desktop` (`audio.rs`); shared bits in new `system/desk.rs` (status square/label, `.err` text, pane at rect, "2 s ago"). Kiosk dispatch, pages and floating plugin windows untouched.
- Sidebar (desktop only): MIDI and Serial are one entry "MIDI & Serial"; `System_Serial` opens the same page. The kiosk keeps both tabs.
- Plugins: Show UI draws the plugin's UI embedded in the Interface pane; on the desktop the floating window for the main screen is skipped (`plugin_ui.rs`), Pop out uses the existing viewport, its Pop in returns it. Crashed plugins show an error line and "Reload plugin" (`HostCommand::ReloadPlugins`).
- Engine (no ABI change, PROTOCOL_VERSION 4 -> 5): `PluginState.reload_on_change` (from `enable_watcher`), `MidiHealth`/`SerialHealth.last_activity` (Unix ms, set on input), all `#[serde(default)]`.
- MIDI & Serial: status Connected / Available / Disconnected (error as tooltip). Reconnect has no per-device engine command; it reloads the plugins, which reopen their devices. Follow-up if a per-device reconnect is wanted.
- Remote UI: address from the listening socket, or the machine's local IP (UDP connect trick, nothing sent); port remembered while off. Passcode field applies on Enter/blur, Esc cancels, empty = no passcode. Remote UIs see Access read-only. Client column says "Remote UI" (the server does not record web vs native).
- Audio: Change device opens the studio dropdown (no ComboBox); Info still opens the existing analysis dialog (not restyled).
- Verified with kt (tools/blenv/KITTEST.md, not blenv) at 1500x950, dark and light, against headless-Chrome mockup shots: list/detail, Show/Hide UI with embedded inspector UI, crashed plugin, device dropdown pick, fader drag, passcode set/Show/Remove, access switch off/on, Reconnect click. Device rows and a connected client were checked with temporary fake data (removed before commit): the Mac has no MIDI/serial devices or remote clients. The Auto-calibrate switch click was not confirmed in the tree (no label). Pop out (extra viewport) can't be shown in kt.
- Kiosk: 800x480 kt shots of Audio, System General and Plugins show the old layouts; not pixel-diffed against a decouple build, the Compact paths are unchanged in code.
- Checks on the host with CARGO_BUILD_JOBS=4: clippy --all-targets --all-features (no new warnings), cargo test --all-features (all pass). `cargo fmt` with the host nightly reformats many untouched files on decouple; that churn was reverted and only the touched code is formatted (pre-existing kiosk hunks in audio.rs left as they were).


### Review fixes

- Audio > Input faders keep a local value in egui temp memory while dragged, so the engine's lagging echo no longer eats the drag delta. kt: drags of 59 px then 90 px on Gate moved it 0 -> 95 with the handle under the pointer (151 px travel).
- Audio > Input controls sit at fixed offsets so Boost and Auto-calibrate stay put when Gate's value gains a digit.
- Second review fix (Audio > Input spacing): the real cause was the fader slot being 175px, so the drawn scale (half a cap in from each end) was only 149px against the mockup's 173. The slot is now 199px (scale 173px), slot offsets stay 0/313/613, the switch moves to 916, and `HFaderStyle` got a `value_gap` (default 0, so every other fader is unchanged) set to 17 here so the value sits 36px after the scale as in the mockup. Measured from the pane edge in kt (1500x950) vs headless Chrome on `pages.html#audio`: Volume scale +25..+198 (mockup +25..+198), Gate scale ends +511 (mockup +511), Boost ends +811 (mockup +810), switch +929 (mockup +929), "Volume" name +264 (mockup +263). Kiosk: `hfader` with the default style draws as before (no extra space, readout at +6), and the Input row is only in render_audio_desktop, so Compact is unchanged (reasoned from the code, no kiosk shot).
- MIDI & Serial: the body under the table head (rows and the empty area, and the empty-state note) is painted in `t.list`. Remote UI > Connected clients: the whole body (table or "No remote UI is connected.") is on `t.list`. Checked with kt shots at 1500x950.
- All changes are in desktop-only functions (render_audio_desktop, render_devices_desktop, render_remote_desktop); the Compact paths are untouched.
- Checks on the host with CARGO_BUILD_JOBS=4: clippy -p blaulicht-ui-egui --all-targets --all-features (nothing in the touched files), cargo test -p blaulicht-ui-egui --all-features (171 pass). fmt churn outside the touched code left out again.
