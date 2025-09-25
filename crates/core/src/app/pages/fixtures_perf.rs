use crate::app::{
    components::{self, ButtonSize},
    BlaulichtApp,
};
use blaulicht_shared::{ControlEvent, ControlEventMessage, EventOriginator};
use egui::{Color32, Context, RichText};

impl BlaulichtApp {
    pub fn fixtures_ui(&mut self, ui: &mut egui::Ui, ctx: &Context) {
        let dmx_engine = { self.data.state.dmx_engine.read().unwrap().clone() };
        let groups = dmx_engine.groups();

        //
        // Dialogs start.
        //
        self.render_dmx_simulation_dialog(ctx, groups);
        self.render_scene_animations_dialog(ctx, &dmx_engine);

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

                            ui.separator();

                            // if components::button(
                            //     ui,
                            //     self.show_dmx_simulation,
                            //     "Show DMX",
                            //     ButtonSize::Medium,
                            // ) {
                            //     self.show_dmx_simulation = !self.show_dmx_simulation;
                            // }
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

                                                ui.label(
                                                    RichText::new(format!(
                                                        "Selection: {selection:?}"
                                                    ))
                                                    .color(Color32::LIGHT_GREEN),
                                                );

                                                for (animation_id, animation) in animations {
                                                    let spec = dmx_engine
                                                        .animations
                                                        .get(animation_id)
                                                        .unwrap();

                                                    ui.label(
                                                        RichText::new(format!(
                                                            "[{}] {} | {}",
                                                            animation_id, spec.name, spec.property
                                                        ))
                                                        .color(Color32::WHITE),
                                                    );

                                                    // Remove button
                                                    if components::button(
                                                        ui,
                                                        false,
                                                        "Remove",
                                                        ButtonSize::Medium,
                                                    ) {
                                                        let mut selection_instructions =
                                                            selection.generate_instructions();

                                                        selection_instructions.push_front(
                                                            ControlEvent::PushSelection,
                                                        );
                                                        selection_instructions.push_back(
                                                            ControlEvent::RemoveAnimation(
                                                                *animation_id,
                                                            ),
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
                                                        let mut selection_instructions =
                                                            selection.generate_instructions();

                                                        selection_instructions.push_front(
                                                            ControlEvent::PushSelection,
                                                        );
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
            },
        );
    }
}
