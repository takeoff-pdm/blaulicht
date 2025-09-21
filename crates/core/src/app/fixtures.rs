use std::{
    collections::{BTreeMap, HashSet},
    mem,
    sync::RwLockReadGuard,
};

use blaulicht_shared::{
    AnimationSpeedModifier, ControlEvent, ControlEventMessage, EventOriginator, FixtureProperty,
    RGBColor,
};
use egui::{
    Align2, Button, Color32, Context, FontId, Frame, Margin, RichText, TextBuffer, TextEdit, Vec2,
};

use crate::{
    app::{
        ui::{button, dialog, ButtonSize},
        BlaulichtApp, Selection,
    },
    dmx::{self, EngineGroups, EngineSelection, EngineState, FixtureState},
    event::SystemEventBusConnectionInst,
    routes::DmxBuffer,
};

pub const DEFAULT_NEW_SCENE_NAME: &str = "My Scene";

impl BlaulichtApp {
    fn scene_overview(&mut self, ui: &mut egui::Ui, ctx: &Context, dmx_engine: &EngineState) {
        ui.horizontal(|ui| {
            if button(ui, false, "Scene +", ButtonSize::Medium) {
                self.new_scene_dialog_open = true;
            }

            if self.new_scene_dialog_open {
                const BUTTON_SIZE: ButtonSize = ButtonSize::Large;
                const SPACING: f32 = 16.0;

                let size = egui::vec2(200.0, BUTTON_SIZE.dim().0.y * 2.0 + SPACING);
                dialog(ctx, "Create Scene", size, false, |ui| {
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

                    if button(ui, false, "OK", BUTTON_SIZE) {
                        let mut dmx_engine = self.data.state.dmx_engine.write().unwrap();
                        dmx_engine.new_scene(self.new_scene_name.take());
                        self.new_scene_name = DEFAULT_NEW_SCENE_NAME.to_string();
                        self.new_scene_dialog_open = false;
                    }
                });
            }

            // // TODO: limit width. OR use popups for creation stuff.
            // ui.vertical(|ui| {});

            for (scene_id, scene) in &dmx_engine.scenes {
                let is_selected = dmx_engine.current_scene_focus == *scene_id;

                let rect = ui.allocate_exact_size(egui::vec2(180.0, 60.0), egui::Sense::click());
                let painter = ui.painter();
                let bg_color = if is_selected {
                    egui::Color32::from_rgb(60, 120, 200)
                } else {
                    egui::Color32::from_gray(40)
                };
                painter.rect_filled(rect.0, 6.0, bg_color);

                // let fixture_count = group.fixtures.len();
                let name = format!("Scene {scene_id}");
                painter.text(
                    rect.0.left_top() + egui::vec2(12.0, 8.0),
                    egui::Align2::LEFT_TOP,
                    &name,
                    egui::FontId::proportional(16.0),
                    egui::Color32::WHITE,
                );
                // painter.text(
                //     rect.0.left_center() - egui::vec2(-12.0, 8.0),
                //     egui::Align2::LEFT_CENTER,
                //     format!("TODO: overlay or not"),
                //     egui::FontId::proportional(12.0),
                //     egui::Color32::GRAY,
                // );
                painter.text(
                    rect.0.left_bottom() - egui::vec2(-12.0, 8.0),
                    egui::Align2::LEFT_BOTTOM,
                    format!("Changes: {}", scene.sink.changeset.len()),
                    egui::FontId::proportional(12.0),
                    egui::Color32::GRAY,
                );

                if rect.1.clicked() {
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
                ui.add_space(8.0);
            }
        });
    }

    fn fixture_selection(
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

        // let group_count = groups.len();
        // let mut selected_group = self.selected_fixture_group;
        // Layout: left (groups), right (fixtures if one group selected)

        // let mut selected_groups = vec![];

        ui.horizontal(|ui| {
            ui.set_min_height(ui.available_height());
            // ui.horizontal(|ui| {
            // Left: groups list
            ui.vertical(|ui| {
                ui.label("Groups:");
                ui.add_space(8.0);
                for (group_id, group) in groups.iter() {
                    let is_selected = selection.group_ids.contains(group_id);

                    // if is_selected {
                    //     selected_groups.push(group_id);
                    // }

                    let rect =
                        ui.allocate_exact_size(egui::vec2(180.0, 48.0), egui::Sense::click());
                    let painter = ui.painter();
                    let bg_color = if is_selected {
                        egui::Color32::from_rgb(60, 120, 200)
                    } else {
                        egui::Color32::from_gray(40)
                    };
                    painter.rect_filled(rect.0, 6.0, bg_color);
                    let fixture_count = group.fixtures.len();
                    let name = format!("Group {}", group_id);
                    painter.text(
                        rect.0.left_top() + egui::vec2(12.0, 8.0),
                        egui::Align2::LEFT_TOP,
                        &name,
                        egui::FontId::proportional(16.0),
                        egui::Color32::WHITE,
                    );
                    painter.text(
                        rect.0.left_bottom() - egui::vec2(-12.0, 8.0),
                        egui::Align2::LEFT_BOTTOM,
                        format!("{} fixtures", fixture_count),
                        egui::FontId::proportional(12.0),
                        egui::Color32::GRAY,
                    );
                    if rect.1.clicked() {
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
                    ui.add_space(8.0);
                }
                // self.selected_fixture_group = selected_group;
            });

            // Right: fixtures in selected group
            // Get selection info
            // Layout: left (fixtures), right (controls)

            ui.horizontal(|ui| {
                // Fixtures list
                if !selection.group_ids.is_empty() {
                    ui.vertical(|ui| {
                        ui.label(format!("Fixtures in Group"));
                        ui.add_space(8.0);

                        for (group_id, fix_id, fixture_selection) in &total_fixtures {
                            // let is_selected =
                            //      highlight_fixtures.contains(fix_id);
                            let fixture = groups
                                .get(&group_id)
                                .unwrap()
                                .fixtures
                                .get(&fix_id)
                                .unwrap();

                            let fix_rect = ui
                                .allocate_exact_size(egui::vec2(180.0, 36.0), egui::Sense::click());
                            let painter = ui.painter();

                            let bg = fixture_selection.color();

                            painter.rect_filled(fix_rect.0, 4.0, bg);
                            painter.text(
                                fix_rect.0.left_center() + egui::vec2(12.0, 0.0),
                                egui::Align2::LEFT_CENTER,
                                &fixture.name,
                                egui::FontId::proportional(14.0),
                                egui::Color32::WHITE,
                            );
                            painter.text(
                                fix_rect.0.left_center() + egui::vec2(12.0, 12.0),
                                egui::Align2::LEFT_CENTER,
                                format!("{}", fixture.type_),
                                egui::FontId::proportional(14.0),
                                egui::Color32::WHITE,
                            );

                            // Click to toggle fixture selection if exactly one group is selected
                            if fix_rect.1.clicked()
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
                        }
                    });
                } else {
                    let _ = ui.allocate_exact_size(egui::vec2(180.0, 36.0), egui::Sense::empty());
                }
                // }
            });
        });

        {
            // let selection = .selection();
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

    pub fn fixtures_ui(&mut self, ui: &mut egui::Ui, ctx: &Context) {
        ui.label("Fixtures");

        ui.separator();

        let dmx_engine = { self.data.state.dmx_engine.read().unwrap().clone() };
        let groups = dmx_engine.groups();

        // ui.allocate_ui(ve ui.available_height(), |ui| {
        //
        // });
        //

        ui.allocate_ui_with_layout(
            egui::vec2(ui.available_width(), ui.available_height()), // fixed width, max height
            egui::Layout::top_down(egui::Align::Min),
            |ui| {
                self.scene_overview(ui, ctx, &dmx_engine);

                // Scene overview

                ui.separator();

                ui.allocate_ui_with_layout(
                    egui::vec2(ui.available_width(), ui.available_height()), // fixed width, max height
                    egui::Layout::left_to_right(egui::Align::Min),
                    |ui| {
                        self.fixture_selection(groups, ui);

                        // TODO: Show animation groups.

                        ui.separator();

                        ui.vertical(|ui| {
                            if button(ui, self.show_dmx_simulation, "Show DMX", ButtonSize::Medium)
                            {
                                self.show_dmx_simulation = !self.show_dmx_simulation;
                            }

                            if self.show_dmx_simulation {
                                dialog(ctx, "DMX Output", egui::vec2(500.0, 500.0), true, |ui| {
                                    let dmx_buffer = self.data.state.dmx_buffer.read().unwrap();
                                    simulate_dmx(ui, groups, dmx_buffer);
                                });
                            }

                            ui.separator();

                            // Scene stats.
                            //

                            ui.horizontal(|ui| {
                                let scene = dmx_engine.curr_scene();

                                ui.vertical(|ui| {
                                    ui.label("Scene Changeset");

                                    let mut changeset_organized: BTreeMap<
                                        (u8, u8),
                                        Vec<FixtureProperty>,
                                    > = BTreeMap::new();

                                    for change in &scene.sink.changeset {
                                        match changeset_organized.get_mut(&(change.gid, change.fid))
                                        {
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

                                ui.separator();

                                ui.vertical(|ui| {
                                    for (selection, animations) in &scene.sink.active_animations {
                                        ui.label(
                                            RichText::new(format!("Selection: {selection:?}"))
                                                .color(Color32::LIGHT_GREEN),
                                        );

                                        for (animation_id, animation) in animations {
                                            let spec =
                                                dmx_engine.animations.get(animation_id).unwrap();

                                            ui.label(
                                                RichText::new(format!(
                                                    "[{}] {} | {}",
                                                    animation_id, spec.name, spec.property
                                                ))
                                                .color(Color32::WHITE),
                                            );

                                            // Remove button
                                            if button(ui, false, "Remove", ButtonSize::Medium) {
                                                let mut selection_instructions =
                                                    selection.generate_instructions();

                                                selection_instructions
                                                    .push_front(ControlEvent::PushSelection);
                                                selection_instructions.push_back(
                                                    ControlEvent::RemoveAnimation(*animation_id),
                                                );
                                                selection_instructions
                                                    .push_back(ControlEvent::PopSelection);

                                                self.data.event_bus_connection.send(
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

                                            let (label, enabled, event) = match animation.enabled {
                                                true => (
                                                    "Pause",
                                                    true,
                                                    ControlEvent::PauseAnimation(*animation_id),
                                                ),
                                                false => (
                                                    "Play",
                                                    false,
                                                    ControlEvent::PlayAnimation(*animation_id),
                                                ),
                                            };

                                            if button(ui, enabled, label, ButtonSize::Medium) {
                                                let mut selection_instructions =
                                                    selection.generate_instructions();

                                                selection_instructions
                                                    .push_front(ControlEvent::PushSelection);
                                                selection_instructions.push_back(event);
                                                selection_instructions
                                                    .push_back(ControlEvent::PopSelection);

                                                self.data.event_bus_connection.send(
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
                                        }
                                    }
                                });
                            });
                        });
                    },
                );
            },
        );
    }

    fn animation_groups(
        ui: &mut egui::Ui,
        animations: &[u8],
        event_bus_connection: SystemEventBusConnectionInst,
    ) {
        todo!("animations")
        // // List currently applied animations
        // ui.label(format!("Animations len {}", buf.animations.len()));
        //
        // for (applied_id, applied) in buf.animations.iter() {
        //     ui.horizontal(|ui| {
        //         ui.label(format!("Animation {}", applied.id));
        //         // Speed factor knob/slider
        //         // let speeds = AnimationSpeedModifier::iter();
        //
        //         let mut index = applied.speed_factor.as_index();
        //         let max_index = AnimationSpeedModifier::ALL.len() - 1;
        //
        //         ui.label(format!("Selected: {}", applied.speed_factor.as_str()));
        //         if ui
        //             .add(egui::Slider::new(&mut index, 0..=max_index).text("Enum"))
        //             .changed()
        //         {
        //             // applied.speed_factor = AnimationSpeedModifier::from_index(index);
        //             event_bus_connection.send(ControlEventMessage::new(
        //                 EventOriginator::Web,
        //                 ControlEvent::SetAnimationSpeed(
        //                     *applied_id,
        //                     AnimationSpeedModifier::from_index(index),
        //                 ),
        //             ));
        //         }
        //
        //     });
        // }
    }

    fn fixture_controls(
        &self,
        ui: &mut egui::Ui,
        buf: &FixtureState,
        event_bus_connection: SystemEventBusConnectionInst,
        animations: &[u8],
    ) {
        Frame::new()
            .fill(Color32::from_rgb(50, 50, 50))
            .show(ui, |ui| {
                ui.set_max_width(200.0);
                ui.vertical(|ui| {
                    ui.label("Fixture Controls");
                    ui.add_space(8.0);

                    // Brightness slider
                    {
                        let mut brightness = buf.alpha as u32;
                        if ui
                            .add(egui::Slider::new(&mut brightness, 0..=255).text("Brightness"))
                            .changed()
                        {
                            event_bus_connection.send(ControlEventMessage::new(
                                EventOriginator::Web,
                                ControlEvent::SetAlpha(brightness as u8),
                            ));
                        }
                    }

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

                    // --- Animation Controls ---
                    ui.separator();
                    ui.label("Animations");

                    // TODO: Animations come to a different 'window / tab'
                    //
                    // Add Animation Selection: Prettier fixed-height boxes
                    ui.label("Add Animation:");
                    ui.add_space(4.0);
                    let mut selected_anim: Option<u8> = None;
                    egui::ScrollArea::vertical()
                        .max_height(120.0)
                        .show(ui, |ui| {
                            for anim_id in animations {
                                // Draw custom box background
                                let rect = ui.allocate_exact_size(
                                    egui::vec2(150.0, 36.0),
                                    egui::Sense::hover(),
                                );
                                let painter = ui.painter();
                                let bg_color = Color32::from_rgb(70, 70, 120);
                                painter.rect_filled(rect.0, 6.0, bg_color);
                                // Overlay input element for accessibility and keyboard navigation
                                let response = ui.put(
                                    rect.0,
                                    Button::selectable(false, format!("Animation {}", anim_id)),
                                );
                                if response.clicked() {
                                    selected_anim = Some(*anim_id);
                                }
                                ui.add_space(4.0);
                            }
                        });

                    if let Some(anim_id) = selected_anim {
                        event_bus_connection.send(ControlEventMessage::new(
                            EventOriginator::Web,
                            ControlEvent::AddAnimation(anim_id),
                        ));
                    }
                });
            });
    }
}

fn simulate_dmx(ui: &mut egui::Ui, _groups: &EngineGroups, dmx: RwLockReadGuard<'_, DmxBuffer>) {
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
                    1..=85 => (Color32::from_rgb(255, 0, 0), Color32::MAGENTA),
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
                            size: 5.0,
                            family: egui::FontFamily::Monospace,
                        },
                        fg_color,
                    );
                }
            }
        }
    });
}
