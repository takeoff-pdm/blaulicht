# 042 — EngineClient trait and Programmer as the pilot

| | |
|---|---|
| Epic | E10 Decouple |
| Depends on | 040, 041 |
| ABI / showfile | none, unless a new `ControlEvent` variant is needed (append at the end) |

**Touches:** `crates/core/src/app/mod.rs`, `crates/core/src/app/page.rs`, `crates/core/src/app/pages/programmer.rs`, `crates/core/src/app/pages/programmer_animations.rs`, `crates/core/src/state.rs`, new `crates/core/src/client.rs`

## Problem
Pages read and write `Arc<AppState>` directly: about 170 lock sites in `app/`, about 40 of them writes, often a
read-modify-write in the middle of drawing. A frontend should only see "a snapshot of show state, my session, and
a way to send commands", so the same page code can later run against a remote engine.

## Scope
- Add `trait EngineClient`, roughly:
  `fn show(&self) -> ShowSnapshot`, `fn session(&self) -> SessionSnapshot`, `fn send(&self, cmd: ControlEvent)`,
  plus accessors for the read-only live data pages need (DMX buffers, health, plugin UI ops, logs).
  Settle the exact shape in this ticket and write it down in Notes.
- `LocalClient`: the in-process implementation. Reads go through the existing `Arc` (a read guard or
  `clone_for_ui`, never a deep clone per frame), and `send` uses the existing event bus channel with `DESK`
  session. **Must not be slower than today:** compare frame times before and after in Notes.
- Pages get the client through `page.rs` instead of `self.data.state`.
- Move `programmer.rs` and `programmer_animations.rs` fully onto the client: no `dmx_engine.write()` and no
  direct `AppState` access left in those files. Missing operations become `ControlEvent` variants (appended at
  the end) handled in `dmx/mod.rs`.
- The UI shows state one frame after sending a command. If a drag or fader looks wrong because of that, keep a
  local value in the widget while it is being dragged.

## Out of scope
- Other pages (043–045).
- Remote transport (047).
- Programmer redesign (036). Don't run them at the same time; see the epic README.

## Key code
- Direct writes — `crates/core/src/app/pages/programmer.rs:299`, `:1363`, `:1372`
- `clone_for_ui` — `crates/core/src/dmx/state.rs:31`
- Page plumbing — `crates/core/src/app/page.rs`
- UI event sending today — `crates/core/src/app/page.rs:104`, `crates/core/src/app/jog.rs:67`

## Acceptance criteria
- [ ] `grep -nE '\.(write|lock)\(\)' crates/core/src/app/pages/programmer*.rs` finds nothing.
- [ ] Every Programmer function works as before: selection, attributes, palette apply, animation presets, scene
      change with undo-all, LIVE.
- [ ] Frame time on the Programmer page is not worse than before.

## Verification
- `cargo clippy --all-targets --all-features`, `cargo fmt`, `cargo test --all-features`.
- UI: before and after screenshots of the Programmer page per `crates/core/AGENTS.md`, plus an interaction test
  (select a group, set a color, apply a palette) with the blaulicht-env harness.

## Notes
