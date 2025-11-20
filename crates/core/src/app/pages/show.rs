use crate::{
    app::{
        components::{self, ButtonSize},
        BlaulichtApp,
    },
    dmx::EngineState,
};
use blaulicht_shared::{view::View, ControlEvent, ControlEventMessage, EventOriginator};
use egui::{Context, FontId, Frame, Key, Margin, RichText, ScrollArea, TextEdit, Vec2};

const DEFAULT_NEW_VIEW_NAME: &str = "New View";
const VIEW_OVERVIEW_PAGE_SIZE: usize = 12;

pub struct ShowUI {
    add_view_open: bool,
    delete_view_open: bool,
    selected_view_id: Option<u8>,
    delete_view_id: Option<u8>,
    new_view_name: String,
    overlay_picker_open: bool,
    overlay_picker_view_id: Option<u8>,
    view_page: usize,
}

impl Default for ShowUI {
    fn default() -> Self {
        Self {
            add_view_open: false,
            delete_view_open: false,
            selected_view_id: None,
            delete_view_id: None,
            new_view_name: DEFAULT_NEW_VIEW_NAME.to_string(),
            overlay_picker_open: false,
            overlay_picker_view_id: None,
            view_page: 0,
        }
    }
}

impl BlaulichtApp {
    pub fn render_delete_view(&mut self, ctx: &Context) {
        if self.show_ui.delete_view_open {
            let Some(view_id) = self.show_ui.delete_view_id else {
                self.show_ui.delete_view_open = false;
                return;
            };

            components::dialog(
                ctx,
                "View Deletion",
                egui::vec2(220.0, 120.0),
                false,
                |ui| {
                    ui.heading(RichText::new("Delete this view?").strong());
                    ui.add_space(12.0);

                    ui.horizontal(|ui| {
                        let confirm = components::button(ui, false, "Confirm", ButtonSize::Large);
                        if confirm {
                            let mut engine = self.data.state.dmx_engine.write().unwrap();

                            if engine.0.views.len() > 1 {
                                engine.0.views.remove(&view_id);
                                let total_views = engine.0.views.len();
                                let total_pages = if total_views == 0 {
                                    0
                                } else {
                                    (total_views + VIEW_OVERVIEW_PAGE_SIZE - 1)
                                        / VIEW_OVERVIEW_PAGE_SIZE
                                };

                                let mut target_page = self.show_ui.view_page;
                                if total_pages == 0 {
                                    target_page = 0;
                                } else if target_page >= total_pages {
                                    target_page = total_pages - 1;
                                }

                                let next = engine
                                    .0
                                    .views
                                    .iter()
                                    .skip(target_page * VIEW_OVERVIEW_PAGE_SIZE)
                                    .map(|(id, _)| *id)
                                    .next()
                                    .or_else(|| engine.0.views.keys().next().copied());
                                drop(engine);

                                self.show_ui.selected_view_id = next;
                                self.show_ui.view_page = target_page;
                            } else {
                                drop(engine);
                            }

                            self.show_ui.delete_view_open = false;
                            self.show_ui.delete_view_id = None;
                        }

                        if components::button(ui, true, "Cancel", ButtonSize::Large) {
                            self.show_ui.delete_view_open = false;
                            self.show_ui.delete_view_id = None;
                        }
                    });
                },
            );
        }
    }

    pub fn render_overlay_picker_dialog(&mut self, ctx: &Context) {
        if !self.show_ui.overlay_picker_open {
            return;
        }

        let Some(view_id) = self.show_ui.overlay_picker_view_id else {
            self.show_ui.overlay_picker_open = false;
            return;
        };

        let engine_snapshot = { self.data.state.dmx_engine.read().unwrap().clone() };
        let Some(view_snapshot) = engine_snapshot.0.views.get(&view_id) else {
            self.show_ui.overlay_picker_open = false;
            self.show_ui.overlay_picker_view_id = None;
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

                Some((*scene_id, scene.name.clone()))
            })
            .collect();

        components::dialog(
            ctx,
            "Add Overlay Scene",
            egui::vec2(280.0, 260.0),
            false,
            |ui| {
                ui.heading("Select scene to overlay");
                ui.add_space(8.0);

                if available_scenes.is_empty() {
                    ui.label("No additional scenes available.");
                } else {
                    ScrollArea::vertical()
                        .id_source("overlay_picker_scene_scroll")
                        .auto_shrink([false; 2])
                        .show(ui, |ui| {
                            for (scene_id, name) in &available_scenes {
                                let label = format!("{name} ({scene_id})");
                                if components::button(ui, false, &label, ButtonSize::Medium) {
                                    let mut new_page = self.show_ui.view_page;
                                    {
                                        let mut engine =
                                            self.data.state.dmx_engine.write().unwrap();
                                        if let Some(view) = engine.0.views.get_mut(&view_id) {
                                            if !view.overlays.contains(scene_id) {
                                                view.overlays.push(*scene_id);
                                                view.overlays.sort_unstable();
                                            }

                                            if let Some(idx) = engine
                                                .0
                                                .views
                                                .keys()
                                                .position(|vid| *vid == view_id)
                                            {
                                                new_page = idx / VIEW_OVERVIEW_PAGE_SIZE;
                                            }
                                        }
                                    }
                                    self.show_ui.view_page = new_page;

                                    self.show_ui.overlay_picker_open = false;
                                    self.show_ui.overlay_picker_view_id = None;
                                    return;
                                }
                            }
                        });
                }

                ui.add_space(12.0);
                if components::button(ui, true, "Cancel", ButtonSize::Medium) {
                    self.show_ui.overlay_picker_open = false;
                    self.show_ui.overlay_picker_view_id = None;
                }
            },
        );
    }

    pub fn render_add_view_dialog(&mut self, ctx: &Context) {
        if self.show_ui.add_view_open {
            const BUTTON_SIZE: ButtonSize = ButtonSize::Large;
            const SPACING: f32 = 16.0;
            let size = egui::vec2(260.0, BUTTON_SIZE.dim().0.y * 2.0 + SPACING + 8.0);

            components::dialog(ctx, "Create View", size, false, |ui| {
                ui.spacing_mut().interact_size = egui::vec2(44.0, 36.0);
                Frame::new()
                    .inner_margin(Margin::symmetric(10, 6))
                    .show(ui, |ui| {
                        ui.add(
                            TextEdit::singleline(&mut self.show_ui.new_view_name)
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

                    let mut new_name = self.show_ui.new_view_name.trim().to_string();
                    if new_name.is_empty() {
                        new_name = DEFAULT_NEW_VIEW_NAME.to_string();
                    }

                    let base_scene = engine.0.current_scene_focus;
                    let new_id = (0..=u8::MAX)
                        .find(|candidate| !engine.0.views.contains_key(candidate))
                        .expect("view id overflow");

                    engine.0.views.insert(
                        new_id,
                        View {
                            name: new_name.clone(),
                            base_scene,
                            overlays: vec![],
                        },
                    );

                    let new_index = engine
                        .0
                        .views
                        .keys()
                        .position(|id| *id == new_id)
                        .unwrap_or(0);
                    let new_page = new_index / VIEW_OVERVIEW_PAGE_SIZE;

                    drop(engine);

                    self.show_ui.selected_view_id = Some(new_id);
                    self.show_ui.view_page = new_page;
                    self.show_ui.new_view_name = DEFAULT_NEW_VIEW_NAME.to_string();
                    self.show_ui.add_view_open = false;
                }
            });
        }
    }

    pub fn show_ui(&mut self, ui: &mut egui::Ui, ctx: &Context) {
        ui.spacing_mut().interact_size = egui::vec2(44.0, 36.0);

        let mut dmx_engine = { self.data.state.dmx_engine.read().unwrap().clone() };
        let groups = dmx_engine.groups().clone();

        self.render_dmx_simulation_dialog(ctx, &groups);
        self.render_add_scene_dialog(ctx);
        self.render_clone_scene_dialog(ctx);
        self.render_rename_scene_dialog(ctx);
        self.render_delete_scene_dialog(ctx);
        self.render_scene_changeset_dialog(ctx, &dmx_engine);
        self.render_scene_animations_dialog(ctx, &dmx_engine);
        self.render_add_view_dialog(ctx);
        self.render_delete_view(ctx);
        self.render_overlay_picker_dialog(ctx);

        dmx_engine = { self.data.state.dmx_engine.read().unwrap().clone() };

        let total_views = dmx_engine.0.views.len();
        let total_pages = if total_views == 0 {
            0
        } else {
            (total_views + VIEW_OVERVIEW_PAGE_SIZE - 1) / VIEW_OVERVIEW_PAGE_SIZE
        };

        if total_pages == 0 {
            self.show_ui.view_page = 0;
        } else if self.show_ui.view_page >= total_pages {
            self.show_ui.view_page = total_pages - 1;
        }

        if let Some(selected) = self.show_ui.selected_view_id {
            if !dmx_engine.0.views.contains_key(&selected) {
                self.show_ui.selected_view_id = dmx_engine.0.views.keys().next().copied();
                self.show_ui.view_page = 0;
            } else if let Some(idx) = dmx_engine.0.views.keys().position(|vid| *vid == selected) {
                let selected_page = idx / VIEW_OVERVIEW_PAGE_SIZE;
                if self.show_ui.view_page != selected_page {
                    self.show_ui.view_page = selected_page;
                }
            }
        } else {
            self.show_ui.selected_view_id = dmx_engine.0.views.keys().next().copied();
            self.show_ui.view_page = 0;
        }

        ui.allocate_ui_with_layout(
            egui::vec2(ui.available_width(), ui.available_height()),
            egui::Layout::left_to_right(egui::Align::Min),
            |ui| {
                self.render_view_overview_column(ui, &dmx_engine);

                ui.separator();

                self.scene_overview(ui, ctx, &dmx_engine);

                ui.separator();

                ui.allocate_ui_with_layout(
                    egui::vec2(ui.available_width(), ui.available_height()),
                    egui::Layout::top_down(egui::Align::Min),
                    |ui| {
                        ui.horizontal(|ui| {
                            if components::button(ui, false, "Add View", ButtonSize::Medium) {
                                self.show_ui.new_view_name = DEFAULT_NEW_VIEW_NAME.to_string();
                                self.show_ui.add_view_open = true;
                            }

                            let can_delete = self.show_ui.selected_view_id.is_some()
                                && dmx_engine.0.views.len() > 1;
                            if components::button(ui, false, "Delete View", ButtonSize::Medium)
                                && can_delete
                            {
                                self.show_ui.delete_view_open = true;
                                self.show_ui.delete_view_id = self.show_ui.selected_view_id;
                            }
                        });

                        ui.separator();

                        ui.horizontal(|ui| {
                            ui.vertical(|ui| {
                                ui.set_width(220.0);
                                ui.heading("Views");
                                ui.add_space(4.0);

                                if dmx_engine.0.views.is_empty() {
                                    ui.label("No views defined.");
                                } else {
                                    ScrollArea::vertical().id_source("views_scroll").show(
                                        ui,
                                        |ui| {
                                            for (id, view) in dmx_engine.0.views.iter() {
                                                let selected =
                                                    Some(*id) == self.show_ui.selected_view_id;
                                                if ui
                                                    .selectable_label(
                                                        selected,
                                                        format!("{} ({id})", view.name),
                                                    )
                                                    .clicked()
                                                {
                                                    self.show_ui.selected_view_id = Some(*id);
                                                    if let Some(idx) = dmx_engine
                                                        .0
                                                        .views
                                                        .keys()
                                                        .position(|vid| *vid == *id)
                                                    {
                                                        self.show_ui.view_page =
                                                            idx / VIEW_OVERVIEW_PAGE_SIZE;
                                                    }
                                                }
                                            }
                                        },
                                    );
                                }
                            });

                            ui.separator();

                            ui.vertical(|ui| {
                                ui.heading("View Details");
                                ui.add_space(4.0);

                                if let Some(view_id) = self.show_ui.selected_view_id {
                                    if let Some(view_snapshot) =
                                        dmx_engine.0.views.get(&view_id).cloned()
                                    {
                                        let scene_name = |scene_id: u8| {
                                            dmx_engine
                                                .0
                                                .scenes
                                                .get(&scene_id)
                                                .map(|scene| scene.name.as_str())
                                                .unwrap_or("Unknown")
                                        };

                                        ui.label(format!("Name: {}", view_snapshot.name));

                                        let mut base_scene = view_snapshot.base_scene;
                                        let selected_text =
                                            format!("{} ({base_scene})", scene_name(base_scene));
                                        egui::ComboBox::from_id_source(format!(
                                            "view_base_scene_{view_id}"
                                        ))
                                        .width(220.0)
                                        .selected_text(selected_text)
                                        .show_ui(
                                            ui,
                                            |ui| {
                                                for (scene_id, scene) in dmx_engine.0.scenes.iter()
                                                {
                                                    ui.selectable_value(
                                                        &mut base_scene,
                                                        *scene_id,
                                                        format!("{} ({scene_id})", scene.name),
                                                    );
                                                }
                                            },
                                        );

                                        if base_scene != view_snapshot.base_scene {
                                            if let Some(local_view) =
                                                dmx_engine.0.views.get_mut(&view_id)
                                            {
                                                local_view.base_scene = base_scene;
                                            }

                                            let mut engine =
                                                self.data.state.dmx_engine.write().unwrap();
                                            if let Some(state_view) =
                                                engine.0.views.get_mut(&view_id)
                                            {
                                                state_view.base_scene = base_scene;
                                            }
                                        }

                                        ui.add_space(8.0);
                                        ui.label("Overlay Scenes");
                                        ui.add_space(4.0);

                                        if view_snapshot.overlays.is_empty() {
                                            ui.label("No overlay scenes.");
                                        } else {
                                            for overlay_id in &view_snapshot.overlays {
                                                let overlay_name = dmx_engine
                                                    .0
                                                    .scenes
                                                    .get(overlay_id)
                                                    .map(|scene| scene.name.as_str())
                                                    .unwrap_or("Unknown");

                                                ui.horizontal(|ui| {
                                                    ui.label(format!(
                                                        "{} ({overlay_id})",
                                                        overlay_name
                                                    ));
                                                    if components::button(
                                                        ui,
                                                        false,
                                                        "Remove",
                                                        ButtonSize::Small,
                                                    ) {
                                                        if let Some(local_view) =
                                                            dmx_engine.0.views.get_mut(&view_id)
                                                        {
                                                            local_view
                                                                .overlays
                                                                .retain(|id| id != overlay_id);
                                                        }

                                                        let mut engine = self
                                                            .data
                                                            .state
                                                            .dmx_engine
                                                            .write()
                                                            .unwrap();
                                                        if let Some(state_view) =
                                                            engine.0.views.get_mut(&view_id)
                                                        {
                                                            state_view
                                                                .overlays
                                                                .retain(|id| id != overlay_id);
                                                        }
                                                    }
                                                });
                                            }
                                        }

                                        let available_overlay_count = dmx_engine
                                            .0
                                            .scenes
                                            .keys()
                                            .filter(|scene_id| {
                                                **scene_id != base_scene
                                                    && !view_snapshot.overlays.contains(scene_id)
                                            })
                                            .count();

                                        let add_enabled = available_overlay_count > 0;
                                        if components::button(
                                            ui,
                                            !add_enabled,
                                            "Add Overlay",
                                            ButtonSize::Medium,
                                        ) && add_enabled
                                        {
                                            self.show_ui.overlay_picker_open = true;
                                            self.show_ui.overlay_picker_view_id = Some(view_id);
                                        }
                                    } else {
                                        ui.label("Select a view to edit.");
                                    }
                                } else {
                                    ui.label("No views available.");
                                }
                            });
                        });
                    },
                );
            },
        );
    }

    fn render_view_overview_column(&mut self, ui: &mut egui::Ui, dmx_engine: &EngineState) {
        ui.vertical(|ui| {
            ui.set_width(220.0);
            ui.heading("View Overview");
            ui.add_space(6.0);

            let total_views = dmx_engine.0.views.len();
            if total_views == 0 {
                self.show_ui.view_page = 0;
                ui.label("No views defined.");
                return;
            }

            let total_pages = (total_views + VIEW_OVERVIEW_PAGE_SIZE - 1) / VIEW_OVERVIEW_PAGE_SIZE;
            if self.show_ui.view_page >= total_pages {
                self.show_ui.view_page = total_pages - 1;
            }

            ui.horizontal(|ui| {
                ui.label(format!(
                    "Page {}/{}",
                    self.show_ui.view_page + 1,
                    total_pages
                ));

                let can_prev = self.show_ui.view_page > 0;
                let can_next = self.show_ui.view_page + 1 < total_pages;

                if components::button(ui, !can_prev, "<", ButtonSize::Small) && can_prev {
                    self.show_ui.view_page -= 1;
                }

                if components::button(ui, !can_next, ">", ButtonSize::Small) && can_next {
                    self.show_ui.view_page += 1;
                }
            });

            ui.add_space(6.0);

            let start = self.show_ui.view_page * VIEW_OVERVIEW_PAGE_SIZE;
            let end = (start + VIEW_OVERVIEW_PAGE_SIZE).min(total_views);
            let view_entries: Vec<_> = dmx_engine.0.views.iter().collect();

            for (id, view) in &view_entries[start..end] {
                let view_id = **id;
                let is_selected = Some(view_id) == self.show_ui.selected_view_id;
                if ui
                    .selectable_label(is_selected, format!("{} ({view_id})", view.name))
                    .clicked()
                {
                    self.show_ui.selected_view_id = Some(view_id);
                    if let Some(idx) = dmx_engine.0.views.keys().position(|vid| *vid == view_id) {
                        self.show_ui.view_page = idx / VIEW_OVERVIEW_PAGE_SIZE;
                    }

                    let (_, entry) = view_entries[view_id as usize];

                    let transaction = vec![
                        ControlEvent::SetSceneFocus(entry.base_scene),
                        ControlEvent::SetOverlays(entry.overlays.clone()),
                    ];

                    self.data
                        .event_bus_connection
                        .send(ControlEventMessage::new(
                            EventOriginator::Web,
                            ControlEvent::Transaction(transaction),
                        ));
                }
            }
        });
    }
}
