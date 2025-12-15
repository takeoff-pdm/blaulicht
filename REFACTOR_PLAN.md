# Refactor Plan

## UI (`crates/core/src/app/ui.rs`)
- **Phase 1 – isolate services**: Extract command execution (e.g. shutdown handling) into a headless service module so the UI becomes a pure event source. Back it with a trait to make unit tests feasible.
- **Phase 2 – feature submodules**: Split the monolith into feature-focused modules (`shell`, `spectrogram`, `plugin_panel`, `dialogs`). Wire them together through a thin `AppView` façade that the egui entrypoint calls.
- **Phase 3 – state view model**: Introduce view-model structs for persistent UI state (popups, animations, selections). Convert direct `AppState` locks into explicit DTOs to reduce lock contention and make rendering deterministic.
- **Phase 4 – verification**: Add snapshot or interaction tests per submodule and document the public entrypoints to keep the surface stable.

## Plugin host (`crates/core/src/plugin/wasm.rs`)
- **Phase 1 – configuration boundary**: Move Wasmtime configuration and plugin discovery into a new `runtime` module. The manager should depend on a trait so swapping runtimes or using the mock implementation is simple.
- **Phase 2 – host function registry**: Extract host function wiring into dedicated builders (e.g. `host::log`, `host::state`). Convert stringly-typed indices into typed descriptors declared in one place.
- **Phase 3 – execution pipeline**: Separate lifecycle responsibilities (`loader`, `instance`, `scheduler`). Each component should expose clear error types so plugin faults are isolated from engine-wide failures.
- **Phase 4 – testing scaffold**: Provide an in-process harness for running synthetic Wasm fixtures and basic contract tests to guard against regression when rearranging modules.
