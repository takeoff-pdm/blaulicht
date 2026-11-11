# 069 — Studio tokens, 40px bars, folder bands and lit buttons

| | |
|---|---|
| Epic | E13 UI port |
| Depends on | 058 |
| ABI / showfile | none expected; check while implementing |

**Touches:** `app/ui_tokens.rs`, `app/components/button.rs`, `app/components/folder_tree.rs`, `app/components/tree.rs`

## Problem
The mockups settled several shared looks after 058 was written: 40px page bars and top pane bars, folder bands with a top line, and on/off buttons with a dot. Mockup: `design/studio`; gaps listed in `design/studio/UI_GAP.md`.

## Scope
- Page bars and the first row of pane headers on master-detail pages are 40px with 28px buttons; lower pane titles stay 26px (`studio.css` "Top bars").
- Pane title prefix: muted label before the name ("Selected group:", "Fixture:").
- Folder rows (lists, trees, dropdown groups): title-bar grey with a 1px line on top, grey folder icon (decision A4 B1, `folder-accent-variants.html`).
- Lit on/off button (`.livebtn`): dot grey when off, red blinking when on (steady with reduced motion), fixed text and width. Used by Exclusive preview, Identify, Initialize.
- Number scrub: dragging sideways on a number field changes it by its step per 4px, Shift 10x finer, Esc restores, applies live; a click still edits.

## Out of scope
- Page layouts (later tickets).
- The kiosk layout (`Density::Compact`) stays unchanged.

## Acceptance criteria
- [x] Matches the mockup at the same window size, both themes.
- [x] Every component above appears on at least one ported page in both themes.
- [x] A lit button keeps its width when toggled.

## Verification
- `cargo clippy --all-targets --all-features`, `cargo fmt`, `cargo test --all-features`.
- UI: screenshots per `crates/core/AGENTS.md` next to the mockup: a page bar, a folder tree, a lit button off and on, both themes.

## Notes
- Components in `app/components/studio.rs` (desktop only): `page_bar` is now 40px with 8px padding, a light top
  line and a 1px bottom line; `mini_button` pads 10px at 28px; new `pane_header(prefix, name, top, actions)` (40px
  top row, 26px lower titles, muted prefix); `live_button` (`.livebtn`: grey dot off, red dot blinking 1 s on,
  fixed label and width, `live_button_width`); `num_field` + `scrub_value` (step per 4px, Shift 10x finer, Esc
  restores, applies live, a click edits the text with Enter/blur apply and Esc cancel). Tokens:
  `PAGE_BAR_HEIGHT` 40, new `PANE_TITLE_HEIGHT` 26.
- Folder bands: the desktop tree (`Tree::show_rows_compact`, used by every Setup/Programmer folder tree) paints
  folder rows in studio `head` (hover `ctl`) with a 1px studio `line` on top and a solid grey folder icon. The kiosk
  path (`show_rows`) is untouched. `folder_band` already matched.
- Lit buttons in the app: Programmer "Exclusive preview" (desktop replaces the LIVE button; the old corner dot for
  "another session previews" is gone, its tooltip says so), Setup Fixtures "Initialize" (lit for the 10 s
  initialization, a second click while lit is ignored). Identify does not exist in egui yet (UI_GAP: Setup); the
  component is shown in the studio showcase until that ticket.
- "Ported pages": no page layout is ported yet (later tickets), so pane headers with prefixes, the 40px page bar and
  the number scrub appear on the studio showcase (Ctrl+Shift+F12); folder bands and lit buttons also on the real
  Setup/Programmer pages. Checked in both themes on the showcase, dark on the real pages.
- Reduced motion: egui has no platform query, so `BLAULICHT_REDUCED_MOTION=1` makes the dot steady.
- Verified in blenv at 1500x950: page bar and pane headers next to the `setup.html` mockup, folder bands on Setup
  Fixtures/Scenes and Programmer, Exclusive preview off/on (width unchanged, engine LIVE on), Identify toggle,
  scrub +80px = +1.00 at step 0.05, click-to-edit with Enter. Kiosk (800x480) Programmer and Setup screenshots are
  pixel-identical to decouple (4d7da48).
- Not verified: Shift and Esc while scrubbing (blenv drag is one atomic gesture); dropdown folder groups (the
  dropdown is ticket 071); Initialize lit state in the light theme.
- Checks ran in the blenv container, scoped to `-p blaulicht-ui-egui`: `cargo fmt --check` clean for the touched
  files (other files had pre-existing diffs, left alone), `cargo clippy --all-targets --all-features` has no
  warning in the touched code (pre-existing warnings elsewhere), `cargo test --all-features` 131 passed.
