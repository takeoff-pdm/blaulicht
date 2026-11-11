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
- [ ] Blackout is visible on every page and survives page changes.
- [ ] Holding the key toggles once; a typed `b` still goes into a focused field.
- [ ] Restore returns the previous level, including after changing Grand Master.
- [ ] With the engine stopped, no cell shows a live value and Blackout reports it was not sent.
- [ ] A failed save never leaves "Saved" showing.
- [ ] Load and Close warn about unsaved changes only when there are some.
- [ ] Remote clients can't reach the hidden actions.

## Verification
- `cargo clippy --all-targets --all-features`, `cargo fmt`, `cargo test --all-features`.
- UI: run with `--desktop` via `scripts/blenv` and compare screenshots against the mockup, in dark and light theme, at 1280x800 and 2560x1440. Confirm the 800x480 kiosk is unchanged.
- Test a remote client losing its connection.

## Notes
Mockup: any page with `?state=disconnected`, `?state=restarting` or `?remote`, and `chrome.js` for the behaviour. Decide what the Blackout cell shows when another client changes Grand Master. See `design/studio/RULES.md`.
