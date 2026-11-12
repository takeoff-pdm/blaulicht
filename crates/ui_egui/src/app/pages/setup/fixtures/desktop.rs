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
use blaulicht_engine::hub::HostCommand;
use blaulicht_engine::state::NUM_DMX_UNIVERSES;
use blaulicht_shared::folder::{FolderId, FolderKind};
use egui::{pos2, vec2, Align2, Context, Id, Rect, RichText, Sense, Stroke, StrokeKind, Ui};
use std::collections::BTreeSet;
use std::time::Duration;

/// Width of the Groups pane (`setup.html`).
const GROUPS_W: f32 = 210.0;
/// Width of the Fixture pane.
const FIXTURE_W: f32 = 300.0;
/// The 1px gutter between panes.
const GAP: f32 = 1.0;
/// Label column of the Fixture pane's multi-selection form (`.frm`: 84px
/// plus the gap).
const FORM_LABEL_W: f32 = 86.0;

/// The fixtures selected in the table (`st.fxs` in `setup.html`), kept in
/// egui temp memory for the group it was made in. Another group (or none
/// stored yet) means "the fixture in the editor".
#[derive(Clone, Debug, Default, PartialEq, Eq)]
struct FixtureSelection {
    group: u8,
    ids: BTreeSet<u8>,
}

fn selection_id() -> Id {
    Id::new("setup_fixtures_selection")
}

/// A drag of fixture rows onto a group of the tree (`treedrag.js`): the
/// fixtures it carries, from `group`.
#[derive(Clone, Debug, Default)]
struct FixtureDrag {
    group: u8,
    ids: Vec<u8>,
    label: String,
}

fn drag_id() -> Id {
    Id::new("setup_fixtures_drag")
}

/// The confirm step of Delete (`del("fx")` in the mockup), for this
/// selection.
fn delete_confirm_id() -> Id {
    Id::new("setup_fixtures_delete_confirm")
}

/// Applies a click on fixture `id` of a table in `order` to `selected`
/// (`setup.html`): a plain click selects only it, Ctrl/Cmd toggles it,
/// Shift adds the range from the `anchor` (replacing the selection unless
/// Ctrl/Cmd is held too). `anchor` is the fixture in the editor.
fn click_select(
    order: &[u8],
    selected: &mut BTreeSet<u8>,
    anchor: &mut Option<u8>,
    id: u8,
    toggle: bool,
    range: bool,
) {
    let from = anchor.and_then(|a| order.iter().position(|x| *x == a));
    let to = order.iter().position(|x| *x == id);
    if let (true, Some(a), Some(b)) = (range, from, to) {
        let (a, b) = if a <= b { (a, b) } else { (b, a) };
        if !toggle {
            selected.clear();
        }
        selected.extend(order[a..=b].iter().copied());
    } else if toggle {
        if !selected.remove(&id) {
            selected.insert(id);
        }
        *anchor = if selected.contains(&id) {
            Some(id)
        } else {
            order.iter().rev().find(|x| selected.contains(x)).copied()
        };
    } else {
        selected.clear();
        selected.insert(id);
        *anchor = Some(id);
    }
}

/// "Moved MAC250E-0 #1 to DIMMER" or "Moved 3 fixtures to DIMMER".
fn moved_text(names: &[String], to: &str) -> String {
    match names {
        [one] => format!("Moved {one} to {to}"),
        _ => format!("Moved {} fixtures to {to}", names.len()),
    }
}

/// The Delete confirm label: "Confirm delete" for one fixture, "Delete N
/// fixtures" for several.
fn delete_label(count: usize) -> String {
    if count > 1 {
        format!("Delete {count} fixtures")
    } else {
        "Confirm delete".to_owned()
    }
}

/// The Delete confirm (`.mini.confirm`): a 28px mini button with red text.
fn confirm_button(ui: &mut Ui, label: &str) -> egui::Response {
    let t = studio_tokens(ui.ctx());
    let f = Density::of(ui.ctx()).font(Weight::Regular, 12.0);
    let galley = ui
        .painter()
        .layout_no_wrap(label.to_owned(), f.clone(), t.red_text);
    let w = galley.size().x + 20.0;
    let (rect, response) = ui.allocate_exact_size(vec2(w, TARGET_SHOW), Sense::click());
    let fill = if response.hovered() { t.ctl_hi } else { t.ctl };
    let p = ui.painter();
    p.rect_filled(rect, 0.0, fill);
    if response.has_focus() {
        p.rect_stroke(rect, 0.0, Stroke::new(1.0_f32, t.blue), StrokeKind::Inside);
    }
    p.text(rect.center(), Align2::CENTER_CENTER, label, f, t.red_text);
    response.widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::Button, true, label));
    response
}

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
                self.pick_setup_group(ctx, gid);
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

        let group_rows = self.desktop_groups_pane(&mut groups_ui, dmx_engine);
        self.desktop_centre_pane(&mut centre_ui, dmx_engine);
        self.desktop_fixture_pane(&mut fixture_ui, ctx, dmx_engine);
        self.desktop_fixture_drag(ui, dmx_engine, &group_rows, groups_rect);
    }

    /// The selected fixtures of the shown group, in table order. Without a
    /// selection made in this group it is the fixture in the editor.
    fn desktop_fixture_selection(&self, ctx: &Context, dmx_engine: &EngineState) -> Vec<u8> {
        let Some(gid) = self.add_fixture_group else {
            return Vec::new();
        };
        let Some(group) = dmx_engine.groups().get(&gid) else {
            return Vec::new();
        };
        let stored: Option<FixtureSelection> = ctx.data(|d| d.get_temp(selection_id()));
        match stored.filter(|s| s.group == gid) {
            Some(s) => group
                .fixtures
                .keys()
                .filter(|id| s.ids.contains(id))
                .copied()
                .collect(),
            None => group
                .fixtures
                .contains_key(&self.setup_fixture_id)
                .then_some(vec![self.setup_fixture_id])
                .unwrap_or_default(),
        }
    }

    /// Shows group `gid` with nothing selected, as picking a group in
    /// setup.html does (`st.fx = null; st.fxs.clear()`).
    fn pick_setup_group(&mut self, ctx: &Context, gid: u8) {
        self.close_edit_fixture_numberpads();
        self.add_fixture_group = Some(gid);
        Self::store_fixture_selection(ctx, gid, BTreeSet::new());
    }

    fn store_fixture_selection(ctx: &Context, group: u8, ids: BTreeSet<u8>) {
        ctx.data_mut(|d| d.insert_temp(selection_id(), FixtureSelection { group, ids }));
        ctx.data_mut(|d| d.remove_temp::<bool>(delete_confirm_id()));
    }

    /// Moves fixtures `ids` of group `from` to group `to` (Move to group or
    /// a drag onto the tree), keeping their patch. Clears the selection and
    /// shows "Moved …" once the last move is done; there is no undo.
    fn move_fixtures_to_group(
        &mut self,
        ctx: &Context,
        dmx_engine: &EngineState,
        from: u8,
        ids: &[u8],
        to: u8,
    ) {
        let groups = dmx_engine.groups();
        let (Some(source), Some(target)) = (groups.get(&from), groups.get(&to)) else {
            return;
        };
        if from == to {
            return;
        }
        let ids: Vec<u8> = ids
            .iter()
            .filter(|id| source.fixtures.contains_key(id))
            .copied()
            .collect();
        if ids.is_empty() {
            return;
        }
        let names: Vec<String> = ids
            .iter()
            .map(|id| source.fixtures[id].name.to_string())
            .collect();
        let text = moved_text(&names, &target.name);
        Self::store_fixture_selection(ctx, from, BTreeSet::new());
        let last = ids.len() - 1;
        for (i, fixture) in ids.into_iter().enumerate() {
            let command = HostCommand::MoveFixtureToGroup {
                group: from,
                fixture,
                target: to,
            };
            if i == last {
                let (ctx, text) = (ctx.clone(), text.clone());
                self.send_setup_command(command, move |_, _| {
                    components::notify_info(&ctx, text);
                });
            } else {
                self.send_setup_command(command, |_, _| {});
            }
        }
    }

    /// Fixture rows dragged onto a group of the tree (`treedrag.js`): the
    /// target row is outlined, a ghost names what is carried, release
    /// moves, Esc cancels.
    fn desktop_fixture_drag(
        &mut self,
        ui: &mut Ui,
        dmx_engine: &EngineState,
        group_rows: &[(u8, Rect)],
        groups_rect: Rect,
    ) {
        let ctx = ui.ctx().clone();
        let Some(drag) = ctx.data(|d| d.get_temp::<FixtureDrag>(drag_id())) else {
            return;
        };
        if ctx.input_mut(|i| i.consume_key(egui::Modifiers::NONE, egui::Key::Escape)) {
            ctx.data_mut(|d| d.remove_temp::<FixtureDrag>(drag_id()));
            ctx.request_repaint();
            return;
        }
        let (pointer, down) = ctx.input(|i| (i.pointer.latest_pos(), i.pointer.primary_down()));
        let target = pointer.filter(|p| groups_rect.contains(*p)).and_then(|p| {
            group_rows
                .iter()
                .find(|(gid, rect)| *gid != drag.group && rect.contains(p))
                .copied()
        });
        if !down {
            ctx.data_mut(|d| d.remove_temp::<FixtureDrag>(drag_id()));
            ctx.request_repaint();
            if let Some((to, _)) = target {
                self.move_fixtures_to_group(&ctx, dmx_engine, drag.group, &drag.ids, to);
            }
            return;
        }
        let t = studio_tokens(&ctx);
        if let Some((_, rect)) = target {
            ui.painter().with_clip_rect(groups_rect).rect_stroke(
                rect,
                0.0,
                Stroke::new(1.0_f32, t.blue),
                StrokeKind::Inside,
            );
        }
        ctx.set_cursor_icon(egui::CursorIcon::Grabbing);
        if let Some(pointer) = pointer {
            // The ghost (`.treeghost`), like the folder trees' (073).
            let painter = ctx.layer_painter(egui::LayerId::new(
                egui::Order::Tooltip,
                drag_id().with("ghost"),
            ));
            let ok = target.is_some();
            let text = if ok { t.strong } else { t.muted };
            let galley = painter.layout_no_wrap(
                drag.label.clone(),
                Density::of(&ctx).font(Weight::Regular, 12.0),
                text,
            );
            let min = pointer + vec2(12.0, 8.0);
            let rect = Rect::from_min_size(min, galley.size() + vec2(16.0, 6.0));
            painter.rect_filled(rect, 0.0, t.ctl);
            painter.rect_stroke(
                rect,
                0.0,
                Stroke::new(1.0_f32, if ok { t.blue } else { t.line }),
                StrokeKind::Outside,
            );
            painter.galley(min + vec2(8.0, 3.0), galley, text);
        }
        ctx.request_repaint();
    }

    /// The Groups pane: a 40px header and the folder tree, member rows with
    /// their fixture count. Returns the group rows drawn, as drop targets
    /// for a fixture drag.
    fn desktop_groups_pane(&mut self, ui: &mut Ui, dmx_engine: &EngineState) -> Vec<(u8, Rect)> {
        studio::pane_header(ui, None, "Groups", true, |_| {});
        let id = tree_id(SETUP_GROUP_TREE);
        let mut state = TreeState::<FolderId>::load(ui.ctx(), id);
        let mut picked = None;
        let mut toggled = None;
        let mut rows = Vec::new();
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
                    &mut rows,
                );
            });
        if let Some(folder) = toggled {
            state.toggle(folder);
            state.store(ui.ctx(), id);
        }
        if let Some(gid) = picked {
            self.pick_setup_group(ui.ctx(), gid);
        }
        rows
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
        rows: &mut Vec<(u8, Rect)>,
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
                    rows,
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
            rows.push((*gid, row.rect.intersect(ui.clip_rect())));
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
        let selected: BTreeSet<u8> = self
            .desktop_fixture_selection(ui.ctx(), dmx_engine)
            .into_iter()
            .collect();
        let dragging: Option<FixtureDrag> = ui.data(|d| d.get_temp(drag_id()));
        let mut picked = None;
        let mut drag_from = None;
        let mut order = Vec::new();
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
                    order.push(*fid);
                    let (rect, response) =
                        ui.allocate_exact_size(vec2(width, 24.0), Sense::click_and_drag());
                    let active = selected.contains(fid);
                    // `.row.dragsrc`: the carried rows at 40 %.
                    let carried = dragging
                        .as_ref()
                        .is_some_and(|d| d.group == gid && d.ids.contains(fid));
                    let mut p = ui.painter().clone();
                    if carried {
                        p.set_opacity(0.4);
                    }
                    if active {
                        p.rect_filled(rect, 0.0, t.blue_dim);
                        p.rect_filled(
                            Rect::from_min_size(rect.min, vec2(2.0, rect.height())),
                            0.0,
                            t.blue,
                        );
                    } else if response.hovered() && dragging.is_none() {
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
                        let m = ui.input(|i| i.modifiers);
                        picked = Some((*fid, m.command || m.ctrl, m.shift));
                    }
                    if response.drag_started() {
                        drag_from = Some(*fid);
                    }
                }
            });
        if let Some((fid, toggle, range)) = picked {
            let mut sel = selected.clone();
            let mut anchor = group
                .fixtures
                .contains_key(&self.setup_fixture_id)
                .then_some(self.setup_fixture_id);
            click_select(&order, &mut sel, &mut anchor, fid, toggle, range);
            match anchor {
                Some(a) if a != self.setup_fixture_id => {
                    self.select_setup_fixture(dmx_engine, gid, a);
                }
                // Reload the editor when its fixture is the only one again.
                Some(a) if sel.len() == 1 && selected.len() != 1 => {
                    self.select_setup_fixture(dmx_engine, gid, a);
                }
                _ => {}
            }
            Self::store_fixture_selection(ui.ctx(), gid, sel);
        }
        if let Some(fid) = drag_from {
            // A selected row carries the whole selection.
            let ids: Vec<u8> = if selected.contains(&fid) {
                selected.iter().copied().collect()
            } else {
                vec![fid]
            };
            let label = match ids.as_slice() {
                [one] => group.fixtures[one].name.to_string(),
                _ => format!("{} fixtures", ids.len()),
            };
            ui.data_mut(|d| {
                d.insert_temp(
                    drag_id(),
                    FixtureDrag {
                        group: gid,
                        ids,
                        label,
                    },
                )
            });
        }
    }

    /// The Fixture pane: a 40px header and the editor of the selected
    /// fixture. Number fields scrub (drag sideways, Shift finer, Esc
    /// restores); Save applies them.
    fn desktop_fixture_pane(&mut self, ui: &mut Ui, ctx: &Context, dmx_engine: &EngineState) {
        let t = studio_tokens(ui.ctx());
        let density = Density::of(ui.ctx());
        let ids = self.desktop_fixture_selection(ctx, dmx_engine);
        let group = self
            .add_fixture_group
            .and_then(|gid| dmx_engine.groups().get(&gid).map(|g| (gid, g)));
        self.desktop_fixture_header(ui, ctx, dmx_engine, &ids);

        if let (Some((gid, group)), true) = (group, ids.len() > 1) {
            let fixtures: Vec<_> = ids.iter().filter_map(|id| group.fixtures.get(id)).collect();
            let mut body = ui.new_child(
                egui::UiBuilder::new()
                    .max_rect(ui.available_rect_before_wrap().shrink(8.0))
                    .layout(egui::Layout::top_down(egui::Align::Min)),
            );
            self.desktop_multi_editor(&mut body, gid, &ids, &fixtures);
            return;
        }
        let selected = group.and_then(|(gid, g)| {
            ids.first()
                .and_then(|id| g.fixtures.get(id))
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
                    RichText::new(
                        "Select a fixture to edit it, or add a new one. Ctrl or Cmd+click \
                     selects several, Shift+click a range.",
                    )
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
        });
    }

    /// The Fixture pane's 40px title bar: "Fixture: <name>" or "Fixtures:
    /// N selected", and for a selection Move to group (the group dropdown),
    /// a divider and Delete with its confirm step naming the count.
    fn desktop_fixture_header(
        &mut self,
        ui: &mut Ui,
        ctx: &Context,
        dmx_engine: &EngineState,
        ids: &[u8],
    ) {
        let t = studio_tokens(ctx);
        let Some(gid) = self.add_fixture_group else {
            studio::pane_header(ui, None, "Fixture", true, |_| {});
            return;
        };
        let groups = dmx_engine.groups();
        let name = match ids {
            [] => String::new(),
            [one] => groups
                .get(&gid)
                .and_then(|g| g.fixtures.get(one))
                .map(|f| f.name.to_string())
                .unwrap_or_default(),
            _ => format!("{} selected", ids.len()),
        };
        let (prefix, name) = match ids.len() {
            0 => (None, "Fixture".to_owned()),
            1 => (Some("Fixture:"), name),
            _ => (Some("Fixtures:"), name),
        };
        let confirm = ctx.data(|d| d.get_temp::<bool>(delete_confirm_id()).unwrap_or(false));
        // The confirm step needs the whole bar in a 300px pane.
        let (prefix, name) = if confirm && !ids.is_empty() {
            (None, String::new())
        } else {
            (prefix, name)
        };
        let mut move_button = None;
        let (mut ask, mut delete, mut cancel) = (false, false, false);
        let dd = Id::new("setup_fixtures_move_to_group");
        studio::pane_header(ui, prefix, &name, true, |ui| {
            if ids.is_empty() {
                return;
            }
            ui.spacing_mut().item_spacing.x = 2.0;
            let open = components::dropdown::is_open(ui.ctx(), dd);
            move_button = Some(studio::mini_button(ui, "Move to group", open, TARGET_SHOW));
            // `.hsep`: a 1x16 line with 4px either side.
            let (r, _) = ui.allocate_exact_size(vec2(9.0, 16.0), Sense::hover());
            ui.painter()
                .vline(r.center().x, r.y_range(), Stroke::new(1.0_f32, t.line));
            if confirm {
                delete = confirm_button(ui, &delete_label(ids.len())).clicked();
                cancel = studio::mini_button(ui, "Cancel", false, TARGET_SHOW).clicked();
            } else {
                ask = studio::mini_button(ui, "Delete", false, TARGET_SHOW).clicked();
            }
        });
        if ask {
            ctx.data_mut(|d| d.insert_temp(delete_confirm_id(), true));
        }
        if cancel {
            ctx.data_mut(|d| d.remove_temp::<bool>(delete_confirm_id()));
        }
        if delete {
            Self::store_fixture_selection(ctx, gid, BTreeSet::new());
            let last = ids.len().saturating_sub(1);
            for (i, fixture) in ids.iter().copied().enumerate() {
                let command = HostCommand::DeleteFixture {
                    group: gid,
                    fixture,
                };
                if i == last {
                    self.send_setup_command(command, move |app, _| {
                        app.setup_fixture_id = app.client.with_show(|show| {
                            show.0
                                .groups
                                .get(&gid)
                                .and_then(|g| g.fixtures.keys().next().copied())
                                .unwrap_or_default()
                        });
                    });
                } else {
                    self.send_setup_command(command, |_, _| {});
                }
            }
        }
        let Some(button) = move_button else {
            return;
        };
        if button.clicked() {
            components::dropdown::toggle(ctx, dd);
        }
        // Every other group, under its folder band, with its fixture count.
        let folders = &dmx_engine.0.folders;
        let mut targets: Vec<(String, u8, String, usize)> = groups
            .iter()
            .filter(|(id, _)| **id != gid)
            .map(|(id, g)| {
                let band = g
                    .folder
                    .and_then(|f| folders.get(&f))
                    .map_or_else(|| "No folder".to_owned(), |f| f.name.clone());
                (band, *id, g.name.clone(), g.fixtures.len())
            })
            .collect();
        targets.sort_by(|a, b| a.0.cmp(&b.0));
        let items: Vec<components::dropdown::Item> = targets
            .iter()
            .map(|(band, _, name, count)| {
                components::dropdown::Item::new(name.clone())
                    .group(band.clone())
                    .meta(count.to_string())
            })
            .collect();
        let picked = components::dropdown::Popover {
            id: dd,
            anchor: button.rect,
            anchor_id: Some(button.id),
            items: &items,
            options: components::dropdown::Options {
                width: Some(220.0),
                placeholder: "Filter groups".into(),
                ..Default::default()
            },
        }
        .show_single(ctx, None);
        if let components::dropdown::Outcome::Picked(i) = picked {
            if let Some((_, to, _, _)) = targets.get(i) {
                let to = *to;
                self.move_fixtures_to_group(ctx, dmx_engine, gid, ids, to);
            }
        }
    }

    /// The Fixture pane body for several fixtures (`multiEditor` in
    /// `setup.html`): count and type ("Mixed" for several types), the
    /// per-fixture fields frozen as `--`, Identify and Initialize for the
    /// whole selection.
    fn desktop_multi_editor(
        &mut self,
        ui: &mut Ui,
        gid: u8,
        ids: &[u8],
        fixtures: &[&blaulicht_shared::fixture::state::Fixture],
    ) {
        let t = studio_tokens(ui.ctx());
        let density = Density::of(ui.ctx());
        let f12 = density.font(Weight::Regular, 12.0);
        let f13 = density.font(Weight::Regular, 13.0);
        ui.spacing_mut().item_spacing = vec2(6.0, 6.0);
        let label = |ui: &mut Ui, text: &str, height: f32| {
            let (r, _) = ui.allocate_exact_size(vec2(FORM_LABEL_W, height), Sense::hover());
            ui.painter().text(
                pos2(r.left(), r.center().y),
                Align2::LEFT_CENTER,
                text,
                f12.clone(),
                t.muted,
            );
        };
        let text_row = |ui: &mut Ui, name: &str, value: &str, font: egui::FontId| {
            let w = ui.available_width();
            let (r, _) = ui.allocate_exact_size(vec2(w, 16.0), Sense::hover());
            let p = ui.painter();
            p.text(
                pos2(r.left(), r.center().y),
                Align2::LEFT_CENTER,
                name,
                f12.clone(),
                t.muted,
            );
            p.text(
                pos2(r.left() + FORM_LABEL_W + 6.0, r.center().y),
                Align2::LEFT_CENTER,
                value,
                font,
                t.strong,
            );
        };
        // `input.num:disabled`: well, line, "--", at half opacity.
        let frozen = |ui: &mut Ui, width: f32, what: &str| {
            let (r, resp) = ui.allocate_exact_size(vec2(width, TARGET_MIN), Sense::hover());
            let mut p = ui.painter().clone();
            p.set_opacity(0.5);
            p.rect_filled(r, 0.0, t.well);
            p.rect_stroke(r, 0.0, Stroke::new(1.0_f32, t.line), StrokeKind::Inside);
            p.text(
                pos2(r.left() + 7.0, r.center().y),
                Align2::LEFT_CENTER,
                "--",
                egui::FontId::monospace(12.0),
                t.strong,
            );
            resp.on_hover_text(format!("{what}, only for a single fixture"));
        };
        let heading = |ui: &mut Ui, text: &str| {
            let w = ui.available_width();
            // `.frm h4`: 6px margin, a line, 8px padding, the label.
            let (r, _) = ui.allocate_exact_size(vec2(w, 30.0), Sense::hover());
            ui.painter()
                .hline(r.x_range(), r.top() + 8.5, Stroke::new(1.0_f32, t.line));
            ui.painter().text(
                pos2(r.left(), r.bottom()),
                Align2::LEFT_BOTTOM,
                text,
                // `.frm h4` is font-weight 500 in --muted.
                density.font(Weight::Medium, 12.0),
                t.muted,
            );
        };

        let types: BTreeSet<String> = fixtures.iter().map(|f| f.type_.model_string()).collect();
        let kind = match types.len() {
            1 => types.into_iter().next().unwrap_or_default(),
            _ => "Mixed".to_owned(),
        };
        text_row(
            ui,
            "Fixtures",
            &format!("{} selected", ids.len()),
            f13.clone(),
        );
        text_row(ui, "Type", &kind, f13.clone());
        heading(ui, "Patch");
        for name in ["Universe", "Start channel"] {
            ui.horizontal(|ui| {
                label(ui, name, TARGET_MIN);
                let w = ui.available_width();
                frozen(ui, w, name);
            });
        }
        text_row(ui, "Channels", "--", egui::FontId::monospace(12.0));
        // `.msg.o`: well fill, a 2px ok edge, muted text.
        let w = ui.available_width();
        let (r, _) = ui.allocate_exact_size(vec2(w, 24.0), Sense::hover());
        ui.painter().rect_filled(r, 0.0, t.well);
        ui.painter()
            .rect_filled(Rect::from_min_size(r.min, vec2(2.0, r.height())), 0.0, t.ok);
        ui.painter().text(
            pos2(r.left() + 8.0, r.center().y),
            Align2::LEFT_CENTER,
            "Universe and channels are set one fixture at a time.",
            f12.clone(),
            t.muted,
        );
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = 4.0;
            // Identify steps through the selection (078); the button is
            // here so the row matches the mockup.
            studio::live_button(ui, "Identify", false, TARGET_MIN)
                .on_hover_text("Step through the selected fixtures and flash each one");
            let init_id = Id::new(("fixtures_initializing", gid, ids.to_vec()));
            let since: Option<f64> = ui.data(|d| d.get_temp(init_id));
            let now = ui.input(|i| i.time);
            let on = since.is_some_and(|t| now - t < 10.0);
            if on {
                ui.ctx().request_repaint_after(Duration::from_millis(250));
            }
            let init = studio::live_button(ui, "Initialize", on, TARGET_MIN).on_hover_text(
                "Send the selected fixtures their start-up values, such as lamp on and reset",
            );
            if init.clicked() && !on {
                ui.data_mut(|d| d.insert_temp(init_id, now));
                let label = format!("Initializing {} fixtures (10 s)", ids.len());
                let keys = ids.iter().map(|id| (gid, *id)).collect();
                self.send_setup_command(HostCommand::InitFixtures(keys), move |app, _| {
                    app.show_popup(PopupSpec::with_duration(Duration::from_secs(3), label));
                });
            }
        });
        heading(ui, "Position");
        ui.label(
            RichText::new("Select one fixture to position it.")
                .font(f12.clone())
                .color(t.muted),
        );
        for name in ["Position", "Rotation"] {
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = 2.0;
                label(ui, name, TARGET_MIN);
                ui.add_space(4.0);
                let w = ((ui.available_width() - 4.0) / 3.0).floor();
                for _ in 0..3 {
                    frozen(ui, w, name);
                }
            });
        }
    }
}

#[cfg(test)]
mod selection_tests {
    use super::*;

    fn set(ids: &[u8]) -> BTreeSet<u8> {
        ids.iter().copied().collect()
    }

    #[test]
    fn click_ctrl_and_shift_select_like_the_mockup() {
        let order = [0, 1, 2, 3, 4, 5];
        let mut sel = BTreeSet::new();
        let mut anchor = None;
        click_select(&order, &mut sel, &mut anchor, 1, false, false);
        assert_eq!((sel.clone(), anchor), (set(&[1]), Some(1)));
        click_select(&order, &mut sel, &mut anchor, 3, false, true);
        assert_eq!((sel.clone(), anchor), (set(&[1, 2, 3]), Some(1)));
        click_select(&order, &mut sel, &mut anchor, 5, true, false);
        assert_eq!((sel.clone(), anchor), (set(&[1, 2, 3, 5]), Some(5)));
        click_select(&order, &mut sel, &mut anchor, 5, true, false);
        assert_eq!((sel.clone(), anchor), (set(&[1, 2, 3]), Some(3)));
        // Ctrl+Shift adds a range to the selection.
        click_select(&order, &mut sel, &mut anchor, 5, true, true);
        assert_eq!(sel, set(&[1, 2, 3, 4, 5]));
        // Shift alone replaces it with the range.
        click_select(&order, &mut sel, &mut anchor, 4, false, true);
        assert_eq!(sel, set(&[3, 4]));
        click_select(&order, &mut sel, &mut anchor, 0, false, false);
        assert_eq!((sel, anchor), (set(&[0]), Some(0)));
    }

    #[test]
    fn labels_name_the_count() {
        assert_eq!(delete_label(1), "Confirm delete");
        assert_eq!(delete_label(3), "Delete 3 fixtures");
        assert_eq!(moved_text(&["A".into()], "FOG"), "Moved A to FOG");
        assert_eq!(
            moved_text(&["A".into(), "B".into(), "C".into()], "FOG"),
            "Moved 3 fixtures to FOG"
        );
    }
}
