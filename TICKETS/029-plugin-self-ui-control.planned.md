# 029 — Plugins can open/close their own UI and navigate pages

| | |
|---|---|
| Epic | E8 Plugins |
| Depends on | 005 |
| ABI / showfile | **ABI bump** if a host function is added; none if built purely on `bl_send_event` |

**Touches:** `crates/plugin_framework/src/blaulicht.rs`, `crates/core/src/app/event.rs`, `crates/core/src/plugin/wasm.rs (only if a host fn is needed)`, `crates/shared/src/abi.rs`

## Problem
Original review notes (2026-09-27):

> also allow plugins to open themselves and close themselves
> also allow sub-page navigation

## Scope
- Add framework helpers `ui::open()`, `ui::close()` (own plugin id), `ui::navigate(page)` and `ui::navigate_sub(subpage)`, built on `ControlEvent::MainUi(SetPluginUIOpen | NavigatePage | NavigateSubPage)` via `bl_send_event`, with the own id from the existing plugin context.
- Fix `SetPluginUIOpen` handling (`app/event.rs:~38`): today it only updates `Entry::Occupied`, so a plugin whose visibility entry doesn't exist yet cannot open. Also set `screen_id` sensibly.
- Port `plugins/console` (which hand-builds these events, lib.rs:711) to the helpers as a usage example.

## Out of scope


## Key code
- `ui::is_open`, `ui::maximize_screen` — `crates/plugin_framework/src/blaulicht.rs:~500`
- `SetPluginUIOpen` handling — `crates/core/src/app/event.rs:~38`
- `notify_plugin_ui_open` — `crates/core/src/app/plugin_ui.rs:9`

## Acceptance criteria
- [ ] A test plugin (or console) can open its UI, close it and navigate to a sub-page, verified live.

## Verification
- `cargo clippy --all-targets --all-features` and `cargo fmt` are clean.
- `cargo test --all-features` passes, and new logic has co-located `#[cfg(test)]` tests.
- If the ABI was bumped: `PLUGIN_ABI_VERSION` in `crates/shared/src/abi.rs` is incremented and `cd crates/plugins && make build_noopt` was run. The Makefile reports success even on failure, so grep its output for `^error`.
- Follow **UI Verification** in `crates/core/AGENTS.md` (check that `swaylock` is not running, launch from the repo root, navigate via blctl, capture with grim). Use the headless Xvfb recipe when clicks are needed or another session is running (see memory notes). Take before and after screenshots, keep them in the scratchpad, and never commit them.
- Screenshot before and after a plugin-triggered open and navigation.

## Notes
<!-- Implementation notes, decisions, and follow-ups found while working. Keep this updated before renaming to .done.md. -->
