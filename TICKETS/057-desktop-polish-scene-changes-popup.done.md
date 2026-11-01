# 057 — Desktop polish (radius, tree nesting, fixture tiles) and the Scene Changes popup

| | |
|---|---|
| Epic | E11 Desktop layout |
| Depends on | 056 |
| ABI / showfile | none |

**Touches:** `crates/ui_egui/src/app/ui_tokens.rs`, `crates/ui_egui/src/app/components/{button.rs,tree.rs,
folder_tree.rs,fixtures_shared.rs}`, `crates/ui_egui/src/app/pages/programmer.rs`

## Problem
The maintainer, verbatim (2026-11-01), on a Programmer screenshot of the desktop layout:

> its a bit ugly: dont round the buttons this much, make it more similar to default egui. overflowing selection,
> folder structure not clear enough. Target thec cleanups for the desktop ui only. and for both non-desktop and
> desktop make the scene changes a popup with a clear button

## Scope
Desktop (Compact density) only:
- Buttons: `RADIUS` 2 (egui's default). Tonal and Danger buttons are filled without a border at rest and get one
  under the pointer, like egui's own buttons; disabled buttons keep the fill with faint text. Labels are regular
  weight at 13 px (`Density::button_font`), padding 8 px.
- Trees (`show_rows_compact`): a child's label starts 12 px right of its parent's label (the folder icon width is added
  when shown), one guide line per parent level drops from that parent's caret, folder labels are medium weight,
  leaves regular. No hairlines between rows. The selection fill and accent bar start at the row's indentation, so
  they no longer cover the column edge and the parents' guides. Narrow trees (`Tree::compact`) drop the folder icon.
- Narrow columns are wider on the desktop: group trees 160 px (`narrow_tree_width`), the Programmer's scene column
  150 px. Group names no longer truncate and the details (`2 FX | #8`) fit.
- Programmer fixture tiles: 150 x 38 px, name and kind truncated to the tile (the name overflowed the tile before).
  "Fixtures" uses the column header style. The Scene Changes dialog's fixture headers are `muted` instead of green.

Both layouts:
- The Scene Changes list in the Programmer's right column is gone. A "Scene Changes (N)" button opens the Scene
  Changes dialog (per-property remove buttons, grouped by fixture). Its "Undo all" is now "Clear" (danger variant),
  with the "Clear all changes?" confirmation.

## Verification
- `cargo test -p blaulicht-ui-egui`: 128 pass.
- blenv with a copy of `SPARTACUS_SOLI.json`: desktop Programmer with two groups selected, the Scene Changes dialog
  and the Clear confirmation (cancelled); kiosk Programmer and the Scene Changes dialog at 800 x 480. The kiosk only
  changed where the Scene Changes list was.

## Notes
- The Setup pages' trees use the same compact rows, so they get the clearer nesting as well.
