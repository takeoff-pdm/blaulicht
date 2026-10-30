---
name: blaulicht-env
description: Run blaulicht-core headless in a per-worktree Docker container (Xvfb + VNC) and drive it with real clicks, drags, keys and screenshots via scripts/blenv or the blaulicht-env MCP tools. Use for visual verification of UI changes, interaction tests of pages and plugin UIs, and reproducing UI bugs without touching the user's screen.
---

# blaulicht-env (blenv)

`blenv` runs the real `blaulicht-core` UI (egui on X11, software GL) in a Docker container
on display `:9`. Each git worktree gets its own container, build cache and inspector port,
so several agents can run instances in parallel. The worktree is bind-mounted at `/work`,
so edits are picked up by the next `blenv build`.

Two equivalent interfaces:
- **MCP tools** (`blaulicht-env` server from `.mcp.json`): `blenv_up`, `blenv_build`,
  `blenv_run`, `screenshot` (returns the image inline), `click`, `drag`, `scroll`, `key`,
  `type_text`, `navigate`, `plugin_ui`, `windows`, `blctl`, `logs`, `blenv_status`,
  `blenv_stop`, `blenv_down`. Input tools take `screenshot_after=true` to save a round-trip.
- **CLI**: `scripts/blenv <cmd>` (`scripts/blenv --help`). Screenshots go to
  `.blenv/shots/<ts>.png`; open the printed path with the Read tool.

## Loop

1. `scripts/blenv up` (builds the image on first use).
2. `scripts/blenv build` (core + default plugins; `build core` / `build plugins <dir>...`
   for partial rebuilds). The first build per worktree takes several minutes, so run it in the
   background and wait for it to finish.
3. `scripts/blenv run` starts the app in the 800x480 kiosk layout and waits for the
   inspector ping. `--desktop` gives a resizable 1500x950 window. `--showfile <path>` loads
   a copy of a showfile. `-e 1920,1080` adds an external screen window `bl_ext_0`.
4. `scripts/blenv screenshot` and read the PNG. Pick coordinates from the image:
   **coordinates are pixels relative to the main window**, the same as the screenshot.
5. Act with `click X Y`, `drag X1 Y1 X2 Y2`, `scroll X Y down`, `key Return`, `type "text"`.
   Each waits ~400 ms so egui can repaint. Take another screenshot and check the result.
6. Cross-check engine state: `scripts/blenv blctl audio|beat|state <path>`, and
   `scripts/blenv logs --grep 'ERROR|WARN'`.
7. When done: `scripts/blenv stop`, or `scripts/blenv down` (the build cache is kept;
   `--volumes` removes it too).

Shortcuts: `scripts/blenv nav <Page>` (Logs System Audio Setup View
ViewPerformance Programmer Animations Palettes SceneGraph Visualizer; Animations and
Palettes open the Setup tabs of the same name, and other tabs are selected with
`scripts/blenv blctl event '{"MainUi":{"NavigateSubPage":"Setup_Palettes"}}'`),
`scripts/blenv plugin-ui <id>` (ids: 0 inspector, 1 julia, 2 audio reactor, 3 chaos,
4 sample egui), `scripts/blenv windows` (titles for `--window`), `scripts/blenv vnc` (lets
the user watch live with macOS Screen Sharing).

## Rules

- Never click Quit, Restart or Shutdown in the UI, and don't run plugins that shell out to
  hardware scripts.
- Plugin tab selection lives in the host and can't be changed by events, so click tabs for
  real. Check that the intended tab is active before clicking inside it.
- In other windows (`bl_ext_0`, plugin windows), pass `--window <title>`; coordinates are
  then relative to that window. The target window is raised before each action/capture.
  A loaded showfile replaces `-e` screens with its own saved external screens. `--screen` uses raw screen coordinates (screen is 1600x1000).
- Audio is the built-in `Dummy: Noise` input, so volume, bass and BPM move but have no musical
  meaning. The Audio page runs the real ALSA enumeration, but the container has no sound devices, so
  the list only shows the two `Dummy:` entries.
- The config is regenerated from `tools/blenv/config.template.toml` on every `run`
  (`--keep-config` keeps runtime changes). The repo `config.toml` and showfiles are never
  modified.
- Report which screens were actually captured and checked. If a step failed (build, ping,
  window not found), say so instead of claiming visual verification.

## Troubleshooting

- `blenv run` fails during startup: it prints the last log lines; also `scripts/blenv logs`.
- Plugins rejected with an ABI mismatch: `scripts/blenv build plugins`.
- `no window named 'blaulicht'`: the app is not running; check `scripts/blenv status`.
- The build runs out of memory: raise Docker Desktop memory, or `CARGO_BUILD_JOBS=4 scripts/blenv build`
  (`BLENV_FEATURES` is forwarded the same way).
- Interactive debugging: `scripts/blenv shell` (DISPLAY=:9, xdotool, import, blctl available).
