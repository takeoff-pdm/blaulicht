use crate::{
    app::{
        components::{self, ButtonSize, Dialog, Knob, Pagination, SpeedKnob},
        BlaulichtApp,
    },
    dmx::EngineState,
};
use blaulicht_shared::{view::View, ControlEvent, ControlEventMessage, EventOriginator};
use egui::{Context, FontId, Frame, Key, Margin, RichText, TextEdit, Vec2};

const DEFAULT_NEW_VIEW_NAME: &str = "New View";
const VIEWS_PER_PAGE: usize = 5;
const SCENE_NAME_WIDTH: f32 = 150.0;
const SCENE_ROW_HEIGHT: f32 = 80.0;

pub struct ViewUI {
    pagination: Pagination,
    add_view_open: bool,
    delete_view_open: bool,
    selected_view_id: Option<u8>,
    delete_view_id: Option<u8>,
    new_view_name: String,
    overlay_picker_open: bool,
    base_picker_open: bool,
    // For both overlay and base
    scene_picker_view_id: Option<u8>,
}

impl Default for ViewUI {
    fn default() -> Self {
        Self {
            pagination: Pagination::default().with_items_per_page(VIEWS_PER_PAGE),
            add_view_open: false,
            delete_view_open: false,
            selected_view_id: None,
            delete_view_id: None,
            new_view_name: DEFAULT_NEW_VIEW_NAME.to_string(),
            overlay_picker_open: false,
            base_picker_open: false,
            scene_picker_view_id: None,
        }
    }
}

impl BlaulichtApp {
    pub fn render_delete_view(&mut self, ctx: &Context) {
        if self.view_ui_state.delete_view_open {
            let Some(view_id) = self.view_ui_state.delete_view_id else {
                self.view_ui_state.delete_view_open = false;
                return;
            };

            Dialog::new("View Deletion".to_string(), egui::vec2(220.0, 120.0))
                .with_backdrop()
                .show(ctx, |ui| {
                    ui.heading(RichText::new("Delete this view?").strong());
                    ui.add_space(12.0);

                    ui.horizontal(|ui| {
                        let confirm = components::button(ui, false, "Confirm", ButtonSize::Large);
                        if confirm {
                            let mut engine = self.data.state.dmx_engine.write().unwrap();

                            if engine.0.views.len() > 1 {
                                engine.0.views.remove(&view_id);
                                // let total_views = engine.0.views.len();
                                let total_pages = self.view_ui_state.pagination.pages();

                                let mut target_page = self.view_ui_state.pagination.current_page();
                                if total_pages == 0 {
                                    target_page = 0;
                                } else if target_page >= total_pages {
                                    target_page = total_pages - 1;
                                }

                                let next = engine
                                    .0
                                    .views
                                    .iter()
                                    .skip(
                                        target_page
                                            * self.view_ui_state.pagination.items_per_page(),
                                    )
                                    .map(|(id, _)| *id)
                                    .next()
                                    .or_else(|| engine.0.views.keys().next().copied());
                                drop(engine);

                                self.view_ui_state.selected_view_id = next;

                                // self.view_ui_state.current_page = target_page;
                            } else {
                                drop(engine);
                            }

                            self.view_ui_state.delete_view_open = false;
                            self.view_ui_state.delete_view_id = None;
                        }

                        if components::button(ui, true, "Cancel", ButtonSize::Large) {
                            self.view_ui_state.delete_view_open = false;
                            self.view_ui_state.delete_view_id = None;
                        }
                    });
                });
        }
    }

    fn render_base_picker_dialog(&mut self, ctx: &Context) {
        if !self.view_ui_state.base_picker_open {
            return;
        }

        let Some(view_id) = self.view_ui_state.scene_picker_view_id else {
            self.view_ui_state.base_picker_open = false;
            return;
        };

        let engine_snapshot = { self.data.state.dmx_engine.read().unwrap().clone() };

        let Some(view) = engine_snapshot.0.views.get(&view_id) else {
            self.view_ui_state.base_picker_open = false;
            self.view_ui_state.scene_picker_view_id = None;
            return;
        };

        let options: Vec<_> = engine_snapshot
            .0
            .scenes
            .iter()
            .filter(|(scene_id, _)| !view.overlays.contains(scene_id))
            .map(|(key, scen)| (key, format!("{key} | {}", scen.name)))
            .collect();

        let current_selection = view.base_scene;

        let (new_id, changed) = components::id_selection_dialog(
            ctx,
            options,
            &current_selection,
            &mut self.view_ui_state.base_picker_open,
            "Base Scene".to_string(),
        );

        if changed {
            self.view_ui_state.scene_picker_view_id = None;
            let mut engine = self.data.state.dmx_engine.write().unwrap();
            if let Some(view) = engine.0.views.get_mut(&view_id) {
                view.base_scene = *new_id;
                view.prune_masters();
            }
        }
    }

    pub fn render_overlay_picker_dialog(&mut self, ctx: &Context) {
        if !self.view_ui_state.overlay_picker_open {
            return;
        }

        let Some(view_id) = self.view_ui_state.scene_picker_view_id else {
            self.view_ui_state.overlay_picker_open = false;
            return;
        };

        let engine_snapshot = { self.data.state.dmx_engine.read().unwrap().clone() };
        let Some(view_snapshot) = engine_snapshot.0.views.get(&view_id) else {
            self.view_ui_state.overlay_picker_open = false;
            self.view_ui_state.scene_picker_view_id = None;
            return;
        };

        let available_scenes: Vec<(u8, String)> = engine_snapshot
            .0
            .scenes
            .iter()
            .filter_map(|(scene_id, scene)| {
                if *scene_id == view_snapshot.base_scene {
                    return None;
                }

                if view_snapshot.overlays.contains(scene_id) {
                    return None;
                }

                Some((*scene_id, format!("{} ({})", scene.name.clone(), scene_id)))
            })
            .collect();

        let (new_id, changed) = components::id_selection_dialog(
            ctx,
            available_scenes,
            255,
            &mut self.view_ui_state.overlay_picker_open,
            "Select Overlay".to_string(),
        );

        if changed {
            self.view_ui_state.scene_picker_view_id = None;
            let mut engine = self.data.state.dmx_engine.write().unwrap();
            if let Some(view) = engine.0.views.get_mut(&view_id) {
                if !view.overlays.contains(&new_id) {
                    view.overlays.push(new_id);
                }
            }

            self.view_ui_state.overlay_picker_open = false;
        }
    }

    pub fn render_add_view_dialog(&mut self, ctx: &Context) {
        if self.view_ui_state.add_view_open {
            const BUTTON_SIZE: ButtonSize = ButtonSize::Large;
            const SPACING: f32 = 16.0;
            let size = egui::vec2(260.0, BUTTON_SIZE.dim().0.y * 2.0 + SPACING + 8.0);

            Dialog::new("Create View".to_string(), size)
                .with_backdrop()
                .show(ctx, |ui| {
                    ui.spacing_mut().interact_size = egui::vec2(44.0, 36.0);
                    Frame::new()
                        .inner_margin(Margin::symmetric(10, 6))
                        .show(ui, |ui| {
                            ui.add(
                                TextEdit::singleline(&mut self.view_ui_state.new_view_name)
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
                        let mut engine = self.data.state.dmx_engine.write().unwrap();

                        let mut new_name = self.view_ui_state.new_view_name.trim().to_string();
                        if new_name.is_empty() {
                            new_name = DEFAULT_NEW_VIEW_NAME.to_string();
                        }

                        let base_scene = engine.0.current_scene_focus;
                        let new_id = (0..=u8::MAX)
                            .find(|candidate| !engine.0.views.contains_key(candidate))
                            .expect("view id overflow");

                        engine
                            .0
                            .views
                            .insert(new_id, View::new(new_name.clone(), base_scene, vec![]));

                        drop(engine);

                        self.view_ui_state.selected_view_id = Some(new_id);
                        // TODO: can we focus this?
                        // self.view_ui_state.current_page = new_page;
                        self.view_ui_state.new_view_name = DEFAULT_NEW_VIEW_NAME.to_string();
                        self.view_ui_state.add_view_open = false;
                    }
                });
        }
    }

    fn view_pagination(&mut self, ui: &mut egui::Ui, dmx_engine: &EngineState, compact: bool) {
        // TODO: can we do this without the allocation?
        let raw_items: Vec<_> = dmx_engine.0.views.iter().collect();
        let paginated_views = self
            .view_ui_state
            .pagination
            .prepare_current_page_items(&raw_items);

        let panel_padding = 2.0;

        let (mut changed, mut selected_id) = (false, None);
        let selected_view_id = self.view_ui_state.selected_view_id;
        let page_width = self.view_ui_state.pagination.width();

        self.view_ui_state
            .pagination
            .ui_responsive(ui, compact, |ui| {
                for (id, view) in paginated_views {
                    let id = **id;

                    let label = format!("{id} | {}", view.name);
                    let is_selected = selected_view_id == Some(id);
                    if components::button(
                        ui,
                        is_selected,
                        &label,
                        ButtonSize::Large
                            .with_width(page_width - 2.0 * panel_padding)
                            .with_font_size(12.0),
                    ) {
                        // Toggle group selection
                        if !is_selected {
                            selected_id = Some(id);
                            changed = true;
                        };
                    }

                    ui.add_space(5.0);
                }
            });

        if changed {
            self.view_ui_state.selected_view_id = selected_id;
        }
    }

    /// One scene line of the view editor: name, master alpha / speed knobs and a
    /// trailing button. Knob edits write straight into the view definition.
    /// Returns whether the trailing button was pressed.
    #[allow(clippy::too_many_arguments)]
    fn scene_row(
        &mut self,
        ui: &mut egui::Ui,
        view_id: u8,
        scene_id: u8,
        scene_name: &str,
        view_snapshot: &View,
        button_label: &str,
        button_selected: bool,
    ) -> bool {
        let mut masters = view_snapshot.masters_for(scene_id);
        let mut pressed = false;
        let mut changed = false;

        ui.horizontal(|ui| {
            ui.set_min_height(SCENE_ROW_HEIGHT);
            ui.allocate_ui_with_layout(
                egui::vec2(SCENE_NAME_WIDTH, SCENE_ROW_HEIGHT),
                egui::Layout::left_to_right(egui::Align::Center),
                |ui| {
                    ui.set_min_size(egui::vec2(SCENE_NAME_WIDTH, SCENE_ROW_HEIGHT));
                    ui.add(egui::Label::new(format!("{scene_name} ({scene_id})")).truncate());
                },
            );

            let mut alpha = masters.master_alpha as f32;
            if ui
                .add(Knob::new(&mut alpha, 0.0..=100.0).with_label("Alpha"))
                .changed()
            {
                masters.master_alpha = alpha.round() as u8;
                changed = true;
            }

            if ui
                .add(SpeedKnob::new(&mut masters.master_speed).with_label("Speed"))
                .changed()
            {
                changed = true;
            }

            ui.add_space(4.0);
            pressed = components::button(ui, button_selected, button_label, ButtonSize::Medium);
        });

        if changed {
            let mut dmx_engine = self.data.state.dmx_engine.write().unwrap();
            if let Some(view) = dmx_engine.0.views.get_mut(&view_id) {
                view.masters.insert(scene_id, masters);
            }
        }

        pressed
    }

    pub fn view_ui(
        &mut self,
        ui: &mut egui::Ui,
        ctx: &Context,
        render_context: crate::app::page::PageRenderContext,
    ) {
        self.render_add_view_dialog(ctx);
        self.render_delete_view(ctx);
        self.render_overlay_picker_dialog(ctx);
        self.render_base_picker_dialog(ctx);

        ui.allocate_ui_with_layout(
            egui::vec2(ui.available_width(), ui.available_height()),
            render_context.primary_layout(),
            |ui| {
                let dmx_engine = { self.data.state.dmx_engine.read().unwrap().clone() };
                self.view_pagination(ui, &dmx_engine, render_context.is_narrow_dynamic());

                ui.separator();

                ui.allocate_ui_with_layout(
                    egui::vec2(ui.available_width(), ui.available_height()),
                    egui::Layout::top_down(egui::Align::Min),
                    |ui| {
                        render_context.horizontal(ui, egui::Align::Min, |ui| {
                            if components::button(ui, false, "Add View", ButtonSize::Medium) {
                                self.view_ui_state.new_view_name =
                                    DEFAULT_NEW_VIEW_NAME.to_string();
                                self.view_ui_state.add_view_open = true;
                            }

                            let can_delete = self.view_ui_state.selected_view_id.is_some()
                                && dmx_engine.0.views.len() > 1;
                            if components::action_button(
                                ui,
                                can_delete,
                                "Delete View",
                                ButtonSize::Medium,
                                Some("Select a view; the final remaining view cannot be deleted"),
                            ) {
                                self.view_ui_state.delete_view_open = true;
                                self.view_ui_state.delete_view_id =
                                    self.view_ui_state.selected_view_id;
                            }
                        });

                        ui.separator();

                        let Some(view_id) = self.view_ui_state.selected_view_id else {
                            return;
                        };

                        let Some(view_snapshot) = dmx_engine.0.views.get(&view_id).cloned() else {
                            self.view_ui_state.scene_picker_view_id = None;
                            self.view_ui_state.base_picker_open = false;
                            self.view_ui_state.overlay_picker_open = false;
                            return;
                        };

                        ui.horizontal(|ui| {
                            ui.vertical(|ui| {
                                ui.horizontal(|ui| {
                                    ui.heading("View Details");
                                    ui.allocate_ui_with_layout(
                                        egui::vec2(ui.available_width(), 0.0),
                                        egui::Layout::right_to_left(egui::Align::Center),
                                        |ui| {
                                            if components::button(
                                                ui,
                                                true,
                                                "Apply",
                                                ButtonSize::Medium,
                                            ) {
                                                self.data.event_bus_connection.send(
                                                    ControlEventMessage::new(
                                                        EventOriginator::Web,
                                                        ControlEvent::Transaction(
                                                            view_snapshot.apply_events(),
                                                        ),
                                                    ),
                                                );
                                            }
                                        },
                                    );
                                });
                                ui.add_space(4.0);

                                ui.label(format!("Name: {}", view_snapshot.name));
                                ui.add_space(4.0);

                                let scene_name = |scene_id: u8| -> String {
                                    dmx_engine
                                        .0
                                        .scenes
                                        .get(&scene_id)
                                        .map(|scene| scene.name.clone())
                                        .unwrap_or_else(|| "Unknown".to_string())
                                };

                                // Base scene row.
                                ui.label(RichText::new("Base Scene").strong());
                                let base_id = view_snapshot.base_scene;
                                let base_action = self.scene_row(
                                    ui,
                                    view_id,
                                    base_id,
                                    &scene_name(base_id),
                                    &view_snapshot,
                                    "Set Base",
                                    true,
                                );
                                if base_action {
                                    self.view_ui_state.base_picker_open = true;
                                    self.view_ui_state.scene_picker_view_id = Some(view_id);
                                }

                                ui.add_space(6.0);

                                // Overlay rows.
                                ui.horizontal(|ui| {
                                    ui.label(RichText::new("Overlay Scenes").strong());

                                    let available_overlay_count = dmx_engine
                                        .0
                                        .scenes
                                        .keys()
                                        .filter(|scene_id| {
                                            **scene_id != view_snapshot.base_scene
                                                && !view_snapshot.overlays.contains(scene_id)
                                        })
                                        .count();
                                    let add_enabled = available_overlay_count > 0;

                                    if components::button(
                                        ui,
                                        add_enabled,
                                        "Add Overlay",
                                        ButtonSize::Medium,
                                    ) && add_enabled
                                    {
                                        self.view_ui_state.overlay_picker_open = true;
                                        self.view_ui_state.scene_picker_view_id = Some(view_id);
                                    }
                                });

                                if view_snapshot.overlays.is_empty() {
                                    ui.label("No overlay scenes.");
                                } else {
                                    for overlay_id in view_snapshot.overlays.iter().copied() {
                                        let remove = self.scene_row(
                                            ui,
                                            view_id,
                                            overlay_id,
                                            &scene_name(overlay_id),
                                            &view_snapshot,
                                            "Remove",
                                            false,
                                        );
                                        if remove {
                                            let mut dmx_engine =
                                                self.data.state.dmx_engine.write().unwrap();
                                            if let Some(view) = dmx_engine.0.views.get_mut(&view_id)
                                            {
                                                view.overlays.retain(|id| *id != overlay_id);
                                                view.prune_masters();
                                            }
                                        }
                                    }
                                }
                            });
                        });

                        ui.separator();
                    },
                );
            },
        );
    }
}
