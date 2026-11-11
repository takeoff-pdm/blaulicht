//! Setup › Fixtures on the desktop (`setup.html`): a Groups pane with the
//! folder tree, the centre pane with the page bar ("Selected group:", count,
//! Add fixture / Overrides / Simulate) and the fixture table, and the Fixture
//! pane with the editor. Desktop only ([`Density::Compact`]); the kiosk keeps
//! [`BlaulichtApp::setup_fixtures_ui`]'s touch layout.

use crate::app::components::studio::{self, ListRow, ListRowState};
use crate::app::components::{self, tree_id, TreeState, SETUP_GROUP_TREE};
use crate::app::ui_tokens::{studio as studio_tokens, Density, Weight, TARGET_MIN, TARGET_SHOW};
use crate::app::{BlaulichtApp, PopupSpec};
use blaulicht_engine::client::EngineClient;
use blaulicht_engine::dmx::EngineState;
use blaulicht_engine::hub::{CreatedId, HostCommand};
use blaulicht_engine::state::NUM_DMX_UNIVERSES;
use blaulicht_shared::folder::{FolderId, FolderKind};
use egui::{pos2, vec2, Align2, Context, Rect, RichText, Sense, Stroke, StrokeKind, Ui};
use std::time::Duration;

/// Width of the Groups pane (`setup.html`).
const GROUPS_W: f32 = 210.0;
/// Width of the Fixture pane.
const FIXTURE_W: f32 = 300.0;
/// The 1px gutter between panes.
const GAP: f32 = 1.0;

/// The group folders of the desktop tree under `parent`, in `(order, id)`
/// order: siblings nobody reordered keep the order they were made in, like
/// the mockup.
fn child_folders(state: &EngineState, parent: Option<FolderId>) -> Vec<FolderId> {
    let mut ids: Vec<(i64, FolderId)> = state
        .0
        .folders
        .iter()
        .filter(|(_, f)| f.kind == FolderKind::Groups && f.parent == parent)
        .map(|(id, f)| (f.order as i64, *id))
        .collect();
    ids.sort();
    ids.into_iter().map(|(_, id)| id).collect()
}

impl BlaulichtApp {
    pub(in crate::app::pages::setup) fn setup_fixtures_desktop_ui(
        &mut self,
        ui: &mut Ui,
        ctx: &Context,
        dmx_engine: &EngineState,
    ) {
        let t = studio_tokens(ctx);
        // The panes fill the page edge to edge (the page frame's margin
        // included), with the gutter colour between them.
        let full = ui.max_rect().expand(8.0).intersect(ui.clip_rect());
        ui.allocate_rect(ui.max_rect(), Sense::hover());
        ui.painter().rect_filled(full, 0.0, t.gutter);

        // Pick a group so the page is never empty while groups exist.
        if self.add_fixture_group.is_none() {
            if let Some(gid) = dmx_engine.groups().keys().next().copied() {
                let first = dmx_engine.groups()[&gid]
                    .fixtures
                    .keys()
                    .next()
                    .copied()
                    .unwrap_or(0);
                self.select_setup_fixture(dmx_engine, gid, first);
            }
        }

        let groups_rect = Rect::from_min_size(full.min, vec2(GROUPS_W, full.height()));
        let fixture_rect = Rect::from_min_max(pos2(full.right() - FIXTURE_W, full.top()), full.max);
        let centre_rect = Rect::from_min_max(
            pos2(groups_rect.right() + GAP, full.top()),
            pos2(fixture_rect.left() - GAP, full.bottom()),
        );

        let mut pane = |rect: Rect, fill| {
            ui.painter().rect_filled(rect, 0.0, fill);
            let mut child = ui.new_child(
                egui::UiBuilder::new()
                    .max_rect(rect)
                    .layout(egui::Layout::top_down(egui::Align::Min)),
            );
            child.set_clip_rect(rect);
            child.spacing_mut().item_spacing = vec2(0.0, 0.0);
            child
        };
        let mut groups_ui = pane(groups_rect, t.list);
        let mut centre_ui = pane(centre_rect, t.list);
        let mut fixture_ui = pane(fixture_rect, t.pane);

        self.desktop_groups_pane(&mut groups_ui, dmx_engine);
        self.desktop_centre_pane(&mut centre_ui, dmx_engine);
        self.desktop_fixture_pane(&mut fixture_ui, ctx, dmx_engine);
    }

    /// The Groups pane: a 40px header and the folder tree, member rows with
    /// their fixture count.
    fn desktop_groups_pane(&mut self, ui: &mut Ui, dmx_engine: &EngineState) {
        studio::pane_header(ui, None, "Groups", true, |_| {});
        let id = tree_id(SETUP_GROUP_TREE);
        let mut state = TreeState::<FolderId>::load(ui.ctx(), id);
        let mut picked = None;
        let mut toggled = None;
        egui::ScrollArea::vertical()
            .id_salt("setup-fixtures-groups")
            .auto_shrink([false, false])
            .show(ui, |ui| {
                ui.spacing_mut().item_spacing = vec2(0.0, 0.0);
                let mut first = true;
                self.desktop_group_level(
                    ui,
                    dmx_engine,
                    None,
                    &state,
                    &mut first,
                    &mut picked,
                    &mut toggled,
                );
            });
        if let Some(folder) = toggled {
            state.toggle(folder);
            state.store(ui.ctx(), id);
        }
        if let Some(gid) = picked {
            let first = dmx_engine
                .groups()
                .get(&gid)
                .and_then(|g| g.fixtures.keys().next().copied())
                .unwrap_or(0);
            self.select_setup_fixture(dmx_engine, gid, first);
        }
    }

    /// One level of the group tree: folders (bands, then their contents when
    /// open), then the groups filed directly here.
    #[allow(clippy::too_many_arguments)]
    fn desktop_group_level(
        &self,
        ui: &mut Ui,
        dmx_engine: &EngineState,
        parent: Option<FolderId>,
        state: &TreeState<FolderId>,
        first: &mut bool,
        picked: &mut Option<u8>,
        toggled: &mut Option<FolderId>,
    ) {
        let groups = dmx_engine.groups();
        for folder in child_folders(dmx_engine, parent) {
            let name = &dmx_engine.0.folders[&folder].name;
            let open = !state.is_collapsed(folder);
            let members = groups.values().filter(|g| g.folder == Some(folder)).count();
            if studio::folder_band(ui, name, Some(members), open, *first).clicked() {
                *toggled = Some(folder);
            }
            *first = false;
            if open {
                self.desktop_group_level(
                    ui,
                    dmx_engine,
                    Some(folder),
                    state,
                    first,
                    picked,
                    toggled,
                );
            }
        }
        for (gid, group) in groups.iter().filter(|(_, g)| g.folder == parent) {
            let active = self.add_fixture_group == Some(*gid);
            let row = ListRow::new(&group.name)
                .count(group.fixtures.len())
                .in_folder(parent.is_some())
                .state(if active {
                    ListRowState::Active
                } else {
                    ListRowState::Idle
                })
                .show(ui);
            *first = false;
            if row.clicked() {
                *picked = Some(*gid);
            }
        }
    }

    /// The centre pane: page bar, filter field and the fixture table of the
    /// selected group.
    fn desktop_centre_pane(&mut self, ui: &mut Ui, dmx_engine: &EngineState) {
        let t = studio_tokens(ui.ctx());
        let density = Density::of(ui.ctx());
        let group = self
            .add_fixture_group
            .and_then(|gid| dmx_engine.groups().get(&gid).map(|g| (gid, g)));
        let name = group.map_or("None", |(_, g)| g.name.as_str()).to_owned();
        let count = group.map_or(0, |(_, g)| g.fixtures.len());

        let mut add = false;
        let mut overrides = false;
        let mut simulate = false;
        let (add_on, overrides_on, simulate_on) = (
            self.add_fixture_open,
            self.dmx_override_dialog_open,
            self.dmx_simulator.open,
        );
        studio::page_bar(
            ui,
            |ui| {
                ui.label(
                    RichText::new("Selected group:")
                        .font(density.font(Weight::Regular, 13.0))
                        .color(t.muted),
                );
                ui.add_space(6.0);
                ui.label(
                    RichText::new(&name)
                        .font(density.font(Weight::Medium, 13.0))
                        .color(t.strong),
                );
            },
            |ui| {
                ui.label(
                    RichText::new(count.to_string())
                        .font(density.font(Weight::SemiBold, 13.0))
                        .color(t.strong),
                );
                ui.add_space(4.0);
                ui.spacing_mut().item_spacing.x = 2.0;
                add = studio::mini_button(ui, "Add fixture", add_on, TARGET_SHOW).clicked();
                overrides =
                    studio::mini_button(ui, "Overrides", overrides_on, TARGET_SHOW).clicked();
                simulate = studio::mini_button(ui, "Simulate", simulate_on, TARGET_SHOW).clicked();
            },
        );
        if add {
            self.toggle_add_fixture(dmx_engine);
        }
        if overrides {
            self.toggle_dmx_overrides();
        }
        if simulate {
            self.dmx_simulator.open = !self.dmx_simulator.open;
        }

        // Filter field (`input`): well fill, 1px line.
        let filter_id = ui.id().with("setup-fixture-filter");
        let mut filter: String = ui.data(|d| d.get_temp(filter_id)).unwrap_or_default();
        ui.add_space(4.0);
        ui.horizontal(|ui| {
            ui.add_space(4.0);
            let w = ui.available_width() - 4.0;
            let (rect, _) = ui.allocate_exact_size(vec2(w, TARGET_MIN), Sense::hover());
            ui.painter().rect_filled(rect, 0.0, t.well);
            ui.painter()
                .rect_stroke(rect, 0.0, Stroke::new(1.0_f32, t.line), StrokeKind::Inside);
            let mut child =
                ui.new_child(egui::UiBuilder::new().max_rect(rect.shrink2(vec2(6.0, 0.0))));
            child.add_sized(
                child.available_size(),
                egui::TextEdit::singleline(&mut filter)
                    .frame(egui::Frame::NONE)
                    .hint_text("Filter fixtures")
                    .font(density.font(Weight::Regular, 12.0))
                    .vertical_align(egui::Align::Center),
            );
        });
        ui.data_mut(|d| d.insert_temp(filter_id, filter.clone()));
        ui.add_space(4.0);

        // Table header: pane grey, muted column names.
        let width = ui.available_width();
        let cols = [0.0, 0.27, 0.57, 0.73];
        let (head, _) = ui.allocate_exact_size(vec2(width, 22.0), Sense::hover());
        ui.painter().rect_filled(head, 0.0, t.pane);
        let f = density.font(Weight::Regular, 12.0);
        for (label, x) in ["Fixture", "Type", "Universe", "Channels"].iter().zip(cols) {
            ui.painter().text(
                pos2(head.left() + 8.0 + x * (width - 16.0), head.center().y),
                Align2::LEFT_CENTER,
                label,
                f.clone(),
                t.muted,
            );
        }
        ui.painter().text(
            pos2(head.right() - 8.0, head.center().y),
            Align2::RIGHT_CENTER,
            "Ch",
            f.clone(),
            t.muted,
        );

        let Some((gid, group)) = group else {
            return;
        };
        let needle = filter.trim().to_lowercase();
        let mut picked = None;
        egui::ScrollArea::vertical()
            .id_salt("setup-fixtures-table")
            .auto_shrink([false, false])
            .show(ui, |ui| {
                ui.spacing_mut().item_spacing = vec2(0.0, 0.0);
                let mono = egui::FontId::monospace(12.0);
                for (fid, fix) in &group.fixtures {
                    let model = fix.type_.model_string();
                    if !needle.is_empty()
                        && !fix.name.to_lowercase().contains(&needle)
                        && !model.to_lowercase().contains(&needle)
                    {
                        continue;
                    }
                    let (rect, response) =
                        ui.allocate_exact_size(vec2(width, 24.0), Sense::click());
                    let active = self.setup_fixture_id == *fid;
                    let p = ui.painter();
                    if active {
                        p.rect_filled(rect, 0.0, t.blue_dim);
                        p.rect_filled(
                            Rect::from_min_size(rect.min, vec2(2.0, rect.height())),
                            0.0,
                            t.blue,
                        );
                    } else if response.hovered() {
                        p.rect_filled(rect, 0.0, t.head);
                    }
                    let text = if active { t.strong } else { t.text };
                    let col = |i: usize| rect.left() + 8.0 + cols[i] * (width - 16.0);
                    let cy = rect.center().y;
                    let footprint = fix.type_.footprint();
                    let end = fix.start_addr + footprint.saturating_sub(1);
                    p.text(
                        pos2(col(0), cy),
                        Align2::LEFT_CENTER,
                        &fix.name,
                        f.clone(),
                        text,
                    );
                    p.text(
                        pos2(col(1), cy),
                        Align2::LEFT_CENTER,
                        &model,
                        f.clone(),
                        text,
                    );
                    p.text(
                        pos2(col(2), cy),
                        Align2::LEFT_CENTER,
                        fix.universe_no.to_string(),
                        mono.clone(),
                        text,
                    );
                    p.text(
                        pos2(col(3), cy),
                        Align2::LEFT_CENTER,
                        format!("{:03}-{:03}", fix.start_addr, end),
                        mono.clone(),
                        t.strong,
                    );
                    p.text(
                        pos2(rect.right() - 8.0, cy),
                        Align2::RIGHT_CENTER,
                        footprint.to_string(),
                        f.clone(),
                        text,
                    );
                    if response.clicked() {
                        picked = Some(*fid);
                    }
                }
            });
        if let Some(fid) = picked {
            self.select_setup_fixture(dmx_engine, gid, fid);
        }
    }

    /// The Fixture pane: a 40px header and the editor of the selected
    /// fixture. Number fields scrub (drag sideways, Shift finer, Esc
    /// restores); Save applies them.
    fn desktop_fixture_pane(&mut self, ui: &mut Ui, ctx: &Context, dmx_engine: &EngineState) {
        let t = studio_tokens(ui.ctx());
        let density = Density::of(ui.ctx());
        studio::pane_header(ui, None, "Fixture", true, |_| {});

        let selected = self.add_fixture_group.and_then(|gid| {
            dmx_engine
                .groups()
                .get(&gid)
                .and_then(|g| g.fixtures.get(&self.setup_fixture_id))
                .map(|fix| (gid, fix.clone()))
        });
        let mut body = ui.new_child(
            egui::UiBuilder::new()
                .max_rect(ui.available_rect_before_wrap().shrink(8.0))
                .layout(egui::Layout::top_down(egui::Align::Min)),
        );
        let ui = &mut body;
        ui.spacing_mut().item_spacing = vec2(6.0, 4.0);
        let Some((gid, fix)) = selected else {
            ui.add(
                egui::Label::new(
                    RichText::new("Select a fixture to edit it, or add a new one.")
                        .font(density.font(Weight::Regular, 12.0))
                        .color(t.text),
                )
                .wrap(),
            );
            return;
        };
        let fid = self.setup_fixture_id;
        let label_font = density.font(Weight::Regular, 12.0);
        let row = |ui: &mut Ui, label: &str, add: &mut dyn FnMut(&mut Ui)| {
            ui.horizontal(|ui| {
                let (r, _) = ui.allocate_exact_size(vec2(90.0, TARGET_MIN), Sense::hover());
                ui.painter().text(
                    pos2(r.left(), r.center().y),
                    Align2::LEFT_CENTER,
                    label,
                    label_font.clone(),
                    t.muted,
                );
                add(ui);
            });
        };

        row(ui, "Name", &mut |ui| {
            ui.add_sized(
                vec2(ui.available_width(), TARGET_MIN),
                egui::TextEdit::singleline(&mut self.new_fixture_name)
                    .font(density.font(Weight::Regular, 12.0))
                    .vertical_align(egui::Align::Center),
            );
        });
        row(ui, "Type", &mut |ui| {
            ui.label(
                RichText::new(fix.type_.model_string())
                    .font(density.font(Weight::Regular, 12.0))
                    .color(t.text),
            );
        });
        let field_w = 110.0;
        let mut addr = self.new_fixture_addr as f64;
        row(ui, "Start addr", &mut |ui| {
            studio::num_field(ui, &mut addr, 1.0, 1.0..=512.0, 0, field_w);
        });
        self.new_fixture_addr = addr.round() as usize;
        let mut uni = self.new_fixture_uni as f64;
        row(ui, "Universe", &mut |ui| {
            studio::num_field(
                ui,
                &mut uni,
                1.0,
                0.0..=(NUM_DMX_UNIVERSES - 1) as f64,
                0,
                field_w,
            );
        });
        self.new_fixture_uni = uni.round() as usize;
        for (label, value, step, decimals, range) in [
            (
                "Position X",
                &mut self.new_fixture_pos_x,
                0.1,
                1,
                -1000.0..=1000.0,
            ),
            (
                "Position Y",
                &mut self.new_fixture_pos_y,
                0.1,
                1,
                -1000.0..=1000.0,
            ),
            (
                "Position Z",
                &mut self.new_fixture_pos_z,
                0.1,
                1,
                -1000.0..=1000.0,
            ),
            (
                "Rotation X",
                &mut self.new_fixture_rot_x,
                1.0,
                0,
                -360.0..=360.0,
            ),
            (
                "Rotation Y",
                &mut self.new_fixture_rot_y,
                1.0,
                0,
                -360.0..=360.0,
            ),
            (
                "Rotation Z",
                &mut self.new_fixture_rot_z,
                1.0,
                0,
                -360.0..=360.0,
            ),
        ] {
            let mut v = *value as f64;
            row(ui, label, &mut |ui| {
                studio::num_field(ui, &mut v, step, range.clone(), decimals, field_w);
            });
            *value = v as f32;
        }

        ui.add_space(8.0);
        let group = &dmx_engine.groups()[&gid];
        let prev_fid = group.fixtures.range(..fid).next_back().map(|(k, _)| *k);
        let next_fid = group
            .fixtures
            .range((std::ops::Bound::Excluded(fid), std::ops::Bound::Unbounded))
            .next()
            .map(|(k, _)| *k);

        ui.horizontal_wrapped(|ui| {
            ui.spacing_mut().item_spacing = vec2(2.0, 2.0);
            let can_save = (1..=512).contains(&self.new_fixture_addr);
            if studio::mini_button(ui, "Save", false, TARGET_MIN).clicked() && can_save {
                let mut updated = fix.clone();
                updated.name = self.new_fixture_name.clone();
                updated.start_addr = self.new_fixture_addr;
                updated.universe_no = self.new_fixture_uni;
                updated.pos.x = self.new_fixture_pos_x;
                updated.pos.y = self.new_fixture_pos_y;
                updated.pos.z = self.new_fixture_pos_z;
                updated.rotation.x = self.new_fixture_rot_x;
                updated.rotation.y = self.new_fixture_rot_y;
                updated.rotation.z = self.new_fixture_rot_z;
                self.send_setup_command(
                    HostCommand::UpdateFixture {
                        group: gid,
                        fixture: fid,
                        value: updated,
                    },
                    |_, _| {},
                );
            }
            if studio::mini_button(ui, "Move up", false, TARGET_MIN).clicked() {
                if let Some(prev) = prev_fid {
                    self.swap_setup_fixtures(gid, fid, prev);
                }
            }
            if studio::mini_button(ui, "Move down", false, TARGET_MIN).clicked() {
                if let Some(next) = next_fid {
                    self.swap_setup_fixtures(gid, fid, next);
                }
            }
            if studio::mini_button(ui, "Move to group", false, TARGET_MIN).clicked()
                && dmx_engine.groups().len() > 1
            {
                self.move_fixture_group_dialog_open = true;
            }
        });
        ui.horizontal_wrapped(|ui| {
            ui.spacing_mut().item_spacing = vec2(2.0, 2.0);
            // Lit for the 10 s the fixture initializes.
            let init_id = egui::Id::new(("fixture_initializing", gid, fid));
            let since: Option<f64> = ui.data(|d| d.get_temp(init_id));
            let now = ui.input(|i| i.time);
            let on = since.is_some_and(|t| now - t < 10.0);
            if on {
                ui.ctx().request_repaint_after(Duration::from_millis(250));
            }
            if studio::live_button(ui, "Initialize", on, TARGET_MIN).clicked() && !on {
                ui.data_mut(|d| d.insert_temp(init_id, now));
                let label = format!("Initializing `{}` (10 s)", self.new_fixture_name);
                self.send_setup_command(
                    HostCommand::InitFixtures(vec![(gid, fid)]),
                    move |app, _| {
                        app.show_popup(PopupSpec::with_duration(Duration::from_secs(3), label));
                    },
                );
            }
            if cfg!(feature = "visualizer")
                && studio::mini_button(ui, "Show in Visualizer", false, TARGET_MIN).clicked()
            {
                self.show_fixtures_in_visualizer([(gid, fid)]);
            }
            if studio::mini_button(ui, "Delete", false, TARGET_MIN).clicked() {
                self.delete_fixture = Some((gid, fid));
            }
        });

        if self.move_fixture_group_dialog_open {
            let options: Vec<(u8, String)> = dmx_engine
                .groups()
                .iter()
                .filter(|(target, _)| **target != gid)
                .map(|(target, g)| (*target, format!("{} | #{}", g.name, target)))
                .collect();
            let current = options.first().map(|(g, _)| *g).unwrap_or(gid);
            let (target, changed) = components::id_selection_dialog(
                ctx,
                options,
                current,
                &mut self.move_fixture_group_dialog_open,
                "Move Fixture To Group".to_string(),
            );
            if changed {
                self.send_setup_command(
                    HostCommand::MoveFixtureToGroup {
                        group: gid,
                        fixture: fid,
                        target,
                    },
                    |app, created| {
                        if let Some(CreatedId::Fixture { group, fixture }) = created {
                            let moved = app.client.with_show(|show| {
                                show.0
                                    .groups
                                    .get(&group)
                                    .and_then(|g| g.fixtures.get(&fixture))
                                    .cloned()
                            });
                            app.add_fixture_group = Some(group);
                            app.setup_fixture_id = fixture;
                            if let Some(fix) = moved {
                                app.load_fixture_into_editor(&fix);
                            }
                        }
                    },
                );
            }
        }
    }
}
