use crate::app::{
    components::{self, ButtonSize, Knob, SpeedKnob, TreeItem},
    BlaulichtApp,
};
use blaulicht_engine::client::EngineClient;
use blaulicht_engine::hub::HostCommand;
use blaulicht_shared::{
    folder::FolderId, scene::BLANK_SCENE_ID, AnimationSpeedModifier, ControlEvent,
    ControlEventMessage, EventOriginator,
};
use egui::{Color32, Context, Margin, Pos2};

const DEFAULT_NEW_VIEW_NAME: &str = "New View";
const VIEWS_PER_PAGE: usize = 5;

#[derive(Default)]
pub struct ViewPerfUI {
    overlay_picker_open: bool,
}

/// What one scene card shows, copied out of the engine state.
struct SceneCard {
    id: u8,
    name: String,
    master_alpha: u8,
    master_speed: AnimationSpeedModifier,
    /// False for the exclusive live-mode card: that scene is the render
    /// base, not an overlay, so there is nothing to remove.
    removable: bool,
}

const VIEW_PERFORMANCE_SCENE_WIDTH: f32 = 128.0;
const VIEW_PERFORMANCE_SCENE_HEIGHT: f32 = 82.0;

impl BlaulichtApp {
    fn draw_circle(ui: &mut egui::Ui, flag: bool) {
        // Decide color
        let color = if flag { Color32::GREEN } else { Color32::RED };

        let rect = ui.min_rect(); // content rect
        let painter = ui.painter();

        // Decide circle size
        let radius = 4.0;

        // Choose which corner: top-right
        let pos = Pos2::new(rect.right() - radius - 2.0, rect.top() + radius + 2.0);

        // Draw circle
        painter.circle_filled(pos, radius, color);
    }

    fn render_scene_performance(&mut self, ui: &mut egui::Ui, scene: &SceneCard) {
        let removable = scene.removable;
        egui::Frame::NONE
            .fill(ui.visuals().widgets.active.bg_fill)
            .inner_margin(Margin::symmetric(5, 5))
            .show(ui, |ui| {
                ui.set_width(VIEW_PERFORMANCE_SCENE_WIDTH);
                ui.set_height(VIEW_PERFORMANCE_SCENE_HEIGHT);

                ui.vertical_centered(|ui| {
                    ui.set_width(ui.available_width());

                    let label = format!("{} ({})", scene.name, scene.id);
                    ui.label(label);

                    let master_alpha = scene.master_alpha;
                    Self::draw_circle(ui, master_alpha > 0);

                    ui.add_space(5.0);

                    ui.columns(3, |columns| {
                        columns[0].vertical_centered(|ui| {
                            ui.add_space(5.0);
                            let mut master_alpha = scene.master_alpha as f32;
                            if ui.add(Knob::new(&mut master_alpha, 0.0..=100.0)).changed() {
                                self.client.send(ControlEventMessage::new(
                                    EventOriginator::Web,
                                    ControlEvent::SetSceneMasterAlpha(scene.id, master_alpha as u8),
                                ));
                            }
                        });

                        columns[1].vertical_centered(|ui| {
                            let rst_enabled = scene.master_alpha != 100
                                || scene.master_speed != AnimationSpeedModifier::_1;

                            if components::action_button(
                                ui,
                                rst_enabled,
                                "RST",
                                ButtonSize::Small.with_width(34.0),
                                Some("Scene level and speed are already at their defaults"),
                            ) {
                                self.client.send(ControlEventMessage::new(
                                    EventOriginator::Web,
                                    ControlEvent::Transaction(vec![
                                        ControlEvent::SetSceneMasterSpeed(
                                            scene.id,
                                            AnimationSpeedModifier::_1,
                                        ),
                                        ControlEvent::SetSceneMasterAlpha(scene.id, 100),
                                    ]),
                                ));
                            }

                            ui.add_space(5.0);

                            let is_off = scene.master_alpha == 0;

                            if components::button(
                                ui,
                                !is_off,
                                "OFF",
                                ButtonSize::Small.with_width(34.0),
                            ) {
                                self.client.send(ControlEventMessage::new(
                                    EventOriginator::Web,
                                    ControlEvent::SetSceneMasterAlpha(scene.id, 0),
                                ));
                            }

                            ui.add_space(5.0);

                            if components::action_button(
                                ui,
                                removable,
                                "DEL",
                                ButtonSize::Small.with_width(34.0),
                                Some("The live preview scene is not an overlay"),
                            ) {
                                self.client.send(ControlEventMessage::new(
                                    EventOriginator::Web,
                                    ControlEvent::RemoveOverlayScene(scene.id),
                                ));
                            }
                        });

                        columns[2].vertical_centered(|ui| {
                            ui.add_space(5.0);
                            let mut speed = scene.master_speed;
                            if ui.add(SpeedKnob::new(&mut speed)).changed() {
                                self.client.send(ControlEventMessage::new(
                                    EventOriginator::Web,
                                    ControlEvent::SetSceneMasterSpeed(scene.id, speed),
                                ));
                            }
                        });
                    });

                    // ui.horizontal_centered(|ui| {
                    //     ui.add_space(25.0);
                    // });
                });
            });
    }

    /// The overlay picker: the scene tree with only the scenes that are not
    /// overlays yet (and only folders holding one of them).
    fn render_view_perf_overlay_picker_dialog(
        &mut self,
        ctx: &Context,
        available_scenes: &[TreeItem<FolderId, u8>],
    ) {
        if !self.view_perf_ui_state.overlay_picker_open {
            return;
        }

        if available_scenes.is_empty() {
            self.view_perf_ui_state.overlay_picker_open = false;
            return;
        }

        if let Some(scene_id) = components::tree_picker_dialog(
            ctx,
            "Select Overlay Scene",
            components::SCENE_PICKER_TREE,
            available_scenes,
            None,
            "No more scenes to add",
            &mut self.view_perf_ui_state.overlay_picker_open,
        ) {
            self.client.send(ControlEventMessage::new(
                EventOriginator::Web,
                ControlEvent::AddOverlayScene(scene_id),
            ));
        }
    }

    pub fn view_perf_ui(
        &mut self,
        ui: &mut egui::Ui,
        ctx: &Context,
        render_context: crate::app::page::PageRenderContext,
    ) {
        // Live mode is exclusive: only the scene being previewed (the preview
        // owner's focus, which need not be this frontend's) renders, and it
        // occupies the base slot rather than the overlay stack.
        let (live_mode, cards, available_overlay_scenes) = self.client.with_show(|show| {
            let live_mode = show.0.live_mode;
            let shown: Vec<(u8, bool)> = if live_mode {
                vec![(show.render_base_scene(), false)]
            } else {
                show.0
                    .current_overlay_scenes
                    .iter()
                    .map(|id| (*id, true))
                    .collect()
            };
            let cards: Vec<SceneCard> = shown
                .into_iter()
                .filter_map(|(id, removable)| {
                    let scene = show.get_scene(id)?;
                    Some(SceneCard {
                        id,
                        name: scene.name.clone(),
                        master_alpha: scene.sink.master_alpha_fader,
                        master_speed: scene.sink.master_speed,
                        removable,
                    })
                })
                .collect();
            let overlays = &show.0.current_overlay_scenes;
            let available =
                components::prune_empty_folders(components::scene_tree_items(&show.0, |id| {
                    id != BLANK_SCENE_ID && !overlays.contains(&id)
                }));
            (live_mode, cards, available)
        });
        let grand_master = self.client.grand_master_percent();

        if self.dmx_simulator.open {
            let groups = self.client.with_show(|show| show.groups().clone());
            self.render_dmx_simulation_dialog(ctx, &groups);
        }

        ui.allocate_ui_with_layout(
            egui::vec2(ui.available_width(), ui.available_height()),
            egui::Layout::top_down(egui::Align::Min),
            |ui| {
                render_context.horizontal(ui, egui::Align::Min, |ui| {
                    let can_add_overlay = !available_overlay_scenes.is_empty();
                    if components::action_button(
                        ui,
                        can_add_overlay,
                        "ADD OVERLAY",
                        ButtonSize::Medium,
                        Some("No additional scenes are available as overlays"),
                    ) {
                        self.view_perf_ui_state.overlay_picker_open = true;
                    }

                    components::toolbar_separator(ui, ButtonSize::Medium.dim().0.y);
                    ui.label("Grand master");
                    let mut grand_master_value = grand_master as i32;
                    if ui
                        .add_sized(
                            [180.0, 20.0],
                            egui::Slider::new(&mut grand_master_value, 0..=100).suffix("%"),
                        )
                        .changed()
                    {
                        self.client
                            .send(HostCommand::SetGrandMaster(grand_master_value as u8));
                    }
                    if components::action_button(
                        ui,
                        grand_master != 100,
                        "RESET",
                        ButtonSize::Medium,
                        Some("Grand master is already at 100%"),
                    ) {
                        self.client.send(HostCommand::SetGrandMaster(100));
                    }
                    if components::button(ui, grand_master == 0, "BLACKOUT", ButtonSize::Medium) {
                        self.client.send(HostCommand::SetGrandMaster(if grand_master == 0 {
                            100
                        } else {
                            0
                        }));
                    }
                });

                if live_mode {
                    ui.add_space(2.0);
                    ui.label(
                        egui::RichText::new(
                            "LIVE - exclusive preview: overlays are suppressed until it is turned off",
                        )
                        .color(egui::Color32::RED)
                        .strong(),
                    );
                }

                ui.separator();

                // 1. Define your card width (adjust to match your frame content)
                let padding = 10.0;
                let total_card_width = VIEW_PERFORMANCE_SCENE_WIDTH + padding;

                // 2. Calculate how many columns fit in the current window width
                //    We use a minimal check (max(1)) to prevent division by zero or 0 columns.
                let available_width = if render_context.is_dynamic() {
                    ui.available_width()
                } else {
                    ui.available_width() - 200.0
                };
                let cols = (available_width / total_card_width).floor().max(1.0) as usize;

                ui.allocate_ui_with_layout(
                    egui::vec2(ui.available_width(), ui.available_height()),
                    egui::Layout::left_to_right(egui::Align::Min),
                    |ui| {
                        // 3. Use a Grid, which handles alignment perfectly
                        egui::Frame::NONE
                            // .fill(egui::Color32::GREEN) // Debug color
                            .inner_margin(0.0) // Set to 0.0 to see exact Grid boundaries
                            .show(ui, |ui| {
                                egui::Grid::new("scene_grid")
                                    .spacing(egui::vec2(padding, padding)) // Space between cards
                                    .show(ui, |ui| {
                                        if !render_context.is_dynamic() {
                                            ui.set_min_width(500.0);
                                        }
                                        for (i, card) in cards.iter().enumerate() {
                                            if i > 0 && i % cols == 0 {
                                                ui.end_row();
                                            }

                                            // Key each card's widget ids on its
                                            // scene rather than its position in
                                            // the list -- live mode changes that
                                            // list's length, and positional ids
                                            // would migrate between cards.
                                            ui.push_id(card.id, |ui| {
                                                self.render_scene_performance(ui, card);
                                            });
                                        }
                                    });
                            });
                    },
                );
            },
        );

        self.render_view_perf_overlay_picker_dialog(ctx, &available_overlay_scenes);
    }
}
