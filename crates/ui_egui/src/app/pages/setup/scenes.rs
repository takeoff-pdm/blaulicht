//! Setup › Scenes: the scene tree (scenes in folders) with new, clone,
//! rename, delete and move.
//!
//! The selected scene is the engine's focused scene (`current_scene_focus`),
//! the one the Programmer edits. A folder can be selected instead
//! (`setup_scenes_ui`, a [`SetupFolderUi`]); Rename and Delete act on
//! whichever is selected, and new scenes and folders go into the selected
//! folder (the top level otherwise). Clone acts on the focused scene, and the
//! clone lands next to it. The clone, rename and delete dialogs remember the
//! scene they were opened for (`scene_dialog_target`) and send its id.
//!
//! Layout as in Setup › Groups: toolbar on top, then
//! [`BlaulichtApp::setup_scene_tree`] on the left and the details of the
//! selected scene or folder on the right. Every row has a "Move…" button that
//! opens the folder picker. The ephemeral BLANK scene is not listed: it is
//! never user-selectable and can't be moved into a folder.
//!
//! Scene *content* (changes, animations) is edited on the Programmer page
//! (`pages/programmer.rs`): its Scene Changes panel and dialog, and the Scene
//! Animations dialog.

#[cfg(doc)]
use crate::app::components::SetupFolderUi;
use crate::app::{
    components::{
        self, folder_details, folder_path, scene_tree_items, ButtonSize, Dialog, FolderDialog,
        Tree, TreeAction, TreeKey, DEFAULT_NEW_SCENE_NAME, SETUP_SCENE_TREE,
    },
    page::PageRenderContext,
    BlaulichtApp,
};
use blaulicht_engine::client::EngineClient;
use blaulicht_engine::dmx::EngineState;
use blaulicht_engine::hub::HostCommand;
use blaulicht_shared::{
    folder::{FolderItem, FolderKind},
    scene::BLANK_SCENE_ID,
    ControlEvent, ControlEventMessage, EventOriginator,
};
use egui::{Color32, Context, Frame, Key, Margin, RichText, TextBuffer};

impl BlaulichtApp {
    pub(super) fn setup_scenes_ui(
        &mut self,
        ui: &mut egui::Ui,
        ctx: &Context,
        render_context: PageRenderContext,
        dmx_engine: &EngineState,
    ) {
        self.render_add_scene_dialog(ctx);
        self.render_clone_scene_dialog(ctx);
        self.render_rename_scene_dialog(ctx);
        self.render_delete_scene_dialog(ctx, dmx_engine);
        if let Some(event) = self.setup_scenes_ui.dialog.show(ctx, &dmx_engine.0) {
            self.setup_scenes_ui.follow(ctx, SETUP_SCENE_TREE, &event);
            self.client
                .send(ControlEventMessage::new(EventOriginator::Web, event));
        }
        self.setup_scenes_ui.drop_missing_folder(&dmx_engine.0);

        render_context.horizontal(ui, egui::Align::Min, |ui| {
            self.scene_toolbar(ui, dmx_engine);
        });

        ui.separator();

        ui.allocate_ui_with_layout(
            ui.available_size(),
            egui::Layout::left_to_right(egui::Align::Min),
            |ui| {
                let tree_width = (ui.available_width() * 0.45).clamp(220.0, 380.0);
                ui.allocate_ui_with_layout(
                    egui::vec2(tree_width, ui.available_height()),
                    egui::Layout::top_down(egui::Align::Min),
                    |ui| {
                        ui.set_width(tree_width);
                        self.setup_scene_tree(ui, dmx_engine);
                    },
                );

                ui.separator();

                ui.vertical(|ui| match self.setup_scenes_ui.folder {
                    Some(folder) => folder_details(ui, &dmx_engine.0, FolderKind::Scenes, folder),
                    None => Self::focused_scene_summary(ui, dmx_engine),
                });
            },
        );
    }

    fn scene_toolbar(&mut self, ui: &mut egui::Ui, dmx_engine: &EngineState) {
        let folder = self.setup_scenes_ui.folder;
        let scene = focused_user_scene(dmx_engine);
        let has_selection = folder.is_some() || scene.is_some();

        if components::button(ui, false, "Scene +", ButtonSize::Medium) {
            self.new_scene_dialog_open = true;
        }

        if components::action_button(
            ui,
            folder.is_none() && scene.is_some(),
            "Clone Sc.",
            ButtonSize::Medium,
            Some("Select a scene"),
        ) {
            if let Some((id, _)) = scene {
                self.scene_dialog_target = id;
                self.clone_scene_dialog_open = true;
            }
        }

        if components::button(ui, false, "New Folder", ButtonSize::Medium) {
            self.setup_scenes_ui.dialog = FolderDialog::create(FolderKind::Scenes, folder);
        }

        if components::action_button(
            ui,
            has_selection,
            "Rename",
            ButtonSize::Medium,
            Some("Select a scene or folder"),
        ) {
            if let Some(folder) = folder {
                self.setup_scenes_ui.dialog = FolderDialog::rename(&dmx_engine.0, folder);
            } else if let Some((id, scene)) = scene {
                self.scene_dialog_target = id;
                self.rename_scene_name = scene.name.clone();
                self.rename_scene_dialog_open = true;
            }
        }

        if components::action_button_as(
            ui,
            has_selection,
            false,
            components::ButtonVariant::Danger,
            "Delete",
            ButtonSize::Medium,
            Some("Select a scene or folder"),
        ) {
            if let Some(folder) = folder {
                self.setup_scenes_ui.dialog = FolderDialog::delete(folder);
            } else if let Some((id, _)) = scene {
                self.scene_dialog_target = id;
                self.delete_scene_dialog_open = true;
            }
        }
    }

    /// The tree of folders and scenes. Tapping a scene focuses it (for the
    /// Programmer too); tapping a folder selects the folder; "Move…" opens
    /// the folder picker for that row.
    fn setup_scene_tree(&mut self, ui: &mut egui::Ui, dmx_engine: &EngineState) {
        let state = &dmx_engine.0;
        components::column_header(ui, "Scenes:");
        ui.add_space(8.0);

        let items = scene_tree_items(state, |id| id != BLANK_SCENE_ID);
        if items.is_empty() {
            ui.label(RichText::new("No scenes yet").color(Color32::GRAY));
            return;
        }

        let selected = match self.setup_scenes_ui.folder {
            Some(folder) => TreeKey::Folder(folder),
            None => TreeKey::Leaf(state.current_scene_focus),
        };

        let action = egui::ScrollArea::vertical()
            .id_salt("setup-scenes")
            .show(ui, |ui| {
                Tree::new(SETUP_SCENE_TREE, &items)
                    .selected(Some(selected))
                    .with_move_button(true)
                    .show(ui)
            })
            .inner;

        match action {
            Some(TreeAction::SelectLeaf(scene_id)) => {
                self.setup_scenes_ui.folder = None;
                if state.current_scene_focus != scene_id {
                    self.client.send(ControlEventMessage::new(
                        EventOriginator::Web,
                        ControlEvent::SetSceneFocus(scene_id),
                    ));
                }
            }
            Some(TreeAction::TapFolder(folder)) => {
                self.setup_scenes_ui.folder = Some(folder);
            }
            Some(TreeAction::MoveRequested(key)) => {
                let item = match key {
                    TreeKey::Folder(id) => FolderItem::Folder(id),
                    TreeKey::Leaf(id) => FolderItem::Scene(id),
                };
                self.setup_scenes_ui.dialog = FolderDialog::move_item(state, item);
            }
            None => {}
        }
    }

    /// The focused scene, which clone, rename and delete act on.
    fn focused_scene_summary(ui: &mut egui::Ui, dmx_engine: &EngineState) {
        let Some((scene_id, scene)) = focused_user_scene(dmx_engine) else {
            ui.label(RichText::new("No scene selected").color(Color32::GRAY));
            return;
        };

        ui.label(RichText::new(format!("Selected Scene: #{scene_id}")).size(12.0));
        ui.add_space(6.0);
        ui.label(RichText::new(&scene.name).size(18.0).strong());
        ui.add_space(6.0);
        ui.label(format!(
            "In: {}",
            folder_path(&dmx_engine.0.folders, scene.folder)
        ));
        ui.add_space(6.0);
        ui.label(
            RichText::new("Clone, rename and delete act on the selected scene.")
                .size(12.0)
                .color(Color32::GRAY),
        );
    }

    fn render_clone_scene_dialog(&mut self, ctx: &Context) {
        if !self.clone_scene_dialog_open {
            return;
        }

        const BUTTON_SIZE: ButtonSize = ButtonSize::Large;
        const SPACING: f32 = 16.0;

        let size = egui::vec2(200.0, BUTTON_SIZE.dim().0.y * 2.0 + SPACING);

        Dialog::new("Clone Scene".to_string(), size)
            .with_backdrop()
            .show(ctx, |ui| {
                Frame::new()
                    .inner_margin(Margin::symmetric(10, 6))
                    .show(ui, |ui| {
                        components::TextInput::new(180.0)
                            .with_hint_text("New Name")
                            .ui(ui, &mut self.new_scene_name);
                    });

                ui.add_space(SPACING);

                let mut button_pressed = false;

                ui.horizontal(|ui| {
                    if components::button(ui, false, "CANCEL", BUTTON_SIZE) {
                        self.new_scene_name = DEFAULT_NEW_SCENE_NAME.to_string();
                        self.clone_scene_dialog_open = false;
                    }
                    button_pressed = components::button_as(
                        ui,
                        true,
                        components::ButtonVariant::Primary,
                        "OK",
                        BUTTON_SIZE,
                    );
                });

                ctx.input(|input| {
                    if input.key_pressed(Key::Enter) {
                        button_pressed = true;
                    }
                });

                if button_pressed {
                    let clone = HostCommand::CloneScene {
                        scene: self.scene_dialog_target,
                        name: self.new_scene_name.take(),
                    };
                    self.send_setup_command(clone, |_, _| {});
                    self.new_scene_name = DEFAULT_NEW_SCENE_NAME.to_string();
                    self.clone_scene_dialog_open = false;
                }
            });
    }

    fn render_add_scene_dialog(&mut self, ctx: &Context) {
        if !self.new_scene_dialog_open {
            return;
        }

        const BUTTON_SIZE: ButtonSize = ButtonSize::Large;
        const SPACING: f32 = 16.0;

        let size = egui::vec2(200.0, BUTTON_SIZE.dim().0.y * 2.0 + SPACING);

        Dialog::new("Create Scene".to_string(), size)
            .with_backdrop()
            .show(ctx, |ui| {
                Frame::new()
                    .inner_margin(Margin::symmetric(10, 6))
                    .show(ui, |ui| {
                        components::TextInput::new(180.0)
                            .with_hint_text("Scene Name")
                            .ui(ui, &mut self.new_scene_name);
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

                if button_pressed {
                    // New scenes go into the selected folder.
                    let create = HostCommand::CreateScene {
                        name: self.new_scene_name.take(),
                        folder: self.setup_scenes_ui.folder,
                    };
                    self.send_setup_command(create, |_, _| {});
                    self.new_scene_name = DEFAULT_NEW_SCENE_NAME.to_string();
                    self.new_scene_dialog_open = false;
                }
            });
    }

    fn render_rename_scene_dialog(&mut self, ctx: &Context) {
        if self.rename_scene_dialog_open {
            const BUTTON_SIZE: ButtonSize = ButtonSize::Large;
            const SPACING: f32 = 16.0;
            let size = egui::vec2(200.0, BUTTON_SIZE.dim().0.y * 2.0 + SPACING);

            Dialog::new("Rename Scene".to_string(), size)
                .with_backdrop()
                .show(ctx, |ui| {
                    Frame::new()
                        .inner_margin(Margin::symmetric(10, 6))
                        .show(ui, |ui| {
                            components::TextInput::new(180.0)
                                .with_hint_text("New Name")
                                .ui(ui, &mut self.rename_scene_name);
                        });

                    ui.add_space(SPACING);

                    let mut confirm_pressed = false;
                    ui.horizontal(|ui| {
                        if components::button(ui, false, "CANCEL", BUTTON_SIZE) {
                            self.rename_scene_dialog_open = false;
                            self.rename_scene_name.clear();
                        }

                        if components::button_as(
                            ui,
                            true,
                            components::ButtonVariant::Primary,
                            "OK",
                            BUTTON_SIZE,
                        ) {
                            confirm_pressed = true;
                        }
                    });

                    ctx.input(|input| {
                        if input.key_pressed(Key::Enter) {
                            confirm_pressed = true;
                        }
                    });

                    // Closes once the engine has renamed the scene.
                    if confirm_pressed && !self.setup_reply_pending() {
                        let trimmed = self.rename_scene_name.trim().to_string();

                        if !trimmed.is_empty() {
                            let rename = HostCommand::RenameScene {
                                scene: self.scene_dialog_target,
                                name: trimmed,
                            };
                            self.send_setup_command(rename, |app, _| {
                                app.rename_scene_name.clear();
                                app.rename_scene_dialog_open = false;
                            });
                        }
                    }
                });
        }
    }

    fn render_delete_scene_dialog(&mut self, ctx: &Context, dmx_engine: &EngineState) {
        if self.delete_scene_dialog_open {
            const BUTTON_SIZE: ButtonSize = ButtonSize::Large;
            const WIDTH: f32 = 240.0;
            const HEIGHT: f32 = 130.0;

            // `BLANK` doesn't count: the last user scene can't be deleted.
            let can_delete = dmx_engine.0.user_scenes().count() > 1;

            Dialog::new("Delete Scene".to_string(), egui::vec2(WIDTH, HEIGHT))
                .with_backdrop()
                .show(ctx, |ui| {
                    ui.heading(RichText::new("Delete this scene?").strong());
                    ui.add_space(12.0);

                    if !can_delete {
                        ui.label("At least one scene must remain.");
                        ui.add_space(12.0);
                    }

                    ui.horizontal(|ui| {
                        if can_delete {
                            // Closes once the engine has deleted the scene.
                            if components::button_as(
                                ui,
                                false,
                                components::ButtonVariant::Primary,
                                "Confirm",
                                BUTTON_SIZE,
                            ) && !self.setup_reply_pending()
                            {
                                let delete = HostCommand::DeleteScene(self.scene_dialog_target);
                                self.send_setup_command(delete, |app, _| {
                                    app.delete_scene_dialog_open = false;
                                });
                            }
                        }

                        if components::button_as(
                            ui,
                            true,
                            components::ButtonVariant::Tonal,
                            "Cancel",
                            BUTTON_SIZE,
                        ) {
                            self.delete_scene_dialog_open = false;
                        }
                    });
                });
        }
    }
}

/// The focused scene, unless it is BLANK (not a user scene) or missing.
fn focused_user_scene(dmx_engine: &EngineState) -> Option<(u8, &blaulicht_shared::scene::Scene)> {
    let id = dmx_engine.0.current_scene_focus;
    if id == BLANK_SCENE_ID {
        return None;
    }
    dmx_engine.0.scenes.get(&id).map(|scene| (id, scene))
}
