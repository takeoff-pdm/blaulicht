# 097 — Position view: small mismatches with the mockup

| | |
|---|---|
| Epic | E13 UI port |
| Depends on | 079 |
| ABI / showfile | none |

**Touches:** `app/components/position_view.rs`, `app/pages/setup/fixtures/desktop.rs`

## Problem
The independent review of 079 (kt at 1500x950 against `design/studio/setup.html#fixtures`) passed with three small visual differences left over.

## Scope
- **Dialog footer:** the large position dialog puts Done straight in the body on the pane colour. The mockup has a footer strip with the `--head` background and 8px 10px padding (`.dlg footer`, `studio.css`). Use the same footer as the other studio dialogs.
- **Number format:** position fields show "-2.00 / 3.60 / -0.50"; the mockup shows "-2 / 3.6 / -0.5" (no trailing zeros, still 5 cm steps).
- **"Position" label:** the mockup centres it vertically on the fields plus their captions; the app aligns it with the top of the fields.

## Out of scope
- The rest of the fixture editor (Patch block, conflict line, extra Save / Move buttons).
- The kiosk layout (`Density::Compact`) stays unchanged.

## Acceptance criteria
- [ ] The dialog has a footer strip matching the other studio dialogs, Done on the right.
- [ ] Position values drop trailing zeros, in the fields and while dragging.
- [ ] The "Position" label is centred like the mockup.
- [ ] Checked with kt at 1500x950 against the mockup, dark and light; kiosk unchanged.

## Notes
