//! Setup › Scenes: the scene list with new, clone, rename and delete.
//!
//! Scene *content* (changes, animations) is edited elsewhere: the Scene
//! Changes and Scene Animations dialogs live on FixturesPerformance until the
//! Programmer page (ticket 011) takes them over.

use crate::app::{
    components::{self, ButtonSize, Dialog, DEFAULT_NEW_SCENE_NAME},
    page::PageRenderContext,
    BlaulichtApp,
};
use crate::dmx::EngineState;
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
        self.render_delete_scene_dialog(ctx);

        ui.allocate_ui_with_layout(ui.available_size(), render_context.primary_layout(), |ui| {
            // Ticket 018 replaces this list with a folder tree.
            self.scene_overview(ui, dmx_engine, render_context.is_narrow_dynamic());

            ui.separator();

            ui.vertical(|ui| {
                render_context.horizontal(ui, egui::Align::Min, |ui| {
                    self.scene_toolbar(ui, dmx_engine);
                });

                ui.separator();

                Self::focused_scene_summary(ui, dmx_engine);
            });
        });
    }

    fn scene_toolbar(&mut self, ui: &mut egui::Ui, dmx_engine: &EngineState) {
        if components::button(ui, false, "Scene +", ButtonSize::Medium) {
            self.new_scene_dialog_open = true;
        }

        if components::button(ui, false, "Clone Sc.", ButtonSize::Medium) {
            self.clone_scene_dialog_open = true;
        }

        if components::button(ui, false, "Ren. Sc.", ButtonSize::Medium) {
            if !dmx_engine.0.scenes.is_empty() {
                self.rename_scene_name = dmx_engine.curr_scene().name.clone();
            } else {
                self.rename_scene_name.clear();
            }
            self.rename_scene_dialog_open = true;
        }

        if components::button(ui, false, "Del. Sc.", ButtonSize::Medium) {
            self.delete_scene_dialog_open = true;
        }
    }

    /// Which scene the toolbar acts on: clone, rename and delete all target the
    /// focused scene.
    fn focused_scene_summary(ui: &mut egui::Ui, dmx_engine: &EngineState) {
        let scene_id = dmx_engine.0.current_scene_focus;
        let Some(scene) = dmx_engine.0.scenes.get(&scene_id) else {
            ui.label(RichText::new("No scene selected").color(Color32::GRAY));
            return;
        };

        ui.label(RichText::new(format!("Selected Scene: #{scene_id}")).size(12.0));
        ui.add_space(6.0);
        ui.label(RichText::new(&scene.name).size(18.0).strong());
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
                    button_pressed = components::button(ui, true, "OK", BUTTON_SIZE);
                });

                ctx.input(|input| {
                    if input.key_pressed(Key::Enter) {
                        button_pressed = true;
                    }
                });

                if button_pressed {
                    let mut dmx_engine = self.data.state.dmx_engine.write().unwrap();
                    dmx_engine.clone_scene(self.new_scene_name.take());
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

                        if components::button(ui, true, "OK", BUTTON_SIZE) {
                            confirm_pressed = true;
                        }
                    });

                    ctx.input(|input| {
                        if input.key_pressed(Key::Enter) {
                            confirm_pressed = true;
                        }
                    });

                    if confirm_pressed {
                        let trimmed = self.rename_scene_name.trim();

                        if !trimmed.is_empty() {
                            let mut dmx_engine = self.data.state.dmx_engine.write().unwrap();
                            let scene_id = dmx_engine.0.current_scene_focus;

                            if dmx_engine.rename_scene(scene_id, trimmed.to_string()) {
                                self.rename_scene_name.clear();
                                self.rename_scene_dialog_open = false;
                            }
                        }
                    }
                });
        }
    }

    fn render_delete_scene_dialog(&mut self, ctx: &Context) {
        if self.delete_scene_dialog_open {
            const BUTTON_SIZE: ButtonSize = ButtonSize::Large;
            const WIDTH: f32 = 240.0;
            const HEIGHT: f32 = 130.0;

            let can_delete = {
                let dmx_engine = self.data.state.dmx_engine.read().unwrap();
                dmx_engine.0.scenes.len() > 1
            };

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
                            if components::button(ui, false, "Confirm", BUTTON_SIZE) {
                                let mut dmx_engine = self.data.state.dmx_engine.write().unwrap();
                                let scene_id = dmx_engine.0.current_scene_focus;

                                if dmx_engine.delete_scene(scene_id) {
                                    self.delete_scene_dialog_open = false;
                                }
                            }
                        }

                        if components::button(ui, true, "Cancel", BUTTON_SIZE) {
                            self.delete_scene_dialog_open = false;
                        }
                    });
                });
        }
    }
}
