//! Setup › Groups: the group tree (groups in folders) with add, rename,
//! delete and move.
//!
//! The selected group is `add_fixture_group`, shared with the Fixtures tab, so
//! picking a group here and switching tabs shows its fixtures there. A folder
//! can be selected instead (`setup_groups_ui`, a [`SetupFolderUi`]); Rename and Delete act
//! on whichever is selected, and new groups and folders go into the selected
//! folder (the top level otherwise).
//!
//! Layout: toolbar on top, then [`BlaulichtApp::setup_group_tree`] on the left
//! and the details of the selected group or folder on the right. Every row has
//! a "Move…" button that opens the folder picker ([`FolderDialog::Move`]).

use super::duo::{self, folder_desktop_pane};
#[cfg(doc)]
use crate::app::components::SetupFolderUi;
use crate::app::{
    components::{
        self, folder_details, group_tree_items, studio, ButtonSize, Dialog, FolderDialog, Tree,
        TreeAction, TreeKey, DEFAULT_NEW_GROUP_NAME, SETUP_GROUP_TREE,
    },
    page::PageRenderContext,
    BlaulichtApp,
};
use blaulicht_engine::client::EngineClient;
use blaulicht_engine::dmx::EngineState;
use blaulicht_engine::hub::HostCommand;
use blaulicht_shared::{
    folder::{FolderId, FolderItem, FolderKind},
    AppSubPage, ControlEvent, ControlEventMessage, EngineGroups, EventOriginator,
};
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
        if let Some(event) = self.setup_groups_ui.dialog.show(ctx, &dmx_engine.0) {
            self.apply_group_folder_event(ctx, event);
        }

        // A folder that no longer exists (deleted, or a new showfile) is
        // no selection, unless it was just created and is still on its way.
        self.setup_groups_ui.drop_missing_folder(&dmx_engine.0);

        if crate::app::ui_tokens::Density::of(ctx).is_compact() {
            self.setup_groups_desktop_ui(ui, dmx_engine);
            return;
        }

        render_context.horizontal(ui, egui::Align::Min, |ui| {
            self.group_toolbar(ui, dmx_engine);
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
                        components::column_header(ui, "Groups:");
                        ui.add_space(8.0);
                        self.setup_group_tree(ui, dmx_engine);
                    },
                );

                ui.separator();

                ui.vertical(|ui| match self.setup_groups_ui.folder {
                    Some(folder) => Self::setup_folder_details(ui, dmx_engine, folder),
                    None => self.setup_group_details(ui, dmx_engine.groups()),
                });
            },
        );
    }

    /// Sends a folder event from a dialog and follows it in the tree: a new
    /// folder gets selected, the target of a move is expanded, and a deleted
    /// folder is deselected.
    fn apply_group_folder_event(&mut self, ctx: &Context, event: ControlEvent) {
        self.setup_groups_ui.follow(ctx, SETUP_GROUP_TREE, &event);
        self.client
            .send(ControlEventMessage::new(EventOriginator::Web, event));
    }

    fn group_toolbar(&mut self, ui: &mut egui::Ui, dmx_engine: &EngineState) {
        let groups = dmx_engine.groups();
        let folder = self.setup_groups_ui.folder;
        let group = self.add_fixture_group.filter(|id| groups.contains_key(id));
        let has_selection = folder.is_some() || group.is_some();

        if components::button(ui, self.add_group_open, "Add Group", ButtonSize::Medium) {
            self.add_group_open = !self.add_group_open;
        }

        if components::button(ui, false, "New Folder", ButtonSize::Medium) {
            self.setup_groups_ui.dialog = FolderDialog::create(FolderKind::Groups, folder);
        }

        if components::action_button(
            ui,
            has_selection,
            "Rename",
            ButtonSize::Medium,
            Some("Select a group or folder"),
        ) {
            if let Some(folder) = folder {
                self.setup_groups_ui.dialog = FolderDialog::rename(&dmx_engine.0, folder);
            } else if let Some(group) = group.and_then(|id| groups.get(&id)) {
                self.rename_group_name = group.name.clone();
                self.rename_group_dialog_open = true;
            }
        }

        if components::action_button_as(
            ui,
            has_selection,
            false,
            components::ButtonVariant::Danger,
            "Delete",
            ButtonSize::Medium,
            Some("Select a group or folder"),
        ) {
            if let Some(folder) = folder {
                self.setup_groups_ui.dialog = FolderDialog::delete(folder);
            } else if group.is_some() {
                self.delete_group_open = !self.delete_group_open;
            }
        }
    }

    /// Setup › Groups on the desktop (`setup.html` `groups()`): the tree
    /// under a "Groups" bar with Add and Folder, and the selected group under
    /// a "Group:" bar with its actions, its name editable in place, and its
    /// fixtures below. A selected folder shows under a "Folder:" bar.
    fn setup_groups_desktop_ui(&mut self, ui: &mut egui::Ui, dmx_engine: &EngineState) {
        // Pick a group so the page is never empty while groups exist.
        if self.add_fixture_group.is_none() && self.setup_groups_ui.folder.is_none() {
            if let Some((gid, group)) = dmx_engine.groups().iter().next() {
                let first = group.fixtures.keys().next().copied().unwrap_or_default();
                self.select_setup_fixture(dmx_engine, *gid, first);
            }
        }
        let (mut list, mut detail) = duo::duo(ui);
        let folder = self.setup_groups_ui.folder;

        let (mut add, mut new_folder) = (false, false);
        let add_open = self.add_group_open;
        studio::pane_header(&mut list, None, "Groups", true, |ui| {
            add = duo::bar_button(ui, "Add", add_open);
            new_folder = duo::bar_button(ui, "Folder", false);
        });
        if add {
            self.add_group_open = !self.add_group_open;
        }
        if new_folder {
            self.setup_groups_ui.dialog = FolderDialog::create(FolderKind::Groups, folder);
        }
        self.setup_group_tree(&mut list, dmx_engine);

        if let Some(folder) = folder {
            self.setup_folder_desktop_pane(&mut detail, dmx_engine, folder);
            return;
        }
        let groups = dmx_engine.groups();
        let Some((group_id, group)) = self
            .add_fixture_group
            .and_then(|id| groups.get(&id).map(|group| (id, group)))
        else {
            studio::pane_header(&mut detail, None, "Group", true, |_| {});
            duo::body(&mut detail, None, |ui| {
                duo::note(ui, "Select a group to edit it, or add a new one.")
            });
            return;
        };

        let (mut edit, mut show, mut delete) = (false, false, false);
        let delete_open = self.delete_group_open;
        studio::pane_header(&mut detail, Some("Group:"), &group.name, true, |ui| {
            edit = duo::bar_button(ui, "Edit fixtures", false);
            if cfg!(feature = "visualizer") {
                show = duo::bar_button(ui, "Show in Visualizer", false);
            }
            delete = duo::bar_button(ui, "Delete", delete_open);
        });

        let channels: usize = group.fixtures.values().map(|f| f.type_.footprint()).sum();
        let mut types: Vec<(String, usize)> = Vec::new();
        for fixture in group.fixtures.values() {
            let name = fixture.type_.model_string();
            match types.iter_mut().find(|(n, _)| *n == name) {
                Some((_, count)) => *count += 1,
                None => types.push((name, 1)),
            }
        }
        let mut rename = None;
        duo::body(&mut detail, Some(duo::form_height(3)), |ui| {
            rename = duo::name_row(ui, ui.id().with(("group", group_id)), &group.name);
            duo::form_row(ui, "Contains", |ui| {
                duo::text(
                    ui,
                    format!(
                        "{} {}, {channels} channels",
                        group.fixtures.len(),
                        if group.fixtures.len() == 1 {
                            "fixture"
                        } else {
                            "fixtures"
                        }
                    ),
                )
            });
            duo::form_row(ui, "Types", |ui| {
                let list: Vec<String> = types.iter().map(|(n, c)| format!("{c} {n}")).collect();
                duo::text(
                    ui,
                    if list.is_empty() {
                        "--".into()
                    } else {
                        list.join(", ")
                    },
                )
            });
        });

        duo::lower_pane(&mut detail, "Fixtures");
        duo::body(&mut detail, None, |ui| {
            egui::ScrollArea::vertical()
                .id_salt("setup-group-fixtures")
                .auto_shrink([false, false])
                .show(ui, |ui| {
                    let rows: Vec<Vec<duo::Cell>> = group
                        .fixtures
                        .values()
                        .map(|fixture| {
                            let first = fixture.start_addr;
                            let last = first + fixture.type_.footprint().max(1) - 1;
                            vec![
                                (fixture.name.clone(), false),
                                (fixture.type_.model_string(), true),
                                (fixture.universe_no.to_string(), false),
                                (format!("{first}-{last}"), false),
                            ]
                        })
                        .collect();
                    duo::table(
                        ui,
                        &["Fixture", "Type", "Universe", "Channels"],
                        &[0.0, 0.3, 0.63, 0.82],
                        &rows,
                    );
                });
        });

        if let Some(name) = rename {
            self.send_setup_command(
                HostCommand::RenameGroup {
                    group: group_id,
                    name,
                },
                |_, _| {},
            );
        }
        if edit {
            self.open_subpage(AppSubPage::Setup_Fixtures);
        }
        if show {
            let fixtures: Vec<_> = group.fixtures.keys().map(|fid| (group_id, *fid)).collect();
            self.show_fixtures_in_visualizer(fixtures);
        }
        if delete {
            self.delete_group_open = !self.delete_group_open;
        }
    }

    /// The selected group folder on the desktop: a "Folder:" bar with
    /// Rename and Delete over its details.
    fn setup_folder_desktop_pane(
        &mut self,
        ui: &mut egui::Ui,
        dmx_engine: &EngineState,
        folder: FolderId,
    ) {
        if let Some(event) = folder_desktop_pane(ui, &dmx_engine.0, FolderKind::Groups, folder) {
            self.setup_groups_ui.dialog = event;
        }
    }

    /// The tree of folders and groups. Tapping a group selects it (and its
    /// first fixture for the Fixtures tab); tapping a folder selects the
    /// folder; "Move…" opens the folder picker for that row.
    fn setup_group_tree(&mut self, ui: &mut egui::Ui, dmx_engine: &EngineState) {
        let state = &dmx_engine.0;

        if state.groups.is_empty() && !state.folders.values().any(|f| f.kind == FolderKind::Groups)
        {
            ui.label(RichText::new("No groups yet").color(Color32::GRAY));
            return;
        }

        let items = group_tree_items(state);
        let selected = match self.setup_groups_ui.folder {
            Some(folder) => Some(TreeKey::Folder(folder)),
            None => self.add_fixture_group.map(TreeKey::Leaf),
        };

        let action = egui::ScrollArea::vertical()
            .id_salt("setup-groups")
            .show(ui, |ui| {
                Tree::new(SETUP_GROUP_TREE, &items)
                    .selected(selected)
                    .with_move_button(!crate::app::ui_tokens::Density::of(ui.ctx()).is_compact())
                    .drag_to_folders(true)
                    .show(ui)
            })
            .inner;

        match action {
            Some(TreeAction::SelectLeaf(group_id)) => {
                self.setup_groups_ui.folder = None;
                if self.add_fixture_group != Some(group_id) {
                    let first_fixture = state
                        .groups
                        .get(&group_id)
                        .and_then(|group| group.fixtures.keys().next().copied())
                        .unwrap_or_default();
                    self.select_setup_fixture(dmx_engine, group_id, first_fixture);
                }
            }
            Some(TreeAction::TapFolder(folder)) => {
                self.setup_groups_ui.folder = Some(folder);
            }
            Some(TreeAction::MoveRequested(key)) => {
                let item = match key {
                    TreeKey::Folder(id) => FolderItem::Folder(id),
                    TreeKey::Leaf(id) => FolderItem::Group(id),
                };
                self.setup_groups_ui.dialog = FolderDialog::move_item(state, item);
            }
            Some(TreeAction::DropLeaf(id, folder)) => {
                if let Some(event) = components::drop_to_folder_event(
                    ui.ctx(),
                    state,
                    SETUP_GROUP_TREE,
                    FolderItem::Group(id),
                    folder,
                ) {
                    self.client
                        .send(ControlEventMessage::new(EventOriginator::Web, event));
                }
            }
            None => {}
        }
    }

    /// Name, place and contents of the selected folder.
    fn setup_folder_details(ui: &mut egui::Ui, dmx_engine: &EngineState, folder: FolderId) {
        folder_details(ui, &dmx_engine.0, FolderKind::Groups, folder);
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
        ui.horizontal(|ui| {
            if components::button(ui, false, "Edit Fixtures", ButtonSize::Medium) {
                self.open_subpage(AppSubPage::Setup_Fixtures);
            }
            if cfg!(feature = "visualizer")
                && components::button(ui, false, "Show in Visualizer", ButtonSize::Medium)
            {
                let fixtures: Vec<_> = group.fixtures.keys().map(|fid| (group_id, *fid)).collect();
                self.show_fixtures_in_visualizer(fixtures);
            }
        });
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
                        if components::button_as(
                            ui,
                            false,
                            components::ButtonVariant::Primary,
                            "Confirm",
                            ButtonSize::Large,
                        ) {
                            if let Some(id) = self.add_fixture_group {
                                self.send_setup_command(HostCommand::DeleteGroup(id), |_, _| {});
                                self.delete_group_open = false;
                                self.add_fixture_group = None;
                            }
                        }

                        if components::button_as(
                            ui,
                            true,
                            components::ButtonVariant::Tonal,
                            "Cancel",
                            ButtonSize::Large,
                        ) {
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
                    ui.spacing_mut().interact_size =
                        crate::app::ui_tokens::Density::of(ui.ctx()).interact_size();
                    Frame::new()
                        .inner_margin(Margin::symmetric(10, 6))
                        .show(ui, |ui| {
                            components::TextInput::new(180.0)
                                .with_hint_text("Group Name")
                                .ui(ui, &mut self.new_group_name);
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
                        // New groups go into the selected folder.
                        let create = HostCommand::CreateGroup {
                            name: self.new_group_name.clone(),
                            folder: self.setup_groups_ui.folder,
                        };
                        self.send_setup_command(create, |_, _| {});
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

                    // Closes once the engine has renamed the group.
                    if confirm_pressed && !self.setup_reply_pending() {
                        let trimmed = self.rename_group_name.trim().to_string();

                        if let (false, Some(group)) = (trimmed.is_empty(), self.add_fixture_group) {
                            let rename = HostCommand::RenameGroup {
                                group,
                                name: trimmed,
                            };
                            self.send_setup_command(rename, |app, _| {
                                app.rename_group_name.clear();
                                app.rename_group_dialog_open = false;
                            });
                        }
                    }
                });
        }
    }
}
