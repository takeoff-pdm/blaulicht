# 028 — Plugin list: kind tags, no Show UI for animation plugins

| | |
|---|---|
| Epic | E8 Plugins |
| Depends on | none |
| ABI / showfile | none |

**Touches:** `crates/core/src/app/pages/system/misc.rs (render_plugin_management)`, `crates/core/src/state.rs (read-only)`

## Problem
Original review notes (2026-09-27):

> do not include show ui button for animation plugins
> also add icons / tags in the ui for different plugin kinds
> also make the show ui button inside the plugin entry

## Scope
- In `render_plugin_management` (`system/misc.rs:~413`), show a kind tag or icon per plugin from `state.plugin_runtime_kinds` (Normal / Animation `display_name` / Unregistered), plus the existing status color.
- Animation plugins get no Show UI button.
- Move the Show UI / Hide UI button inside the plugin entry's frame (e.g. right-aligned in the same row) instead of next to it.

## Out of scope
- Enable/disable/reload from the UI (root backlog item).

## Key code
- `render_plugin_management` — `crates/core/src/app/pages/system/misc.rs:~413`
- `PluginRuntimeKind` — `crates/core/src/state.rs:282`

## Acceptance criteria
- [ ] Animation plugins show a tag and no button. Normal plugins show the button inside their entry.

## Verification
- `cargo clippy --all-targets --all-features` and `cargo fmt` are clean.
- `cargo test --all-features` passes, and new logic has co-located `#[cfg(test)]` tests.
- Follow **UI Verification** in `crates/core/AGENTS.md` (check that `swaylock` is not running, launch from the repo root, navigate via blctl, capture with grim). Use the headless Xvfb recipe when clicks are needed or another session is running (see memory notes). Take before and after screenshots, keep them in the scratchpad, and never commit them.
- Screenshot System › Plugins in both layouts.

## Notes
<!-- Implementation notes, decisions, and follow-ups found while working. Keep this updated before renaming to .done.md. -->
