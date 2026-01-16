# Repository Guidelines
## Project Structure & Module Organization
Core Rust logic lives in `src/`: `app/` drives UI state, `dmx/` powers fixture output, `event/` schedules cues, and `mainloop/` stitches the runtime together. Shared state and helpers sit in `state.rs`, `util.rs`, and `utils.rs`, with defaults in `config.toml`. The Svelte dashboard is under `web/` (assets in `web/static`); `web-old/` keeps legacy prototypes for reference. Ship binaries or vendor payloads in `vendor/`, and skim `overview.puml` or `overview.png` for the system map.

## Build, Test, and Development Commands
- `make all-run`: debug launch with wayland, audio, wasmtime, x11; mirrors `cargo run --features "wayland audio wasmtime x11"`.
- `make prod-run`: optimized release run for packaging.
- `cargo test --all-features`: execute Rust unit tests; add `-- --nocapture` when debugging runtime logs.
- `cargo clippy --all-targets --all-features` then `cargo fmt`: required lint + format gate.
- `pnpm install && pnpm dev --dir web`: install and start the Svelte UI locally; use `pnpm build --dir web` for production.

## Coding Style & Naming Conventions
Rust code uses four-space indentation, snake_case modules/functions, and PascalCase types/enums. Keep imports sorted std/third-party/internal and gate feature code with `#[cfg(feature = "...")]`. Front-end files follow Prettier + ESLint (`pnpm format`, `pnpm lint`); components stay in PascalCase with co-located styles.

## Testing Guidelines
Co-locate `#[cfg(test)]` modules beside the logic they cover; reach for `tests/` when validating multi-module flows. Mock audio hardware via `cargo test --no-default-features --features "audio-mock wayland"`. Web changes must pass `pnpm check` and `pnpm lint`, and include before/after screenshots when visuals shift.

## Commit & Pull Request Guidelines
Commits follow conventional prefixes (`feat:`, `fix:`, `refactor:`, `chore:`) and should stay atomic. Reference issues inline when relevant (`feat: add midi panic button (#342)`) and document config toggles in the body. Pull requests need a concise summary, verification steps, and linked issues; attach UI media or DMX capture notes when behavior changes. Confirm the Rust and web test commands above before requesting review.

## Configuration & Deployment Notes
Adjust defaults via `config.toml` and keep secrets out of version control. Release artifacts live in `target/`; do not commit them. Coordinate with maintainers before modifying `run.sh`, which powers automation experiments.
