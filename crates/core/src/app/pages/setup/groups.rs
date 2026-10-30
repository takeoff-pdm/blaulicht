//! Setup › Groups: the group list with add, rename and delete.
//!
//! The selected group is `add_fixture_group`, shared with the Fixtures tab, so
//! picking a group here and switching tabs shows its fixtures there.
//!
//! Layout: toolbar on top, then [`BlaulichtApp::setup_group_list`] on the left
//! and [`BlaulichtApp::setup_group_details`] on the right. Ticket 017 replaces
//! the flat list with a folder tree; the toolbar is where folder actions go.

use crate::app::{
    components::{self, ButtonSize, Dialog, DEFAULT_NEW_GROUP_NAME},
    page::PageRenderContext,
    BlaulichtApp,
};
use crate::dmx::EngineState;
use blaulicht_shared::{AppSubPage, EngineGroups};
use egui::{Color32, Context, Frame, Key, Margin, RichText};

impl BlaulichtApp {
    pub(super) fn setup_groups_ui(
        &mut self,
        ui: &mut egui::Ui,
        ctx: &Context,
        render_context: PageRenderContext,
        dmx_engine: &EngineState,
    ) {
        self.render_add_group_dialog(ctx);
        self.render_rename_group_dialog(ctx);
        self.render_delete_group_dialog(ctx);

        render_context.horizontal(ui, egui::Align::Min, |ui| {
            self.group_toolbar(ui, dmx_engine.groups());
        });

        ui.separator();

        ui.allocate_ui_with_layout(
            ui.available_size(),
            egui::Layout::left_to_right(egui::Align::Min),
            |ui| {
                self.setup_group_list(ui, dmx_engine);

                ui.separator();

                ui.vertical(|ui| {
                    self.setup_group_details(ui, dmx_engine.groups());
                });
            },
        );
    }

    fn group_toolbar(&mut self, ui: &mut egui::Ui, groups: &EngineGroups) {
        if components::button(ui, self.add_group_open, "Add Group", ButtonSize::Medium) {
            self.add_group_open = !self.add_group_open;
        }

        if components::button(ui, false, "Ren. Group", ButtonSize::Medium) {
            if let Some(group) = self.add_fixture_group.and_then(|id| groups.get(&id)) {
                self.rename_group_name = group.name.clone();
                self.rename_group_dialog_open = true;
            }
        }

        if components::button(
            ui,
            self.delete_group_open,
            "Delete Group",
            ButtonSize::Medium,
        ) {
            if let Some(id) = self.add_fixture_group {
                if groups.get(&id).is_some() {
                    self.delete_group_open = !self.delete_group_open;
                }
            }
        }
    }

    /// The selectable list of all groups. Selecting a group also selects its
    /// first fixture for the Fixtures tab.
    fn setup_group_list(&mut self, ui: &mut egui::Ui, dmx_engine: &EngineState) {
        let groups = dmx_engine.groups();
        ui.vertical(|ui| {
            ui.label("Groups:");
            ui.add_space(8.0);

            if groups.is_empty() {
                ui.label(RichText::new("No groups yet").color(Color32::GRAY));
                return;
            }

            let mut clicked = None;
            egui::ScrollArea::vertical()
                .id_salt("setup-groups")
                .show(ui, |ui| {
                    for (group_id, group) in groups {
                        let is_selected = self.add_fixture_group == Some(*group_id);
                        if components::group_tile(ui, *group_id, group, is_selected) && !is_selected
                        {
                            clicked = Some(*group_id);
                        }
                        ui.add_space(2.0);
                    }
                });

            if let Some(group_id) = clicked {
                let first_fixture = groups
                    .get(&group_id)
                    .and_then(|group| group.fixtures.keys().next().copied())
                    .unwrap_or_default();
                self.select_setup_fixture(dmx_engine, group_id, first_fixture);
            }
        });
    }

    /// Name, id and fixtures of the selected group.
    fn setup_group_details(&mut self, ui: &mut egui::Ui, groups: &EngineGroups) {
        let Some((group_id, group)) = self
            .add_fixture_group
            .and_then(|id| groups.get(&id).map(|group| (id, group)))
        else {
            ui.label(RichText::new("No group selected").color(Color32::GRAY));
            return;
        };

        ui.label(RichText::new(format!("Selected Group: #{group_id}")).size(12.0));
        ui.add_space(6.0);
        ui.label(RichText::new(&group.name).size(18.0).strong());
        ui.add_space(6.0);
        ui.label(format!("{} fixtures", group.fixtures.len()));
        ui.add_space(6.0);

        egui::ScrollArea::vertical()
            .id_salt("setup-group-fixtures")
            .max_height((ui.available_height() - ButtonSize::Medium.dim().0.y - 12.0).max(0.0))
            .show(ui, |ui| {
                for (fixture_id, fixture) in &group.fixtures {
                    ui.label(
                        RichText::new(format!(
                            "#{fixture_id}  {}  ({})",
                            fixture.name,
                            fixture.type_.model_string()
                        ))
                        .size(12.0),
                    );
                }
            });

        ui.add_space(6.0);
        if components::button(ui, false, "Edit Fixtures", ButtonSize::Medium) {
            self.open_subpage(AppSubPage::Setup_Fixtures);
        }
    }

    fn render_delete_group_dialog(&mut self, ctx: &Context) {
        if self.delete_group_open {
            // TODO: create a confirmation dialog component
            Dialog::new("Confirm Deletion".to_string(), egui::vec2(200.0, 100.0))
                .with_backdrop()
                .show(ctx, |ui| {
                    ui.heading(RichText::new("Confirm Deletion").strong());

                    ui.add_space(12.0);

                    ui.horizontal(|ui| {
                        if components::button(ui, false, "Confirm", ButtonSize::Large) {
                            if let Some(id) = self.add_fixture_group {
                                let mut dmx_engine = self.data.state.dmx_engine.write().unwrap();
                                dmx_engine.delete_group(id);
                                self.delete_group_open = false;
                                self.add_fixture_group = None;
                            }
                        }

                        if components::button(ui, true, "Cancel", ButtonSize::Large) {
                            self.delete_group_open = false;
                        }
                    });
                });
        }
    }

    fn render_add_group_dialog(&mut self, ctx: &Context) {
        if self.add_group_open {
            const BUTTON_SIZE: ButtonSize = ButtonSize::Large;
            const SPACING: f32 = 16.0;
            let size = egui::vec2(200.0, BUTTON_SIZE.dim().0.y * 2.0 + SPACING + 8.0);

            Dialog::new("Create Group".to_string(), size)
                .with_backdrop()
                .show(ctx, |ui| {
                    ui.spacing_mut().interact_size = egui::vec2(44.0, 36.0);
                    Frame::new()
                        .inner_margin(Margin::symmetric(10, 6))
                        .show(ui, |ui| {
                            components::TextInput::new(180.0)
                                .with_hint_text("Group Name")
                                .ui(ui, &mut self.new_group_name);
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
                        dmx_engine.create_group(self.new_group_name.clone());
                        self.new_group_name = DEFAULT_NEW_GROUP_NAME.to_string();
                        self.add_group_open = false;
                    }
                });
        }
    }

    fn render_rename_group_dialog(&mut self, ctx: &Context) {
        if self.rename_group_dialog_open {
            const BUTTON_SIZE: ButtonSize = ButtonSize::Large;
            const SPACING: f32 = 16.0;
            let size = egui::vec2(200.0, BUTTON_SIZE.dim().0.y * 2.0 + SPACING);

            Dialog::new("Rename Group".to_string(), size)
                .with_backdrop()
                .show(ctx, |ui| {
                    Frame::new()
                        .inner_margin(Margin::symmetric(10, 6))
                        .show(ui, |ui| {
                            components::TextInput::new(180.0)
                                .with_hint_text("New Name")
                                .ui(ui, &mut self.rename_group_name);
                        });

                    ui.add_space(SPACING);

                    let mut confirm_pressed = false;
                    ui.horizontal(|ui| {
                        if components::button(ui, false, "CANCEL", BUTTON_SIZE) {
                            self.rename_group_dialog_open = false;
                            self.rename_group_name.clear();
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
                        let trimmed = self.rename_group_name.trim();

                        if let (false, Some(group_id)) =
                            (trimmed.is_empty(), self.add_fixture_group)
                        {
                            let mut dmx_engine = self.data.state.dmx_engine.write().unwrap();

                            if dmx_engine.rename_group(group_id, trimmed.to_string()) {
                                self.rename_group_name.clear();
                                self.rename_group_dialog_open = false;
                            }
                        }
                    }
                });
        }
    }
}
