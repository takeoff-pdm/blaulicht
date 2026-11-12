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
- [x] With the engine stopped, no cell shows a live value and Blackout reports it was not sent.
- [x] A failed save never leaves "Saved" showing.
- [x] Load and Close warn about unsaved changes only when there are some.
- [x] Remote clients can't reach the hidden actions.

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

### Review fixes (2026-11-11, desktop only)
- Programmer page-bar Blackout now matches `.okblack`: always filled #b80f0f (hover #d40f0f), white
  text, 28px. It reads "Restore" while Grand Master is 0 and "Blackout" otherwise (as `blk()` in the
  mockup); no ring, since the mockup never sets `.okblack.on` there.
- Messages: new `components/notes.rs`, a minimal desktop notification stack (bottom right above the
  status bar, max 4, info 5 s paused on hover, errors stay with a close button, repeats count "x2").
  Blackout shows "Blackout on" (error) and "Grand Master restored to N%" (info) from Ctrl+B, the status
  cell and the page-bar button; offline it shows "Blackout not sent: engine disconnected" as an error
  card instead of the centred popup. Kiosk keeps its popup path. 070 should grow this stack, not replace it.
- "N routing pending" status cell added left of the issues (changed colour, opens DMX), from
  `BlaulichtApp::routing_pending`. Nothing sets it yet: 062 owns the pending routing state. Verified
  with a temporary count of 2 (reverted).

### Verified after review (blenv)
- 1500x950 desk: idle/on Blackout button, both toasts, restore to 100%, routing cell; vs programmer.html.
- 1280x800 (desk window resized with xdotool) and 2560x1440 (web client in chromium on a second Xvfb
  :10 in the container): status bar, toast and page bar fit; light theme checked at 2560x1440.
- Remote client (web bundle via `blenv build web`, chromium in the container): General shows no
  Load, Save-to, Close, Quit, Restart or Shutdown. Engine stopped (`blenv stop`): red offline strip on its
  own row, page dimmed and disabled, status cells show `---`/`--`, sparklines stop, "Engine
  disconnected" cell; Ctrl+B shows "Blackout not sent: engine disconnected". Matches
  `?state=disconnected` (the app adds a reconnect countdown and a Reconnect now button).
- Kiosk 800x480 Programmer screenshot is byte-identical to the decouple baseline.
- rustfmt on touched files, `cargo clippy --all-targets --all-features` (no warnings in touched code),
  `cargo test --all-features`: all in the blenv container with CARGO_BUILD_JOBS=4, all pass.
- Note for blenv: the engine serves the web UI from `/build/target/debug/web`, `build web` writes
  `/build/target/web`; a symlink in the container was needed.

### Still open
- The routing pending count has no source until 062.

### Second review fix (2026-11-12)
- Programmer page-bar button reads "Restore" at Grand Master 0, "Blackout" otherwise; the white inset
  ring is gone (commit 4d7da48). Checked in blenv desktop screenshots (both states). Kiosk Programmer
  is unchanged: the page bar is drawn only on the desktop density. rustfmt on the changed file, clippy
  and tests for blaulicht-ui-egui (0 errors, 130 passed) inside blenv.
- Bookkeeping: 4d7da48 had put this note in a stray TICKETS/ui/059-...done.md; that file is removed and
  the note lives here.
