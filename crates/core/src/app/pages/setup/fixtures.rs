//! Setup › Fixtures: group picker, fixture editor, add / delete / move
//! fixtures. Also hosts the Show Overrides and Simulate tools until 019 and
//! 024 move them.

use crate::app::components::Dialog;
use crate::app::PopupSpec;
use crate::app::{
    components::{self, ButtonSize, HFader},
    page::PageRenderContext,
    BlaulichtApp,
};
use crate::client::EngineClient;
use crate::dmx::EngineState;
use crate::hub::{CreatedId, HostCommand};
use crate::state::NUM_DMX_UNIVERSES;
use blaulicht_shared::fixture::dimmer::Dimmer;
use blaulicht_shared::fixture::light::Light;
use blaulicht_shared::fixture::moving_head::MovingHead;
use blaulicht_shared::fixture::state::{Fixture, Position, Rotation};
use blaulicht_shared::fixture::FixtureType;
use blaulicht_shared::{ControlEvent, ControlEventMessage, EventOriginator};
use egui::{Color32, Context, Frame, Key, Label, RichText};
use std::fmt;
use std::time::Duration;
use strum::IntoEnumIterator;

#[cfg(test)]
mod layout_tests {
    use crate::app::components::{self, ButtonSize};

    #[test]
    fn toolbar_divider_does_not_expand_horizontal_row() {
        let ctx = egui::Context::default();
        let input = egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(
                egui::Pos2::ZERO,
                egui::vec2(800.0, 600.0),
            )),
            ..Default::default()
        };

        let _ = ctx.run(input, |ctx| {
            egui::CentralPanel::default().show(ctx, |ui| {
                let row = ui.horizontal_top(|ui| {
                    ui.button("Before");
                    components::toolbar_separator(ui, ButtonSize::Medium.dim().0.y);
                    ui.button("After");
                });

                assert!(row.response.rect.height() < 100.0);
                assert!(ui.available_height() > 400.0);
            });
        });
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AddFixtureKind {
    MovingHead,
    Light,
    Dimmer,
}

impl AddFixtureKind {
    pub const ALL: [Self; 3] = [Self::MovingHead, Self::Light, Self::Dimmer];
}

/// When creating multiple fixtures at once, optionally offset each successive
/// fixture's position along one axis by a fixed step.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum AddFixtureIncrementAxis {
    #[default]
    None,
    X,
    Y,
    Z,
}

impl AddFixtureIncrementAxis {
    /// Position offset applied per fixture index when this axis is selected.
    const STEP: usize = 10;
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

    fn render_delete_fixture_dialog(&mut self, ctx: &Context) {
        let Some((group_id, fixture_id)) = self.delete_fixture else {
            return;
        };

        Dialog::new("Delete Fixture".to_string(), egui::vec2(280.0, 130.0))
            .with_backdrop()
            .show(ctx, |ui| {
                ui.heading(RichText::new("Delete this fixture?").strong());
                ui.add_space(12.0);
                ui.horizontal(|ui| {
                    if components::button(ui, false, "Confirm", ButtonSize::Medium) {
                        let delete = HostCommand::DeleteFixture {
                            group: group_id,
                            fixture: fixture_id,
                        };
                        self.send_setup_command(delete, move |app, _| {
                            app.setup_fixture_id = app.client.with_show(|show| {
                                show.0
                                    .groups
                                    .get(&group_id)
                                    .and_then(|group| group.fixtures.keys().next().copied())
                                    .unwrap_or_default()
                            });
                        });
                        self.delete_fixture = None;
                    }
                    if components::button(ui, true, "Cancel", ButtonSize::Medium) {
                        self.delete_fixture = None;
                    }
                });
            });
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
                        self.client.send(ControlEventMessage::new(
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

    pub(super) fn render_dmx_override_dialog(&mut self, ctx: &Context, dmx_engine: &EngineState) {
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
                                self.client.send(ControlEventMessage::new(
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
                            self.client.send(ControlEventMessage::new(
                                EventOriginator::Web,
                                ControlEvent::RemoveChannelOverride(*universe as u16, *chan as u16),
                            ));
                        }
                    },
                );

                ui.separator();
            }
        });
    }

    fn render_add_fixture_dialog(&mut self, ctx: &Context, dmx_engine: &EngineState) {
        if !self.add_fixture_open {
            return;
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

                        let footprint = match self.add_fixture_kind {
                            AddFixtureKind::MovingHead => {
                                FixtureType::from(self.add_fixture_selected_moving_head).footprint()
                            }
                            AddFixtureKind::Light => {
                                FixtureType::from(self.add_fixture_selected_light).footprint()
                            }
                            AddFixtureKind::Dimmer => {
                                FixtureType::from(self.add_fixture_selected_dimmer).footprint()
                            }
                        };
                        let count = self.add_fixture_count.max(1) as usize;
                        let final_channel = (self.add_fixture_start_addr as usize)
                            .saturating_add(footprint.saturating_mul(count))
                            .saturating_sub(1);
                        let available_ids = self
                            .add_fixture_group
                            .and_then(|group_id| dmx_engine.groups().get(&group_id))
                            .map(|group| {
                                (u8::MAX as usize + 1).saturating_sub(group.fixtures.len())
                            })
                            .unwrap_or(0);
                        let batch_fits = final_channel <= 512 && count <= available_ids;
                        ui.label(
                            RichText::new(format!(
                                "Universe {} • channels {}–{} • {} group slots free",
                                self.add_fixture_universe_no,
                                self.add_fixture_start_addr,
                                final_channel,
                                available_ids
                            ))
                            .size(12.0)
                            .color(if batch_fits {
                                Color32::LIGHT_GREEN
                            } else {
                                Color32::LIGHT_RED
                            }),
                        );

                        // Optionally step one axis by +10 per fixture when creating many.
                        ui.horizontal(|ui| {
                            ui.add_sized([LABEL_W, cell_h], Label::new("Step +10:"));
                            let axis_button_size = ButtonSize::Medium.with_width(70.0);
                            for (axis, label) in [
                                (AddFixtureIncrementAxis::None, "Off"),
                                (AddFixtureIncrementAxis::X, "X"),
                                (AddFixtureIncrementAxis::Y, "Y"),
                                (AddFixtureIncrementAxis::Z, "Z"),
                            ] {
                                if components::button(
                                    ui,
                                    self.add_fixture_increment_axis == axis,
                                    label,
                                    axis_button_size,
                                ) {
                                    self.add_fixture_increment_axis = axis;
                                }
                            }
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
                            && (self.add_fixture_universe_no as usize) < NUM_DMX_UNIVERSES
                            && self.add_fixture_start_addr >= 1
                            && self.add_fixture_start_addr <= 512;

                        let mut button_pressed = components::action_button(
                            ui,
                            can_create,
                            "Create",
                            ButtonSize::Medium,
                            Some("Choose a valid fixture type, group, address, and count"),
                        );
                        ctx.input(|input| {
                            if input.key_pressed(Key::Enter) {
                                button_pressed = true;
                            }
                        });

                        if !can_create {
                            button_pressed = false;
                        }

                        if button_pressed
                            && self.add_fixture_group.is_some()
                            && !self.setup_reply_pending()
                        {
                            let group_id = self.add_fixture_group.unwrap();
                            let base_name = self.add_fixture_name.clone();
                            let mut start_addr = self.add_fixture_start_addr as usize;
                            let universe_no = self.add_fixture_universe_no as usize;
                            let pos = Position {
                                x: self.add_fixture_pos_x,
                                y: self.add_fixture_pos_y,
                                z: 0.0,
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
                            let count = self.add_fixture_count.max(1) as usize;

                            let mut fixtures = Vec::with_capacity(count);
                            for i in 0..count {
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
                                let mut fixture_pos = pos.clone();
                                let offset = (i * AddFixtureIncrementAxis::STEP) as f32;
                                match self.add_fixture_increment_axis {
                                    AddFixtureIncrementAxis::None => {}
                                    AddFixtureIncrementAxis::X => fixture_pos.x += offset,
                                    AddFixtureIncrementAxis::Y => fixture_pos.y += offset,
                                    AddFixtureIncrementAxis::Z => fixture_pos.z += offset,
                                }
                                fixture.pos = fixture_pos;
                                fixture.rotation = rotation.clone();
                                fixtures.push(fixture);
                                start_addr = start_addr.saturating_add(footprint);
                            }

                            // All or none; a refusal (e.g. a DMX overlap) shows
                            // as a popup and keeps the dialog open.
                            let add = HostCommand::AddFixtures {
                                group: group_id,
                                fixtures,
                            };
                            self.send_setup_command(add, |app, _| {
                                // Reset fields and close only after the full batch succeeds.
                                app.add_fixture_open = false;
                                app.add_fixture_kind_dialog_open = false;
                                app.add_fixture_model_dialog_open = false;
                                app.close_add_fixture_numberpads();
                                app.add_fixture_name = String::from("New Fixture");
                            });
                        }
                    });
                });
            });
    }

    pub(super) fn setup_fixtures_ui(
        &mut self,
        ui: &mut egui::Ui,
        ctx: &Context,
        render_context: PageRenderContext,
        dmx_engine: &EngineState,
    ) {
        self.render_delete_fixture_dialog(ctx);

        self.render_add_fixture_dialog(ctx, dmx_engine);

        render_context.horizontal(ui, egui::Align::Min, |ui| {
            self.fixture_toolbar(ui, dmx_engine);
        });

        ui.separator();

        ui.allocate_ui_with_layout(
            egui::vec2(ui.available_width(), ui.available_height()), // fixed width, max height
            egui::Layout::left_to_right(egui::Align::Min),
            |ui| {
                let (new_g, new_f, changed) = self.group_selection(
                    dmx_engine,
                    self.add_fixture_group,
                    self.setup_fixture_id,
                    ui,
                );

                if changed {
                    self.select_setup_fixture(dmx_engine, new_g, new_f);
                }

                ui.separator();

                // Scrolls only when the area is shorter than the editor (small
                // dock panes); the 800x480 layout fits as is.
                ui.vertical(|ui| {
                    egui::ScrollArea::vertical()
                        .id_salt("setup-fixture-editor")
                        .show(ui, |ui| self.fixture_editor(ui, ctx, dmx_engine));
                });
            },
        );
    }

    fn fixture_toolbar(&mut self, ui: &mut egui::Ui, dmx_engine: &EngineState) {
        if components::button(ui, self.add_fixture_open, "Add Fixture", ButtonSize::Medium) {
            if dmx_engine.groups().is_empty() {
                self.show_popup(PopupSpec::with_duration(
                    Duration::from_secs(3),
                    "No groups yet: add one under Groups".to_string(),
                ));
            } else if self.add_fixture_group.is_none() {
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

        components::toolbar_separator(ui, ButtonSize::Medium.dim().0.y);

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

        components::toolbar_separator(ui, ButtonSize::Medium.dim().0.y);

        if components::button(ui, self.dmx_simulator.open, "Simulate", ButtonSize::Medium) {
            self.dmx_simulator.open = !self.dmx_simulator.open;
        }
    }

    /// Selects fixture `fixture_id` of group `group_id` for the editor and
    /// loads its values into the edit fields. Also used by the Groups tab.
    pub(super) fn select_setup_fixture(
        &mut self,
        dmx_engine: &EngineState,
        group_id: u8,
        fixture_id: u8,
    ) {
        self.close_edit_fixture_numberpads();
        self.add_fixture_group = Some(group_id);
        self.setup_fixture_id = fixture_id;

        let Some(fix) = dmx_engine
            .groups()
            .get(&group_id)
            .and_then(|group| group.fixtures.get(&fixture_id))
        else {
            return;
        };
        self.load_fixture_into_editor(fix);
    }

    /// Swaps fixtures `fid` and `other` of group `gid` (Move Up / Down); the
    /// editor keeps the moved fixture selected, at its new id `other`.
    fn swap_setup_fixtures(&mut self, gid: u8, fid: u8, other: u8) {
        let swap = HostCommand::SwapFixtures {
            group: gid,
            a: fid,
            b: other,
        };
        self.send_setup_command(swap, move |app, _| {
            if app.add_fixture_group == Some(gid) && app.setup_fixture_id == fid {
                app.setup_fixture_id = other;
            }
        });
    }

    fn load_fixture_into_editor(&mut self, fix: &Fixture) {
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

    /// Edit fields and actions for the selected fixture.
    fn fixture_editor(&mut self, ui: &mut egui::Ui, ctx: &Context, dmx_engine: &EngineState) {
        const LABEL_W: f32 = 120.0;

        let Some(gid) = self.add_fixture_group else {
            return;
        };
        let fid = self.setup_fixture_id;

        if let Some(group) = dmx_engine.groups().get(&gid) {
            if let Some(fix) = group.fixtures.get(&fid) {
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
                    ui.add_sized([LABEL_W, ButtonSize::Medium.dim().0.y], Label::new("Name:"));

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
                        let can_save = (1..=512).contains(&self.new_fixture_addr);
                        if components::button(ui, can_save, "Save", ButtonSize::Medium) && can_save
                        {
                            let mut updated = fix.clone();
                            updated.name = self.new_fixture_name.clone();
                            updated.start_addr = self.new_fixture_addr;
                            updated.universe_no = self.new_fixture_uni;
                            updated.pos.x = self.new_fixture_pos_x;
                            updated.pos.y = self.new_fixture_pos_y;
                            updated.pos.z = self.new_fixture_pos_z;
                            updated.rotation.x = self.new_fixture_rot_x;
                            updated.rotation.y = self.new_fixture_rot_y;
                            updated.rotation.z = self.new_fixture_rot_z;

                            let update = HostCommand::UpdateFixture {
                                group: gid,
                                fixture: fid,
                                value: updated,
                            };
                            self.send_setup_command(update, |_, _| {});
                        }

                        ui.add_space(6.0);

                        let prev_fid = group.fixtures.range(..fid).next_back().map(|(k, _)| *k);
                        let next_fid = group
                            .fixtures
                            .range((std::ops::Bound::Excluded(fid), std::ops::Bound::Unbounded))
                            .next()
                            .map(|(k, _)| *k);

                        if components::button(ui, prev_fid.is_some(), "Move Up", ButtonSize::Medium)
                        {
                            if let Some(prev) = prev_fid {
                                self.swap_setup_fixtures(gid, fid, prev);
                            }
                        }

                        ui.add_space(6.0);

                        if components::button(
                            ui,
                            next_fid.is_some(),
                            "Move Down",
                            ButtonSize::Medium,
                        ) {
                            if let Some(next) = next_fid {
                                self.swap_setup_fixtures(gid, fid, next);
                            }
                        }

                        ui.add_space(6.0);

                        let target_group_options: Vec<(u8, String)> = dmx_engine
                            .groups()
                            .iter()
                            .filter(|(target_gid, _)| **target_gid != gid)
                            .map(|(target_gid, target_group)| {
                                (
                                    *target_gid,
                                    format!("{} | #{}", target_group.name, target_gid),
                                )
                            })
                            .collect();
                        let can_move_group = !target_group_options.is_empty();

                        if components::action_button(
                            ui,
                            can_move_group,
                            "Move Group",
                            ButtonSize::Medium,
                            Some("Create another group before moving fixtures"),
                        ) && can_move_group
                        {
                            self.move_fixture_group_dialog_open = true;
                        }

                        if self.move_fixture_group_dialog_open {
                            let current_target = target_group_options
                                .first()
                                .map(|(target_gid, _)| *target_gid)
                                .unwrap_or(gid);

                            let (target_gid, changed) = components::id_selection_dialog(
                                ctx,
                                target_group_options,
                                current_target,
                                &mut self.move_fixture_group_dialog_open,
                                "Move Fixture To Group".to_string(),
                            );

                            if changed {
                                // The editor follows the fixture to its new
                                // group and id.
                                let move_fixture = HostCommand::MoveFixtureToGroup {
                                    group: gid,
                                    fixture: fid,
                                    target: target_gid,
                                };
                                self.send_setup_command(move_fixture, |app, created| {
                                    if let Some(CreatedId::Fixture { group, fixture }) = created {
                                        let moved = app.client.with_show(|show| {
                                            show.0
                                                .groups
                                                .get(&group)
                                                .and_then(|group| group.fixtures.get(&fixture))
                                                .cloned()
                                        });
                                        app.add_fixture_group = Some(group);
                                        app.setup_fixture_id = fixture;
                                        app.close_edit_fixture_numberpads();
                                        if let Some(fix) = moved {
                                            app.load_fixture_into_editor(&fix);
                                        }
                                    }
                                });
                            }
                        }

                        ui.add_space(6.0);

                        if components::button(ui, false, "Initialize", ButtonSize::Medium) {
                            let label = format!("Initializing `{}` (10 s)", self.new_fixture_name);
                            let init = HostCommand::InitFixtures(vec![(gid, fid)]);
                            self.send_setup_command(init, move |app, _| {
                                app.show_popup(PopupSpec::with_duration(
                                    Duration::from_secs(3),
                                    label,
                                ));
                            });
                        }

                        ui.add_space(6.0);

                        if components::button(ui, false, "Show in Visualizer", ButtonSize::Medium) {
                            self.show_fixtures_in_visualizer([(gid, fid)]);
                        }

                        ui.add_space(6.0);

                        if components::button(ui, true, "Delete", ButtonSize::Medium) {
                            self.delete_fixture = Some((gid, fid));
                        }
                    });
                });
            } else {
                ui.label(RichText::new("No fixture selected in this group").color(Color32::GRAY));
            }
        } else {
            ui.label(RichText::new("No group selected").color(Color32::GRAY));
        }
    }
}
