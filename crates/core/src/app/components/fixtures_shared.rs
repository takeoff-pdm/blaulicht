use crate::{
    app::{
        components::{self, ButtonColor, ButtonSize, HFader},
        BlaulichtApp, Selection,
    },
    dmx::{EngineGroups, EngineState, FixtureSelection, FixtureState},
    event::SystemEventBusConnectionInst,
    state::DmxBuffer,
};
use blaulicht_shared::{
    ControlEvent, ControlEventMessage, EventOriginator, FixtureProperty, RGBColor,
};
use egui::{
    Align2, Color32, Context, FontId, Frame, Key, Margin, RichText, TextBuffer, TextEdit, Vec2,
};
use map_range::MapRange;
use std::{
    collections::BTreeMap,
    sync::{RwLockReadGuard, RwLockWriteGuard},
};

pub const DEFAULT_NEW_SCENE_NAME: &str = "My Scene";
pub const DEFAULT_NEW_GROUP_NAME: &str = "My Group";

pub fn simulate_dmx(
    ui: &mut egui::Ui,
    _groups: &EngineGroups,
    dmx: RwLockReadGuard<'_, DmxBuffer>,
) {
    let len = dmx.dmx_buffer.len() as f32;
    let dimensions = len.sqrt() as usize;

    let base_height = 16.0;
    let padding = 1.0;

    let dim_pixels = dimensions as f32 * (base_height + padding);

    let dmx_buffer = dmx.dmx_buffer;

    ui.horizontal(|ui| {
        // Remove spacing in this container
        ui.spacing_mut().item_spacing = egui::vec2(0.0, 0.0);

        // Allocate fixed square space for the matrix
        let (rect, _response) =
            ui.allocate_exact_size(egui::vec2(dim_pixels, dim_pixels), egui::Sense::hover());
        let painter = ui.painter();

        // Draw the cells manually
        for row in 0..dimensions {
            for col in 0..dimensions {
                let value = dmx_buffer[row * dimensions + col];

                // Calculate top-left corner of this cell
                let x = rect.min.x + col as f32 * (base_height + padding);
                let y = rect.min.y + row as f32 * (base_height + padding);

                let cell_rect = egui::Rect::from_min_size(
                    egui::pos2(x, y),
                    egui::vec2(base_height, base_height),
                );

                // Color based on value
                let (bg_color, fg_color) = match value {
                    0 => (Color32::from_rgb(10, 10, 10), Color32::WHITE),
                    1..=85 => (Color32::from_rgb(255, 0, 0), Color32::WHITE),
                    86..=170 => (Color32::from_rgb(255, 255, 0), Color32::BLACK),
                    171..=255 => (Color32::from_rgb(0, 255, 0), Color32::MAGENTA),
                };

                painter.rect_filled(cell_rect, 0.0, bg_color);
                if value > 0 {
                    painter.text(
                        egui::Pos2 { x, y },
                        Align2::LEFT_TOP,
                        value.to_string(),
                        egui::FontId {
                            size: 9.0,
                            family: egui::FontFamily::Monospace,
                        },
                        fg_color,
                    );
                }
            }
        }
    });
}

impl BlaulichtApp {
    pub fn render_dmx_simulation_dialog(&self, ctx: &Context, groups: &EngineGroups) {
        for (universe, open) in self.show_dmx_simulation_universes.iter().enumerate() {
            if *open {
                components::dialog(
                    ctx,
                    &format!("DMX Universe {universe}"),
                    egui::vec2(500.0, 500.0),
                    true,
                    |ui| {
                        let dmx_buffer = self.data.state.dmx_universes[universe].read().unwrap();
                        simulate_dmx(ui, groups, dmx_buffer);
                    },
                );
            }
        }
    }

    pub fn render_scene_animations_dialog(&self, ctx: &Context, dmx_engine: &EngineState) {
        if self.current_scene_animations_dialog_open {
            const HEIGHT: f32 = 430.0;
            const WIDTH: f32 = 500.0;

            components::dialog(
                ctx,
                "Current Scene Animations",
                egui::vec2(WIDTH, HEIGHT),
                true,
                |ui| {
                    let scene = dmx_engine.curr_scene();

                    ui.set_min_height(HEIGHT - 100.0);

                    ui.heading("Active Animations");
                    ui.separator();

                    egui::ScrollArea::vertical().show(ui, |ui| {
                        if scene.sink.active_animations.is_empty() {
                            ui.label("No Animations Yet");
                        }

                        for (selection, animations) in &scene.sink.active_animations {
                            // ui.horizontal(|ui| {
                            ui.label(
                                RichText::new(format!("Selection: {selection:?}"))
                                    .color(Color32::LIGHT_GREEN),
                            );

                            for (animation_id, animation) in animations {
                                let spec = dmx_engine.animations.get(animation_id).unwrap();

                                ui.label(
                                    RichText::new(format!(
                                        "[{}] {} | {}",
                                        animation_id, spec.name, spec.property
                                    ))
                                    .color(Color32::WHITE),
                                );

                                // Remove button
                                if components::button(ui, false, "Remove", ButtonSize::Medium) {
                                    let mut selection_instructions =
                                        selection.generate_instructions();

                                    selection_instructions.push_front(ControlEvent::PushSelection);
                                    selection_instructions
                                        .push_back(ControlEvent::RemoveAnimation(*animation_id));
                                    selection_instructions.push_back(ControlEvent::PopSelection);

                                    self.data
                                        .event_bus_connection
                                        .send(ControlEventMessage::new(
                                            EventOriginator::Web,
                                            ControlEvent::Transaction(
                                                selection_instructions.into_iter().collect(),
                                            ),
                                        ));
                                }

                                let (label, enabled, event) = match animation.enabled {
                                    true => {
                                        ("Pause", true, ControlEvent::PauseAnimation(*animation_id))
                                    }
                                    false => {
                                        ("Play", false, ControlEvent::PlayAnimation(*animation_id))
                                    }
                                };

                                if components::button(ui, enabled, label, ButtonSize::Medium) {
                                    let mut selection_instructions =
                                        selection.generate_instructions();

                                    selection_instructions.push_front(ControlEvent::PushSelection);
                                    selection_instructions.push_back(event);
                                    selection_instructions.push_back(ControlEvent::PopSelection);

                                    self.data
                                        .event_bus_connection
                                        .send(ControlEventMessage::new(
                                            EventOriginator::Web,
                                            ControlEvent::Transaction(
                                                selection_instructions.into_iter().collect(),
                                            ),
                                        ));
                                }
                            }

                            ui.separator();
                        }
                    });
                },
            );
        }
    }

    pub fn render_scene_changeset_dialog(&self, ctx: &Context, dmx_engine: &EngineState) {
        if self.current_scene_changeset_dialog_open {
            const HEIGHT: f32 = 500.0;
            const WIDTH: f32 = 200.0;
            components::dialog(
                ctx,
                "Current Scene Changeset",
                egui::vec2(WIDTH, HEIGHT),
                true,
                |ui| {
                    let scene = dmx_engine.curr_scene();

                    ui.allocate_ui_with_layout(
                        egui::vec2(WIDTH, HEIGHT),
                        egui::Layout::left_to_right(egui::Align::Min),
                        |ui| {
                            ui.vertical(|ui| {
                                ui.heading("Scene Changeset");
                                ui.separator();

                                let mut changeset_organized: BTreeMap<
                                    (u8, u8),
                                    Vec<FixtureProperty>,
                                > = BTreeMap::new();

                                for change in &scene.sink.changeset {
                                    match changeset_organized.get_mut(&(change.gid, change.fid)) {
                                        Some(entry) => entry.push(change.property),
                                        None => {
                                            changeset_organized.insert(
                                                (change.gid, change.fid),
                                                vec![change.property],
                                            );
                                        }
                                    }
                                }

                                egui::ScrollArea::vertical().show(ui, |ui| {
                                    if changeset_organized.is_empty() {
                                        ui.label("No Changes Yet");
                                    }

                                    for ((gid, fid), properties) in changeset_organized {
                                        ui.label(
                                            RichText::new(format!("GID: {gid} | FID: {fid}"))
                                                .color(Color32::LIGHT_GREEN),
                                        );

                                        for prop in properties {
                                            ui.label(format!("- {prop}"));
                                        }
                                    }
                                });
                            });
                        },
                    );
                },
            );
        }
    }

    pub fn render_add_scene_dialog(&mut self, ctx: &Context) {
        if self.new_scene_dialog_open {
            const BUTTON_SIZE: ButtonSize = ButtonSize::Large;
            const SPACING: f32 = 16.0;

            let size = egui::vec2(200.0, BUTTON_SIZE.dim().0.y * 2.0 + SPACING);
            components::dialog(ctx, "Create Scene", size, false, |ui| {
                Frame::new()
                    .inner_margin(Margin::symmetric(10, 6))
                    .show(ui, |ui| {
                        ui.add(
                            TextEdit::singleline(&mut self.new_scene_name)
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
                    dmx_engine.new_scene(self.new_scene_name.take());
                    self.new_scene_name = DEFAULT_NEW_SCENE_NAME.to_string();
                    self.new_scene_dialog_open = false;
                }
            });
        }
    }

    pub fn group_selection(
        &mut self,
        groups: &EngineGroups,
        selected_group: u8,
        selected_fixture: u8,
        ui: &mut egui::Ui,
    ) -> (u8, u8, bool) {
        let dmx_engine = self.data.state.dmx_engine.read().unwrap();

        let mut group_result = selected_group;
        let mut fixture_result = selected_fixture;
        let mut changed = false;

        ui.horizontal(|ui| {
            ui.set_min_height(ui.available_height());
            ui.vertical(|ui| {
                ui.label("Groups:");
                ui.add_space(8.0);
                for (group_id, group) in groups.iter() {
                    let is_selected = selected_group == *group_id;

                    let name = format!("GRP {}", group_id);

                    if components::clickable(
                        ui,
                        is_selected,
                        ButtonColor::Blue.into(),
                        ButtonSize::Large.with_height(50.0),
                        |ui, rect, fg_color| {
                            let painter = ui.painter();
                            let fixture_count = group.fixtures.len();
                            let name = format!("GRP {}", group_id);
                            painter.text(
                                rect.left_top() + egui::vec2(12.0, 8.0),
                                egui::Align2::LEFT_TOP,
                                &name,
                                egui::FontId::proportional(16.0),
                                fg_color,
                            );
                            painter.text(
                                rect.left_bottom() - egui::vec2(-12.0, 8.0),
                                egui::Align2::LEFT_BOTTOM,
                                format!("{} fixtures", fixture_count),
                                egui::FontId::proportional(12.0),
                                fg_color,
                            );
                        },
                    ) {
                        // Toggle group selection
                        if !is_selected {
                            group_result = *group_id;
                            changed = true;
                        };
                    }

                    ui.add_space(2.0);
                }
            });

            ui.vertical(|ui| {
                ui.label("Fixtures");
                ui.add_space(8.0);

                if dmx_engine.groups().is_empty() {
                    return;
                }

                let group_id = self.add_fixture_group;

                for (fix_id, fixture) in dmx_engine
                    .groups()
                    .get(&group_id)
                    .as_ref()
                    .unwrap()
                    .fixtures
                    .iter()
                {
                    let fixture = groups
                        .get(&group_id)
                        .unwrap()
                        .fixtures
                        .get(&fix_id)
                        .unwrap();

                    let button_clicked = components::clickable(
                        ui,
                        true,
                        if self.setup_fixture_id == *fix_id {
                            Selection::Limited.color()
                        } else {
                            Selection::Off.color()
                        },
                        ButtonSize::Large.with_height(50.0),
                        |ui, rect, fg| {
                            let painter = ui.painter();

                            painter.text(
                                rect.left_center() + egui::vec2(12.0, 0.0),
                                egui::Align2::LEFT_CENTER,
                                &fixture.name,
                                egui::FontId::proportional(14.0),
                                fg,
                            );

                            painter.text(
                                rect.left_center() + egui::vec2(12.0, 12.0),
                                egui::Align2::LEFT_CENTER,
                                fixture.type_.kind_string(),
                                egui::FontId::proportional(9.0),
                                fg,
                            );
                        },
                    );

                    if button_clicked {
                        fixture_result = *fix_id;
                        changed = true;
                    }

                    ui.add_space(2.0);
                }
            });
        });

        (group_result, fixture_result, changed)
    }

    pub fn fixture_selection(
        &mut self,
        groups: &EngineGroups,
        ui: &mut egui::Ui,
        // selection: &EngineSelection,
        // total_fixtures: &[(u8, u8, Selection)],
        // highlight_fixtures: &HashSet<u8>,
    ) {
        let dmx_engine = self.data.state.dmx_engine.read().unwrap();

        let selection = dmx_engine.selection();

        let highlight_fixtures = &selection.fixtures_in_group;

        let mut total_fixtures = vec![];
        for g_id in selection.group_ids.iter() {
            let group = groups.get(g_id).unwrap();
            if selection.fixtures_in_group.is_empty() {
                total_fixtures.extend(
                    group
                        .fixtures
                        .keys()
                        .map(|f_id| (*g_id, *f_id, Selection::Cascading)),
                );
            } else {
                total_fixtures.extend(group.fixtures.keys().map(|f_id| {
                    let is_limited = selection.fixtures_in_group.contains(f_id);
                    let selection = match is_limited {
                        true => Selection::Limited,
                        false => Selection::Off,
                    };

                    (*g_id, *f_id, selection)
                }));
            }
        }

        ui.horizontal(|ui| {
            ui.set_min_height(ui.available_height());
            ui.vertical(|ui| {
                ui.label("Groups:");
                ui.add_space(8.0);
                for (group_id, group) in groups.iter() {
                    let is_selected = selection.group_ids.contains(group_id);

                    let name = format!("GRP {}", group_id);

                    if components::clickable(
                        ui,
                        is_selected,
                        ButtonColor::Blue.into(),
                        ButtonSize::Large.with_height(50.0),
                        |ui, rect, fg_color| {
                            let painter = ui.painter();
                            let fixture_count = group.fixtures.len();
                            let name = format!("GRP {}", group_id);
                            painter.text(
                                rect.left_top() + egui::vec2(12.0, 8.0),
                                egui::Align2::LEFT_TOP,
                                &name,
                                egui::FontId::proportional(16.0),
                                fg_color,
                            );
                            painter.text(
                                rect.left_bottom() - egui::vec2(-12.0, 8.0),
                                egui::Align2::LEFT_BOTTOM,
                                format!("{} fixtures", fixture_count),
                                egui::FontId::proportional(12.0),
                                fg_color,
                            );
                        },
                    ) {
                        // Toggle group selection
                        let msg = if is_selected {
                            ControlEvent::DeSelectGroup(*group_id)
                        } else {
                            ControlEvent::SelectGroup(*group_id)
                        };

                        self.data
                            .event_bus_connection
                            .send(ControlEventMessage::new(EventOriginator::Web, msg));
                    }

                    ui.add_space(2.0);
                }
                // self.selected_fixture_group = selected_group;
            });

            // Right: fixtures in selected group
            // Get selection info
            // Layout: left (fixtures), right (controls)

            // ui.horizontal(|ui| {
            // Fixtures list
            if !selection.group_ids.is_empty() {
                ui.vertical(|ui| {
                    ui.label("Fixtures");
                    ui.add_space(8.0);

                    for (group_id, fix_id, fixture_selection) in &total_fixtures {
                        let fixture = groups
                            .get(&group_id)
                            .unwrap()
                            .fixtures
                            .get(&fix_id)
                            .unwrap();

                        let button_clicked = components::clickable(
                            ui,
                            true, //*fixture_selection == Selection::Limited,
                            fixture_selection.color(),
                            ButtonSize::Large.with_height(50.0),
                            |ui, rect, fg| {
                                let painter = ui.painter();

                                // painter.rect_filled(rect, 0.0, bg);

                                painter.text(
                                    rect.left_center() + egui::vec2(12.0, 0.0),
                                    egui::Align2::LEFT_CENTER,
                                    &fixture.name,
                                    egui::FontId::proportional(14.0),
                                    fg,
                                );

                                painter.text(
                                    rect.left_center() + egui::vec2(12.0, 12.0),
                                    egui::Align2::LEFT_CENTER,
                                    fixture.type_.kind_string(),
                                    egui::FontId::proportional(9.0),
                                    fg,
                                );
                            },
                        );

                        if button_clicked
                            && selection.group_ids.len() == 1
                            && total_fixtures.len() != 1
                        {
                            let is_fix_selected = highlight_fixtures.contains(fix_id);

                            let msg = if is_fix_selected {
                                ControlEvent::UnLimitSelectionToFixtureInCurrentGroup(*fix_id)
                            } else {
                                ControlEvent::LimitSelectionToFixtureInCurrentGroup(*fix_id)
                            };

                            self.data
                                .event_bus_connection
                                .send(ControlEventMessage::new(EventOriginator::Web, msg));
                        }

                        ui.add_space(2.0);
                    }
                });
            } else {
                let _ = ui.allocate_exact_size(ButtonSize::Large.dim().0, egui::Sense::empty());
            }
        });

        ui.separator();

        {
            let selected_fixture = if selection.fixtures_in_group.len() == 1 {
                total_fixtures
                    .iter()
                    .filter(|(_, _, select)| *select == Selection::Limited)
                    .next()
                    .cloned()
            } else if total_fixtures.len() == 1 {
                total_fixtures.first().cloned()
            } else {
                None
            };

            let buf = match selected_fixture {
                Some((g_id, f_id, _)) => {
                    let fixture = dmx_engine
                        .curr_scene()
                        .sink
                        .fixture_states
                        .get(&(g_id, f_id))
                        .unwrap();
                    fixture.clone()
                }
                None => dmx_engine.control_buffer.clone(),
            };

            let animations: Vec<u8> = dmx_engine.animations.keys().copied().collect();

            self.fixture_controls(
                ui,
                &buf,
                self.data.event_bus_connection.clone(),
                animations.as_slice(),
            );
            // todo!("FIXTURE CONTROLS")
        }
    }

    fn fixture_controls(
        &self,
        ui: &mut egui::Ui,
        buf: &FixtureState,
        event_bus_connection: SystemEventBusConnectionInst,
        animations: &[u8],
    ) {
        Frame::new()
            .fill(ui.visuals().widgets.inactive.weak_bg_fill)
            .inner_margin(Margin::symmetric(12, 6))
            .show(ui, |ui| {
                ui.set_max_width(200.0);
                ui.vertical(|ui| {
                    ui.label("Fixture Controls");
                    ui.add_space(8.0);

                    // Brightness slider.
                    {
                        let mut brightness = buf.alpha as f32;
                        if ui
                            .add(HFader::new(&mut brightness, 0.0..=255.0).with_label("Alpha"))
                            .changed()
                        {
                            event_bus_connection.send(ControlEventMessage::new(
                                EventOriginator::Web,
                                ControlEvent::SetAlpha(brightness as u8),
                            ));
                        };
                    }

                    ui.add_space(3.0);
                    ui.separator();
                    ui.add_space(3.0);

                    // ColorHue slider
                    {
                        let mut hue = buf.color.h as f32;
                        if ui
                            .add(HFader::new(&mut hue, 0.0..=360.0).with_label("Hue"))
                            .changed()
                        {
                            event_bus_connection.send(ControlEventMessage::new(
                                EventOriginator::Web,
                                ControlEvent::SetColorHue(hue as u16),
                            ));
                        };
                    }

                    ui.add_space(3.0);
                    ui.separator();
                    ui.add_space(3.0);

                    // ColorSaturation slider
                    {
                        let mut saturation = buf.color.s.map_range(0.0..1.0, 0.0..255.0) as f32;
                        if ui
                            .add(HFader::new(&mut saturation, 0.0..=255.0).with_label("Saturation"))
                            .changed()
                        {
                            event_bus_connection.send(ControlEventMessage::new(
                                EventOriginator::Web,
                                ControlEvent::SetColorSaturation(saturation as u8),
                            ));
                        };
                    }

                    ui.add_space(3.0);
                    ui.separator();
                    ui.add_space(3.0);

                    // ColorValue slider
                    {
                        let mut value = buf.color.v.map_range(0.0..1.0, 0.0..255.0) as f32;
                        if ui
                            .add(HFader::new(&mut value, 0.0..=255.0).with_label("Value"))
                            .changed()
                        {
                            event_bus_connection.send(ControlEventMessage::new(
                                EventOriginator::Web,
                                ControlEvent::SetColorValue(value as u8),
                            ));
                        };
                    }

                    ui.add_space(3.0);
                    ui.separator();
                    ui.add_space(3.0);

                    // Color picker
                    let b_color: RGBColor = buf.color.into();
                    let mut color = [
                        b_color.r as f32 / 255.0,
                        b_color.g as f32 / 255.0,
                        b_color.b as f32 / 255.0,
                    ];
                    if ui.color_edit_button_rgb(&mut color).changed() {
                        let r = (color[0] * 255.0) as u8;
                        let g = (color[1] * 255.0) as u8;
                        let b = (color[2] * 255.0) as u8;

                        let tup = (r, g, b);
                        if RGBColor::from(tup) != b_color {
                            println!(
                                "RGBColor::from(tup) != b_color ({:?} != {:?})",
                                RGBColor::from(tup),
                                b_color
                            );
                            event_bus_connection.send(ControlEventMessage::new(
                                EventOriginator::Web,
                                ControlEvent::SetColor(tup),
                            ));
                        }
                    }
                });
            });
    }

    pub fn scene_overview(&mut self, ui: &mut egui::Ui, ctx: &Context, dmx_engine: &EngineState) {
        let panel_width = 100.0;
        let panel_padding = 2.0;

        ui.allocate_ui_with_layout(
            egui::vec2(panel_width, ui.available_height()), // fixed width, max height
            egui::Layout::top_down(egui::Align::Center),
            |ui| {
                let number_of_items_total = dmx_engine.scenes.len();
                const ITEMS_PER_PAGE: usize = 7;
                let total_pages = number_of_items_total / ITEMS_PER_PAGE;

                ui.set_min_width(panel_width);

                ui.vertical_centered(|ui| {
                    let scene_panel_page_button_sizes =
                        ButtonSize::Medium.with_width(ButtonSize::Medium.dim().0.x / 2.0);

                    ui.horizontal(|ui| {
                        if components::button(ui, false, "◀", scene_panel_page_button_sizes)
                            && self.scene_page_index > 0
                        {
                            self.scene_page_index -= 1;
                        }

                        if components::button(ui, false, "▶", scene_panel_page_button_sizes)
                            && self.scene_page_index < total_pages
                        {
                            self.scene_page_index += 1;
                        }
                    });

                    ui.add_space(5.0);

                    ui.label(format!("Page {} / {total_pages}", self.scene_page_index));
                    ui.label(format!("Scenes: {number_of_items_total}"));
                });

                ui.add_space(5.0);

                let start = self.scene_page_index * ITEMS_PER_PAGE;
                let page_items = dmx_engine.scenes.iter().skip(start).take(ITEMS_PER_PAGE);

                for (scene_id, scene) in page_items {
                    let is_selected = dmx_engine.current_scene_focus == *scene_id;

                    let label = format!("{scene_id} | {}", scene.name);
                    if components::button(
                        ui,
                        is_selected,
                        &label,
                        ButtonSize::Large
                            .with_width(panel_width - 2.0 * panel_padding)
                            .with_font_size(12.0),
                    ) {
                        // Toggle group selection
                        if !is_selected {
                            self.data
                                .event_bus_connection
                                .send(ControlEventMessage::new(
                                    EventOriginator::Web,
                                    ControlEvent::SetSceneFocus(*scene_id),
                                ));
                        };
                    }

                    // let rect =
                    //     ui.allocate_exact_size(egui::vec2(180.0, 60.0), egui::Sense::click());
                    // let painter = ui.painter();
                    // let bg_color = if is_selected {
                    //     egui::Color32::from_rgb(60, 120, 200)
                    // } else {
                    //     egui::Color32::from_gray(40)
                    // };
                    // painter.rect_filled(rect.0, 6.0, bg_color);
                    //
                    // // let fixture_count = group.fixtures.len();
                    // painter.text(
                    //     rect.0.left_top() + egui::vec2(12.0, 8.0),
                    //     egui::Align2::LEFT_TOP,
                    //     &name,
                    //     egui::FontId::proportional(12.0),
                    //     egui::Color32::WHITE,
                    // );
                    //
                    // painter.text(
                    //     rect.0.left_center() - egui::vec2(-12.0, 8.0),
                    //     egui::Align2::LEFT_CENTER,
                    //     format!("TODO: overlay or not"),
                    //     egui::FontId::proportional(12.0),
                    //     egui::Color32::GRAY,
                    // );
                    // painter.text(
                    //     rect.0.left_bottom() - egui::vec2(-12.0, 8.0),
                    //     egui::Align2::LEFT_BOTTOM,
                    //     format!("Changes: {}", scene.sink.changeset.len()),
                    //     egui::FontId::proportional(12.0),
                    //     egui::Color32::GRAY,
                    // );

                    // if rect.1.clicked() {}
                    ui.add_space(5.0);
                }
            },
        );
    }
}
