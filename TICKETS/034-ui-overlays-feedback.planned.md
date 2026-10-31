# 034 — Sheets, context menus, toasts and engine refusal feedback

| | |
|---|---|
| Epic | E9 Console UI |
| Depends on | 032, 033 |
| ABI / showfile | Possibly **ABI bump** if engine refusals reach the UI through a new shared event type (append at the end) |

**Touches:** `crates/core/src/app/components/dialog.rs`, `selection_dialog.rs`, `id_selection_dialog.rs`, `folder_tree.rs` (`FolderDialog`), `crates/core/src/app/mod.rs` (`show_popup`), `popup.rs`, `crates/core/src/app/event.rs`, engine error path in `crates/core/src/dmx/mod.rs`

## Problem
Dialogs are fixed-size `egui::Window`s with hard-coded grey frames; there is no long-press or right-click; the only feedback is a centred 200 × 100 popup; and when the engine rejects an event nothing reaches the UI (palette clashes and refused animations fail silently). See Sheet, ContextMenu and Toast in the design system **Blaulicht Console** (https://claude.ai/artifact/ChF4HkbnuyT8bycizagoja).

## Scope
- One overlay component with two presentations chosen by density: **Sheet** (bottom, scrim, grab handle, full-width options, action row) on Touch; **ContextMenu** (for action lists) or a centred popover (for forms) on Compact. Rebuild `Dialog` on it, so existing dialogs pick it up.
- **Row actions**: long-press (500 ms) on touch and right-click or the hover "⋯" on desktop open the same action list for a ListRow. Wire it for group, scene and palette trees (Move to…, Rename, Delete, and for groups Select all fixtures).
- **Toast** stack at the bottom centre (ok, refused, pending; up to three; timeouts 4 s and 8 s; optional action). Replace `show_popup` for non-blocking messages.
- **Refusals**: when `DmxEngine::apply` returns an error for an event the UI sent, show it as a refusal toast that names the object and the reason. Rephrase the engine's error strings where needed so they read as operator-facing sentences.

## Out of scope
- Header and navigation (035). Page layouts (036–038).

## Key code
- `Dialog` — `crates/core/src/app/components/dialog.rs:8-345`
- `show_popup`, `PopupSpec` — `crates/core/src/app/mod.rs:67-113`, `popup.rs:12-60`
- Engine apply and its `(error, rollback)` result — `crates/core/src/dmx/mod.rs`

## Acceptance criteria
- [ ] The same dialog shows as a sheet at 800 × 480 and as a popover or menu on desktop, with no page-specific code.
- [ ] Long-press and right-click open row actions on the group, scene and palette trees.
- [ ] Applying a palette that clashes with an active one shows a refusal toast with the reason (check with `blctl` that the state did not change).

## Verification
- `cargo clippy --workspace --all-targets --all-features`, `cargo fmt --all --check` and `cargo test --workspace --all-features` are clean (run them in the blenv container).
- Follow the `blaulicht-env` skill: run the app in blenv (`scripts/blenv run` and `run --desktop`), drive it with real clicks, and look at every screenshot yourself. Take before and after shots; keep them in `.blenv/shots/` and never commit them.
- Compare against the design system's component previews and rules (Blaulicht Console, linked under Problem).
- Screenshots of a row-action sheet, a context menu, a form dialog in both densities and a refusal toast.

## Notes
<!-- Implementation notes, decisions, and follow-ups found while working. Keep this updated before renaming to .done.md. -->
