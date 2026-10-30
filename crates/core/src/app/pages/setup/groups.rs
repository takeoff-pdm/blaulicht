//! Setup › Groups: the group tree (groups in folders) with add, rename,
//! delete and move.
//!
//! The selected group is `add_fixture_group`, shared with the Fixtures tab, so
//! picking a group here and switching tabs shows its fixtures there. A folder
//! can be selected instead ([`SetupGroupsUi::folder`]); Rename and Delete act
//! on whichever is selected, and new groups and folders go into the selected
//! folder (the top level otherwise).
//!
//! Layout: toolbar on top, then [`BlaulichtApp::setup_group_tree`] on the left
//! and the details of the selected group or folder on the right. Every row has
//! a "Move…" button that opens the folder picker ([`FolderDialog::Move`]).

use crate::app::{
    components::{
        self, delete_summary, folder_path, group_tree_items, kind_noun, tree_id, ButtonSize,
        Dialog, FolderDialog, Tree, TreeAction, TreeKey, TreeState, DEFAULT_NEW_GROUP_NAME,
        SETUP_GROUP_TREE,
    },
    page::PageRenderContext,
    BlaulichtApp,
};
use crate::dmx::EngineState;
use blaulicht_shared::{
    folder::{FolderId, FolderItem, FolderKind},
    AppSubPage, ControlEvent, ControlEventMessage, EngineGroups, EventOriginator,
};
use egui::{Color32, Context, Frame, Key, Margin, RichText};
use std::time::{Duration, Instant};

/// Setup › Groups state beyond the shared `add_fixture_group`.
#[derive(Debug, Default)]
pub struct SetupGroupsUi {
    /// The selected folder. While set, it is the selection (not the group).
    pub folder: Option<FolderId>,
    /// Folder dialogs (create, rename, delete, move to…).
    pub dialog: FolderDialog,
    /// When `folder` was just created: the engine applies `CreateFolder` on
    /// its next tick, so until then (at most [`PENDING_FOLDER_GRACE`]) a
    /// missing folder is not a reason to drop the selection.
    pending_since: Option<Instant>,
}

const PENDING_FOLDER_GRACE: Duration = Duration::from_secs(2);

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
        let ui_state = &mut self.setup_groups_ui;
        if let Some(id) = ui_state.folder {
            if dmx_engine.0.folders.contains_key(&id) {
                ui_state.pending_since = None;
            } else if !ui_state
                .pending_since
                .is_some_and(|since| since.elapsed() < PENDING_FOLDER_GRACE)
            {
                ui_state.folder = None;
                ui_state.pending_since = None;
            }
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
        let expand = match &event {
            ControlEvent::CreateFolder { id, parent, .. } => {
                self.setup_groups_ui.folder = Some(*id);
                self.setup_groups_ui.pending_since = Some(Instant::now());
                *parent
            }
            ControlEvent::MoveToFolder { folder, .. } => *folder,
            ControlEvent::DeleteFolder(id) => {
                if self.setup_groups_ui.folder == Some(*id) {
                    self.setup_groups_ui.folder = None;
                }
                None
            }
            _ => None,
        };
        if let Some(folder) = expand {
            let id = tree_id(SETUP_GROUP_TREE);
            let mut state = TreeState::<FolderId>::load(ctx, id);
            state.set_collapsed(folder, false);
            state.store(ctx, id);
        }
        self.data
            .event_bus_connection
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

        if components::action_button(
            ui,
            has_selection,
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

    /// The tree of folders and groups. Tapping a group selects it (and its
    /// first fixture for the Fixtures tab); tapping a folder selects the
    /// folder; "Move…" opens the folder picker for that row.
    fn setup_group_tree(&mut self, ui: &mut egui::Ui, dmx_engine: &EngineState) {
        let state = &dmx_engine.0;
        ui.label("Groups:");
        ui.add_space(8.0);

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
                    .with_move_button(true)
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
            None => {}
        }
    }

    /// Name, place and contents of the selected folder.
    fn setup_folder_details(ui: &mut egui::Ui, dmx_engine: &EngineState, folder: FolderId) {
        let state = &dmx_engine.0;
        let (Some(f), Some(summary)) = (state.folders.get(&folder), delete_summary(state, folder))
        else {
            ui.label(RichText::new("No group selected").color(Color32::GRAY));
            return;
        };

        ui.label(RichText::new("Folder").size(12.0));
        ui.add_space(6.0);
        ui.label(RichText::new(&f.name).size(18.0).strong());
        ui.add_space(6.0);
        ui.label(format!("In: {}", folder_path(&state.folders, f.parent)));
        ui.add_space(6.0);

        let all = state.folder_group_ids(folder);
        let mut contents = format!(
            "{} {} here",
            summary.items,
            kind_noun(FolderKind::Groups, summary.items)
        );
        if summary.subfolders > 0 {
            contents += &format!(
                ", {} {}, {} {} in total",
                summary.subfolders,
                if summary.subfolders == 1 {
                    "subfolder"
                } else {
                    "subfolders"
                },
                all.len(),
                kind_noun(FolderKind::Groups, all.len()),
            );
        }
        ui.label(contents);
        ui.add_space(6.0);

        egui::ScrollArea::vertical()
            .id_salt("setup-folder-groups")
            .show(ui, |ui| {
                for gid in &all {
                    let Some(group) = state.groups.get(gid) else {
                        continue;
                    };
                    let place = if group.folder == Some(folder) {
                        String::new()
                    } else {
                        format!("  ({})", folder_path(&state.folders, group.folder))
                    };
                    ui.label(RichText::new(format!("#{gid}  {}{place}", group.name)).size(12.0));
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
                        let created = dmx_engine.create_group(self.new_group_name.clone());
                        // New groups go into the selected folder.
                        if let (Some(group), Some(folder)) = (created, self.setup_groups_ui.folder)
                        {
                            if let Err(err) = dmx_engine
                                .0
                                .move_to_folder(FolderItem::Group(group), Some(folder))
                            {
                                tracing::warn!("New group {group} stays at the top level: {err}");
                            }
                        }
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
