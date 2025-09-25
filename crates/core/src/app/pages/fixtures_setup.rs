use crate::dmx::{Fixture, FixtureType, Light, MovingHead, Position};
use crate::{
    app::{
        components::{self, button, ButtonSize, HFader},
        BlaulichtApp,
    },
    dmx::animation::state,
};
use blaulicht_shared::{ControlEvent, ControlEventMessage, EventOriginator};
use egui::{Color32, Context, FontFamily, FontId, Frame, Label, RichText, TextStyle, Vec2};

impl BlaulichtApp {
    pub fn fixtures_ui_setup(&mut self, ui: &mut egui::Ui, ctx: &Context) {
        let dmx_engine = { self.data.state.dmx_engine.read().unwrap().clone() };
        let groups = dmx_engine.groups();

        //
        // Dialogs start.
        //
        self.render_dmx_simulation_dialog(ctx, groups);
        self.render_add_scene_dialog(ctx);
        self.render_scene_changeset_dialog(ctx, &dmx_engine);
        self.render_scene_animations_dialog(ctx, &dmx_engine);

        if self.add_dmx_override_open {
            let cell_h = ButtonSize::Large.dim().0.y;
            let height = cell_h * 3.0 + 15.0;

            components::dialog(
                ctx,
                "Add Override",
                egui::vec2(200.0, height),
                false,
                |ui| {
                    ui.horizontal_centered(|ui| {
                        ui.set_height(cell_h);

                        ui.add_sized(
                            [100.0, cell_h],
                            egui::widgets::DragValue::new(&mut self.add_dmx_override_chan)
                                .speed(1)
                                .range(1..=512),
                        );

                        ui.label("DMX Channel:");
                    });

                    ui.separator();

                    ui.horizontal_centered(|ui| {
                        ui.add_sized(
                            [100.0, cell_h],
                            egui::widgets::DragValue::new(&mut self.add_dmx_override_value)
                                .speed(1)
                                .range(0..=255),
                        );
                        ui.label("Value");
                    });

                    ui.separator();

                    ui.horizontal(|ui| {
                        ui.set_height(cell_h);

                        if components::button(ui, false, "Cancel", ButtonSize::Large) {
                            self.add_dmx_override_open = false;
                        }

                        if components::button(ui, true, "Add", ButtonSize::Large) {
                            self.data
                                .event_bus_connection
                                .send(ControlEventMessage::new(
                                    EventOriginator::Web,
                                    ControlEvent::SetChannelOverride(
                                        self.add_dmx_override_chan,
                                        self.add_dmx_override_value,
                                    ),
                                ));
                            self.add_dmx_override_open = false;
                        }
                    });
                },
            );
        }

        if self.dmx_override_dialog_open {
            let r = ctx.screen_rect();

            components::dialog_fixed_pos(
                ctx,
                "Overrides",
                egui::vec2(r.width() / 2.0, r.height()),
                egui::vec2(0.0, 0.0),
                false,
                |ui| {
                    let frame = Frame::NONE;

                    frame.show(ui, |ui| {
                        ui.set_height(30.0);
                        ui.set_width(ui.available_width());

                        ui.horizontal_centered(|ui| {
                            ui.label("DMX Overrides");

                            if components::button(ui, false, "Add", ButtonSize::Medium) {
                                self.add_dmx_override_open = true;
                            }
                        });
                    });

                    ui.allocate_ui_with_layout(
                        egui::vec2(ui.available_width(), 20.0),
                        egui::Layout::left_to_right(egui::Align::Min),
                        |ui| {},
                    );

                    ui.separator();

                    for (chan, value) in &dmx_engine.overrides {
                        ui.allocate_ui_with_layout(
                            egui::vec2(ui.available_width(), 30.0),
                            egui::Layout::left_to_right(egui::Align::Min),
                            |ui| {
                                ui.add_sized(
                                    [60.0, 16.0],
                                    Label::new(
                                        RichText::new(format!("CH: {chan}"))
                                            .color(Color32::LIGHT_RED)
                                            .size(16.0),
                                    ),
                                );

                                ui.add_space(20.0);

                                {
                                    let mut value = *value as f32;
                                    if ui
                                        .add(
                                            HFader::new(&mut value, 0.0..=255.0)
                                                .with_label("Value"),
                                        )
                                        .changed()
                                    {
                                        self.data.event_bus_connection.send(
                                            ControlEventMessage::new(
                                                EventOriginator::Web,
                                                ControlEvent::SetChannelOverride(
                                                    *chan as u16,
                                                    value as u8,
                                                ),
                                            ),
                                        );
                                    }
                                };

                                ui.add_space(50.0);

                                if components::button(ui, false, "Delete", ButtonSize::Medium) {
                                    self.data
                                        .event_bus_connection
                                        .send(ControlEventMessage::new(
                                            EventOriginator::Web,
                                            ControlEvent::RemoveChannelOverride(*chan as u16),
                                        ));
                                }
                            },
                        );

                        ui.separator();
                    }
                },
            )
        }

        // Add Fixture Dialog
        if self.add_fixture_open {
            let cell_h = ButtonSize::Large.dim().0.y;
            let width = 420.0;
            let height = cell_h * 7.0 + 40.0;

            components::dialog(ctx, "Add Fixture", egui::vec2(width, height), false, |ui| {
                // Preselect group on first open if not set
                if self.add_fixture_group.is_none() {
                    self.add_fixture_group = self.selected_fixture_group.or_else(|| {
                        // pick group 0 if exists
                        groups.keys().next().copied()
                    });
                }

                // Group selector
                ui.horizontal(|ui| {
                    ui.label("Group:");
                    let mut selected_gid = self.add_fixture_group.unwrap_or(0);
                    egui::ComboBox::from_id_source("add_fixture_group_combo")
                        .selected_text(format!("#{}", selected_gid))
                        .show_ui(ui, |ui| {
                            for gid in groups.keys() {
                                ui.selectable_value(&mut selected_gid, *gid, format!("#{}", gid));
                            }
                        });
                    self.add_fixture_group = Some(selected_gid);
                });

                ui.separator();

                // Name
                ui.horizontal(|ui| {
                    ui.label("Name:");
                    ui.text_edit_singleline(&mut self.add_fixture_name);
                });

                // Start address
                ui.horizontal(|ui| {
                    ui.label("Start Addr:");
                    ui.add(
                        egui::widgets::DragValue::new(&mut self.add_fixture_start_addr)
                            .speed(1)
                            .range(1..=512),
                    );
                });

                // Position
                ui.horizontal(|ui| {
                    ui.label("Pos X:");
                    ui.add(egui::widgets::DragValue::new(&mut self.add_fixture_pos_x).speed(1));
                    ui.add_space(10.0);
                    ui.label("Pos Y:");
                    ui.add(egui::widgets::DragValue::new(&mut self.add_fixture_pos_y).speed(1));
                });

                ui.separator();

                // Kind selector
                ui.horizontal(|ui| {
                    ui.label("Type:");
                    let kinds = ["MovingHead", "Light", "Dimmer"];
                    egui::ComboBox::from_id_source("add_fixture_kind_combo")
                        .selected_text(kinds[self.add_fixture_kind])
                        .show_ui(ui, |ui| {
                            for (idx, label) in kinds.iter().enumerate() {
                                ui.selectable_value(&mut self.add_fixture_kind, idx, *label);
                            }
                        });
                });

                // Model selector depending on kind
                ui.horizontal(|ui| {
                    ui.label("Model:");
                    match self.add_fixture_kind {
                        0 => {
                            // MovingHead
                            let models = [MovingHead::MartinMacAura];
                            let labels = [models[0].to_string()];
                            let mut idx = self.add_fixture_model_index.min(labels.len() - 1);
                            egui::ComboBox::from_id_source("add_fixture_model_combo")
                                .selected_text(&labels[idx])
                                .show_ui(ui, |ui| {
                                    for (i, l) in labels.iter().enumerate() {
                                        ui.selectable_value(&mut idx, i, l);
                                    }
                                });
                            self.add_fixture_model_index = idx;
                        }
                        1 => {
                            // Light
                            let labels = [
                                Light::Generic3ChanNoAlpha.to_string(),
                                Light::Generic4ChanWithAlpha.to_string(),
                                Light::LEDPartyTCLSpot.to_string(),
                            ];
                            let mut idx = self.add_fixture_model_index.min(labels.len() - 1);
                            egui::ComboBox::from_id_source("add_fixture_model_combo")
                                .selected_text(&labels[idx])
                                .show_ui(ui, |ui| {
                                    for (i, l) in labels.iter().enumerate() {
                                        ui.selectable_value(&mut idx, i, l);
                                    }
                                });
                            self.add_fixture_model_index = idx;
                        }
                        _ => {
                            ui.label(RichText::new("No models").color(Color32::GRAY));
                            self.add_fixture_model_index = 0;
                        }
                    }
                });

                ui.separator();

                ui.horizontal(|ui| {
                    if components::button(ui, false, "Cancel", ButtonSize::Large) {
                        self.add_fixture_open = false;
                    }

                    let can_create = self.add_fixture_group.is_some()
                        && self.add_fixture_start_addr >= 1
                        && self.add_fixture_start_addr <= 512
                        && match self.add_fixture_kind {
                            2 => false,
                            _ => true,
                        };

                    if components::button(ui, can_create, "Create", ButtonSize::Large) && can_create
                    {
                        let group_id = self.add_fixture_group.unwrap();
                        let name = std::mem::take(&mut self.add_fixture_name);
                        let start_addr = self.add_fixture_start_addr as usize;
                        let pos = Position {
                            x: self.add_fixture_pos_x,
                            y: self.add_fixture_pos_y,
                        };

                        // Build fixture type
                        let fixture_type = match self.add_fixture_kind {
                            0 => {
                                // MovingHead
                                let model = match self.add_fixture_model_index {
                                    _ => MovingHead::MartinMacAura,
                                };
                                FixtureType::from(model)
                            }
                            1 => {
                                // Light
                                let model = match self.add_fixture_model_index {
                                    0 => Light::Generic3ChanNoAlpha,
                                    1 => Light::Generic4ChanWithAlpha,
                                    _ => Light::LEDPartyTCLSpot,
                                };
                                FixtureType::from(model)
                            }
                            _ => unreachable!(),
                        };

                        let mut fixture = Fixture::new(start_addr, name.into(), fixture_type);
                        fixture.pos = pos;

                        {
                            let mut engine = self.data.state.dmx_engine.write().unwrap();
                            engine.add_fixture_to_group(group_id, fixture);
                        }

                        // Reset some fields and close
                        self.add_fixture_open = false;
                        self.add_fixture_model_index = 0;
                        self.add_fixture_kind = 0;
                        self.add_fixture_name = String::from("New Fixture");
                    }
                });
            });
        }

        ui.allocate_ui_with_layout(
            egui::vec2(ui.available_width(), ui.available_height()), // fixed width, max height
            egui::Layout::left_to_right(egui::Align::Min),
            |ui| {
                self.scene_overview(ui, ctx, &dmx_engine);

                ui.separator();

                ui.allocate_ui_with_layout(
                    egui::vec2(ui.available_width(), ui.available_height()), // fixed width, max height
                    egui::Layout::top_down(egui::Align::Min),
                    |ui| {
                        ui.horizontal(|ui| {
                            if components::button(ui, false, "Scene +", ButtonSize::Medium) {
                                self.new_scene_dialog_open = true;
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

                            ui.separator();

                            if components::button(
                                ui,
                                self.current_scene_animations_dialog_open,
                                "Scene Animations",
                                ButtonSize::Medium,
                            ) {
                                self.current_scene_animations_dialog_open =
                                    !self.current_scene_animations_dialog_open;
                            }

                            ui.separator();

                            if components::button(
                                ui,
                                self.show_dmx_simulation,
                                "Show DMX",
                                ButtonSize::Medium,
                            ) {
                                self.show_dmx_simulation = !self.show_dmx_simulation;
                            }

                            if components::button(
                                ui,
                                self.dmx_override_dialog_open,
                                "Show Overrides",
                                ButtonSize::Medium,
                            ) {
                                self.dmx_override_dialog_open = !self.dmx_override_dialog_open;
                            }

                            ui.separator();

                            if components::button(
                                ui,
                                self.add_fixture_open,
                                "Add Fixture",
                                ButtonSize::Medium,
                            ) {
                                self.add_fixture_open = !self.add_fixture_open;
                            }
                        });

                        ui.separator();

                        ui.allocate_ui_with_layout(
                            egui::vec2(ui.available_width(), ui.available_height()), // fixed width, max height
                            egui::Layout::top_down(egui::Align::Min),
                            |ui| {
                                if button(ui, false, "Add Fixture", ButtonSize::Medium) {
                                    self.add_fixture_open = true;
                                }
                            },
                        );
                    },
                );
            },
        );
    }
}
