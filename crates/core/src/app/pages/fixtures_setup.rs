use crate::app::components::{Dialog, DEFAULT_NEW_GROUP_NAME};
use crate::app::{
    components::{self, ButtonSize, HFader},
    BlaulichtApp,
};
use crate::app::{PopupButtonSpec, PopupSpec};
use crate::dmx::EngineState;
use blaulicht_shared::fixture::dimmer::Dimmer;
use blaulicht_shared::fixture::light::Light;
use blaulicht_shared::fixture::moving_head::MovingHead;
use blaulicht_shared::fixture::state::{Fixture, Position, Rotation};
use blaulicht_shared::fixture::FixtureType;
use blaulicht_shared::{ControlEvent, ControlEventMessage, EventOriginator};
use egui::{Color32, Context, Frame, Key, Label, Margin, RichText};
use std::time::Duration;
use std::{fmt, mem};
use strum::IntoEnumIterator;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AddFixtureKind {
    MovingHead,
    Light,
    Dimmer,
}

impl AddFixtureKind {
    pub const ALL: [Self; 3] = [Self::MovingHead, Self::Light, Self::Dimmer];
}

impl fmt::Display for AddFixtureKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let label = match self {
            AddFixtureKind::MovingHead => "MovingHead",
            AddFixtureKind::Light => "Light",
            AddFixtureKind::Dimmer => "Dimmer",
        };
        f.write_str(label)
    }
}

impl BlaulichtApp {
    fn close_add_fixture_numberpads(&mut self) {
        self.add_fixture_start_addr_numberpad.close();
        self.add_fixture_universe_numberpad.close();
        self.add_fixture_pos_x_numberpad.close();
        self.add_fixture_pos_y_numberpad.close();
        self.add_fixture_rot_x_numberpad.close();
        self.add_fixture_rot_y_numberpad.close();
        self.add_fixture_rot_z_numberpad.close();
        self.add_fixture_count_numberpad.close();
    }

    fn close_edit_fixture_numberpads(&mut self) {
        self.edit_fixture_start_addr_numberpad.close();
        self.edit_fixture_universe_numberpad.close();
        self.edit_fixture_pos_x_numberpad.close();
        self.edit_fixture_pos_y_numberpad.close();
        self.edit_fixture_pos_z_numberpad.close();
        self.edit_fixture_rot_x_numberpad.close();
        self.edit_fixture_rot_y_numberpad.close();
        self.edit_fixture_rot_z_numberpad.close();
    }

    fn close_dmx_override_numberpads(&mut self) {
        self.add_dmx_override_uni_numberpad.close();
        self.add_dmx_override_chan_numberpad.close();
        self.add_dmx_override_value_numberpad.close();
    }

    pub fn render_delete_group(&mut self, ctx: &Context) {
        if self.delete_group_open {
            // TODO: create a confirmation dialog component
            Dialog::new("Confirm Deletion".to_string(), egui::vec2(200.0, 100.0))
                .with_backdrop()
                .show(ctx, |ui| {
                    ui.heading(RichText::new("Confirm Deletion").strong());

                    ui.add_space(12.0);

                    ui.horizontal(|ui| {
                        if components::button(ui, false, "Confirm", ButtonSize::Large) {
                            if let Some(id) = self.add_fixture_group {
                                let mut dmx_engine = self.data.state.dmx_engine.write().unwrap();
                                dmx_engine.delete_group(id);
                                self.delete_group_open = false;
                                self.add_fixture_group = None;
                            }
                        }

                        if components::button(ui, true, "Cancel", ButtonSize::Large) {
                            self.delete_group_open = false;
                        }
                    });
                });
        }
    }

    pub fn render_add_group_dialog(&mut self, ctx: &Context) {
        if self.add_group_open {
            const BUTTON_SIZE: ButtonSize = ButtonSize::Large;
            const SPACING: f32 = 16.0;
            let size = egui::vec2(200.0, BUTTON_SIZE.dim().0.y * 2.0 + SPACING + 8.0);

            Dialog::new("Create Group".to_string(), size)
                .with_backdrop()
                .show(ctx, |ui| {
                    ui.spacing_mut().interact_size = egui::vec2(44.0, 36.0);
                    Frame::new()
                        .inner_margin(Margin::symmetric(10, 6))
                        .show(ui, |ui| {
                            components::TextInput::new(180.0)
                                .with_hint_text("Group Name")
                                .ui(ui, &mut self.new_group_name);
                        });

                    ui.add_space(SPACING);

                    let mut button_pressed = components::button(ui, false, "OK", BUTTON_SIZE);
                    ctx.input(|input| {
                        if input.key_pressed(Key::Enter) {
                            button_pressed = true;
                        }
                    });

                    if button_pressed {
                        let mut dmx_engine = self.data.state.dmx_engine.write().unwrap();
                        dmx_engine.create_group(self.new_group_name.clone());
                        self.new_group_name = DEFAULT_NEW_GROUP_NAME.to_string();
                        self.add_group_open = false;
                    }
                });
        }
    }

    fn render_dmx_override_create_dialog(&mut self, ctx: &Context) {
        if !self.add_dmx_override_open {
            self.close_dmx_override_numberpads();
            return;
        }

        let cell_h = ButtonSize::Medium.dim().0.y;
        let height = cell_h * 3.5 + 24.0;
        const LABEL_W: f32 = 120.0;

        Dialog::new("Add Override".to_string(), egui::vec2(260.0, height))
            .with_backdrop()
            .show(ctx, |ui| {
                ui.spacing_mut().interact_size = egui::vec2(44.0, 36.0);

                ui.horizontal_centered(|ui| {
                    ui.set_height(cell_h);

                    self.add_dmx_override_uni_numberpad
                        .ui(ui, &mut self.add_dmx_override_uni);
                    ui.add_sized([LABEL_W, cell_h], Label::new("DMX Uni:"));
                });

                ui.separator();

                ui.horizontal_centered(|ui| {
                    ui.set_height(cell_h);

                    self.add_dmx_override_chan_numberpad
                        .ui(ui, &mut self.add_dmx_override_chan);
                    ui.add_sized([LABEL_W, cell_h], Label::new("DMX Channel:"));
                });

                ui.separator();

                ui.horizontal_centered(|ui| {
                    self.add_dmx_override_value_numberpad
                        .ui(ui, &mut self.add_dmx_override_value);
                    ui.add_sized([LABEL_W, cell_h], Label::new("Value:"));
                });

                ui.separator();

                ui.horizontal(|ui| {
                    ui.set_height(cell_h);

                    if components::button(ui, false, "Cancel", ButtonSize::Medium) {
                        self.add_dmx_override_open = false;
                        self.close_dmx_override_numberpads();
                    }

                    if components::button(ui, true, "Add", ButtonSize::Medium) {
                        self.data
                            .event_bus_connection
                            .send(ControlEventMessage::new(
                                EventOriginator::Web,
                                ControlEvent::SetChannelOverride(
                                    self.add_dmx_override_uni,
                                    self.add_dmx_override_chan,
                                    self.add_dmx_override_value,
                                ),
                            ));
                        self.add_dmx_override_open = false;
                        self.close_dmx_override_numberpads();
                    }
                });
            });
    }

    fn render_dmx_override_dialog(&mut self, ctx: &Context, dmx_engine: &EngineState) {
        if !self.dmx_override_dialog_open {
            self.close_dmx_override_numberpads();
            return;
        }

        let r = ctx.screen_rect();

        Dialog::new(
            "Overrides".to_string(),
            egui::vec2(r.width() / 2.0, r.height()),
        )
        .with_backdrop()
        .fixed_pos(egui::vec2(0.0, 0.0))
        .show(ctx, |ui| {
            self.render_dmx_override_create_dialog(ctx);

            let frame = Frame::NONE;

            frame.show(ui, |ui| {
                ui.set_height(30.0);
                ui.set_width(ui.available_width());

                ui.horizontal_centered(|ui| {
                    ui.label("DMX Overrides");

                    if components::button(ui, false, "Add", ButtonSize::Medium) {
                        self.add_dmx_override_open = true;
                        self.close_dmx_override_numberpads();
                    }

                    if components::button(ui, false, "Close", ButtonSize::Medium) {
                        self.add_dmx_override_open = false;
                        self.dmx_override_dialog_open = false;
                        self.close_dmx_override_numberpads();
                    }
                });
            });

            ui.separator();

            for ((universe, chan), value) in &dmx_engine.0.overrides {
                ui.allocate_ui_with_layout(
                    egui::vec2(ui.available_width(), 30.0),
                    egui::Layout::left_to_right(egui::Align::Min),
                    |ui| {
                        ui.add_sized(
                            [60.0, 16.0],
                            Label::new(
                                RichText::new(format!("UN: {universe} | CH: {chan:0>3}"))
                                    .color(Color32::LIGHT_RED)
                                    .size(16.0),
                            ),
                        );

                        ui.add_space(20.0);

                        {
                            let mut value = *value as f32;
                            if ui
                                .add(HFader::new(&mut value, 0.0..=255.0).with_label("Value"))
                                .changed()
                            {
                                self.data
                                    .event_bus_connection
                                    .send(ControlEventMessage::new(
                                        EventOriginator::Web,
                                        ControlEvent::SetChannelOverride(
                                            *universe as u16,
                                            *chan as u16,
                                            value as u8,
                                        ),
                                    ));
                            }
                        };

                        ui.add_space(50.0);

                        if components::button(ui, false, "Delete", ButtonSize::Medium) {
                            self.data
                                .event_bus_connection
                                .send(ControlEventMessage::new(
                                    EventOriginator::Web,
                                    ControlEvent::RemoveChannelOverride(
                                        *universe as u16,
                                        *chan as u16,
                                    ),
                                ));
                        }
                    },
                );

                ui.separator();
            }
        });
    }

    fn render_add_fixture_dialog(&mut self, ctx: &Context, dmx_engine: &EngineState) -> bool {
        let mut skip_rest = false;

        if !self.add_fixture_open {
            return skip_rest;
        }

        const BUTTON_SIZE: ButtonSize = ButtonSize::Medium;
        let cell_h = BUTTON_SIZE.dim().0.y;
        let width = 570.0;
        let height = 400.0;
        const LABEL_W: f32 = 120.0;

        Dialog::new("Add Fixture".to_string(), egui::vec2(width, height))
            .with_backdrop()
            .show(ctx, |ui| {
                ui.spacing_mut().interact_size = egui::vec2(44.0, 36.0);
                // Group selector
                ui.label(format!("Group #{}", self.add_fixture_group.unwrap_or(0)));

                ui.separator();

                // Name
                ui.horizontal(|ui| {
                    ui.add_sized([LABEL_W, cell_h], Label::new("Name:"));

                    components::TextInput::new(415.0)
                        .with_hint_text("Fixture Name")
                        .ui(ui, &mut self.add_fixture_name);
                });

                // Start address
                ui.horizontal(|ui| {
                    ui.add_sized([LABEL_W, cell_h], Label::new("Start Addr:"));
                    self.add_fixture_start_addr_numberpad
                        .ui(ui, &mut self.add_fixture_start_addr);

                    ui.add_sized([LABEL_W, cell_h], Label::new("Universe:"));
                    self.add_fixture_universe_numberpad
                        .ui(ui, &mut self.add_fixture_universe_no);
                });

                // Universe
                // ui.horizontal(|ui| {
                // });

                // Position
                ui.horizontal(|ui| {
                    ui.add_sized([LABEL_W, cell_h], Label::new("Pos X:"));
                    self.add_fixture_pos_x_numberpad
                        .ui(ui, &mut self.add_fixture_pos_x);

                    ui.add_sized([LABEL_W, cell_h], Label::new("Pos Y:"));
                    self.add_fixture_pos_y_numberpad
                        .ui(ui, &mut self.add_fixture_pos_y);
                });

                // Rotation
                ui.horizontal(|ui| {
                    ui.add_sized([LABEL_W, cell_h], Label::new("Rot X:"));
                    self.add_fixture_rot_x_numberpad
                        .ui(ui, &mut self.add_fixture_rot_x);

                    ui.add_sized([LABEL_W, cell_h], Label::new("Rot Y:"));
                    self.add_fixture_rot_y_numberpad
                        .ui(ui, &mut self.add_fixture_rot_y);
                });

                ui.horizontal(|ui| {
                    ui.add_sized([LABEL_W, cell_h], Label::new("Rot Z:"));
                    self.add_fixture_rot_z_numberpad
                        .ui(ui, &mut self.add_fixture_rot_z);
                });

                ui.separator();

                // Kind selector
                ui.horizontal(|ui| {
                    ui.add_sized([LABEL_W, cell_h], Label::new("Type:"));
                    let kind_button_size = BUTTON_SIZE.with_width(140.0);
                    if components::button(
                        ui,
                        self.add_fixture_kind_dialog_open,
                        &self.add_fixture_kind.to_string(),
                        kind_button_size,
                    ) {
                        self.add_fixture_kind_dialog_open = true;
                    }

                    let (new_kind, kind_changed) = components::selection_dialog(
                        ctx,
                        AddFixtureKind::ALL,
                        self.add_fixture_kind,
                        &mut self.add_fixture_kind_dialog_open,
                        "Select Fixture Type".to_string(),
                    );

                    if kind_changed {
                        self.add_fixture_kind = new_kind;
                        self.add_fixture_model_dialog_open = false;
                    }

                    ui.add_sized([LABEL_W, cell_h], Label::new("Model:"));
                    let model_button_size = BUTTON_SIZE.with_width(140.0).with_font_size(8.5);
                    match self.add_fixture_kind {
                        AddFixtureKind::MovingHead => {
                            let label = self.add_fixture_selected_moving_head.to_string();
                            if components::button(
                                ui,
                                self.add_fixture_model_dialog_open,
                                &label,
                                model_button_size,
                            ) {
                                self.add_fixture_model_dialog_open = true;
                            }

                            let options: Vec<_> = MovingHead::iter().collect();
                            let (new_model, changed) = components::selection_dialog(
                                ctx,
                                options,
                                self.add_fixture_selected_moving_head,
                                &mut self.add_fixture_model_dialog_open,
                                "Select Moving Head".to_string(),
                            );

                            if changed {
                                self.add_fixture_selected_moving_head = new_model;
                            }
                        }
                        AddFixtureKind::Light => {
                            let label = self.add_fixture_selected_light.to_string();
                            if components::button(
                                ui,
                                self.add_fixture_model_dialog_open,
                                &label,
                                model_button_size,
                            ) {
                                self.add_fixture_model_dialog_open = true;
                            }

                            let options: Vec<_> = Light::iter().collect();
                            let (new_model, changed) = components::selection_dialog(
                                ctx,
                                options,
                                self.add_fixture_selected_light,
                                &mut self.add_fixture_model_dialog_open,
                                "Select Light".to_string(),
                            );

                            if changed {
                                self.add_fixture_selected_light = new_model;
                            }
                        }
                        AddFixtureKind::Dimmer => {
                            let label = self.add_fixture_selected_dimmer.to_string();
                            if components::button(
                                ui,
                                self.add_fixture_model_dialog_open,
                                &label,
                                model_button_size,
                            ) {
                                self.add_fixture_model_dialog_open = true;
                            }

                            let options: Vec<_> = Dimmer::iter().collect();
                            let (new_model, changed) = components::selection_dialog(
                                ctx,
                                options,
                                self.add_fixture_selected_dimmer,
                                &mut self.add_fixture_model_dialog_open,
                                "Select Dimmer".to_string(),
                            );

                            if changed {
                                self.add_fixture_selected_dimmer = new_model;
                            }
                        }
                    }
                });

                // Model selector depending on kind
                // ui.horizontal(|ui| {});

                ui.separator();

                // Count + actions
                ui.horizontal(|ui| {
                    ui.vertical(|ui| {
                        ui.horizontal(|ui| {
                            ui.add_sized([LABEL_W, cell_h], Label::new("Count:"));
                            self.add_fixture_count_numberpad
                                .ui(ui, &mut self.add_fixture_count);
                        });
                    });

                    ui.add_space(16.0);

                    ui.vertical(|ui| {
                        if components::button(ui, false, "Cancel", ButtonSize::Medium) {
                            self.add_fixture_open = false;
                            self.add_fixture_kind_dialog_open = false;
                            self.add_fixture_model_dialog_open = false;
                            self.close_add_fixture_numberpads();
                        }

                        let can_create = self.add_fixture_group.is_some()
                            && dmx_engine
                                .groups()
                                .get(&self.add_fixture_group.unwrap())
                                .is_some()
                            && self.add_fixture_start_addr >= 1
                            && self.add_fixture_start_addr <= 512;

                        let mut button_pressed =
                            components::button(ui, can_create, "Create", ButtonSize::Medium);
                        ctx.input(|input| {
                            if input.key_pressed(Key::Enter) {
                                button_pressed = true;
                            }
                        });

                        if !can_create {
                            button_pressed = false;
                        }

                        if button_pressed && self.add_fixture_group.is_some() {
                            let group_id = self.add_fixture_group.unwrap();
                            let base_name = std::mem::take(&mut self.add_fixture_name);
                            let mut start_addr = self.add_fixture_start_addr as usize;
                            let universe_no = self.add_fixture_universe_no as usize;
                            let pos = Position {
                                x: self.add_fixture_pos_x,
                                y: self.add_fixture_pos_y,
                                z: 0,
                            };
                            let rotation = Rotation {
                                x: self.add_fixture_rot_x,
                                y: self.add_fixture_rot_y,
                                z: self.add_fixture_rot_z,
                            };

                            // Build fixture type
                            let fixture_type = match self.add_fixture_kind {
                                AddFixtureKind::MovingHead => {
                                    FixtureType::from(self.add_fixture_selected_moving_head)
                                }
                                AddFixtureKind::Light => {
                                    FixtureType::from(self.add_fixture_selected_light)
                                }
                                AddFixtureKind::Dimmer => {
                                    FixtureType::from(self.add_fixture_selected_dimmer)
                                }
                            };

                            // Determine channel footprint for address stepping
                            let footprint = fixture_type.footprint();

                            // Create multiple fixtures if requested
                            {
                                let mut dmx_engine = self.data.state.dmx_engine.write().unwrap();
                                let count = self.add_fixture_count.max(1) as usize;
                                for i in 0..count {
                                    if start_addr + fixture_type.footprint() > 513 {
                                        mem::drop(dmx_engine);
                                        self.show_popup(PopupSpec {
                                            label: "Out of Channels".to_string(),
                                            label_size: Some(18.0),
                                            lifetime_duration: Duration::from_secs(3),
                                            button: Some(PopupButtonSpec {
                                                label: "OK".to_string(),
                                            }),
                                        });
                                        break;
                                    }

                                    let name = if count > 1 {
                                        format!("{} #{}", base_name, i + 1)
                                    } else {
                                        base_name.clone()
                                    };
                                    let mut fixture = Fixture::new(
                                        universe_no,
                                        start_addr,
                                        name,
                                        fixture_type.clone(),
                                    );
                                    fixture.pos = pos.clone();
                                    fixture.rotation = rotation.clone();
                                    dmx_engine.add_fixture_to_group(group_id, fixture);
                                    start_addr = start_addr.saturating_add(footprint);
                                }
                                skip_rest = true;
                            }

                            // Reset some fields and close
                            self.add_fixture_open = false;
                            self.add_fixture_kind_dialog_open = false;
                            self.add_fixture_model_dialog_open = false;
                            self.close_add_fixture_numberpads();

                            self.add_fixture_name = String::from("New Fixture");
                        }
                    });
                });
            });

        skip_rest
    }

    pub fn fixtures_ui_setup(&mut self, ui: &mut egui::Ui, ctx: &Context) {
        // Make controls touch-friendly within this page
        ui.spacing_mut().interact_size = egui::vec2(44.0, 36.0);
        let dmx_engine = { self.data.state.dmx_engine.read().unwrap().clone() };
        let groups = dmx_engine.groups();

        //
        // Dialogs start.
        //
        self.render_dmx_override_dialog(ctx, &dmx_engine);
        self.render_dmx_simulation_dialog(ctx, groups);
        self.render_add_scene_dialog(ctx);
        self.render_clone_scene_dialog(ctx);
        self.render_rename_scene_dialog(ctx);
        self.render_delete_scene_dialog(ctx);
        self.render_scene_changeset_dialog(ctx, &dmx_engine);
        self.render_add_group_dialog(ctx);
        self.render_delete_group(ctx);

        // Skipping required because UI breaks at some point.
        let skip_rest = self.render_add_fixture_dialog(ctx, &dmx_engine);
        if skip_rest {
            return;
        }

        ui.allocate_ui_with_layout(
            egui::vec2(ui.available_width(), ui.available_height()), // fixed width, max height
            egui::Layout::left_to_right(egui::Align::Min),
            |ui| {
                self.scene_overview(ui, &dmx_engine);

                ui.separator();

                ui.allocate_ui_with_layout(
                    egui::vec2(ui.available_width(), ui.available_height()), // fixed width, max height
                    egui::Layout::top_down(egui::Align::Min),
                    |ui| {
                        ui.horizontal(|ui| {
                            if components::button(ui, false, "Scene +", ButtonSize::Medium) {
                                self.new_scene_dialog_open = true;
                            }

                            if components::button(ui, false, "Clone Sc.", ButtonSize::Medium) {
                                self.clone_scene_dialog_open = true;
                            }

                            if components::button(ui, false, "Ren. Sc.", ButtonSize::Medium) {
                                if !dmx_engine.0.scenes.is_empty() {
                                    self.rename_scene_name = dmx_engine.curr_scene().name.clone();
                                } else {
                                    self.rename_scene_name.clear();
                                }
                                self.rename_scene_dialog_open = true;
                            }

                            if components::button(ui, false, "Del. Sc.", ButtonSize::Medium) {
                                self.delete_scene_dialog_open = true;
                            }

                            if components::button(
                                ui,
                                self.current_scene_changeset_dialog_open,
                                "Scene Changes",
                                ButtonSize::Medium,
                            ) {
                                self.current_scene_changeset_dialog_open =
                                    !self.current_scene_changeset_dialog_open;
                            }

                            if components::button(
                                ui,
                                self.current_scene_animations_dialog_open,
                                "Scene Animations",
                                ButtonSize::Medium,
                            ) {
                                self.current_scene_animations_dialog_open =
                                    !self.current_scene_animations_dialog_open;
                            }
                        });

                        ui.horizontal(|ui| {
                            if components::button(
                                ui,
                                self.delete_group_open,
                                "Delete Group",
                                ButtonSize::Medium,
                            ) {
                                if let Some(id) = self.add_fixture_group {
                                    if dmx_engine.groups().get(&id).is_some() {
                                        self.delete_group_open = !self.delete_group_open;
                                    }
                                }
                                // self.add_group_open = !self.add_group_open;
                            }

                            if components::button(
                                ui,
                                self.add_group_open,
                                "Add Group",
                                ButtonSize::Medium,
                            ) {
                                // dmx_engine.create_groep(name)
                                self.add_group_open = !self.add_group_open;
                            }

                            if components::button(
                                ui,
                                self.add_fixture_open,
                                "Add Fixture",
                                ButtonSize::Medium,
                            ) {
                                if dmx_engine.groups().is_empty()
                                    || self.add_fixture_group.is_none()
                                {
                                    self.show_popup(PopupSpec::with_duration(
                                        Duration::from_secs(3),
                                        "No Group Selected".to_string(),
                                    ));
                                } else {
                                    self.add_fixture_open = !self.add_fixture_open;
                                    if !self.add_fixture_open {
                                        self.close_add_fixture_numberpads();
                                    }
                                }
                            }

                            ui.separator();

                            if components::button(
                                ui,
                                self.dmx_override_dialog_open,
                                "Show Overrides",
                                ButtonSize::Medium,
                            ) {
                                self.dmx_override_dialog_open = !self.dmx_override_dialog_open;
                                if !self.dmx_override_dialog_open {
                                    self.add_dmx_override_open = false;
                                }
                                self.close_dmx_override_numberpads();
                            }

                            ui.separator();

                            for (universe, simulator) in
                                self.universe_simulations.iter_mut().enumerate()
                            {
                                if components::button(
                                    ui,
                                    simulator.open,
                                    &format!("Sim. DMX {universe}"),
                                    ButtonSize::Medium,
                                ) {
                                    simulator.open = !simulator.open;
                                }
                            }
                        });

                        ui.separator();

                        ui.allocate_ui_with_layout(
                            egui::vec2(ui.available_width(), ui.available_height()), // fixed width, max height
                            egui::Layout::left_to_right(egui::Align::Min),
                            |ui| {
                                const LABEL_W: f32 = 120.0;
                                let (new_g, new_f, changed) = self.group_selection(
                                    dmx_engine.groups(),
                                    self.add_fixture_group,
                                    self.setup_fixture_id,
                                    ui,
                                );

                                // let mut new_name = fix.name.to_string();
                                // let mut new_addr = fix.start_addr as u16;

                                if changed {
                                    self.close_edit_fixture_numberpads();
                                    self.add_fixture_group = Some(new_g);
                                    self.setup_fixture_id = new_f;

                                    let group = dmx_engine.0.groups.get(&new_g).unwrap();
                                    if let Some(fix) = group.fixtures.get(&new_f) {
                                        self.new_fixture_name = fix.name.to_string();
                                        self.new_fixture_addr = fix.start_addr;
                                        self.new_fixture_uni = fix.universe_no;
                                        self.new_fixture_pos_x = fix.pos.x;
                                        self.new_fixture_pos_y = fix.pos.y;
                                        self.new_fixture_pos_z = fix.pos.z;
                                        self.new_fixture_rot_x = fix.rotation.x;
                                        self.new_fixture_rot_y = fix.rotation.y;
                                        self.new_fixture_rot_z = fix.rotation.z;
                                    }
                                }

                                ui.separator();

                                ui.vertical(|ui| {
                                    let Some(gid) = self.add_fixture_group else {
                                        return;
                                    };
                                    let fid = self.setup_fixture_id;

                                    // let dmx_engine =
                                    //     { self.data.state.dmx_engine.write().unwrap().clone() };

                                    // Read current fixture snapshot
                                    // let engine_read = self.data.state.dmx_engine.read().unwrap();
                                    if let Some(group) = dmx_engine.groups().get(&gid) {
                                        if let Some(_fix) = group.fixtures.get(&fid) {
                                            ui.label(
                                                RichText::new(format!(
                                                    "Selected Fixture: Group #{} • Fixture #{}",
                                                    gid, fid
                                                ))
                                                .size(12.0),
                                            );

                                            ui.add_space(6.0);

                                            // Editable fields (local copies, apply on save)

                                            ui.horizontal(|ui| {
                                                ui.add_sized(
                                                    [LABEL_W, ButtonSize::Medium.dim().0.y],
                                                    Label::new("Name:"),
                                                );

                                                components::TextInput::new(140.0)
                                                    .with_hint_text("New Name")
                                                    .ui(ui, &mut self.new_fixture_name);

                                                // ui.add_sized(
                                                //     [140.0, ButtonSize::Medium.dim().0.y],
                                                //     TextEdit::singleline(
                                                //     )
                                                //     .font(FontId::proportional(
                                                //         ButtonSize::Medium.dim().1,
                                                //     )),
                                                // );
                                            });

                                            ui.horizontal(|ui| {
                                                ui.add_sized(
                                                    [LABEL_W, ButtonSize::Medium.dim().0.y],
                                                    Label::new("Start Addr:"),
                                                );
                                                self.edit_fixture_start_addr_numberpad
                                                    .ui(ui, &mut self.new_fixture_addr);
                                            });

                                            ui.horizontal(|ui| {
                                                ui.add_sized(
                                                    [LABEL_W, ButtonSize::Medium.dim().0.y],
                                                    Label::new("Universe:"),
                                                );
                                                self.edit_fixture_universe_numberpad
                                                    .ui(ui, &mut self.new_fixture_uni);
                                            });

                                            ui.horizontal(|ui| {
                                                ui.vertical(|ui| {
                                                    ui.horizontal(|ui| {
                                                        ui.add_sized(
                                                            [LABEL_W, ButtonSize::Medium.dim().0.y],
                                                            Label::new("Position X:"),
                                                        );
                                                        self.edit_fixture_pos_x_numberpad
                                                            .ui(ui, &mut self.new_fixture_pos_x);
                                                    });

                                                    ui.horizontal(|ui| {
                                                        ui.add_sized(
                                                            [LABEL_W, ButtonSize::Medium.dim().0.y],
                                                            Label::new("Position Y:"),
                                                        );
                                                        self.edit_fixture_pos_y_numberpad
                                                            .ui(ui, &mut self.new_fixture_pos_y);
                                                    });

                                                    ui.horizontal(|ui| {
                                                        ui.add_sized(
                                                            [LABEL_W, ButtonSize::Medium.dim().0.y],
                                                            Label::new("Position Z:"),
                                                        );
                                                        self.edit_fixture_pos_z_numberpad
                                                            .ui(ui, &mut self.new_fixture_pos_z);
                                                    });

                                                    ui.horizontal(|ui| {
                                                        ui.add_sized(
                                                            [LABEL_W, ButtonSize::Medium.dim().0.y],
                                                            Label::new("Rotation X:"),
                                                        );
                                                        self.edit_fixture_rot_x_numberpad
                                                            .ui(ui, &mut self.new_fixture_rot_x);
                                                    });

                                                    ui.horizontal(|ui| {
                                                        ui.add_sized(
                                                            [LABEL_W, ButtonSize::Medium.dim().0.y],
                                                            Label::new("Rotation Y:"),
                                                        );
                                                        self.edit_fixture_rot_y_numberpad
                                                            .ui(ui, &mut self.new_fixture_rot_y);
                                                    });

                                                    ui.horizontal(|ui| {
                                                        ui.add_sized(
                                                            [LABEL_W, ButtonSize::Medium.dim().0.y],
                                                            Label::new("Rotation Z:"),
                                                        );
                                                        self.edit_fixture_rot_z_numberpad
                                                            .ui(ui, &mut self.new_fixture_rot_z);
                                                    });
                                                });

                                                ui.add_space(16.0);

                                                ui.vertical(|ui| {
                                                    let can_save =
                                                        (1..=512).contains(&self.new_fixture_addr);
                                                    if components::button(
                                                        ui,
                                                        can_save,
                                                        "Save",
                                                        ButtonSize::Medium,
                                                    ) {
                                                        // let mut eng =
                                                        //     self.data.state.dmx_engine.write().unwrap();
                                                        let mut dmx_engine = self
                                                            .data
                                                            .state
                                                            .dmx_engine
                                                            .write()
                                                            .unwrap();
                                                        if let Some(group_mut) =
                                                            dmx_engine.0.groups.get_mut(&gid)
                                                        {
                                                            if let Some(fix_mut) =
                                                                group_mut.fixtures.get_mut(&fid)
                                                            {
                                                                fix_mut.name = self
                                                                    .new_fixture_name
                                                                    .clone()
                                                                    .into();
                                                                fix_mut.start_addr =
                                                                    self.new_fixture_addr;
                                                                fix_mut.universe_no =
                                                                    self.new_fixture_uni;
                                                                fix_mut.pos.x =
                                                                    self.new_fixture_pos_x;
                                                                fix_mut.pos.y =
                                                                    self.new_fixture_pos_y;
                                                                fix_mut.pos.z =
                                                                    self.new_fixture_pos_z;
                                                                fix_mut.rotation.x =
                                                                    self.new_fixture_rot_x;
                                                                fix_mut.rotation.y =
                                                                    self.new_fixture_rot_y;
                                                                fix_mut.rotation.z =
                                                                    self.new_fixture_rot_z;
                                                            }
                                                        }
                                                    }

                                                    ui.add_space(12.0);

                                                    if components::button(
                                                        ui,
                                                        true,
                                                        "Delete",
                                                        ButtonSize::Medium,
                                                    ) {
                                                        // let mut eng =
                                                        //     self.data.state.dmx_engine.write().unwrap();
                                                        //
                                                        let mut dmx_engine = self
                                                            .data
                                                            .state
                                                            .dmx_engine
                                                            .write()
                                                            .unwrap();

                                                        if let Some(group_mut) =
                                                            dmx_engine.0.groups.get_mut(&gid)
                                                        {
                                                            let removed =
                                                                group_mut.fixtures.remove(&fid);
                                                            if removed.is_some() {
                                                                // Adjust selection to first available fixture in the group, if any
                                                                if let Some((&first_fid, _)) =
                                                                    group_mut.fixtures.iter().next()
                                                                {
                                                                    self.setup_fixture_id =
                                                                        first_fid;
                                                                }

                                                                mem::drop(dmx_engine);

                                                                self.show_popup(
                                                                    PopupSpec::with_duration(
                                                                        Duration::from_millis(750),
                                                                        format!(
                                                            "Deleted fixture #{} from group #{}",
                                                            fid, gid
                                                        ),
                                                                    ),
                                                                );
                                                            }
                                                        }
                                                    }
                                                });
                                            });
                                        } else {
                                            ui.label(
                                                RichText::new("No fixture selected in this group")
                                                    .color(Color32::GRAY),
                                            );
                                        }
                                    } else {
                                        ui.label(
                                            RichText::new("No group selected").color(Color32::GRAY),
                                        );
                                    }
                                });
                            },
                        );

                        // Selected fixture details

                        // mem::drop(dmx_engine);

                        {}
                    },
                );
            },
        );
    }
}
