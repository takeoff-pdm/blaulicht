# blenv: headless blaulicht in Docker

Runs `blaulicht-core` (egui, X11, Mesa llvmpipe) in a Debian container on a virtual display
(Xvfb `:9`, 1600x1000, openbox) with VNC, `xdotool` and ImageMagick, so agents and people can
screenshot and click the real UI without a monitor. One container per git worktree.


> **On macOS, use `scripts/kt` (egui_kittest, no Docker) by default** and blenv only for what it
> can't do: the visualizer's 3D view, window-level behaviour, pop-outs, file dialogs. See
> [`KITTEST.md`](KITTEST.md).

```sh
scripts/blenv up            # build image (first time), start container
scripts/blenv build         # plugins (inspector + sample/animation set) + core (the desk)
scripts/blenv run           # start app (800x480 kiosk layout); --desktop for 1500x950
scripts/blenv build headless && scripts/blenv run --headless  # engine only, no window
scripts/blenv build web     # web UI bundle; the running engine serves it on port 1234
scripts/blenv screenshot    # -> .blenv/shots/<ts>.png
scripts/blenv click 100 40  # window-relative coordinates
scripts/blenv nav Audio     # page switch through the inspector
scripts/blenv blctl audio   # any scripts/blctl.py command, run in the container
scripts/blenv vnc           # watch live (macOS Screen Sharing)
scripts/blenv down          # remove container, keep build cache (--volumes: drop it too)
```

Full command list: `scripts/blenv --help`. Agents can also use the `blaulicht-env` MCP
server (`.mcp.json`, `uv run --script tools/blenv/mcp_server.py`) and the skill in
`.claude/skills/blaulicht-env/`.

## How it fits together

| Piece | Where |
|---|---|
| Image: toolchain (nightly + wasm32), X/GL/input tools. No source. | `Dockerfile`, tag `blenv:<hash of Dockerfile>` |
| Container entry: Xvfb (GLX off, so GL goes through EGL) + openbox + x11vnc | `container/entrypoint.sh`, `container/openbox-rc.xml` |
| Builds: `cargo build -p blaulicht-core --no-default-features --features "x11 audio wasmtime"` (the desk), `build headless`: `cargo build -p blaulicht-engine --bin blaulicht-headless` with the same features minus `x11`/`wayland`, plugins to `crates/plugins/bundle_debug/`, `build web`: `crates/web/build.sh --lint` into `/build/target/web` (installs the matching prebuilt wasm-bindgen-cli into the cargo volume on first use) | `container/build.sh` |
| App start: config from template, inspector ping wait, window at 0,0 | `container/run-app.sh`, `config.template.toml` |
| Host CLI and library | `blenv.py` (Python 3.9+, stdlib only), shim `scripts/blenv` |
| MCP server | `mcp_server.py` |

Per instance (`blenv-<worktree>-<hash>`): container, volume `…-target` (`/build`, cargo
target dirs) and `…-state` (`/state`: generated config, `app.log`, showfile copies, VNC
password). The cargo registry volume `blenv-cargo` and the sccache volume `blenv-sccache` (compiled dependencies, capped at 10 GB; `sccache --show-stats` in the container shows hits) are shared by all instances. VNC is
published on a random `127.0.0.1` port, which `scripts/blenv vnc` prints.

## Notes

- Rendering is Mesa llvmpipe via EGL. Xvfb runs with `-extension GLX` because its GLX
  visuals have no multisampling and the app requests 4x MSAA + 24-bit depth; EGL configs
  have both. The Visualizer page renders (slowly).
- openbox is required: without a window manager, egui external-screen viewports are
  recreated every frame. Decorations are off for every window, as on the kiosk. Input and
  window screenshots raise the target window first, since windows can overlap.
- Loading a showfile replaces the `-e` external screens with the ones saved in the
  showfile (app behaviour, `apply_showfile_ui_state`).
- Audio is built with the real `audio` feature, so the Audio page runs the real cpal/ALSA
  enumeration. The container has no sound devices (no `/dev/snd`, no sound server), so the
  list shows only `Dummy: Silence` and `Dummy: Noise`; the config selects `Dummy: Noise`.
- The template config only enables hardware-free plugins: 0 inspector, 1 julia,
  2 audio reactor, 3 chaos, 4 sample egui. Edit `config.template.toml` to change that.
  The repo `config.toml` is not used.
- `BLENV_FEATURES` and `CARGO_BUILD_JOBS` are forwarded to `blenv build`. If a new nightly
  breaks the build, pin it by changing `ARG RUST_NIGHTLY=nightly` in the Dockerfile to
  `nightly-YYYY-MM-DD`; the image tag follows the Dockerfile hash, so `blenv up` rebuilds.
- `run --headless` starts `blaulicht-headless` (engine only) with the same config and showfile
  handling; `blctl`, `logs`, `status` and `stop` work, screenshots and input don't (no window).
- The web UI (`build web`) is served by the desk or headless engine at `http://127.0.0.1:1234/`
  inside the container. The image has no browser: for checks, install one in the running
  container only (`apt-get update && apt-get install -y --no-install-recommends chromium`) and
  start it on `:9`, e.g. `chromium --no-sandbox --test-type --use-angle=swiftshader
  --enable-unsafe-swiftshader --enable-logging=stderr --user-data-dir=/tmp/chr --app=http://127.0.0.1:1234/`
  (console lines go to its stderr). Its window is titled `Blaulicht`, the desk's `blaulicht`;
  `--window` matching ignores case, so with both open pick windows with
  `xdotool search --name` plus an exact `getwindowname` check.
- `scripts/blenv list` shows all instances; `scripts/blenv gc [--yes]` removes stopped ones
  and the volumes of instances whose worktree was deleted.
