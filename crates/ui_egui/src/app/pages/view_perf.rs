use crate::app::components::studio;
use crate::app::pages::programmer_desktop::exclusive_preview_button;
use crate::app::ui_tokens::{
    studio as studio_tokens, Density, Weight, PAGE_BAR_HEIGHT, TARGET_SHOW, TEXT,
};
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
use egui::{vec2, Align2, Color32, Context, FontId, Margin, Pos2, Rect, Sense};

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

    /// The desktop page bar (`pages.html#performance`, `.okp > .oktool`):
    /// Add overlay, the Grand master fader with its value and a reset icon,
    /// and Blackout and Exclusive preview on the right.
    fn view_perf_page_bar(
        &mut self,
        ui: &mut egui::Ui,
        can_add_overlay: bool,
        grand_master: u8,
        preview_owned: bool,
        preview_any: bool,
    ) {
        let t = studio_tokens(ui.ctx());
        let mut add = false;
        let mut level: Option<u8> = None;
        let mut reset = false;
        let mut blackout = false;
        let mut preview = false;
        studio::page_bar(
            ui,
            |ui| {
                ui.spacing_mut().item_spacing.x = 14.0;
                add = ui
                    .add_enabled_ui(can_add_overlay, |ui| {
                        studio::mini_button(ui, "Add overlay", false, TARGET_SHOW)
                    })
                    .inner
                    .on_disabled_hover_text("Every scene is already an overlay")
                    .clicked();
                let (sep, _) = ui.allocate_exact_size(vec2(1.0, 22.0), Sense::hover());
                ui.painter().rect_filled(sep, 0.0, t.line);
                ui.spacing_mut().item_spacing.x = 10.0;
                ui.label(
                    egui::RichText::new("Grand master")
                        .font(Density::of(ui.ctx()).font(Weight::Regular, TEXT))
                        .color(t.muted),
                );
                // While dragging, the fader follows the pointer rather than
                // the engine's echo of the last level sent.
                let drag_id = egui::Id::new("view_perf_gm_drag");
                let mut value = ui
                    .data(|d| d.get_temp::<f32>(drag_id))
                    .unwrap_or(grand_master as f32);
                let style = studio::HFaderStyle {
                    track: vec2(200.0, 28.0),
                    cap: 22.0,
                    value_width: 40.0,
                    suffix: "%",
                    show_name: false,
                    ..Default::default()
                };
                let response =
                    studio::hfader_with(ui, &mut value, 0.0..=100.0, "Grand master", style);
                if response.dragged() {
                    ui.data_mut(|d| d.insert_temp(drag_id, value));
                } else {
                    ui.data_mut(|d| d.remove::<f32>(drag_id));
                }
                if response.changed() {
                    let v = value.round() as u8;
                    if v != grand_master {
                        level = Some(v);
                    }
                }
                ui.add_space(-4.0);
                reset = gm_reset_button(ui, grand_master != 100);
            },
            |ui| {
                ui.spacing_mut().item_spacing.x = 14.0;
                blackout = studio::blackout_button(ui, grand_master == 0).clicked();
                preview = exclusive_preview_button(ui, preview_owned, preview_any);
            },
        );
        if add {
            self.view_perf_ui_state.overlay_picker_open = true;
        }
        if let Some(v) = level {
            self.client.send(HostCommand::SetGrandMaster(v));
        }
        if reset {
            self.client.send(HostCommand::SetGrandMaster(100));
        }
        if blackout {
            self.toggle_blackout();
        }
        if preview {
            self.client.send(ControlEventMessage::new(
                EventOriginator::Web,
                ControlEvent::ToggleLiveMode,
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
        let desktop = self.layout.is_desktop();
        let (preview_owned, preview_any) = self.client.with_show(|show| {
            (
                show.live_owner() == Some(self.client.session()),
                show.0.live_mode,
            )
        });

        if self.dmx_simulator.open {
            let groups = self.client.with_show(|show| show.groups().clone());
            self.render_dmx_simulation_dialog(ctx, &groups);
        }

        if desktop {
            // Desktop (082): the Programmer's 40px page bar, edge to edge
            // over the page frame's margin.
            let full = ui.max_rect().expand(8.0).intersect(ui.clip_rect());
            let bar = Rect::from_min_size(full.min, vec2(full.width(), PAGE_BAR_HEIGHT));
            let mut bar_ui = ui.new_child(
                egui::UiBuilder::new()
                    .max_rect(bar)
                    .layout(egui::Layout::top_down(egui::Align::Min)),
            );
            self.view_perf_page_bar(
                &mut bar_ui,
                !available_overlay_scenes.is_empty(),
                grand_master,
                preview_owned,
                preview_any,
            );
            ui.add_space(bar.bottom() - ui.cursor().top() + 8.0);
        }

        ui.allocate_ui_with_layout(
            egui::vec2(ui.available_width(), ui.available_height()),
            egui::Layout::top_down(egui::Align::Min),
            |ui| {
                if !desktop {
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
                        // Desktop (059): restores the previous level, and
                        // says so when the engine is offline.
                        if self.layout.is_desktop() {
                            self.toggle_blackout();
                        } else {
                            self.client.send(HostCommand::SetGrandMaster(if grand_master == 0 {
                            100
                        } else {
                            0
                        }));
                        }
                    }
                });
                }

                if live_mode && !desktop {
                    ui.add_space(2.0);
                    ui.label(
                        egui::RichText::new(
                            "LIVE - exclusive preview: overlays are suppressed until it is turned off",
                        )
                        .color(egui::Color32::RED)
                        .strong(),
                    );
                }

                if !desktop {
                    ui.separator();
                }

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

/// The Grand master reset icon (`.okgr`): a 28px square, greyed at 100 %.
/// Returns whether it was clicked.
fn gm_reset_button(ui: &mut egui::Ui, enabled: bool) -> bool {
    let t = studio_tokens(ui.ctx());
    let sense = if enabled {
        Sense::click()
    } else {
        Sense::hover()
    };
    let (rect, response) = ui.allocate_exact_size(vec2(TARGET_SHOW, TARGET_SHOW), sense);
    let hovered = enabled && response.hovered();
    let fill = if hovered { t.ctl_hi } else { t.ctl };
    let color = if hovered { t.strong } else { t.text };
    let alpha = if enabled { 1.0 } else { 0.35 };
    let p = ui.painter();
    p.rect_filled(rect, 0.0, fill.gamma_multiply(alpha));
    p.text(
        rect.center(),
        Align2::CENTER_CENTER,
        egui_phosphor::regular::ARROW_COUNTER_CLOCKWISE,
        FontId::proportional(14.0),
        color.gamma_multiply(alpha),
    );
    response.widget_info(|| {
        egui::WidgetInfo::labeled(egui::WidgetType::Button, enabled, "Reset Grand master")
    });
    response
        .on_hover_text(if enabled {
            "Set Grand master to 100%"
        } else {
            "Grand master is already at 100%"
        })
        .clicked()
        && enabled
}
