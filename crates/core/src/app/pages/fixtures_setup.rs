use std::mem;
use std::time::Duration;

use crate::app::components::DEFAULT_NEW_GROUP_NAME;
use crate::app::{
    components::{self, ButtonSize, HFader},
    BlaulichtApp,
};
use crate::app::{PopupButtonSpec, PopupSpec};
use blaulicht_shared::fixture::dimmer::Dimmer;
use blaulicht_shared::fixture::light::Light;
use blaulicht_shared::fixture::moving_head::MovingHead;
use blaulicht_shared::fixture::state::{Fixture, Position};
use blaulicht_shared::fixture::FixtureType;
use blaulicht_shared::{ControlEvent, ControlEventMessage, EventOriginator};
use egui::{
    Color32, Context, FontId, Frame, Key, Label, Margin, RichText, TextEdit, TextStyle, Vec2,
};
use log::warn;
use strum::IntoEnumIterator;

impl BlaulichtApp {
    pub fn render_delete_group(&mut self, ctx: &Context) {
        if self.delete_group_open {
            components::dialog(
                ctx,
                "Confirm Deletion",
                egui::vec2(200.0, 100.0),
                false,
                |ui| {
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
                },
            );
        }
    }

    pub fn render_add_group_dialog(&mut self, ctx: &Context) {
        if self.add_group_open {
            const BUTTON_SIZE: ButtonSize = ButtonSize::Large;
            const SPACING: f32 = 16.0;
            let size = egui::vec2(260.0, BUTTON_SIZE.dim().0.y * 2.0 + SPACING + 8.0);
            components::dialog(ctx, "Create Group", size, false, |ui| {
                ui.spacing_mut().interact_size = egui::vec2(44.0, 36.0);
                Frame::new()
                    .inner_margin(Margin::symmetric(10, 6))
                    .show(ui, |ui| {
                        ui.add(
                            TextEdit::singleline(&mut self.new_group_name)
                                .font(FontId::proportional(BUTTON_SIZE.dim().1))
                                .min_size(Vec2::new(0.0, BUTTON_SIZE.dim().1)),
                        );
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

    pub fn fixtures_ui_setup(&mut self, ui: &mut egui::Ui, ctx: &Context) {
        // Make controls touch-friendly within this page
        ui.spacing_mut().interact_size = egui::vec2(44.0, 36.0);
        let dmx_engine = { self.data.state.dmx_engine.read().unwrap().clone() };
        let groups = dmx_engine.groups();

        //
        // Dialogs start.
        //
        self.render_dmx_simulation_dialog(ctx, groups);
        self.render_add_scene_dialog(ctx);
        self.render_clone_scene_dialog(ctx);
        self.render_scene_changeset_dialog(ctx, &dmx_engine);
        self.render_scene_animations_dialog(ctx, &dmx_engine);

        // mem::drop(dmx_engine);

        // let mut dmx_engine = self.data.state.dmx_engine.write().unwrap();

        if self.add_dmx_override_open {
            let cell_h = ButtonSize::Medium.dim().0.y;
            let height = cell_h * 3.5 + 24.0;
            const LABEL_W: f32 = 120.0;

            components::dialog(
                ctx,
                "Add Override",
                egui::vec2(260.0, height),
                false,
                |ui| {
                    ui.spacing_mut().interact_size = egui::vec2(44.0, 36.0);

                    ui.horizontal_centered(|ui| {
                        ui.set_height(cell_h);

                        ui.add_sized(
                            [140.0, cell_h],
                            egui::widgets::DragValue::new(&mut self.add_dmx_override_uni)
                                .speed(1)
                                .range(0..=1),
                        );
                        ui.add_sized([LABEL_W, cell_h], Label::new("DMX Uni:"));
                    });

                    ui.separator();

                    ui.horizontal_centered(|ui| {
                        ui.set_height(cell_h);

                        ui.add_sized(
                            [140.0, cell_h],
                            egui::widgets::DragValue::new(&mut self.add_dmx_override_chan)
                                .speed(1)
                                .range(1..=512),
                        );
                        ui.add_sized([LABEL_W, cell_h], Label::new("DMX Channel:"));
                    });

                    ui.separator();

                    ui.horizontal_centered(|ui| {
                        ui.add_sized(
                            [140.0, cell_h],
                            egui::widgets::DragValue::new(&mut self.add_dmx_override_value)
                                .speed(1)
                                .range(0..=255),
                        );
                        ui.add_sized([LABEL_W, cell_h], Label::new("Value:"));
                    });

                    ui.separator();

                    ui.horizontal(|ui| {
                        ui.set_height(cell_h);

                        if components::button(ui, false, "Cancel", ButtonSize::Medium) {
                            self.add_dmx_override_open = false;
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

                    for ((universe, chan), value) in &dmx_engine.0.overrides {
                        ui.allocate_ui_with_layout(
                            egui::vec2(ui.available_width(), 30.0),
                            egui::Layout::left_to_right(egui::Align::Min),
                            |ui| {
                                ui.add_sized(
                                    [60.0, 16.0],
                                    Label::new(
                                        RichText::new(format!("UN: {universe} | CH: {chan}"))
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
                                                    *universe as u16,
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
                },
            )
        }

        self.render_add_group_dialog(ctx);
        self.render_delete_group(ctx);

        let mut skip_rest = false;

        // Add Fixture Dialog
        if self.add_fixture_open {
            const BUTTON_SIZE: ButtonSize = ButtonSize::Medium;
            let cell_h = BUTTON_SIZE.dim().0.y;
            let width = 570.0;
            let height = 330.0;
            const LABEL_W: f32 = 120.0;

            components::dialog(ctx, "Add Fixture", egui::vec2(width, height), false, |ui| {
                ui.spacing_mut().interact_size = egui::vec2(44.0, 36.0);
                // Group selector
                ui.label(format!("Group #{}", self.add_fixture_group.unwrap_or(0)));

                ui.separator();

                // Name
                ui.horizontal(|ui| {
                    ui.add_sized([LABEL_W, cell_h], Label::new("Name:"));

                    ui.add(
                        TextEdit::singleline(&mut self.add_fixture_name)
                            .font(FontId::proportional(BUTTON_SIZE.dim().1))
                            .min_size(Vec2::new(0.0, BUTTON_SIZE.dim().1)),
                    );
                });

                // Start address
                ui.horizontal(|ui| {
                    ui.add_sized([LABEL_W, cell_h], Label::new("Start Addr:"));
                    ui.add_sized(
                        [140.0, cell_h],
                        egui::widgets::DragValue::new(&mut self.add_fixture_start_addr)
                            .speed(1)
                            .range(1..=512),
                    );

                    ui.add_sized([LABEL_W, cell_h], Label::new("Universe:"));
                    ui.add_sized(
                        [140.0, cell_h],
                        egui::widgets::DragValue::new(&mut self.add_fixture_universe_no)
                            .speed(1)
                            .range(0..=1),
                    );
                });

                // Universe
                // ui.horizontal(|ui| {
                // });

                // Position
                ui.horizontal(|ui| {
                    ui.add_sized([LABEL_W, cell_h], Label::new("Pos X:"));
                    ui.add_sized(
                        [140.0, cell_h],
                        egui::widgets::DragValue::new(&mut self.add_fixture_pos_x).speed(1),
                    );

                    ui.add_sized([LABEL_W, cell_h], Label::new("Pos Y:"));
                    ui.add_sized(
                        [140.0, cell_h],
                        egui::widgets::DragValue::new(&mut self.add_fixture_pos_y).speed(1),
                    );
                });

                ui.separator();

                // Kind selector
                ui.horizontal(|ui| {
                    ui.add_sized([LABEL_W, cell_h], Label::new("Type:"));
                    let kinds = ["MovingHead", "Light", "Dimmer"];
                    egui::ComboBox::from_id_source("add_fixture_kind_combo")
                        .width(140.0)
                        .selected_text(kinds[self.add_fixture_kind])
                        .show_ui(ui, |ui| {
                            for (idx, label) in kinds.iter().enumerate() {
                                ui.selectable_value(&mut self.add_fixture_kind, idx, *label);
                            }
                        });

                    ///////

                    ui.add_sized([LABEL_W, cell_h], Label::new("Model:"));
                    match self.add_fixture_kind {
                        0 => {
                            // MovingHead
                            let labels: Vec<_> =
                                MovingHead::iter().map(|l| l.to_string()).collect();

                            let mut idx = self.add_fixture_model_index.min(labels.len() - 1);
                            egui::ComboBox::from_id_source("add_fixture_model_combo")
                                .width(140.0)
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
                            let labels: Vec<_> = Light::iter().map(|l| l.to_string()).collect();
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
                            // Light
                            let labels: Vec<_> = Dimmer::iter().map(|l| l.to_string()).collect();
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
                    }
                });

                // Model selector depending on kind
                // ui.horizontal(|ui| {});

                ui.separator();

                // Count
                ui.horizontal(|ui| {
                    ui.add_sized([LABEL_W, cell_h], Label::new("Count:"));
                    ui.add_sized(
                        [140.0, cell_h],
                        egui::widgets::DragValue::new(&mut self.add_fixture_count)
                            .speed(1)
                            .range(1..=64),
                    );
                });

                ui.separator();

                ui.horizontal(|ui| {
                    if components::button(ui, false, "Cancel", ButtonSize::Medium) {
                        self.add_fixture_open = false;
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

                        // Build fixture type
                        let fixture_type = match self.add_fixture_kind {
                            0 => {
                                // MovingHead
                                let model = match self.add_fixture_model_index {
                                    0 => MovingHead::MartinMac250E,
                                    _ => unreachable!("Not possible"),
                                };
                                FixtureType::from(model)
                            }
                            1 => {
                                // Light
                                let model = match self.add_fixture_model_index {
                                    0 => Light::Generic3ChanNoAlpha,
                                    1 => Light::Generic4ChanWithAlpha,
                                    2 => Light::LEDPartyTCLSpot,
                                    3 => Light::AdjMegaHexPar,
                                    4 => Light::LiteCraftMiniParAT10,
                                    5 => Light::VaryTechVP1,
                                    _ => unreachable!("not possible"),
                                };
                                FixtureType::from(model)
                            }
                            _ => {
                                let model = match self.add_fixture_model_index {
                                    0 => Dimmer::FogMachineSingle,
                                    1 => Dimmer::DimmerSingle,
                                    2 => Dimmer::DimmerWStrobe,
                                    _ => unreachable!("not possible"),
                                };

                                FixtureType::from(model)
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
                                dmx_engine.add_fixture_to_group(group_id, fixture);
                                start_addr = start_addr.saturating_add(footprint);
                            }
                            skip_rest = true;
                        }

                        // Reset some fields and close
                        self.add_fixture_open = false;

                        // self.add_fixture_model_index = 0;
                        // self.add_fixture_kind = 0;

                        self.add_fixture_name = String::from("New Fixture");
                    }
                });
            });
        }

        if skip_rest {
            return;
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

                            if components::button(ui, false, "Clone Sc.", ButtonSize::Medium)
                            {
                                self.clone_scene_dialog_open = true;
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

                            ui.separator();

                            for (universe, open) in
                                self.show_dmx_simulation_universes.iter_mut().enumerate()
                            {
                                if components::button(
                                    ui,
                                    *open,
                                    &format!("Sim. DMX {universe}"),
                                    ButtonSize::Medium,
                                ) {
                                    *open = !*open;
                                }
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
                                if dmx_engine.groups().is_empty() {
                                    self.show_popup(PopupSpec::with_duration(
                                        Duration::from_secs(3),
                                        "No Group Selected".to_string(),
                                    ));
                                } else {
                                    self.add_fixture_open = !self.add_fixture_open;
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
                                    self.add_fixture_group = Some(new_g);
                                    self.setup_fixture_id = new_f;

                                    let group = dmx_engine.0.groups.get(&new_g).unwrap();
                                    if let Some(fix) = group.fixtures.get(&new_f) {
                                        self.new_fixture_name = fix.name.to_string();
                                        self.new_fixture_addr = fix.start_addr;
                                        self.new_fixture_uni = fix.universe_no;
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
                                                ui.add_sized(
                                                    [LABEL_W, ButtonSize::Medium.dim().0.y],
                                                    Label::new("Name:"),
                                                );
                                                ui.add_sized(
                                                    [140.0, ButtonSize::Medium.dim().0.y],
                                                    TextEdit::singleline(
                                                        &mut self.new_fixture_name,
                                                    )
                                                    .font(FontId::proportional(
                                                        ButtonSize::Medium.dim().1,
                                                    )),
                                                );
                                            });

                                            ui.horizontal(|ui| {
                                                ui.add_sized(
                                                    [LABEL_W, ButtonSize::Medium.dim().0.y],
                                                    Label::new("Start Addr:"),
                                                );
                                                ui.add_sized(
                                                    [140.0, ButtonSize::Medium.dim().0.y],
                                                    egui::widgets::DragValue::new(
                                                        &mut self.new_fixture_addr,
                                                    )
                                                    .speed(1)
                                                    .range(1..=512),
                                                );
                                            });

                                            ui.horizontal(|ui| {
                                                ui.add_sized(
                                                    [LABEL_W, ButtonSize::Medium.dim().0.y],
                                                    Label::new("Universe:"),
                                                );
                                                ui.add_sized(
                                                    [140.0, ButtonSize::Medium.dim().0.y],
                                                    egui::widgets::DragValue::new(
                                                        &mut self.new_fixture_uni,
                                                    )
                                                    .speed(1)
                                                    .range(0..=1),
                                                );
                                            });

                                            ui.add_space(6.0);

                                            //mem::drop(engine);

                                            ui.horizontal(|ui| {
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
                                                    let mut dmx_engine =
                                                        self.data.state.dmx_engine.write().unwrap();
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
                                                    let mut dmx_engine =
                                                        self.data.state.dmx_engine.write().unwrap();

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
                                                                self.setup_fixture_id = first_fid;
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
