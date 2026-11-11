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
- [ ] Matches the mockup at the same window size, both themes.
- [ ] Every component above appears on at least one ported page in both themes.
- [ ] A lit button keeps its width when toggled.

## Verification
- `cargo clippy --all-targets --all-features`, `cargo fmt`, `cargo test --all-features`.
- UI: screenshots per `crates/core/AGENTS.md` next to the mockup: a page bar, a folder tree, a lit button off and on, both themes.

## Notes
