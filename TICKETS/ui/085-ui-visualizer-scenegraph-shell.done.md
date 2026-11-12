# 085 — Visualizer and Scene Graph in the studio shell

| | |
|---|---|
| Epic | E13 UI port |
| Depends on | 069 |
| ABI / showfile | none expected; check while implementing |

**Touches:** `app/pages/visualizer/`, `app/pages/scene_graph.rs`

## Problem
These two pages only need to match the shell, not the mockup exactly. Mockup: `design/studio`; gaps listed in `design/studio/UI_GAP.md`.

## Scope
- Page bars 40px with the shared controls (segmented, lit buttons, dropdown); tokens and status bar as everywhere.
- Keep existing canvases and editing behaviour.

## Out of scope
- Visualizer redesign (024, 025, 027).
- The kiosk layout (`Density::Compact`) stays unchanged.

## Acceptance criteria
- [x] Matches the mockup at the same window size, both themes.
- [x] Both pages use the shared bar and no ComboBox.

## Verification
- `cargo clippy --all-targets --all-features`, `cargo fmt`, `cargo test --all-features`.
- UI: screenshots per `crates/core/AGENTS.md` next to the mockup: both pages.

## Notes

- Desktop only (`self.layout.is_desktop()`; the Visualizer also requires no sub-page tabs, so tiles on
  dynamic screens keep the old layout). The kiosk paths were only split into helpers
  (`scene_graph_sync/clamp_nodes/canvas/node_editor`, `visualizer_canvas`, `visualizer_add_menu`,
  attach/detach/duplicate/delete helpers) with the same widgets in the same order; kt kiosk shots at
  800x480 of both pages show the old layouts. Not compared pixel by pixel with a decouple build.
- Visualizer: 40px page bar (View/Edit, 3D/Top and Select/Move/Rotate/Scale segmented, Snap, Add menu,
  Undo/Redo, lit Room/Grid/Axes/Beams/Bodies/Labels; Fit view and Reset cam right). Stage view pane
  ("Stage, 3D view" / "Stage, top view"), Selection pane (Attach/Detach/Duplicate/Delete + the existing
  inspector) and Stage pane (number fields for width/depth/height, brightness and snap steps,
  Diagnostics switch, object count, Import GLB, renderer/scene status lines and Retry renderer).
- Scene Graph: Graphs list (Create opens a name field; Enter or Add creates), canvas pane titled with the
  graph (section, Enabled switch, Add node, Auto layout, Zoom number field, Delete graph) and Node pane
  with the existing editor (shared dropdowns for speed, scenes and edge conditions). Canvas fill, border
  and grid use the studio tokens on desktop.
- Verified with kt at 1500x950, dark and light: mode/tool switching, Add > Platform, Duplicate, Undo,
  Grid toggle, room width typed to 20, graph create, Add node, Auto layout, node select, Delete graph
  dialog (Cancel).
- Not verified / left as is: the 3D view (blank in kt, glow callback); the top view keeps its fixed
  dark stage drawing in the light theme (shared with the kiosk); the node editor and object inspector
  still use egui's default sliders/drag values inside the panes (a full restyle is the redesign
  tickets' scope). Checks: cargo fmt --check, clippy and tests for `blaulicht-ui-egui` on the host
  (CARGO_BUILD_JOBS=4), 171 tests pass, no new clippy warnings in the touched files.

- Review fixes (verified with kt, desktop 1500x950): the desktop object inspector is now label/value
  rows (Name, Flags, Position m, Rotation deg, Scale or Profile/Part/Length/Sections for trusses),
  each axis a prefixed drag value ("X 0.00") so values stay with their labels in the 280px pane; the
  kiosk keeps its wrapped row (moved verbatim into `kiosk_object_row`). The 3D/Top segmented control
  is replaced by a "Top" toggle on the right before Fit view / Reset cam, as in the mockup, usable in
  View and Edit mode; in View mode the top view is read-only (primary-button select/drag gated on
  Edit; the kiosk only shows the top view in Edit, so it is unaffected). Create in Scene Graph selects
  the prefilled name, so typing "Test" + Enter creates "Test". The Node pane title already follows the
  selected node's name: new nodes are simply named "Node" (renaming to "Intro" retitles the pane).
- Kiosk: not re-shot; from the code the Compact path is unchanged (inspector branches on
  `dropdown::is_desktop`, bar and Create changes live only in the desktop functions). Checks rerun on
  the host with CARGO_BUILD_JOBS=4: cargo fmt, clippy and tests for `blaulicht-ui-egui` (171 pass, no
  new clippy warnings in touched lines).
