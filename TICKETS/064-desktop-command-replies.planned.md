# 064 — Show command replies in the desktop UI

| | |
|---|---|
| Epic | E9 Console UI |
| Depends on | 058, 059 |
| ABI / showfile | none expected; check while implementing |

**Touches:** `crates/ui_egui/src/app/event.rs`, `crates/ui_egui/src/client.rs`, `app/components/status_bar.rs`, `crates/engine/src/dmx/` for error wording

## Problem
The UI can't show whether a command took effect, so a command the engine rejects can look as if it worked. The engine already replies to the sending client with `CommandResult`.

## Scope
- Start by checking which commands already get a useful `CommandResult`, and how the UI handles replies today.
- When the engine rejects a command, show a message naming the object and the reason, in sentences an operator can act on.
- Rejected edits leave the control showing the engine's value, not the rejected one.
- Successful commands stay quiet unless the page already confirms them.

## Out of scope
- Offline "not sent" messages (059).
- The kiosk.
- Undo.

## Acceptance criteria
- [ ] A rejected palette or animation edit shows a message naming the object and the reason.
- [ ] After a rejection, the control shows the engine's value.
- [ ] Replies go only to the client that sent the command: a second session sees no message.
- [ ] Dragging a fader doesn't produce a stream of messages.

## Verification
- `cargo clippy --all-targets --all-features`, `cargo fmt`, `cargo test --all-features`.
- UI: run with `--desktop` via `scripts/blenv`, trigger a rejection, and compare the message against the mockup in dark and light theme. Confirm the 800x480 kiosk is unchanged.
- Repeat the rejection from a remote client.
