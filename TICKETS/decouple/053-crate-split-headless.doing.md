# 053 — Engine crate, egui crate and a headless binary

| | |
|---|---|
| Epic | E10 Decouple |
| Depends on | 039, 046, 050, 043, 044, 051, 045, 052 |
| ABI / showfile | none |

**Touches:** `Cargo.toml` (workspace), `crates/core/**`, new `crates/engine`, new `crates/ui_egui`, `Makefile`, `crates/core/Makefile`, `update_version.py`, `tools/blenv/**`, `.claude/skills/blaulicht-env/SKILL.md`, `crates/core/AGENTS.md`, `TICKETS/**` (path sweep)

## Problem
Engine and UI are one crate, and `main.rs` wires the engine threads and `eframe::run_native` together.

## Scope
- Split into crates:
  - `blaulicht-engine`: dmx, mainloop, plugin host, audio bridge, event bus and hub, state, config,
    persistence, `stage.rs`, `EngineClient` + `LocalClient`. No egui, eframe, epaint, winit, glow, three-d or
    image dependency.
  - `blaulicht-ui-egui`: `app/`, `stage_assets.rs`, built on `EngineClient` only. Gate the visualizer, the
    file dialog and plugin pop-out windows behind features, so a later wasm build (048) can turn them off.
  - Binaries: `blaulicht` (engine + native UI in one process, the same behaviour as today; keep the binary name
    `blaulicht-core` if renaming would break `update.sh` or blenv; decide and write it in Notes) and
    `blaulicht-headless` (engine only).
- `Engine::start(cfg) -> EngineHandle` wraps today's startup order: config → audio device → event bus →
  supervisor → plugin watcher → showfile load (`main.rs:23–200`). Both binaries call it.
- Split CLI flags: UI flags (`desktop_mode`, `window_decorations`, `external_screens`) live in the UI binary.
- Headless: handle SIGINT and SIGTERM through `nix` (already in the tree) or `signal-hook`, run the 052 shutdown
  hook and exit.
- Update every build reference: `Makefile:4`, `:21`, `:32`, `:67`; `crates/core/Makefile`;
  `update_version.py:8`; `tools/blenv/{Dockerfile,README.md,blenv.py,mcp_server.py,container/build.sh,container/run-app.sh}`;
  `.claude/skills/blaulicht-env/SKILL.md`; `crates/core/AGENTS.md`.
- Sweep the Touches and Key code paths in every planned ticket under `TICKETS/`.

## Acceptance criteria
- [ ] `cargo tree -p blaulicht-engine | grep -E 'egui|eframe|epaint|winit|glow|three-d'` finds nothing.
- [ ] `blaulicht` behaves as before (blenv smoke test).
- [ ] `blaulicht-headless` with a config that loads the inspector plugin starts, loads the last showfile, runs
      plugins, answers `blctl.py state` and keeps its memory flat over 10 minutes.

## Verification
- `cargo clippy --workspace --all-targets --all-features`, `cargo fmt`, `cargo test --workspace --all-features`.
- `scripts/blenv build` and `run` still work. Plugins: `cd crates/plugins && make build_noopt`, then grep for
  `^error`.

## Notes
