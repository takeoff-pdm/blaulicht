# 080 — Setup pages: title bars and detail panes

| | |
|---|---|
| Epic | E13 UI port |
| Depends on | 069, 073 |
| ABI / showfile | none expected; check while implementing |

**Touches:** `app/pages/setup/*.rs`

## Problem
Small agreed changes on the other Setup pages (`setup.html`). Mockup: `design/studio`; gaps listed in `design/studio/UI_GAP.md`.

## Scope
- 40px top bars with prefixes on Groups, Scenes, Animations, Palettes.
- Scenes detail: no master intensity or speed, no Folder field.
- Groups detail: no Folder field.

## Out of scope
- Nothing beyond the scope.
- The kiosk layout (`Density::Compact`) stays unchanged.

## Acceptance criteria
- [x] Matches the mockup at the same window size, both themes.
- [x] Every Setup page's top bars line up across columns.

## Verification
- `cargo clippy --all-targets --all-features`, `cargo fmt`, `cargo test --all-features`.
- UI: screenshots per `crates/core/AGENTS.md` next to the mockup: each Setup page.

## Notes
- The four pages had no desktop layout yet (kiosk toolbar + tree on the desktop too), so this ticket gives
  them one: `pages/setup/duo.rs` splits the page into a 240px list pane and the detail pane, both topped by
  `studio::pane_header` at 40px (list: "Groups"/"Scenes"/"Animations"/"Palettes" with Add + Folder or Presets;
  detail: "Group:", "Scene:", "Animation:", "Palette:", "Folder:" prefixes with the actions on the right).
  Lower panes ("Fixtures", "Contents", "Shape", "Value"/"Follows") use the 26px title. Gated on
  `Density::Compact` (the desktop density); the kiosk (`Density::Touch`) path is unchanged.
- Names are edited in place in a Name field (Enter or leaving the field renames, Escape reverts); Rename buttons
  stay only for folders. Scenes detail has no folder field and no master intensity/speed; Groups detail has no
  folder field. The tree's "Move…" button is hidden on the desktop (moving is by drag, 073).
- Desktop auto-selects the first group / palette / animation so the detail pane is never empty (like Fixtures).
- Palettes keeps an "Edit" bar button (opens the existing edit dialog), since values can't be edited in the pane
  yet; the mockup shows only Delete. Animations' "Shape" pane hosts the existing `AnimationEditState` editor;
  porting it to `animation-editor.html` is not part of this ticket.
- Verified in blenv `--desktop` (1500x950) against `setup.html#groups/#scenes/#animations/#palettes`, dark and
  light (light compared by eye: the mockup's light theme can't be selected via hash on setup.html). Exercised:
  group/scene/palette/folder selection, scene rename via the Name field (and back), Clone, Folder and Add dialogs.
- Kiosk: 800x480 screenshots of the four tabs on ui/080 and on decouple are pixel-identical.
- Checks (in the blenv container): `cargo clippy -p blaulicht-ui-egui --all-targets --all-features` has no new
  warnings in touched files (palettes.rs `clone` on `HSVColor` warnings are pre-existing); `cargo fmt --check`
  shows only pre-existing diffs in other files; `cargo test -p blaulicht-ui-egui --all-features` 145 passed.
  The whole-workspace `cargo test --all-features` was not run.
- Not verified: Delete confirmations were not clicked through (to keep the show intact); group-tree right labels
  still read "N FX | #id" (mockup: count only), and the old-style create/clone dialogs are unchanged.
- Review fixes (second pass), verified again in blenv `--desktop` with SPARTACUS_SOLI.json next to the mockup:
  - Animations › Shape is now the mockup's read-only pane: a full-width 150px curve preview (well, centre line,
    curve from `phaser::generate`, two cycles; flashes as on/off steps), then Curve / Range / Duration / Sync
    (flashes: Range / On / Off / Beat aligned) and "Curve drawn from the saved settings." Plugin and audio
    animations say their shape isn't known here. The old `AnimationEditState` editor is no longer shown on the
    desktop, so desktop can rename and delete animations but not edit their settings (the kiosk editor stays);
    editing belongs to the `animation-editor.html` port.
  - Palettes: the bar has only Delete (no Edit), Follows has no Unassign button or note, Target is a button chip
    that selects the target palette, and the Follows terms use a 128px column (`.kv`), values at pane + 136px.
    Value palettes lost Unassign too (not in the mockup). Pointer palettes can't be edited on the desktop now.
  - Lists: groups show only the fixture count, palettes only the kind; scenes keep `#id`.
  - Trees (Groups, Scenes, Palettes) use the new `Tree::list_look` (desktop only): no folder counts, semibold
    folder names, leaves 29px in behind one guide at 12px, selected row filled edge to edge in `blue_dim` with the
    accent bar at the pane edge and a strong label and detail. Other desktop trees keep their look.
  - Forms: text rows are 16px with 6px gaps (Name 60, Contains 86, Types 108 like the mockup). Groups' Fixtures
    table is edge to edge with a 22px header and 24px rows; Universe/Channels and Scenes' Contents counts are
    12px mono in the strong colour.
- Kiosk after the fixes: the 800x480 Setup screenshot is byte-identical to the earlier ui/080/decouple one;
  Groups tab unchanged by eye.
- Checks after the fixes (blenv container): `cargo fmt -p blaulicht-ui-egui` clean, clippy has no new warnings in
  touched files, `cargo test -p blaulicht-ui-egui --all-features` 145 passed. Workspace-wide tests not run.

