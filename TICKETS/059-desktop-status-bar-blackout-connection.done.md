# 059 — Status bar, global Blackout and connection state

| | |
|---|---|
| Epic | E9 Console UI |
| Depends on | 058 |
| ABI / showfile | none expected; check while implementing |

**Touches:** `app/components/status_bar.rs`, `app/connection.rs`, `app/ui.rs`, `crates/ui_egui/src/client.rs`, `crates/engine/src/persistence.rs` if save failures need more detail

## Problem
Testing found Blackout only existed on View Performance, and that a lost connection could still look healthy. Both are dangerous during a show.

## Scope
- One status bar on every desktop page, in this order: Blackout when active, showfile and save state, outputs, audio, BPM, engine load, plugin load, then pending routing, warnings and Remote UI. Problem cells link to the page that fixes them, and the warning count matches the Problems list.
- Blackout is derived from Grand Master at 0, using the existing `HostCommand::SetGrandMaster`. The UI remembers the previous level and restores it.
- `Cmd/Ctrl+B` works on every page, in text fields and behind dialogs, and ignores key repeat. Add a Blackout button to the Programmer page bar.
- Status bar, General and dialogs read one save state: Saved, Unsaved changes or Save failed. Failed saves name the reason. Start from what `ShowfileStatus` already provides.
- Offline strip takes its own row: live values dim and stop, controls pause, and commands report that they were not sent. Nothing is queued.
- Web UI (`AppClient::is_remote`): hide Shut down, Restart, Quit and the show's Load and Close.

## Out of scope
- Replies for every command (064).

## Acceptance criteria
- [x] Blackout is visible on every page and survives page changes.
- [x] Holding the key toggles once; a typed `b` still goes into a focused field.
- [x] Restore returns the previous level, including after changing Grand Master.
- [ ] With the engine stopped, no cell shows a live value and Blackout reports it was not sent.
- [x] A failed save never leaves "Saved" showing.
- [x] Load and Close warn about unsaved changes only when there are some.
- [ ] Remote clients can't reach the hidden actions.

## Verification
- `cargo clippy --all-targets --all-features`, `cargo fmt`, `cargo test --all-features`.
- UI: run with `--desktop` via `scripts/blenv` and compare screenshots against the mockup, in dark and light theme, at 1280x800 and 2560x1440. Confirm the 800x480 kiosk is unchanged.
- Test a remote client losing its connection.

## Notes
Mockup: any page with `?state=disconnected`, `?state=restarting` or `?remote`, and `chrome.js` for the behaviour. Decide what the Blackout cell shows when another client changes Grand Master. See `design/studio/RULES.md`.

### Implementation (2026-11-11, desktop only)
- Everything is gated on the desktop layout (`UiLayout::Desktop` / `Density::Compact`). Kiosk screenshots
  (Programmer, System at 800x480) diff to zero against `decouple` except the live speed graphs.
- `components/status_bar.rs` rewritten to the mockup (`chrome.js`, `studio.css .status`): 26px head-grey strip,
  gutter-separated cells: Blackout (red, only at GM 0, click restores), showfile + one save state (Saved /
  Unsaved changes / Saving… / Save failed with the reason as tooltip; click opens General), "Engine
  disconnected/connecting" cell when offline, Outputs (a square per hardware DMX universe + Art-Net, red and
  linked to DMX/Art-Net when one is down), audio level meter, BPM, Engine and Plugins sparklines, then right:
  LIVE, issues ("N issues" with error/warning icons, tooltip split, opens General), Remote UI session count
  (opens Remote UI). Offline: cells dim to 45%, values show `---`/`--`, sparklines stop sampling.
- Issues count comes from `BlaulichtApp::health_summary` (health.rs), the same data as the health indicators:
  errors = offline DMX universes, Art-Net down with a receiver enabled, crashed plugins; warnings = MIDI/serial
  devices in error. There is no separate Problems list yet (070/074); 074 should reuse `health_summary`.
- Blackout: `toggle_blackout` sends `HostCommand::SetGrandMaster(0)` or the remembered level
  (`blackout_restore`, the last non-zero GM seen while online, so a level set by another client is what gets
  restored). Decision for "another client changes GM": the cell shows whenever GM is 0, whoever set it, and
  restores the last non-zero level this UI saw. Offline it sends nothing and shows "Blackout not sent: engine
  disconnected". `Cmd/Ctrl+B` is taken out of the input events at the start of the frame (no repeat, works in
  text fields and behind dialogs). Programmer got a desktop page bar with a Blackout button
  (`studio::blackout_button`); View Performance's BLACKOUT uses the same toggle on desktop.
- Offline strip: on desktop the covering "Connection lost" dialog is replaced by a red 28px strip on its own row
  (with Reconnect now; auto-reconnect every 3 s kept) and the page is drawn disabled at 60% opacity. Commands
  sent while offline are refused by `RemoteClient::send` (nothing queued). The passcode prompt stays a dialog.
- General shows the same save state line (with the failure reason). Close warns about unsaved changes only
  when there are some; Load names unsaved changes when there are some. Remote desktop UI hides Close (Load
  and Save-to were already hidden, Quit/Restart/Shut down already hidden for remote).

### Verified (blenv, 1500x950, local desk)
- Ctrl+B on several pages, inside the Logs filter dialog's text field (typed "bob" stayed intact), held for
  1.5 s (toggled once); blackout cell survives page changes; GM 45% -> Blackout -> restore = 45%.
- Save to a path made a directory: status bar "Save failed" with tooltip, General "Save failed: Is a
  directory (os error 21)"; Close dialog wording clean vs dirty. Dark and light theme.
- fmt (container rustfmt on the touched files; the workspace has unrelated pre-existing fmt drift that was not
  committed), clippy (no new warnings in touched files), `cargo test --all-features`: all run in the blenv
  container with CARGO_BUILD_JOBS=4, all pass.

### Not verified / open
- No remote client could be run: the web bundle needs wasm-bindgen-cli, which is not in the container, and
  installing it plus a wasm build was too heavy for the laptop. So the offline strip, "not sent" notice, dimmed
  cells and hidden Close for a remote client were not seen running (criteria 4 and 7 left unticked).
- Only 1500x950 was checked (blenv's desktop window); 1280x800 and 2560x1440 not checked.
- "Pending routing" cell not added: there is no pending-routing state in the client yet (062).

