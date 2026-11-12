# 070 — Stacked notifications

| | |
|---|---|
| Epic | E13 UI port |
| Depends on | 058 |
| ABI / showfile | none expected; check while implementing |

**Touches:** new `app/components/notifications.rs`, `app/ui.rs`, callers of current status messages

## Problem
Feedback is ad hoc; the mockup settled a JetBrains-like stack (`chrome.js` `BL.toast`, `RULES.md` Notifications). Mockup: `design/studio`; gaps listed in `design/studio/UI_GAP.md`.

## Scope
- Cards bottom right above the status bar, newest at the bottom, max 4 (oldest closes).
- Card: type icon and left edge (blue info, red error), message, optional action as a link, close button (always visible on errors).
- Info closes after 5 s, 8 s with an action, paused while hovered; errors stay until closed.
- A repeated message counts up (x2) and restarts its timer instead of adding a card.
- No Undo actions anywhere (no undo support).
- Raised from a dialog: survives the dialog closing.

## Out of scope
- Command replies in general (064).
- The kiosk layout (`Density::Compact`) stays unchanged.

## Acceptance criteria
- [x] Matches the mockup at the same window size, both themes.
- [x] Five quick notifications show four cards; a duplicate shows x2; an error stays after 10 s.

## Verification
- `cargo clippy --all-targets --all-features`, `cargo fmt`, `cargo test --all-features`.
- UI: screenshots per `crates/core/AGENTS.md` next to the mockup: one info, one error, a x2 card, both themes.

## Notes
- `components/notifications.rs` replaces 059's `notes.rs`: `Notifications` lives on the app (desktop only) and draws in an `Order::Tooltip` area, so cards sit over dialogs and their backdrop like the mockup's stack inside an open `<dialog>`. Code that only has a `Context` raises one with `notify` / `notify_info` / `notify_error` (a ctx queue the app drains each frame); on the kiosk these do nothing.
- Desktop routes `show_popup` to an info card and engine refusals (`show_refusal`, animation preset toggle) to error cards; the kiosk keeps its popups unchanged.
- Action links run a `FnOnce(&mut BlaulichtApp, &Context)` on the frame after the click and close the card. New `show_subpage` (page.rs) switches page + sub-page like a sidebar click; `open_subpage` only selected the tab without changing page, so the showcase's "Show in Setup" did nothing visible.
- Widget ids of the close button and link are derived from their own rect (`rect_id`), not card or slot: cards move when one arrives, and egui warns whenever a rect from the last pass shows up with another id, so keying by rect makes that impossible. Review fix: verified in blenv by raising ~15 cards (info, error, repeat, action) with 0 "changed id between passes" lines in /state/app.log.
- Review fix: a row is as tall as the 22px close button like the mockup's grid column, so `h = max(text + action, CLOSE) + 2*PAD_TOP`; one-line cards are 42px (44 with outline), 48px apart; action cards stay 62px. Theme-independent code; checked in dark at 1500x950. Kiosk is untouched (notifications are desktop-only). Rechecked inside blenv: rustfmt, clippy --all-targets --all-features clean, cargo test --all-features all pass.
- No Undo actions anywhere.
- Verified in blenv `run --desktop` with the studio showcase (Ctrl+Shift+F12): Info, Repeat (x2), Error (stays, X always visible, still there after >10 s), With action (8 s, hover pauses it, link navigates to Setup > Fixtures), Notify from dialog (survives Cancel). Compared at 1500x950 with `programmer.html` + `BL.toast` in both themes: same width 360, right 12 / bottom 36, padding, edge, icon, x2 and link.
- Kiosk Programmer at 800x480 matches decouple visually.
- Checks inside blenv (CARGO_BUILD_JOBS=4): rustfmt on touched files, `cargo clippy --all-targets --all-features` clean, `cargo test --all-features` all pass (5 new notification tests). Note `cargo fmt --all` also rewrites many unrelated files on decouple; those were reverted, not committed.
- Harness quirk: repeated `blenv click` at the exact same pixel sometimes drops a click; nudge the position between clicks.
- Disk: the first review failed to link (no space). Freed 4.2 GB by deleting this worktree's own `target/debug/incremental` in the container; nothing of other worktrees was touched.

