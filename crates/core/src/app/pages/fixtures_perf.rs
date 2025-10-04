use crate::{
    app::{
        components::{self, ButtonSize},
        BlaulichtApp,
    },
    dmx::EngineState,
};
use blaulicht_shared::{
    AnimationSpecBody, AnimationSpeedModifier, ControlEvent, ControlEventMessage, EventOriginator,
    PhaserDuration,
};
use egui::{Color32, Context, RichText};

impl BlaulichtApp {
    pub fn render_add_animations_dialog(&mut self, ctx: &Context, dmx_engine: &EngineState) {
        if self.add_animations_dialog_open {
            const HEIGHT: f32 = 430.0;
            const WIDTH: f32 = 500.0;

            components::dialog(
                ctx,
                "Add Scene Animation",
                egui::vec2(WIDTH, HEIGHT),
                true,
                |ui| {
                    // let scene = dmx_engine.curr_scene();
                    ui.separator();

                    ui.set_min_height(HEIGHT - 100.0);

                    // let mut selected_anim: Option<u8> = None;

                    egui::ScrollArea::vertical().show(ui, |ui| {
                        ui.set_min_height(HEIGHT - 100.0);
                        // Add Animation Selection: Prettier fixed-height boxes
                        ui.label("Add Animation:");
                        ui.add_space(4.0);
                        egui::ScrollArea::vertical()
                            .max_height(HEIGHT - 100.0)
                            .show(ui, |ui| {
                                for (anim_id, anim) in &dmx_engine.0.animations {
                                    if components::button(
                                        ui,
                                        self.add_selected_animation == Some(*anim_id),
                                        &format!("#{} {}", anim_id, anim.name),
                                        ButtonSize::Medium.with_width(300.0),
                                    ) {
                                        self.add_selected_animation = Some(*anim_id);
                                    }
                                    ui.add_space(4.0);
                                }
                            });
                    });

                    ui.separator();

                    ui.allocate_ui_with_layout(
                        egui::vec2(300.0, 20.0),
                        egui::Layout::left_to_right(egui::Align::Center),
                        |ui| {
                            ui.set_width(300.0);

                            if components::button(
                                ui,
                                false,
                                "Cancel",
                                ButtonSize::Medium.with_width(140.0),
                            ) {
                                self.add_animations_dialog_open = false;
                                self.add_selected_animation = None;
                            }
                            ui.add_space(4.0);

                            if components::button(
                                ui,
                                true,
                                "Add",
                                ButtonSize::Medium.with_width(140.0),
                            ) {
                                if let Some(anim_id) = self.add_selected_animation {
                                    self.data
                                        .event_bus_connection
                                        .send(ControlEventMessage::new(
                                            EventOriginator::Web,
                                            ControlEvent::AddAnimation(anim_id),
                                        ));
                                    self.add_animations_dialog_open = false;
                                    self.add_selected_animation = None;
                                }
                            }
                        },
                    );
                },
            );
        }
    }

    pub fn fixtures_ui(&mut self, ui: &mut egui::Ui, ctx: &Context) {
        let dmx_engine = { self.data.state.dmx_engine.read().unwrap().clone() };
        let groups = dmx_engine.groups();

        //
        // Dialogs start.
        //
        self.render_dmx_simulation_dialog(ctx, groups);
        self.render_scene_animations_dialog(ctx, &dmx_engine);
        self.render_add_animations_dialog(ctx, &dmx_engine);

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
                            if components::button(
                                ui,
                                self.current_scene_animations_dialog_open,
                                "Scene Animations",
                                ButtonSize::Medium,
                            ) {
                                self.current_scene_animations_dialog_open =
                                    !self.current_scene_animations_dialog_open;
                            }

                            if components::button(
                                ui,
                                self.add_animations_dialog_open,
                                "Add Animations",
                                ButtonSize::Medium,
                            ) {
                                self.add_animations_dialog_open =
                                    !self.add_animations_dialog_open;
                            }

                            if components::button(
                                ui,
                                !dmx_engine.selection().fixtures_in_group.is_empty(),
                                "UnLmt",
                                ButtonSize::Medium,
                            ) {
                                if dmx_engine.selection().group_ids.len() == 1 {
                                    let curr_group = *dmx_engine.selection().group_ids.iter().next().unwrap();

                                let instr = vec![
                                    ControlEvent::RemoveAllSelection,
                                    ControlEvent::SelectGroup(curr_group),
                                ];

                                self.data.event_bus_connection.send(ControlEventMessage::new(EventOriginator::Web, 
                                    ControlEvent::Transaction(instr)));

                                // self.data.event_bus_connection.send(ControlEventMessage::new(EventOriginator::Web, 
                                //     ControlEvent::Transaction(vec![
                                //         ControlEvent::RemoveAllSelection,
                                //         ControlEvent::SelectGroup(curr_group),
                                //     ])));
                                //     
                                }
                            }

                            if components::button(
                                ui,
                                !dmx_engine.selection().fixtures_in_group.is_empty(),
                                "Lmt All",
                                ButtonSize::Medium,
                            ) {
                                if dmx_engine.selection().group_ids.len() == 1 {
                                    let curr_group = *dmx_engine.selection().group_ids.iter().next().unwrap();

                                    if dmx_engine.selection().fixtures_in_group.is_empty() {
                                        let mut instr = vec![
                                            ControlEvent::RemoveAllSelection,
                                            ControlEvent::SelectGroup(curr_group),
                                        ];

                                        for (fix_id, _) in &dmx_engine.groups().get(&curr_group).unwrap().fixtures {
                                            instr.push(ControlEvent::LimitSelectionToFixtureInCurrentGroup(*fix_id));
                                        }

                                        self.data.event_bus_connection.send(ControlEventMessage::new(EventOriginator::Web, 
                                            ControlEvent::Transaction(instr)));
                                    }
                                    else {
                                        let mut instr = vec![
                                            ControlEvent::RemoveAllSelection,
                                            ControlEvent::SelectGroup(curr_group),
                                        ];

                                        let selec = dmx_engine.selection().clone();

                                        for (fix_id, _) in &dmx_engine.groups().get(&curr_group).unwrap().fixtures {
                                            if !selec.fixtures_in_group.contains(fix_id) {
                                                instr.push(ControlEvent::LimitSelectionToFixtureInCurrentGroup(*fix_id));
                                            }
                                        }

                                        self.data.event_bus_connection.send(ControlEventMessage::new(EventOriginator::Web, 
                                            ControlEvent::Transaction(instr)));

                                        // self.data.event_bus_connection.send(ControlEventMessage::new(EventOriginator::Web, 
                                        //     ControlEvent::Transaction(vec![
                                        //         ControlEvent::RemoveAllSelection,
                                        //         ControlEvent::SelectGroup(curr_group),
                                        //     ])));
                                    }
                                }
                            }
                        });

                        ui.separator();

                        ui.allocate_ui_with_layout(
                            egui::vec2(ui.available_width(), ui.available_height()), // fixed width, max height
                            egui::Layout::left_to_right(egui::Align::Min),
                            |ui| {
                                self.fixture_selection(groups, ui);

                                // TODO: Show animation groups.

                                ui.vertical(|ui| {
                                    // Scene stats.
                                    //

                                    ui.horizontal(|ui| {
                                        let scene = dmx_engine.curr_scene();

                                        ui.separator();

                                        ui.vertical(|ui| {
                                            for (selection, animations) in
                                                &scene.sink.active_animations
                                            {
                                                if *selection != dmx_engine.get_selection() {
                                                    continue;
                                                }

                                                // ui.label(
                                                //     RichText::new(format!(
                                                //         "Selection: {selection:?}"
                                                //     ))
                                                //     .color(Color32::LIGHT_GREEN),
                                                // );

                                                for (animation_id, animation) in animations {
                                                    let spec = dmx_engine.0
                                                        .animations
                                                        .get(animation_id)
                                                        .unwrap();

                                                    // Render each animation in its own rectangle/group
                                                    egui::Frame::group(ui.style()).show(ui, |ui| {
                                                        ui.set_width(ButtonSize::Medium.dim().0.x);
                                                        ui.vertical(|ui| {
                                                            // LEFT: Info (name + property + timing)
                                                            let mut label = spec.name.clone();
                                                            label.truncate(10);

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
}
