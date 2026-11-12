//! The Programmer page (formerly "Fixtures Performance"): scene list, group
//! and fixture selection, fixture attributes, palettes, animations and the
//! focused scene's changes.

use crate::app::{
    components::{self, ButtonSize, Dialog, Held, Knob, SpeedKnob},
    pages::{palette_kind_label, AnimationEditState, AnimationPresetsUi},
    ui_tokens::Density,
    BlaulichtApp,
};
use blaulicht_engine::{client::EngineClient, dmx::EngineState, hub::HostCommand};
use blaulicht_shared::{
    scene::{FixtureSelection, FixtureSelector, Scene},
    AnimationSpeedModifier, ControlEvent, ControlEventMessage, EventOriginator, FixtureProperty,
};
use egui::{Color32, Context, RichText};
use std::collections::{BTreeMap, HashMap, HashSet};

/// Buttons in the Programmer toolbar, LIVE included.
const TOOLBAR_BUTTONS: usize = 7;

pub struct ProgrammerUi {
    pub scene_overview_animation_edit: AnimationEditState,
    pub scene_overview_animation_selection_edit: Option<(FixtureSelection, u8)>,
    pub scene_overview_animation_selection_edit_need_to_load: bool,
    pub close_scene_overview_after_child: bool,
    pub palette_dialog_open: bool,
    pub active_palettes_cache: HashMap<FixtureSelection, Vec<u8>>,
    /// The full Scene Changes dialog (all changes of the focused scene, by
    /// fixture).
    pub scene_changes_dialog_open: bool,
    /// The "Clear all scene changes?" confirmation.
    pub undo_all_changes_confirm_open: bool,
    /// The Animations dialog: presets and templates for the selection.
    pub animation_presets: AnimationPresetsUi,
    /// The focused scene's M. Alpha while it is being turned.
    pub master_alpha_held: Held<f32>,
    /// The focused scene's M. Speed while it is being turned.
    pub master_speed_held: Held<AnimationSpeedModifier>,
    /// The desktop layout's state (ticket 075).
    pub desktop: super::programmer_desktop::ProgrammerDesktop,
}

impl BlaulichtApp {
    /// The `LIVE` toggle: exclusive programming mode.
    ///
    /// Off, the operator's selection has no effect on output -- the render base
    /// is the ephemeral `BLANK` scene and whatever is playing stays playing, so
    /// a scene can be built mid-set without disturbing the rig. On, the
    /// selected scene takes the base slot and every overlay is suppressed,
    /// giving a real-time preview of what is being programmed.
    ///
    /// There is one rig, so one preview: the button is "on" when this
    /// frontend's session owns the preview, and the red dot shows whenever
    /// any session previews (overlays are suppressed for everyone).
    fn live_mode_toggle(&mut self, ui: &mut egui::Ui, dmx_engine: &EngineState, size: ButtonSize) {
        let owned = dmx_engine.live_owner() == Some(self.client.session());
        let any = dmx_engine.0.live_mode;

        // Desktop: the lit button (`.livebtn`), its dot blinking while this
        // session previews. The kiosk keeps its LIVE button below.
        if Density::of(ui.ctx()).is_compact() {
            // In the page bar beside Blackout, at the show height.
            let _ = size;
            let response = components::studio::live_button(
                ui,
                "Exclusive preview",
                owned,
                crate::app::ui_tokens::TARGET_SHOW,
            );
            if response.clicked() {
                self.client.send(ControlEventMessage::new(
                    EventOriginator::Web,
                    ControlEvent::ToggleLiveMode,
                ));
            }
            response.on_hover_text(match (owned, any) {
                (true, _) => "Only the selected scene renders. Click to return to the live rig.",
                (false, true) => {
                    "Another operator is previewing their scene; overlays are suppressed. Click to preview yours instead."
                }
                (false, false) => {
                    "Edits go to the selected scene without affecting output. Click to preview it live."
                }
            });
            return;
        }

        // Scoped so the painter below can anchor the dot to the button's rect.
        let scope = ui.scope(|ui| components::button(ui, owned, "LIVE", size));

        if scope.inner {
            self.client.send(ControlEventMessage::new(
                EventOriginator::Web,
                ControlEvent::ToggleLiveMode,
            ));
        }

        if any {
            const RADIUS: f32 = 4.0;
            let rect = scope.response.rect;
            // The desktop's button fits its label, so the dot sits on the
            // corner like a badge instead of inside.
            let center = if Density::of(ui.ctx()).is_compact() {
                egui::pos2(rect.right() - 1.0, rect.top() + 1.0)
            } else {
                egui::pos2(rect.right() - RADIUS - 3.0, rect.top() + RADIUS + 3.0)
            };
            ui.painter().circle_filled(center, RADIUS, Color32::RED);
        }

        scope.response.on_hover_text(match (owned, any) {
            (true, _) => {
                "Exclusive preview: only the selected scene renders. Click to return to the live rig."
            }
            (false, true) => {
                "Another operator is previewing their scene; overlays are suppressed. Click to preview yours instead."
            }
            (false, false) => {
                "Edits go to the selected scene without affecting output. Click to preview it live."
            }
        });
    }

    pub fn programmer_ui(
        &mut self,
        ui: &mut egui::Ui,
        ctx: &Context,
        render_context: crate::app::page::PageRenderContext,
    ) {
        // One snapshot per frame, as this client's session sees the show:
        // `selection()`, `get_selection()` and `curr_scene()` are its own.
        let session = self.client.session();
        let dmx_engine = self.client.with_show(|show| {
            show.clone_for_session(session)
                .unwrap_or_else(|| show.clone_for_ui())
        });
        let groups = dmx_engine.groups();

        // Desktop (075): its own layout; the kiosk keeps the one below.
        if Density::of(ctx).is_compact() {
            self.refresh_active_palettes_cache(&dmx_engine);
            self.render_scene_animations_dialog(ctx, &dmx_engine);
            self.render_animation_presets_dialog(ctx, &dmx_engine);
            self.render_undo_all_changes_dialog(ctx, &dmx_engine);
            let _ = render_context;
            self.programmer_desktop_ui(ui, ctx, &dmx_engine);
            return;
        }

        self.clear_selection_shortcut(ctx, &dmx_engine);

        //
        // Refresh the active-palettes cache for the current selection.
        //
        self.refresh_active_palettes_cache(&dmx_engine);

        //
        // Dialogs start.
        //
        self.render_dmx_simulation_dialog(ctx, groups);
        self.render_scene_animations_dialog(ctx, &dmx_engine);
        self.render_animation_presets_dialog(ctx, &dmx_engine);
        self.render_palette_assignment_dialog(ctx, &dmx_engine);
        if self.programmer_ui_state.scene_changes_dialog_open {
            // Renders the Clear confirmation itself, stacked on top.
            self.render_scene_changes_dialog(ctx, &dmx_engine);
        } else {
            self.render_undo_all_changes_dialog(ctx, &dmx_engine);
        }

        //
        // Main UI start.
        //

        // Desktop (059): the page bar with Blackout. The kiosk keeps its
        // layout.
        if Density::of(ctx).is_compact() {
            let black = self.client.grand_master_percent() == 0;
            let mut toggle = false;
            // Flush with the content area like Setup's page bar: the page
            // frame's 8px margin is drawn over, and the content below
            // starts under the bar.
            let top = ui.cursor().top();
            let full = ui.max_rect().expand(8.0).intersect(ui.clip_rect());
            let bar_rect = egui::Rect::from_min_size(
                egui::pos2(full.left(), top - 8.0),
                egui::vec2(full.width(), crate::app::ui_tokens::PAGE_BAR_HEIGHT),
            );
            ui.allocate_exact_size(
                egui::vec2(
                    ui.available_width(),
                    crate::app::ui_tokens::PAGE_BAR_HEIGHT - 8.0,
                ),
                egui::Sense::hover(),
            );
            let mut bar_ui = ui.new_child(
                egui::UiBuilder::new()
                    .max_rect(bar_rect)
                    .layout(egui::Layout::top_down(egui::Align::Min)),
            );
            bar_ui.set_clip_rect(bar_rect);
            components::studio::page_bar(
                &mut bar_ui,
                |_| {},
                |ui| {
                    toggle = components::studio::blackout_button(ui, black).clicked();
                    self.live_mode_toggle(ui, &dmx_engine, ButtonSize::Medium);
                },
            );
            if toggle {
                self.toggle_blackout();
            }
        }

        ui.allocate_ui_with_layout(
            egui::vec2(ui.available_width(), ui.available_height()), // fixed width, max height
            render_context.primary_layout(),
            |ui| {
                self.scene_overview(ui, &dmx_engine, render_context.is_narrow_dynamic());

                ui.separator();

                ui.allocate_ui_with_layout(
                    egui::vec2(ui.available_width(), ui.available_height()), // fixed width, max height
                    egui::Layout::top_down(egui::Align::Min),
                    |ui| {
                        render_context.horizontal(ui, egui::Align::Min, |ui| {
                            // Narrow dynamic panes wrap the toolbar instead.
                            let button_size = if render_context.is_narrow_dynamic() {
                                ButtonSize::Medium
                            } else {
                                ButtonSize::Medium.with_touch_width(toolbar_button_width(
                                    ui.available_width(),
                                    ui.spacing().item_spacing.x,
                                    TOOLBAR_BUTTONS,
                                ))
                            };

                            if components::button(
                                ui,
                                self.current_scene_animations_dialog_open,
                                "Scene Animations",
                                button_size,
                            ) {
                                self.current_scene_animations_dialog_open =
                                    !self.current_scene_animations_dialog_open;
                            }

                            if components::button(
                                ui,
                                self.programmer_ui_state.animation_presets.open,
                                &format!(
                                    "Animations ({})",
                                    Self::selection_animation_count(&dmx_engine)
                                ),
                                button_size,
                            ) {
                                self.programmer_ui_state.animation_presets.open =
                                    !self.programmer_ui_state.animation_presets.open;
                            }

                            self.clear_selection_button(
                                ui,
                                !dmx_engine.selection().is_empty(),
                                button_size,
                            );

                            if components::button(
                                ui,
                                !dmx_engine.selection().fixtures_in_group.is_empty(),
                                "UnLmt",
                                button_size,
                            ) {
                                if let (1, Some(&curr_group)) = (
                                    dmx_engine.selection().group_ids.len(),
                                    dmx_engine.selection().group_ids.iter().next(),
                                ) {

                                let instr = vec![
                                    ControlEvent::RemoveAllSelection,
                                    ControlEvent::SelectGroup(curr_group),
                                ];

                                self.client.send(ControlEventMessage::new(EventOriginator::Web,
                                    ControlEvent::Transaction(instr)));
                                }
                            }

                            {
                                let active_count = self
                                    .programmer_ui_state
                                    .active_palettes_cache
                                    .get(&dmx_engine.get_selection())
                                    .map(|v| v.len())
                                    .unwrap_or(0);
                                let label = format!("Palettes ({active_count})");
                                let selection_empty = dmx_engine.selection().is_empty();
                                ui.add_enabled_ui(!selection_empty, |ui| {
                                    if components::button(
                                        ui,
                                        self.programmer_ui_state.palette_dialog_open,
                                        &label,
                                        button_size,
                                    ) {
                                        self.programmer_ui_state.palette_dialog_open =
                                            !self.programmer_ui_state.palette_dialog_open;
                                    }
                                });
                            }

                            if components::button(
                                ui,
                                !dmx_engine.selection().fixtures_in_group.is_empty(),
                                "Lmt All",
                                button_size,
                            ) {
                                if let (1, Some(&curr_group), Some(group)) = (
                                    dmx_engine.selection().group_ids.len(),
                                    dmx_engine.selection().group_ids.iter().next(),
                                    dmx_engine
                                        .selection()
                                        .group_ids
                                        .iter()
                                        .next()
                                        .and_then(|id| dmx_engine.groups().get(id)),
                                ) {

                                    if dmx_engine.selection().fixtures_in_group.is_empty() {
                                        let mut instr = vec![
                                            ControlEvent::RemoveAllSelection,
                                            ControlEvent::SelectGroup(curr_group),
                                        ];

                                        for (fix_id, _) in &group.fixtures {
                                            instr.push(ControlEvent::LimitSelectionToFixtureInCurrentGroup(*fix_id));
                                        }

                                        self.client.send(ControlEventMessage::new(EventOriginator::Web,
                                            ControlEvent::Transaction(instr)));
                                    }
                                    else {
                                        let mut instr = vec![
                                            ControlEvent::RemoveAllSelection,
                                            ControlEvent::SelectGroup(curr_group),
                                        ];

                                        let selec = dmx_engine.selection().clone();

                                        for (fix_id, _) in &group.fixtures {
                                            if !selec.fixtures_in_group.contains(fix_id) {
                                                instr.push(ControlEvent::LimitSelectionToFixtureInCurrentGroup(*fix_id));
                                            }
                                        }

                                        self.client.send(ControlEventMessage::new(EventOriginator::Web,
                                            ControlEvent::Transaction(instr)));

                                        // self.client.send(ControlEventMessage::new(EventOriginator::Web, 
                                        //     ControlEvent::Transaction(vec![
                                        //         ControlEvent::RemoveAllSelection,
                                        //         ControlEvent::SelectGroup(curr_group),
                                        //     ])));
                                    }
                                }
                            };

                            // Pushed to the far right of the toolbar. The height
                            // is pinned to one button: a bare `with_layout` would
                            // claim the whole panel and centre the toggle in it.
                            // The desktop has it in the page bar instead.
                            if !Density::of(ui.ctx()).is_compact() {
                            ui.allocate_ui_with_layout(
                                egui::vec2(
                                    ui.available_width(),
                                    button_size.height_at(Density::of(ui.ctx())),
                                ),
                                egui::Layout::right_to_left(egui::Align::Center),
                                |ui| {
                                    self.live_mode_toggle(ui, &dmx_engine, button_size);
                                },
                            );
                            }
                        });

                        ui.separator();

                        ui.allocate_ui_with_layout(
                            egui::vec2(ui.available_width(), ui.available_height()), // fixed width, max height
                            egui::Layout::left_to_right(egui::Align::Min),
                            |ui| {
                                self.fixture_selection(&dmx_engine, ui);

                                ui.separator();

                                ui.vertical(|ui| {
                                    ui.horizontal(|ui| {
                                        let scene = dmx_engine.curr_scene();

                                        // ui.separator();
                                        ui.vertical(|ui| {
                                            ui.allocate_ui_with_layout(
                                                egui::vec2(ui.available_width(), ui.available_height()), // fixed width, max height
                                                egui::Layout::left_to_right(egui::Align::Center),
                                                |ui| {
                                                ui.add_space(22.5);
                                                let scene_id = dmx_engine.0.current_scene_focus;
                                                let sink = &scene.sink;

                                                let held = &mut self.programmer_ui_state.master_alpha_held;
                                                let mut master_alpha =
                                                    held.shown(scene_id, sink.master_alpha_fader as f32);
                                                let response = ui.add(
                                                    Knob::new(&mut master_alpha, 0.0..=100.0).with_label("M. Alpha"),
                                                );
                                                if response.changed() {
                                                    held.hold(scene_id, master_alpha);
                                                    self.client.send_event(ControlEvent::SetSceneMasterAlpha(
                                                        scene_id,
                                                        master_alpha as u8,
                                                    ));
                                                }
                                                held.settle(response.dragged(), scene_id, |value| {
                                                    value as u8 == sink.master_alpha_fader
                                                });

                                                ui.add_space(25.0);

                                                let held = &mut self.programmer_ui_state.master_speed_held;
                                                let mut speed = held.shown(scene_id, sink.master_speed);
                                                let response = ui.add(
                                                    SpeedKnob::new(&mut speed).with_label("M. Speed"),
                                                );
                                                if response.changed() {
                                                    held.hold(scene_id, speed);
                                                    self.client.send_event(ControlEvent::SetSceneMasterSpeed(
                                                        scene_id, speed,
                                                    ));
                                                }
                                                held.settle(response.dragged(), scene_id, |value| {
                                                    value == sink.master_speed
                                                });
                                            });

                                            ui.separator();
                                            ui.add_space(12.0);
                                            ui.set_max_width(ui.available_width().max(1.0));
                                            self.scene_changes_button(ui, &dmx_engine);
                                            ui.separator();

                                            for (selection, animations) in
                                                &scene.sink.active_animations
                                            {
                                                // The engine keys animations by the sorted selection.
                                                if *selection != dmx_engine.get_selection().sorted() {
                                                    continue;
                                                }

                                                // ui.label(
                                                //     RichText::new(format!(
                                                //         "Selection: {selection:?}"
                                                //     ))
                                                //     .color(Color32::LIGHT_GREEN),
                                                // );

                                                for (animation_id, animation) in animations {
                                                    // let spec = dmx_engine.0
                                                    //     .animation_templates
                                                    //     .get(animation_id)
                                                    //     .unwrap();

                                                    // Render each animation in its own rectangle/group
                                                    egui::Frame::group(ui.style()).show(ui, |ui| {
                                                        ui.set_width(ButtonSize::Medium.dim().0.x);
                                                        ui.vertical(|ui| {
                                                            // LEFT: Info (name + property + timing)
                                                            // let label = animation.spec_cloned.name[0..10];

                                                            // TODO: do this prettier
                                                            let label = match animation.spec_cloned.name.char_indices().nth(10) {
                                                                    None => animation.spec_cloned.name.as_str(),
                                                                    Some((idx, _)) => &animation.spec_cloned.name[..idx],
                                                                };
                                                            // label.truncate(10);

                                                            // [..=10];

                                                            ui.label(
                                                                RichText::new(label)
                                                                    .color(Color32::WHITE),
                                                            );

                                                            ui.add_space(8.0);

                                                            // RIGHT: Buttons stacked vertically and right-aligned
                                                            ui.with_layout(
                                                                egui::Layout::right_to_left(
                                                                    egui::Align::Min,
                                                                ),
                                                                |ui| {
                                                                    ui.vertical(|ui| {

                                                                        let (label, enabled, event) =
                                                                            match animation.enabled {
                                                                                true => (
                                                                                    "Pause",
                                                                                    true,
                                                                                    ControlEvent::PauseAnimation(
                                                                                        *animation_id,
                                                                                    ),
                                                                                ),
                                                                                false => (
                                                                                    "Play",
                                                                                    false,
                                                                                    ControlEvent::PlayAnimation(
                                                                                        *animation_id,
                                                                                    ),
                                                                                ),
                                                                            };

                                                                        if components::button(
                                                                            ui,
                                                                            enabled,
                                                                            label,
                                                                            ButtonSize::Medium,
                                                                        ) {
                                                                            let mut selection_instructions = selection
                                                                                .generate_instructions();

                                                                            selection_instructions.push_front(
                                                                                ControlEvent::PushSelection,
                                                                            );
                                                                            selection_instructions
                                                                                .push_back(event);
                                                                            selection_instructions.push_back(
                                                                                ControlEvent::PopSelection,
                                                                            );

                                                                            self.client.send(
                                                                                ControlEventMessage::new(
                                                                                    EventOriginator::Web,
                                                                                    ControlEvent::Transaction(
                                                                                        selection_instructions
                                                                                            .into_iter()
                                                                                            .collect(),
                                                                                    ),
                                                                                ),
                                                                            );
                                                                        }
                                                                        if components::button(
                                                                            ui,
                                                                            false,
                                                                            "Edit",
                                                                            ButtonSize::Medium,
                                                                        ) {
                                                                            self.current_scene_animations_dialog_open = true;
                                                                            self.programmer_ui_state
                                                                                .scene_overview_animation_selection_edit =
                                                                                Some((selection.to_owned(), *animation_id));
                                                                            self.programmer_ui_state
                                                                                .scene_overview_animation_selection_edit_need_to_load =
                                                                                true;
                                                                            self.programmer_ui_state
                                                                                .close_scene_overview_after_child = true;
                                                                        }
                                                                    });
                                                                },
                                                            );
                                                        });
                                                    });
                                                }
                                            }
                                        });
                                    });
                                });
                            },
                        );
                    },
                );
            },
        );
    }

    /// Sends `RemoveAllSelection`.
    fn clear_selection(&self) {
        self.client.send(ControlEventMessage::new(
            EventOriginator::Web,
            ControlEvent::RemoveAllSelection,
        ));
    }

    /// The toolbar's Clear button; disabled while nothing is selected.
    fn clear_selection_button(&self, ui: &mut egui::Ui, has_selection: bool, size: ButtonSize) {
        let scope = ui.scope(|ui| {
            components::action_button(ui, has_selection, "Clear", size, Some("Nothing selected"))
        });
        if scope.inner {
            self.clear_selection();
        }
        if has_selection {
            scope.response.on_hover_text("Clear the selection (Esc)");
        }
    }

    /// `Esc` clears the selection, unless a text field has focus or a dialog
    /// is open (dialogs close on `Esc`).
    /// The key is consumed, so a Programmer shown in several panes clears once.
    fn clear_selection_shortcut(&self, ctx: &Context, dmx_engine: &EngineState) {
        let modal_open = ctx.memory(|memory| memory.top_modal_layer().is_some());
        let allowed = clear_selection_shortcut_allowed(
            ctx.text_edit_focused(),
            modal_open,
            dmx_engine.selection().is_empty(),
        );
        if allowed
            && ctx.input_mut(|input| input.consume_key(egui::Modifiers::NONE, egui::Key::Escape))
        {
            self.clear_selection();
        }
    }

    /// Removes one change (fixture property) from the focused scene.
    fn remove_scene_change(&self, change: FixtureSelector) {
        let FixtureSelector { gid, fid, property } = change;
        self.client.send(ControlEventMessage::new(
            EventOriginator::Web,
            ControlEvent::RemoveChange {
                selection: FixtureSelection {
                    fixtures: vec![(gid, fid)],
                },
                property,
            },
        ));
    }

    /// The Scene Changes button in the right column: the number of changes
    /// of the focused scene. It opens the Scene Changes dialog, where single
    /// changes are removed and Clear removes all of them.
    fn scene_changes_button(&mut self, ui: &mut egui::Ui, dmx_engine: &EngineState) {
        let count = dmx_engine.curr_scene().sink.changeset.len();
        let width = ui.available_width().clamp(40.0, 200.0);
        if components::button(
            ui,
            self.programmer_ui_state.scene_changes_dialog_open,
            &format!("Scene Changes ({count})"),
            ButtonSize::Medium.with_touch_width(width),
        ) {
            self.programmer_ui_state.scene_changes_dialog_open =
                !self.programmer_ui_state.scene_changes_dialog_open;
        }
    }

    /// All changes of the focused scene, grouped by fixture; every property
    /// can be removed on its own, or all of them with Clear.
    fn render_scene_changes_dialog(&mut self, ctx: &Context, dmx_engine: &EngineState) {
        if !self.programmer_ui_state.scene_changes_dialog_open {
            return;
        }

        const WIDTH: f32 = 560.0;
        const HEIGHT: f32 = 400.0;
        const LIST_HEIGHT: f32 = 260.0;
        const CHIP_WIDTH: f32 = 120.0;

        let scene = dmx_engine.curr_scene();
        let groups = dmx_engine.groups();
        let changes = sorted_changes(&scene.sink.changeset);

        let mut by_fixture: BTreeMap<(u8, u8), Vec<FixtureSelector>> = BTreeMap::new();
        for change in &changes {
            by_fixture
                .entry((change.gid, change.fid))
                .or_default()
                .push(change.clone());
        }

        // Esc belongs to the confirmation while it is open.
        let confirm_open = self.programmer_ui_state.undo_all_changes_confirm_open;

        let response = Dialog::new("Scene Changes".to_string(), egui::vec2(WIDTH, HEIGHT))
            .with_backdrop()
            .show(ctx, |ui| {
                ui.heading(format!("Scene Changes: {} ({})", scene.name, changes.len()));
                ui.label("Tap a property to remove that change from the scene.");
                ui.separator();

                egui::ScrollArea::vertical()
                    .id_salt("programmer_scene_changes_dialog")
                    // The dialog window has no fixed height, so the list
                    // needs an explicit one.
                    .max_height(LIST_HEIGHT)
                    .min_scrolled_height(LIST_HEIGHT)
                    .auto_shrink([false, false])
                    .show(ui, |ui| {
                        if by_fixture.is_empty() {
                            ui.label("No Scene Changes");
                        }
                        for ((gid, fid), fixture_changes) in &by_fixture {
                            let (group_name, fixture_name) =
                                change_names(groups, &fixture_changes[0]);
                            // The desktop keeps to its palette: a muted header.
                            let header_color = if Density::of(ui.ctx()).is_compact() {
                                crate::app::ui_tokens::palette(ui.ctx()).muted
                            } else {
                                Color32::LIGHT_GREEN
                            };
                            ui.label(
                                RichText::new(format!(
                                    "Group {gid} ({group_name}) · Fixture {fid} ({fixture_name})"
                                ))
                                .color(header_color),
                            );
                            ui.horizontal_wrapped(|ui| {
                                for change in fixture_changes {
                                    let label = format!(
                                        "{} {}",
                                        egui_phosphor::regular::X,
                                        change.property
                                    );
                                    if components::button(
                                        ui,
                                        false,
                                        &label,
                                        ButtonSize::Medium.with_width(CHIP_WIDTH),
                                    ) {
                                        self.remove_scene_change(change.clone());
                                    }
                                }
                            });
                            ui.add_space(6.0);
                        }
                    });

                ui.separator();
                ui.horizontal(|ui| {
                    if components::action_button_as(
                        ui,
                        !changes.is_empty(),
                        false,
                        components::ButtonVariant::Danger,
                        "Clear",
                        ButtonSize::Medium.with_touch_width(140.0),
                        Some("No changes"),
                    ) {
                        self.programmer_ui_state.undo_all_changes_confirm_open = true;
                    }
                    if components::button(
                        ui,
                        true,
                        "Close",
                        ButtonSize::Medium.with_touch_width(140.0),
                    ) {
                        self.programmer_ui_state.scene_changes_dialog_open = false;
                    }
                });

                // Nested, so the confirmation gets the higher modal order and
                // stays above this dialog even after it was clicked.
                self.render_undo_all_changes_dialog(ui.ctx(), dmx_engine);
            });

        if response.cancel_requested && !confirm_open {
            self.programmer_ui_state.scene_changes_dialog_open = false;
        }
    }

    /// Confirmation for Clear: removes every change of the focused scene
    /// (one `RemoveChange` per property, in one `Transaction`). Other scenes
    /// are not touched.
    fn render_undo_all_changes_dialog(&mut self, ctx: &Context, dmx_engine: &EngineState) {
        if !self.programmer_ui_state.undo_all_changes_confirm_open {
            return;
        }

        const WIDTH: f32 = 380.0;
        const HEIGHT: f32 = 170.0;

        let scene = dmx_engine.curr_scene();
        let count = scene.sink.changeset.len();

        let response = Dialog::new(
            "Clear Scene Changes".to_string(),
            egui::vec2(WIDTH, HEIGHT),
        )
        .with_backdrop()
        .show(ctx, |ui| {
            ui.heading("Clear all changes?");
            ui.add_space(4.0);
            ui.label(format!(
                "Removes all {count} changes from the scene \"{}\". Other scenes keep their changes.",
                scene.name
            ));
            ui.add_space(12.0);
            ui.horizontal(|ui| {
                if components::button(
                    ui,
                    false,
                    "Cancel",
                    ButtonSize::Medium.with_touch_width(140.0),
                ) {
                    self.programmer_ui_state.undo_all_changes_confirm_open = false;
                }
                if components::action_button_as(
                    ui,
                    count > 0,
                    false,
                    components::ButtonVariant::Danger,
                    "Clear",
                    ButtonSize::Medium.with_touch_width(140.0),
                    Some("No changes"),
                ) {
                    if let Some(event) = undo_all_changes_event(&scene.sink.changeset) {
                        self.client
                            .send(ControlEventMessage::new(EventOriginator::Web, event));
                    }
                    self.programmer_ui_state.undo_all_changes_confirm_open = false;
                }
            });
        });

        if response.cancel_requested {
            self.programmer_ui_state.undo_all_changes_confirm_open = false;
        }
    }

    pub fn render_scene_animations_dialog(&mut self, ctx: &Context, dmx_engine: &EngineState) {
        if self.current_scene_animations_dialog_open {
            const HEIGHT: f32 = 430.0;
            const WIDTH: f32 = 500.0;

            Dialog::new(
                "Current Scene Animations".to_string(),
                egui::vec2(WIDTH, HEIGHT),
            )
            .with_backdrop()
            .show(ctx, |ui| {
                ui.set_min_height(HEIGHT - 100.0);

                let scene = dmx_engine.curr_scene();

                if let Some((selec, anim_id)) = &self
                    .programmer_ui_state
                    .scene_overview_animation_selection_edit
                {
                    let Some(animation) = scene
                        .sink
                        .active_animations()
                        .get(selec)
                        .and_then(|animations| animations.get(anim_id))
                    else {
                        self.programmer_ui_state
                            .scene_overview_animation_selection_edit = None;
                        self.programmer_ui_state
                            .scene_overview_animation_selection_edit_need_to_load = false;
                        return;
                    };

                    if self
                        .programmer_ui_state
                        .scene_overview_animation_selection_edit_need_to_load
                    {
                        self.programmer_ui_state
                            .scene_overview_animation_edit
                            .load_state(animation.spec_cloned.clone());

                        if let blaulicht_shared::AnimationSpecBody::WasmPlugin(wasm) =
                            &animation.spec_cloned.body
                        {
                            let mut fixtures = selec.fixtures.clone();
                            fixtures.sort_unstable();
                            let source = blaulicht_engine::plugin::stable_animation_instance_id(
                                &wasm.plugin_key,
                                dmx_engine.0.current_scene_focus,
                                *anim_id,
                                &fixtures,
                            );
                            self.client.send(HostCommand::CloneAnimationInstanceState {
                                plugin_key: wasm.plugin_key.clone(),
                                source,
                                target: self
                                    .programmer_ui_state
                                    .scene_overview_animation_edit
                                    .wasm_editor_instance_id,
                            });
                        }

                        self.programmer_ui_state
                            .scene_overview_animation_selection_edit_need_to_load = false;
                    }

                    let live_audio = self.live_audio_for_preview();
                    let details_panel_max = ui.clip_rect().max;
                    let new_value_spec =
                        self.programmer_ui_state.scene_overview_animation_edit.show(
                            ui,
                            ctx,
                            &dmx_engine.0.palettes,
                            &live_audio,
                            &self.client,
                            &mut self.view_state.plugin_tabs_selected,
                            &selec.fixtures,
                            details_panel_max,
                        );

                    if let Some(spec) = new_value_spec {
                        if let blaulicht_shared::AnimationSpecBody::WasmPlugin(wasm) = &spec.body {
                            let mut fixtures = selec.fixtures.clone();
                            fixtures.sort_unstable();
                            let target = blaulicht_engine::plugin::stable_animation_instance_id(
                                &wasm.plugin_key,
                                dmx_engine.0.current_scene_focus,
                                *anim_id,
                                &fixtures,
                            );
                            self.client.send(HostCommand::CloneAnimationInstanceState {
                                plugin_key: wasm.plugin_key.clone(),
                                source: self
                                    .programmer_ui_state
                                    .scene_overview_animation_edit
                                    .wasm_editor_instance_id,
                                target,
                            });
                        }
                        self.client.send(ControlEventMessage::new(
                            EventOriginator::Web,
                            ControlEvent::Transaction(vec![ControlEvent::LoadSpecIntoAnimation(
                                *anim_id, spec,
                            )]),
                        ));
                    }

                    if components::button(ui, true, "Close", ButtonSize::Medium) {
                        self.programmer_ui_state
                            .scene_overview_animation_selection_edit = None;
                        if self.programmer_ui_state.close_scene_overview_after_child {
                            self.current_scene_animations_dialog_open = false;
                        }
                        self.programmer_ui_state.close_scene_overview_after_child = false;
                    }
                } else {
                    self.render_normal_scene_animations(ui, scene);
                }
            });
        }
    }

    fn render_normal_scene_animations(&mut self, ui: &mut egui::Ui, scene: &Scene) {
        ui.heading("Active Animations");
        ui.separator();

        egui::ScrollArea::vertical().show(ui, |ui| {
            if scene.sink.active_animations.is_empty() {
                ui.label("No Animations Yet");
            }

            for (selection, animations) in &scene.sink.active_animations {
                ui.label(
                    RichText::new(format!("Selection: {selection:?}")).color(Color32::LIGHT_GREEN),
                );

                for (animation_id, animation) in animations {
                    ui.label(
                        RichText::new(format!(
                            "[{}] {} | {}",
                            animation_id,
                            animation.spec_cloned.name,
                            animation.spec_cloned.property
                        ))
                        .color(Color32::WHITE),
                    );

                    // Remove button
                    if components::button(ui, false, "Remove", ButtonSize::Medium) {
                        let mut selection_instructions = selection.generate_instructions();

                        selection_instructions.push_front(ControlEvent::PushSelection);
                        selection_instructions
                            .push_back(ControlEvent::RemoveAnimation(*animation_id));

                        selection_instructions.push_back(ControlEvent::PopSelection);

                        self.client.send(ControlEventMessage::new(
                            EventOriginator::Web,
                            ControlEvent::Transaction(selection_instructions.into_iter().collect()),
                        ));
                    }

                    let (label, enabled, event) = match animation.enabled {
                        true => ("Pause", true, ControlEvent::PauseAnimation(*animation_id)),
                        false => ("Play", false, ControlEvent::PlayAnimation(*animation_id)),
                    };

                    if components::button(ui, enabled, label, ButtonSize::Medium) {
                        let mut selection_instructions = selection.generate_instructions();

                        selection_instructions.push_front(ControlEvent::PushSelection);
                        selection_instructions.push_back(event);
                        selection_instructions.push_back(ControlEvent::PopSelection);

                        self.client.send(ControlEventMessage::new(
                            EventOriginator::Web,
                            ControlEvent::Transaction(selection_instructions.into_iter().collect()),
                        ));
                    }

                    // This is a discrete multiplier. Using the continuous
                    // HFader here mixed truncation and rounding, so the value
                    // emitted around a tick did not reliably match the value
                    // selected by the user.
                    let mut speed = animation.speed_factor;
                    if ui
                        .add(SpeedKnob::new(&mut speed).with_label("Speed"))
                        .changed()
                    {
                        let mut selection_instructions = selection.generate_instructions();
                        selection_instructions.push_front(ControlEvent::PushSelection);
                        selection_instructions
                            .push_back(ControlEvent::SetAnimationSpeed(*animation_id, speed));
                        selection_instructions.push_back(ControlEvent::PopSelection);

                        self.client.send(ControlEventMessage::new(
                            EventOriginator::Web,
                            ControlEvent::Transaction(selection_instructions.into_iter().collect()),
                        ));
                    }

                    if components::button(ui, false, "Edit", ButtonSize::Medium) {
                        self.programmer_ui_state
                            .scene_overview_animation_selection_edit =
                            Some((selection.to_owned(), *animation_id));
                        self.programmer_ui_state
                            .scene_overview_animation_selection_edit_need_to_load = true;
                        self.programmer_ui_state.close_scene_overview_after_child = false;
                    }
                }
            }
        });

        ui.separator();

        if components::button(ui, true, "Close", ButtonSize::Medium) {
            self.current_scene_animations_dialog_open = false;
        }
    }

    /// Computes the set of palette IDs currently "active" on the selection,
    /// i.e., palettes assigned to *every* fixture in the selection within the
    /// current scene's sink. Stores the result keyed by the current selection.
    fn refresh_active_palettes_cache(&mut self, dmx_engine: &EngineState) {
        let selection = dmx_engine.get_selection();
        self.programmer_ui_state.active_palettes_cache.clear();

        if selection.fixtures.is_empty() {
            self.programmer_ui_state
                .active_palettes_cache
                .insert(selection, Vec::new());
            return;
        }

        let scene = dmx_engine.curr_scene();
        let mut active: Option<Vec<u8>> = None;
        for fixture_key in &selection.fixtures {
            let ids = scene
                .sink
                .palette_assignments
                .get(fixture_key)
                .cloned()
                .unwrap_or_default();
            active = Some(match active {
                None => ids,
                Some(prev) => prev.into_iter().filter(|id| ids.contains(id)).collect(),
            });
        }
        let mut active = active.unwrap_or_default();
        active.sort_unstable();
        active.dedup();
        self.programmer_ui_state
            .active_palettes_cache
            .insert(selection, active);
    }

    fn render_palette_assignment_dialog(&mut self, ctx: &Context, dmx_engine: &EngineState) {
        if !self.programmer_ui_state.palette_dialog_open {
            return;
        }

        const WIDTH: f32 = 460.0;
        const HEIGHT: f32 = 440.0;

        let selection = dmx_engine.get_selection();
        let active: Vec<u8> = self
            .programmer_ui_state
            .active_palettes_cache
            .get(&selection)
            .cloned()
            .unwrap_or_default();

        Dialog::new("Selection Palettes".to_string(), egui::vec2(WIDTH, HEIGHT))
            .with_backdrop()
            .show(ctx, |ui| {
                if selection.fixtures.is_empty() {
                    ui.label("No selection.");
                    ui.add_space(8.0);
                    if components::button(ui, true, "Close", ButtonSize::Medium) {
                        self.programmer_ui_state.palette_dialog_open = false;
                    }
                    return;
                }

                ui.label(format!(
                    "Selection: {} fixture(s)",
                    selection.fixtures.len()
                ));
                ui.add_space(8.0);

                // One palette tree (folders as in Setup › Palettes, which
                // manages them). Active palettes are highlighted, and a tap
                // assigns or unassigns, like groups in the group tree. A
                // folder tap opens or closes the folder. The active list in
                // apply order stays readable above, since a closed folder
                // hides its palettes.
                let state = &dmx_engine.0;
                ui.add(
                    egui::Label::new(
                        RichText::new(active_palettes_summary(state, &active))
                            .size(13.0)
                            .strong(),
                    )
                    .truncate(),
                );
                ui.label(
                    RichText::new("Tap a palette to assign or unassign it.")
                        .size(12.0)
                        .color(Color32::GRAY),
                );
                ui.add_space(4.0);
                ui.separator();

                let items = components::palette_tree_items(
                    state,
                    |_| true,
                    |p| palette_kind_label(&p.kind),
                );
                if state.palettes.is_empty() {
                    ui.label("No palettes. Create them in Setup › Palettes.");
                } else if let Some(id) = palette_assignment_tree(ui, &items, &active) {
                    self.client.send(ControlEventMessage::new(
                        EventOriginator::Web,
                        palette_toggle_event(&active, id),
                    ));
                }

                ui.add_space(12.0);
                ui.separator();
                if components::button(ui, true, "Close", ButtonSize::Medium) {
                    self.programmer_ui_state.palette_dialog_open = false;
                }
            });
    }
}

/// Collapse state of the palette dialog's tree (kept for the session).
const PALETTE_ASSIGN_TREE: &str = "palette_assign";
const PALETTE_TREE_HEIGHT: f32 = 290.0;
const PALETTE_TREE_ROW_HEIGHT: f32 = 48.0;

/// The palette dialog's tree in a scroll area, with the `active` palettes
/// highlighted. Returns the tapped palette; a folder tap opens or closes the
/// folder.
fn palette_assignment_tree(
    ui: &mut egui::Ui,
    items: &[components::TreeItem<u16, u8>],
    active: &[u8],
) -> Option<u8> {
    let width = ui.available_width();
    let rows = {
        let state =
            components::TreeState::<u16>::load(ui.ctx(), components::tree_id(PALETTE_ASSIGN_TREE));
        components::visible_rows(items, state.collapsed()).len()
    };
    let action = egui::ScrollArea::vertical()
        .id_salt(PALETTE_ASSIGN_TREE)
        .max_height(PALETTE_TREE_HEIGHT)
        // The dialog has no fixed height; without this the list shrinks to
        // egui's 64 px minimum and shows only one or two rows.
        .min_scrolled_height(
            (rows as f32 * (PALETTE_TREE_ROW_HEIGHT + 2.0)).min(PALETTE_TREE_HEIGHT),
        )
        .show(ui, |ui| {
            ui.set_width(width);
            components::Tree::new(PALETTE_ASSIGN_TREE, items)
                .selected_keys(active.iter().map(|id| components::TreeKey::Leaf(*id)))
                .row_height(PALETTE_TREE_ROW_HEIGHT)
                .show(ui)
        })
        .inner;
    match action {
        Some(components::TreeAction::SelectLeaf(id)) => Some(id),
        Some(components::TreeAction::TapFolder(folder)) => {
            components::toggle_folder(ui.ctx(), PALETTE_ASSIGN_TREE, folder);
            None
        }
        _ => None,
    }
}

/// Tapping palette `id` in the palette dialog: unassign it if it is active
/// for the selection, assign it otherwise.
pub(super) fn palette_toggle_event(active: &[u8], id: u8) -> ControlEvent {
    if active.contains(&id) {
        ControlEvent::UnassignPaletteFromSelection(id)
    } else {
        ControlEvent::AssignPaletteToSelection(id)
    }
}

/// `Active: Red, Amber` in apply order (`Active: none` without any).
fn active_palettes_summary(state: &blaulicht_shared::EngineState, active: &[u8]) -> String {
    if active.is_empty() {
        return "Active: none".to_string();
    }
    let names: Vec<String> = active
        .iter()
        .map(|id| components::item_name(state, blaulicht_shared::folder::FolderItem::Palette(*id)))
        .collect();
    format!("Active: {}", names.join(", "))
}

/// Width of one toolbar button so `count` of them fit in `available` (at most
/// the usual 90 px, at least 60 px).
fn toolbar_button_width(available: f32, spacing: f32, count: usize) -> f32 {
    let default = ButtonSize::Medium.dim().0.x;
    if count == 0 {
        return default;
    }
    let gaps = spacing * count.saturating_sub(1) as f32;
    ((available - gaps) / count as f32)
        .floor()
        .clamp(60.0, default)
}

/// Whether `Esc` may clear the selection: not while typing, not while a
/// dialog is open (it closes on `Esc`), and only if there is a selection.
fn clear_selection_shortcut_allowed(
    text_edit_focused: bool,
    modal_open: bool,
    selection_empty: bool,
) -> bool {
    !text_edit_focused && !modal_open && !selection_empty
}

/// [`sorted_changes`] for the desktop Scene pane.
pub(super) fn sorted_changes_pub(changeset: &HashSet<FixtureSelector>) -> Vec<FixtureSelector> {
    sorted_changes(changeset)
}

/// A scene's changes ordered by group, fixture and property.
fn sorted_changes(changeset: &HashSet<FixtureSelector>) -> Vec<FixtureSelector> {
    let mut changes: Vec<FixtureSelector> = changeset.iter().cloned().collect();
    changes.sort_by_key(|change| (change.gid, change.fid, change.property as u8));
    changes
}

/// Group and fixture name of a change, or "Unknown …" for stale ids.
fn change_names<'a>(
    groups: &'a blaulicht_shared::EngineGroups,
    change: &FixtureSelector,
) -> (&'a str, &'a str) {
    let Some(group) = groups.get(&change.gid) else {
        return ("Unknown group", "Unknown fixture");
    };
    let fixture = group
        .fixtures
        .get(&change.fid)
        .map(|fixture| fixture.name.as_str())
        .unwrap_or("Unknown fixture");
    (group.name.as_str(), fixture)
}

/// The event behind Clear: one `RemoveChange` per changed property,
/// covering every fixture that changed it, in one `Transaction`. Like the
/// per-row remove, it acts on the focused scene. `None` without changes.
pub(crate) fn undo_all_changes_event(changeset: &HashSet<FixtureSelector>) -> Option<ControlEvent> {
    let mut by_property: BTreeMap<u8, (FixtureProperty, Vec<(u8, u8)>)> = BTreeMap::new();
    for change in sorted_changes(changeset) {
        by_property
            .entry(change.property as u8)
            .or_insert_with(|| (change.property, Vec::new()))
            .1
            .push((change.gid, change.fid));
    }
    if by_property.is_empty() {
        return None;
    }
    Some(ControlEvent::Transaction(
        by_property
            .into_values()
            .map(|(property, fixtures)| ControlEvent::RemoveChange {
                selection: FixtureSelection { fixtures },
                property,
            })
            .collect(),
    ))
}

#[cfg(test)]
mod tests {
    use super::{
        active_palettes_summary, clear_selection_shortcut_allowed, palette_toggle_event,
        sorted_changes, toolbar_button_width, undo_all_changes_event,
    };
    use blaulicht_engine::{dmx::DmxEngine, event::SystemEventBus, state::AppState};
    use blaulicht_shared::{
        palette::{Palette, PaletteKind},
        scene::FixtureSelector,
        ControlEvent, ControlEventMessage, EngineState, EventOriginator, FixtureProperty,
    };
    use std::{collections::HashSet, sync::Arc};

    #[test]
    fn palette_dialog_toggles_and_summarises_in_apply_order() {
        assert_eq!(
            format!("{:?}", palette_toggle_event(&[3, 1], 1)),
            format!("{:?}", ControlEvent::UnassignPaletteFromSelection(1))
        );
        assert_eq!(
            format!("{:?}", palette_toggle_event(&[3, 1], 2)),
            format!("{:?}", ControlEvent::AssignPaletteToSelection(2))
        );

        let mut state = EngineState::default();
        for (id, name) in [(1, "Red"), (3, "Floor")] {
            state.palettes.insert(
                id,
                Palette {
                    name: name.to_string(),
                    kind: PaletteKind::Single(FixtureProperty::Alpha, 0),
                    folder: None,
                },
            );
        }
        assert_eq!(active_palettes_summary(&state, &[]), "Active: none");
        assert_eq!(
            active_palettes_summary(&state, &[3, 1, 9]),
            "Active: Floor, Red, #9"
        );
    }

    fn changes(entries: &[(u8, u8, FixtureProperty)]) -> HashSet<FixtureSelector> {
        entries
            .iter()
            .map(|(gid, fid, property)| ((*gid, *fid), *property).into())
            .collect()
    }

    #[test]
    fn undo_all_groups_the_changes_by_property() {
        let event = undo_all_changes_event(&changes(&[
            (1, 2, FixtureProperty::Alpha),
            (0, 0, FixtureProperty::Alpha),
            (0, 0, FixtureProperty::Pan),
        ]))
        .unwrap();
        let ControlEvent::Transaction(events) = event else {
            panic!("expected a transaction, got {event:?}");
        };
        assert_eq!(events.len(), 2);
        let mut removed = Vec::new();
        for event in events {
            let ControlEvent::RemoveChange {
                selection,
                property,
            } = event
            else {
                panic!("expected RemoveChange, got {event:?}");
            };
            for (gid, fid) in selection.fixtures {
                removed.push((gid, fid, property as u8));
            }
        }
        removed.sort_unstable();
        let mut expected = vec![
            (0, 0, FixtureProperty::Alpha as u8),
            (1, 2, FixtureProperty::Alpha as u8),
            (0, 0, FixtureProperty::Pan as u8),
        ];
        expected.sort_unstable();
        assert_eq!(removed, expected);
    }

    #[test]
    fn undo_all_without_changes_sends_nothing() {
        assert!(undo_all_changes_event(&HashSet::new()).is_none());
    }

    #[test]
    fn undo_all_empties_the_focused_scene_and_keeps_the_others() {
        let state = Arc::new(AppState::new(&[]));
        let bus = SystemEventBus::new();
        let engine = DmxEngine::new(
            state.clone(),
            bus.new_connection(),
            blaulicht_engine::hub::SystemHub::new(),
            Default::default(),
        );

        let focused_changes = changes(&[
            (0, 0, FixtureProperty::Alpha),
            (0, 1, FixtureProperty::ColorHue),
            (1, 0, FixtureProperty::Alpha),
        ]);
        let other_changes = changes(&[
            (0, 0, FixtureProperty::Alpha),
            (1, 0, FixtureProperty::Tilt),
        ]);
        {
            let mut guard = state.dmx_engine.write().unwrap();
            guard.0.new_scene("Focused".to_string());
            guard.0.new_scene("Other".to_string());
            guard.0.current_scene_focus = 0;
            guard.0.scenes.get_mut(&0).unwrap().sink.changeset = focused_changes.clone();
            guard.0.scenes.get_mut(&1).unwrap().sink.changeset = other_changes.clone();
        }

        let event = undo_all_changes_event(&focused_changes).unwrap();
        let mut guard = state.dmx_engine.write().unwrap();
        let (error, _) = engine.apply(
            &mut guard,
            ControlEventMessage::new(EventOriginator::Web, event),
        );

        assert_eq!(error, None);
        assert!(guard.0.scenes[&0].sink.changeset.is_empty());
        assert_eq!(guard.0.scenes[&1].sink.changeset, other_changes);
    }

    #[test]
    fn changes_are_sorted_by_group_fixture_and_property() {
        let sorted = sorted_changes(&changes(&[
            (1, 0, FixtureProperty::Alpha),
            (0, 1, FixtureProperty::Pan),
            (0, 1, FixtureProperty::Alpha),
        ]));
        let keys: Vec<_> = sorted
            .iter()
            .map(|change| (change.gid, change.fid, change.property as u8))
            .collect();
        let mut expected = keys.clone();
        expected.sort_unstable();
        assert_eq!(keys, expected);
        assert_eq!(keys.len(), 3);
    }

    #[test]
    fn escape_clears_only_a_non_empty_selection_outside_text_fields_and_dialogs() {
        assert!(clear_selection_shortcut_allowed(false, false, false));
        assert!(!clear_selection_shortcut_allowed(true, false, false));
        assert!(!clear_selection_shortcut_allowed(false, true, false));
        assert!(!clear_selection_shortcut_allowed(false, false, true));
    }

    #[test]
    fn toolbar_buttons_shrink_to_fit_but_not_below_the_minimum() {
        // Kiosk (800 px window): seven buttons fit next to each other.
        let width = toolbar_button_width(617.0, 8.0, 7);
        assert!(width < 90.0);
        assert!(7.0 * width + 6.0 * 8.0 <= 617.0);
        // Wide panes keep the usual width.
        assert_eq!(toolbar_button_width(2000.0, 8.0, 7), 90.0);
        assert_eq!(toolbar_button_width(100.0, 8.0, 7), 60.0);
    }
}
