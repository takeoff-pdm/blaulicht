# kt: the desk UI in egui_kittest (no window, no Docker)

`scripts/kt` runs the real engine and `BlaulichtApp` inside an
[`egui_kittest`](https://docs.rs/egui_kittest) harness, rendered offscreen with wgpu (Metal on
macOS). There is no window, no virtual display and no Linux VM. It reuses the host's normal
cargo target dir, so it costs one extra ~280 MB example binary instead of blenv's per-worktree
Linux build tree (15+ GB per worktree, see `README.md`).

Driver: `crates/core/examples/kittest_drive.rs`. It listens on a Unix socket; `scripts/kt`
sends one command per call and prints the reply.

## When to use which

**On macOS, use kt by default.** Use blenv only for what kt can't do:

| Need | kt | blenv |
|---|---|---|
| Pages, dialogs, popups, combo boxes, text fields, buttons, sliders | yes | yes |
| Widget labels and positions without reading pixels (`tree`) | yes | no (pixels only) |
| **Visualizer / stage editor 3D view** (glow `PaintCallback`) | **no, blank**: wgpu renderer drops glow callbacks (`egui_wgpu: Unknown paint callback` in the log) | yes |
| Window-level behaviour: winit, focus, DPI, resizing, decorations | no | yes |
| Plugin pop-out windows, external screens (extra viewports) | no (one viewport) | yes |
| Native file dialogs | no | yes |
| Real Linux/X11 behaviour as on the desk | no | yes |
| Someone watching live over VNC | no | yes |
| Disk / startup cost | one example binary, starts in ~1 min | ~2.5 GB image + 15+ GB per worktree |

On Linux, blenv stays the default. kt also works there; it needs a working wgpu adapter
(Vulkan or GL).

## Loop

```sh
scripts/kt start --layout desktop        # builds the example (nice 10), waits for "ready"
scripts/kt tree                          # Role "label" = "value" @ x,y wxh, every labelled node
scripts/kt tree Plugins                  # filtered (label or value, case-insensitive)
scripts/kt click Programmer              # by label: exact match first, then substring
scripts/kt click_at 647 391              # by position (for unlabelled widgets: text fields, canvases)
scripts/kt type Plugin                   # text into the focused widget
scripts/kt key Escape                    # also ctrl+A, cmd+S, shift+Tab (egui::Key names)
scripts/kt drag 272 130 340 130          # press, 10 moves, release
scripts/kt scroll 700 400 -200           # mouse wheel at a point (negative = down)
scripts/kt wait 1000                     # let real time pass (engine events, animations)
scripts/kt shot /path/in/scratchpad.png  # 1280x800 PNG, open it with Read
scripts/kt log 50                        # driver/engine log
scripts/kt stop
```

Other start flags are passed through to the driver: `--width/--height` (harness size in
points, the app pins `pixels_per_point` to 1), `-c <config.toml>` (default: the repo root's
`config.toml`), and the desk's UI flags (`--layout`, `--showfile-home`, ...). Env:
`KT_FEATURES` (default `audio-mock wasmtime`), `KT_NICE` (default 10), `KT_SOCKET`.

The inspector plugin is loaded as on a normal desk run, so `python3 scripts/blctl.py ...`
works next to kt (navigation via events, state dumps, overrides).

## Findings from testing (2026-11-12, macOS, egui 0.34.1, egui_kittest 0.34.1)

- Rendering looks like the desk (not compared pixel by pixel): IBM Plex, phosphor icons, the
  desktop layout and the status bar all render. A screenshot takes ~1.5 s, a command ~0.4 s.
- Startup takes about 1 minute, mostly compiling plugins with wasmtime (cranelift) at
  startup. The "Initializing" window is in the tree until then; wait with
  `until ! scripts/kt tree Initializing | grep -q Window; do scripts/kt wait 2000; done`.
- **Clicks must go through the pointer path.** kittest's `Node::click()` sends press and
  release in one frame; `ComboBox` popups ignore that. The driver hovers, presses and releases
  in separate frames, which opens combo boxes, menus and modals.
- **`tree` is the way to find targets.** Buttons, combo boxes, checkboxes and labels have
  AccessKit labels. Text fields have none (the hint text isn't exposed): they show up as
  `TextInput "" @ x,y wxh`; `click_at` their centre to focus them, then `type`.
- Custom-painted widgets (Programmer faders, knobs, plugin canvases) are not in the tree. Read
  their position from a screenshot and use `click_at` / `drag`. egui sliders work with `drag`
  (Visualizer "Bright" 120% -> 360%).
- Dragging the Programmer's Alpha fader (with no fixture selected) changed nothing. Most
  likely the faders are inactive without a selection, but this is unconfirmed; select
  fixtures first and check the value in a screenshot. Ticket 087 makes the bank look disabled.
- `click <label>` with several matches uses the first one and only notes it in `scripts/kt
  log`. Use `tree <label>` first if a label is common (e.g. "Plugins" is both a nav entry and
  log text), or `click_at` the exact node.
- With the repo root's `config.toml`, the desk shows "Startup failed" and "No showfile": its
  `last_open_showfile` is a Linux path (`/home/mik/...`). DMX serial ports, MIDI devices and
  `pioneer_pro_dj_link.wasm` (not built) also log errors on the Mac. All of this is expected
  and doesn't affect the UI. To load a showfile, copy `config.toml` to the scratchpad, set
  `last_open_showfile` to a copy of the `.json` and start with `scripts/kt start -c <copy>`.
- The app always requests a repaint, so `Harness::run()` never settles. The driver steps a
  fixed number of 16 ms frames after each command, in real time, so engine replies arrive.
- Ports are not isolated: kt uses the config's `port` (1234) and the inspector's 9099, so only
  one kt or desk instance per config can run at a time. For parallel agents, give each a copy
  of `config.toml` with other ports (`-c`). blenv isolates this per container.
- wgpu finds the Mac GPU on its own; no Xvfb, Mesa or permission prompts (Screen Recording /
  Accessibility are not needed, nothing touches the real screen).

## Also possible: kittest in `cargo test`

The same harness works inside a `#[test]` for repeatable checks (snapshot images with the
`snapshot` feature, `harness.snapshot("name")`, compared against `tests/snapshots/`). Not set
up yet; kt is the interactive version for agents.
