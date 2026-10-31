# 046 — Engine crate without egui, and a headless binary

| | |
|---|---|
| Epic | E10 Decouple |
| Depends on | 039, 043, 044, 045 |
| ABI / showfile | none |

**Touches:** `Cargo.toml` (workspace), `crates/core/**`, new `crates/engine`, new `crates/ui_egui`, `Makefile`, `flake.nix`

## Problem
Engine and UI are one crate (`blaulicht-core`), and `main.rs` wires the engine threads and `eframe::run_native`
together. Running headless needs an engine that compiles without egui, eframe or winit.

## Scope
- Split into crates:
  - `blaulicht-engine`: dmx, mainloop, plugin host, audio bridge, event bus, state, config, `EngineClient` +
    `LocalClient`. No egui, eframe, winit or glow dependency.
  - `blaulicht-ui-egui`: `app/`, built on `EngineClient` only.
  - Binaries: `blaulicht` (engine + native UI in one process, the same behavior as today) and
    `blaulicht-headless` (engine only, runs until SIGINT/SIGTERM).
- Engine start-up moves out of `main.rs` into a `Engine::start(cfg) -> EngineHandle` that both binaries call.
- Update `Makefile` targets, the Dockerfile and the flake to use the new crate names.

## Out of scope
- Network transport (047). The headless binary has no frontend yet; it outputs DMX/Art-Net and runs plugins.

## Key code
- Engine wiring — `crates/core/src/main.rs:23–175`

## Acceptance criteria
- [ ] `cargo tree -p blaulicht-engine | grep -E 'egui|eframe|winit'` finds nothing.
- [ ] `blaulicht` behaves as before.
- [ ] `blaulicht-headless` loads the last showfile, runs plugins and sends Art-Net, checked with a receiver or
      `blctl.py`.

## Verification
- `cargo clippy --all-targets --all-features`, `cargo fmt`, `cargo test --all-features`.
- `make prod-build` and `make mock-run` still work.
- Plugins: `cd crates/plugins && make build_noopt`, then grep for `^error`. The host crate path changed, so
  make sure nothing pointed at it.

## Notes
