use crate::app::{
    components::{self, ButtonSize, HFader},
    BlaulichtApp,
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

        //
        // Main UI start.
        //

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
                        });

                        ui.separator();

                        ui.allocate_ui_with_layout(
                            egui::vec2(ui.available_width(), ui.available_height()), // fixed width, max height
                            egui::Layout::left_to_right(egui::Align::Min),
                            |ui| {},
                        );
                    },
                );
            },
        );
    }
}
