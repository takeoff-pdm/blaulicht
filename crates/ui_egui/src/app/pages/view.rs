use crate::app::{
    components::{self, ButtonSize, Dialog, Held, Knob, Pagination, SpeedKnob},
    BlaulichtApp,
};
use blaulicht_engine::client::EngineClient;
use blaulicht_engine::hub::{CommandSeq, CreatedId, HostCommand};
use blaulicht_shared::{
    scene::BLANK_SCENE_ID, view::View, AnimationSpeedModifier, ControlEvent, ControlEventMessage,
    EventOriginator,
};
use egui::{Context, FontId, Frame, Key, Margin, RichText, TextEdit, Vec2};
use std::collections::BTreeMap;

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
    // For both overlay and base
    scene_picker_view_id: Option<u8>,
    /// A scene row's Alpha knob while it is turned, keyed by (view, scene).
    alpha_held: Held<f32, (u8, u8)>,
    /// A scene row's Speed knob while it is turned, keyed by (view, scene).
    speed_held: Held<AnimationSpeedModifier, (u8, u8)>,
    /// The Create View command waiting for its reply (the dialog closes on
    /// success).
    create_reply: Option<CommandSeq>,
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
            scene_picker_view_id: None,
            alpha_held: Held::default(),
            speed_held: Held::default(),
            create_reply: None,
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
                        let confirm = components::button_as(
                            ui,
                            false,
                            components::ButtonVariant::Primary,
                            "Confirm",
                            ButtonSize::Large,
                        );
                        if confirm {
                            let current_page = self.view_ui_state.pagination.current_page();
                            self.send_command(HostCommand::DeleteView(view_id), move |app, _| {
                                app.select_view_after_delete(current_page);
                            });
                            self.view_ui_state.delete_view_open = false;
                            self.view_ui_state.delete_view_id = None;
                        }

                        if components::button_as(
                            ui,
                            true,
                            components::ButtonVariant::Tonal,
                            "Cancel",
                            ButtonSize::Large,
                        ) {
                            self.view_ui_state.delete_view_open = false;
                            self.view_ui_state.delete_view_id = None;
                        }
                    });
                });
        }
    }

    /// After a view was deleted: select the first view of the page that was
    /// shown (or of the last page, if that one is gone now).
    fn select_view_after_delete(&mut self, current_page: usize) {
        let pagination = &self.view_ui_state.pagination;
        let total_pages = pagination.pages();
        let target_page = current_page.min(total_pages.saturating_sub(1));
        let per_page = pagination.items_per_page();
        self.view_ui_state.selected_view_id = self.client.with_show(|show| {
            let views = &show.0.views;
            views
                .keys()
                .nth(target_page * per_page)
                .or_else(|| views.keys().next())
                .copied()
        });
    }

    pub fn render_overlay_picker_dialog(&mut self, ctx: &Context) {
        if !self.view_ui_state.overlay_picker_open {
            return;
        }

        let Some(view_id) = self.view_ui_state.scene_picker_view_id else {
            self.view_ui_state.overlay_picker_open = false;
            return;
        };

        // Scenes in folders; only folders with a scene left to add.
        let available_scenes = self.client.with_show(|show| {
            let view = show.0.views.get(&view_id)?;
            Some(components::prune_empty_folders(
                components::scene_tree_items(&show.0, |scene_id| {
                    scene_id != BLANK_SCENE_ID && !view.overlays.contains(&scene_id)
                }),
            ))
        });
        let Some(available_scenes) = available_scenes else {
            self.view_ui_state.overlay_picker_open = false;
            self.view_ui_state.scene_picker_view_id = None;
            return;
        };

        let picked = components::tree_picker_dialog(
            ctx,
            "Select Overlay",
            components::SCENE_PICKER_TREE,
            &available_scenes,
            None,
            "No more scenes to add",
            &mut self.view_ui_state.overlay_picker_open,
        );

        if let Some(new_id) = picked {
            self.view_ui_state.scene_picker_view_id = None;
            self.send_command(
                HostCommand::AddViewOverlay {
                    view: view_id,
                    scene: new_id,
                },
                |_, _| {},
            );

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
                    ui.spacing_mut().interact_size =
                        crate::app::ui_tokens::Density::of(ui.ctx()).interact_size();
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

                    let mut button_pressed = components::button_as(
                        ui,
                        false,
                        components::ButtonVariant::Primary,
                        "OK",
                        BUTTON_SIZE,
                    );
                    ctx.input(|input| {
                        if input.key_pressed(Key::Enter) {
                            button_pressed = true;
                        }
                    });

                    let waiting = self
                        .view_ui_state
                        .create_reply
                        .is_some_and(|seq| self.reply_handlers.is_waiting(seq));
                    if button_pressed && !waiting {
                        let mut name = self.view_ui_state.new_view_name.trim().to_string();
                        if name.is_empty() {
                            name = DEFAULT_NEW_VIEW_NAME.to_string();
                        }
                        // The engine captures the overlay stack (what is
                        // playing) into the new view.
                        let seq =
                            self.send_command(HostCommand::CreateView { name }, |app, created| {
                                if let Some(CreatedId::View(id)) = created {
                                    app.view_ui_state.selected_view_id = Some(id);
                                }
                                // TODO: can we focus this?
                                app.view_ui_state.new_view_name = DEFAULT_NEW_VIEW_NAME.to_string();
                                app.view_ui_state.add_view_open = false;
                            });
                        self.view_ui_state.create_reply = Some(seq);
                    }
                });
        }
    }

    fn view_pagination(&mut self, ui: &mut egui::Ui, views: &[(u8, String)], compact: bool) {
        let paginated_views = self
            .view_ui_state
            .pagination
            .prepare_current_page_items(views);

        let panel_padding = 2.0;

        let (mut changed, mut selected_id) = (false, None);
        let selected_view_id = self.view_ui_state.selected_view_id;
        let page_width = self.view_ui_state.pagination.width();

        self.view_ui_state
            .pagination
            .ui_responsive(ui, compact, |ui| {
                for (id, name) in paginated_views {
                    let id = *id;

                    let label = components::indexed_label(id, name);
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
    /// trailing button. Knob edits are sent as the view's masters for the scene.
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
        let key = (view_id, scene_id);
        let engine_masters = view_snapshot.masters_for(scene_id);
        let mut masters = engine_masters;
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

            let held = &mut self.view_ui_state.alpha_held;
            let mut alpha = held.shown(key, engine_masters.master_alpha as f32);
            masters.master_alpha = alpha.round() as u8;
            let response = ui.add(Knob::new(&mut alpha, 0.0..=100.0).with_label("Alpha"));
            if response.changed() {
                held.hold(key, alpha);
                masters.master_alpha = alpha.round() as u8;
                changed = true;
            }
            held.settle_row(response.dragged(), key, |value| {
                value.round() as u8 == engine_masters.master_alpha
            });

            let held = &mut self.view_ui_state.speed_held;
            masters.master_speed = held.shown(key, engine_masters.master_speed);
            let response = ui.add(SpeedKnob::new(&mut masters.master_speed).with_label("Speed"));
            if response.changed() {
                held.hold(key, masters.master_speed);
                changed = true;
            }
            held.settle_row(response.dragged(), key, |value| {
                value == engine_masters.master_speed
            });

            ui.add_space(4.0);
            pressed = components::button(ui, button_selected, button_label, ButtonSize::Medium);
        });

        if changed {
            self.client.send(HostCommand::SetViewSceneMasters {
                view: view_id,
                scene: scene_id,
                masters,
            });
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

        ui.allocate_ui_with_layout(
            egui::vec2(ui.available_width(), ui.available_height()),
            render_context.primary_layout(),
            |ui| {
                let selected_view_id = self.view_ui_state.selected_view_id;
                let (views, view_snapshot, scene_names, available_overlay_count) =
                    self.client.with_show(|show| {
                        let views: Vec<(u8, String)> = show
                            .0
                            .views
                            .iter()
                            .map(|(id, view)| (*id, view.name.clone()))
                            .collect();
                        let view_snapshot =
                            selected_view_id.and_then(|id| show.0.views.get(&id).cloned());
                        let scene_names: BTreeMap<u8, String> = show
                            .0
                            .scenes
                            .iter()
                            .map(|(id, scene)| (*id, scene.name.clone()))
                            .collect();
                        let available_overlay_count = view_snapshot.as_ref().map_or(0, |view| {
                            show.0
                                .scenes
                                .keys()
                                .filter(|scene_id| {
                                    **scene_id != BLANK_SCENE_ID
                                        && !view.overlays.contains(scene_id)
                                })
                                .count()
                        });
                        (views, view_snapshot, scene_names, available_overlay_count)
                    });
                self.view_pagination(ui, &views, render_context.is_narrow_dynamic());

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

                            let can_delete =
                                self.view_ui_state.selected_view_id.is_some() && views.len() > 1;
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

                        // Without a selection the panel used to render as a blank slab with no
                        // explanation; mirror the "No Animation Selected" hint of the
                        // animations page so the empty state reads as intentional.
                        let Some(view_id) = self.view_ui_state.selected_view_id else {
                            ui.heading("View Details");
                            ui.add_space(4.0);
                            ui.label("No View Selected");
                            return;
                        };

                        let Some(view_snapshot) = view_snapshot else {
                            self.view_ui_state.scene_picker_view_id = None;
                            self.view_ui_state.overlay_picker_open = false;
                            ui.heading("View Details");
                            ui.add_space(4.0);
                            ui.label("No View Selected");
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
                                                self.client.send(ControlEventMessage::new(
                                                    EventOriginator::Web,
                                                    ControlEvent::Transaction(
                                                        view_snapshot.apply_events(),
                                                    ),
                                                ));
                                            }
                                        },
                                    );
                                });
                                ui.add_space(4.0);

                                ui.label(format!("Name: {}", view_snapshot.name));
                                ui.add_space(4.0);

                                let scene_name = |scene_id: u8| -> String {
                                    scene_names
                                        .get(&scene_id)
                                        .cloned()
                                        .unwrap_or_else(|| "Unknown".to_string())
                                };

                                // Overlay rows.
                                ui.horizontal(|ui| {
                                    ui.label(RichText::new("Overlay Scenes").strong());

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
                                            self.send_command(
                                                HostCommand::RemoveViewOverlay {
                                                    view: view_id,
                                                    scene: overlay_id,
                                                },
                                                |_, _| {},
                                            );
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
