# 050 — Programmer onto EngineClient

| | |
|---|---|
| Epic | E10 Decouple |
| Depends on | 042 |
| ABI / showfile | none (new operations go into `HostCommand`) |

**Touches:** `crates/core/src/app/pages/programmer.rs`, `crates/core/src/app/pages/programmer_animations.rs`, `crates/core/src/dmx/mod.rs`, `crates/core/src/plugin/wasm.rs`

## Problem
Programmer still uses `AppState` directly:
- Production writes: `programmer.rs:299` (scene master alpha and speed) and `programmer_animations.rs:205`,
  `:221` (`toggle_preset`, which reports "Animation limit reached").
- `clone_animation_instance_state` (`wasm.rs:267`) writes plugin state storage and
  `wasm_animation_instances_to_reset`. It is called from `programmer.rs:818`, `:855` and
  `programmer_animations.rs:259`.

The direct writes at `programmer.rs:1363`, `:1372` and `programmer_animations.rs:767`, `:775`, `:810` are in
`#[cfg(test)]` code and may stay.

## Scope
- All reads go through `with_show` and all writes are commands.
  - `programmer.rs:299` uses the existing `SetSceneMasterAlpha` / `SetSceneMasterSpeed` events.
  - `toggle_preset` becomes a `HostCommand`, and its refusal ("Animation limit reached") comes back as
    `CommandResult::Refused`, shown as today.
  - Cloning animation instance state becomes a `HostCommand` handled by the engine.
- Drags and faders keep a local value while they are being dragged, so the one-frame delay doesn't show.

## Out of scope
- Programmer redesign (036).

## Acceptance criteria
- [x] No production code in `programmer*.rs` references `AppState`, `self.data.state` or a lock.
- [x] Selection, attributes, palette apply, animation presets (including the limit message), scene change with
      undo-all and LIVE all work as before.

## Verification
- `cargo clippy --workspace --all-targets --all-features`, `cargo fmt`, `cargo test --workspace --all-features`.
- blenv: before and after screenshots, plus interactions: select a group, set a color, apply a palette, toggle an
  animation preset.

## Notes

### What changed
- **Reads.** `programmer_ui` takes one snapshot per frame through `with_show`:
  `show.clone_for_session(self.client.session())` (new, `dmx/session.rs`). It is `clone_for_ui` as that session
  sees the show: the session's selection, stack, control buffer, scene focus and focused graph are swapped into the
  `DESK` fields of the copy (its focused scene is the one kept whole), so the page's existing `selection()`,
  `get_selection()`, `curr_scene()` and `.0.current_scene_focus` reads are session-correct without rewriting every
  dialog. Lookups by id (`session(id)`, `live_owner()`) stay correct. `clone_for_ui` is now
  `clone_keeping_scene(desk focus)`. Cost for the desk is the same clone as before.
  - `fixture_selection` (`components/fixtures_shared.rs`, Programmer-only) takes that snapshot instead of taking its
    own read lock; `render_dmx_simulation_dialog` (shared by Programmer, Setup, View Performance) reads
    `with_dmx_universe`, and `DmxSimulator` takes `&[u8; 513]` instead of a lock guard.
  - `live_audio_for_preview` reads `with_spectrogram`.
- **Writes.**
  - M. Alpha / M. Speed send `SetSceneMasterAlpha` / `SetSceneMasterSpeed` for the snapshot's focused scene.
    `ProgrammerUi::{master_alpha_held, master_speed_held}: Held<T>` keep the knob's local value while it is dragged
    and until the engine reports it back (or 500 ms after the last change, or when the focus moves). The `Knob` adds
    drag deltas to the drawn value, so without this it would lose movement; as a side effect M. Alpha no longer loses
    the fractional part every frame (it was truncated to `u8` per frame before).
  - **Engine fix** (`DmxEngine::apply`): `SetSceneMasterAlpha` / `SetSceneMasterSpeed` no longer reset the sender's
    control buffer. As events they would have cleared the attribute faders on every knob turn (seen in blenv; the
    direct write didn't). They are not selection events, so this is also right for korg's faders.
- **New `HostCommand`s** (`hub.rs`, handled in `DmxEngine::apply_host`, with the sender's session swapped in):
  - `ToggleAnimationPreset(AnimationPreset)`: ensures the preset's template (lowest-id template named after it,
    created if missing), then applies `toggle_animation_event(id, active)` through `apply` on the sender's selection
    in its focused scene (active = already on that selection). `Ok { created: Some(AnimationTemplate(id)) }` when the
    template was created, `Ok { created: None }` otherwise; `Refused("Animation limit reached")` when every id is
    taken, or the engine's reason if it rejects the toggle (the created template stays, as before).
  - `SaveAnimationAsTemplate { selection, animation_id, name }`: copies the animation's spec from the sender's focused
    scene into a new template named `name.trim()`, and copies a WASM animation's plugin state to the template
    instance. `Ok { created: Some(AnimationTemplate(id)) }`, `Refused("Animation not found")` or
    `Refused("Animation limit reached")`.
  - `CloneAnimationInstanceState { plugin_key, source, target }`: `plugin::wasm::clone_animation_instance_state`
    (storage copy + `target` queued for reset). Always `Ok`.
  - `KeepAnimationEditorAlive { plugin_key, instance_id, fixtures }`: writes the editor request with `last_seen = now`
    (the engine still drops requests older than 250 ms). Open editors resend it every
    `hub::ANIMATION_EDITOR_KEEPALIVE` (100 ms) or at once when plugin or fixtures change.
  - `CreatedId::AnimationTemplate(u8)` is new.
  - `HostCommand::edits_show()`: the desk's `handle_events` marks the show unsaved for host commands seen on the bus
    that change saved data (`ToggleAnimationPreset`, `SaveAnimationAsTemplate`), like it does for `ControlEvent`s.
- **Replies.** `toggle_preset` and `save_animation_as_preset` use `reply_handlers.send_then`; refusals and
  "Saved preset \"…\"" show as popups when the reply arrives.
- **Engine-side helpers** (`ensure_preset_template`, `preset_template_id`, `save_as_template`,
  `toggle_animation_event`, `selection_animations`, `ANIMATION_LIMIT_REACHED`) moved from `programmer_animations.rs`
  to `dmx/animation/templates.rs` (with their unit tests). UI-only helpers (`unique_template_name`,
  `saved_templates`, labels) stay.
- **WASM animation editor** (`AnimationEditState`, shared with Setup › Animations): `show` / `anim_wasm_ui` dropped the
  `data: &AppStateWrapper` parameter. They read the new `EngineClient` readers `with_plugin_runtime_kinds` and
  `with_animation_editor_ui_ops`, and keep the editor alive by command. Setup › Animations' call site only lost the
  argument.
- `crates/core/src/client.rs`: the two new readers on the trait and `LocalClient`.

### Decisions and deviations
- **Session snapshot instead of per-block `with_session` closures.** The page already drew from one cloned snapshot
  per frame; `clone_for_session` keeps that structure and its cost while making every read session-aware. A remote
  client would hand out an owned snapshot anyway. Pages with similar "snapshot per frame" code (Setup, Performance)
  can use the same call.
- **The preset toggle runs entirely in the engine**, so the inner `AddAnimation`/`PlayAnimation`/`RemoveAnimation`
  transaction is no longer on the bus and plugins no longer see it in `TickInput.events` for preset taps (no in-tree
  plugin reacts to those events; the console plugin only sends them). Taps on saved templates still send the
  `ControlEvent` as before.
- **Popups:** every refusal of `ToggleAnimationPreset` shows (before, only "Animation limit reached" could, since
  the UI never saw transaction refusals; the buttons are disabled for the usual causes). "Save as preset" now also
  reports "Animation not found" instead of silently doing nothing.
- **Outside the Touches list:** `components/fixtures_shared.rs` (Programmer-only `fixture_selection`, the shared DMX
  simulation dialog), `pages/animation_spec_editor.rs` (+ one call site in `pages/setup/animations.rs`),
  `app/event.rs` (dirty marking), `app/mod.rs` (new `ProgrammerUi` fields), `client.rs`, `hub.rs`,
  `dmx/session.rs`, `dmx/state.rs`, `dmx/animation/{mod,templates}.rs`. `plugin/wasm.rs` is unchanged (its function
  is now called by the engine only, from `apply_host` and `AddAnimation`, plus Setup › Animations until 043).
- `#[cfg(test)]` code in `programmer.rs` still writes `AppState` directly (allowed by the ticket); the old engine test
  in `programmer_animations.rs` was replaced by the engine tests below.

### Verification
- `cargo test --workspace --all-features`: all pass. blaulicht-core 293 → 301. New: `dmx::host_command_tests`
  (two-client harness): `a_preset_toggles_on_the_senders_selection_and_focus` (remote creates + applies in its scene
  1 on its limited selection, desk reuses the template on its own, second tap removes only the remote's),
  `a_preset_is_refused_when_every_template_id_is_taken`, `a_preset_without_selection_is_refused`,
  `saving_an_animation_reads_the_senders_focused_scene` (desk refused "Animation not found", remote saves trimmed
  name), `cloning_animation_state_copies_it_and_restarts_the_target`, `an_editor_keepalive_registers_the_editor`,
  `scene_master_faders_keep_the_control_buffer`, `a_session_snapshot_reads_that_sessions_fields`; plus
  `programmer::tests::a_turned_knob_shows_its_value_until_the_engine_has_it` and the moved template unit tests.
- `cargo clippy --workspace --all-targets --all-features`: blaulicht-core lib 141, lib test 160 (unchanged; the
  `too_many_arguments` on `AnimationEditState::show` is still one warning, now 9 arguments). Build: 84 lib warnings
  (unchanged). None in new code.
- `cargo fmt`: clean for touched files; the usual unrelated hunks were reverted.
- **blenv** (kiosk 800x480, fresh copies of `Kuze_Theater.json`; screenshots read). Before = this branch's start
  (`894ef84`) rebuilt, after = this change; the same clicks on both:
  - Programmer on load: identical (M. Alpha 3, M. Speed 1, 79 scene changes).
  - Select "Par Truss": group highlighted, its four fixtures listed (inspector: `group_ids: {6}`).
  - Set a color (Hue fader to 258) and drag M. Alpha up: before 3 → 24, after 3 → 26 (no per-frame truncation);
    Hue stays 258 after the knob turn (first after-run showed it reset to 0, which led to the engine fix above);
    inspector `master_alpha_fader: 26`. M. Speed drag → 2, inspector `master_speed: _2`. Unsaved dot shows.
  - Apply a palette: the show has none, so "Red" was created with `blctl event CreatePalette`; tapping it in the
    Palettes dialog shows "Active: Red", the button reads "Palettes (1)" and the color faders lock.
  - Animations dialog: Kick Flash tap applies it ("On selection (1)", button highlighted, toolbar "Animations (1)"),
    tap again removes it, as before. "Save" → name "Kick Flash 2" → popup "Saved preset \"Kick Flash 2\"" and the new
    template under Saved.
  - Limit message: a showfile copy with 256 templates; tapping "Kick Flash" shows the "Animation limit reached" popup
    (from `CommandOutcome::Refused`).
  - Scene change with undo-all: a copy with a second scene "Second" (M. Alpha 77): tapping it focuses it and the knob
    shows 77; Undo all → confirm "Removes all 79 changes from the scene \"Second\"" → "No Scene Changes"; back on
    scene 0 its 79 changes are still there.
  - LIVE on: highlighted with red dot; off: inspector `live_mode: false`.
  - WASM editor (desktop layout, Setup › Animations, new Wasm template, "Chaos engine"): the plugin's editor UI
    appears (served through `KeepAnimationEditorAlive` + `with_animation_editor_ui_ops`) and stays live; dragging
    "Warp speed" to 255 works.
  - Logs: only the usual missing DMX/MIDI devices, XSETTINGS/XRandR and thread-priority warnings.
  - Not checked live: the Programmer's Scene Animations dialog editing a WASM animation (needs a WASM animation
    applied in a scene; its two `CloneAnimationInstanceState` sends are covered by the engine test), and a remote
    session in the running app (no in-tree client opens one; covered by the harness tests).
- Observed, unchanged: attribute faders sometimes end a fast `blenv drag` a step short (the last move and the release
  land in one frame); an inspector `CreatePalette` clears the desk's fader display (control buffer reset for
  non-selection events, the known LIVE behaviour).

### For 043, 044, 051, 045
- `show.clone_for_session(client.session())` inside `with_show` gives a session-correct snapshot for pages that
  draw a whole frame from one copy.
- Setup › Animations still calls `plugin::wasm::clone_animation_instance_state` directly (two places in
  `pages/setup/animations.rs`): switch to `HostCommand::CloneAnimationInstanceState` (043). Its editor already uses
  the client.
- Host commands that change saved data must return `true` from `HostCommand::edits_show`, or the desk won't mark the
  show unsaved.
- Turning an event-backed knob: use the `Held<T>` pattern (`programmer.rs`) for delta-based widgets (`Knob`,
  `SpeedKnob`); absolute widgets (`HFader`) don't need it.
- `apply_host` can reuse `self.apply(state, ControlEventMessage::new(EventOriginator::Web, ..))` for the
  `ControlEvent` part of a command; its rollback is dropped and the inner event is not on the bus.
