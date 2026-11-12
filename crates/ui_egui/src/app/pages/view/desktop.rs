//! Views on the desktop (`pages.html#views`, ticket 081): the Views list
//! on the left (Add in its header, a delete icon on the selected row) and
//! the selected view on the right: a 40px title bar with the view's name as
//! an editable field, Add overlay and Apply, then the view's scenes in
//! folder groups as 72px rows with Intensity and Speed knobs and Remove.
//!
//! Desktop only ([`Density::Compact`]); the kiosk keeps
//! [`BlaulichtApp::view_ui`]'s touch layout.

use crate::app::components::dropdown::{self, Item, Options, Popover};
use crate::app::components::notify_error;
use crate::app::components::studio::{self, KnobSize, ListRow, ListRowState, ModalOutcome};
use crate::app::ui_tokens::{
    studio as studio_tokens, Density, Weight, TARGET_SHOW, TEXT, TEXT_MIN,
};
use crate::app::BlaulichtApp;
use blaulicht_engine::client::EngineClient;
use blaulicht_engine::hub::{CreatedId, HostCommand};
use blaulicht_shared::folder::{self, FolderId, FolderKind};
use blaulicht_shared::{
    scene::BLANK_SCENE_ID, view::View, ControlEvent, ControlEventMessage, EventOriginator,
};
use egui::{
    pos2, vec2, Align2, Color32, Context, FontFamily, FontId, Id, Key, Rect, Sense, Stroke,
    StrokeKind, Ui,
};

/// Width of the Views list (`minmax(200px, 260px)` in `pages.css`).
const LIST_W: f32 = 260.0;
/// The gutter between the panes.
const GAP: f32 = 1.0;
/// Height of a scene row.
const ROW_H: f32 = 72.0;
/// The scene row's columns (`grid-template-columns: minmax(140px, 260px)
/// 64px 64px 1fr`, 20px apart) and its left padding (a tree leaf's).
const NAME_W: f32 = 260.0;
const NAME_MIN_W: f32 = 140.0;
const KNOB_W: f32 = 64.0;
const COL_GAP: f32 = 20.0;
const LEAF_PAD: f32 = 29.0;
/// The name field in the title bar: at most 340px wide.
const NAME_FIELD_W: f32 = 340.0;
/// The delete icon on the selected list row.
const DELETE_W: f32 = 20.0;
/// The red of Remove and the delete icon under the pointer (`#b80f0f`).
const RED: Color32 = Color32::from_rgb(0xb8, 0x0f, 0x0f);

fn add_dialog_id() -> Id {
    Id::new("views_add_view")
}

fn delete_dialog_id() -> Id {
    Id::new("views_delete_view")
}

fn font(ui: &Ui, weight: Weight, size: f32) -> FontId {
    Density::of(ui.ctx()).font(weight, size)
}

/// "New view", or "New view 2", "New view 3", ... when that name is taken.
fn free_view_name<'a>(names: impl Iterator<Item = &'a str> + Clone) -> String {
    let mut n = 1;
    let mut name = "New view".to_string();
    while names.clone().any(|x| x == name) {
        n += 1;
        name = format!("New view {n}");
    }
    name
}

/// The view's overlays grouped by their scene's folder, in folder tree
/// order; scenes not in a folder come first, under no band.
fn folder_groups(
    state: &blaulicht_shared::EngineState,
    view: &View,
) -> Vec<(Option<FolderId>, Vec<u8>)> {
    let folder_of = |scene: u8| state.scenes.get(&scene).and_then(|s| s.folder);
    let mut out: Vec<(Option<FolderId>, Vec<u8>)> = Vec::new();
    let root: Vec<u8> = view
        .overlays
        .iter()
        .copied()
        .filter(|id| folder_of(*id).is_none())
        .collect();
    if !root.is_empty() {
        out.push((None, root));
    }
    for fid in folder::sorted_folder_ids(&state.folders, FolderKind::Scenes) {
        let scenes: Vec<u8> = view
            .overlays
            .iter()
            .copied()
            .filter(|id| folder_of(*id) == Some(fid))
            .collect();
        if !scenes.is_empty() {
            out.push((Some(fid), scenes));
        }
    }
    out
}

/// What the scene pane shows, read from the show once per frame.
struct Snapshot {
    views: Vec<(u8, String, usize)>,
    view: Option<View>,
    groups: Vec<(Option<FolderId>, Vec<u8>)>,
    folder_names: Vec<(FolderId, String)>,
    scene_names: Vec<(u8, String)>,
    /// Scenes that can still be added as an overlay (name, id).
    addable: Vec<(u8, String)>,
}

impl BlaulichtApp {
    pub(super) fn view_desktop_ui(&mut self, ui: &mut Ui, ctx: &Context) {
        let t = studio_tokens(ctx);
        // Keep a view selected while views exist.
        let selected = self.view_ui_state.selected_view_id;
        let first = self.client.with_show(|show| {
            let views = &show.0.views;
            match selected {
                Some(id) if views.contains_key(&id) => Some(id),
                _ => views.keys().next().copied(),
            }
        });
        self.view_ui_state.selected_view_id = first;
        let selected = first;

        let snap = self.client.with_show(|show| {
            let state = &show.0;
            let views = state
                .views
                .iter()
                .map(|(id, v)| (*id, v.name.clone(), v.overlays.len()))
                .collect();
            let view = selected.and_then(|id| state.views.get(&id).cloned());
            let groups = view
                .as_ref()
                .map(|v| folder_groups(state, v))
                .unwrap_or_default();
            let folder_names = state
                .folders
                .iter()
                .map(|(id, f)| (*id, f.name.clone()))
                .collect();
            let scene_names = state
                .scenes
                .iter()
                .map(|(id, s)| (*id, s.name.clone()))
                .collect();
            let addable = view
                .as_ref()
                .map(|v| {
                    state
                        .scenes
                        .iter()
                        .filter(|(id, _)| **id != BLANK_SCENE_ID && !v.overlays.contains(id))
                        .map(|(id, s)| (*id, s.name.clone()))
                        .collect()
                })
                .unwrap_or_default();
            Snapshot {
                views,
                view,
                groups,
                folder_names,
                scene_names,
                addable,
            }
        });

        // The panes fill the page edge to edge, the gutter colour between.
        let full = ui.max_rect().expand(8.0).intersect(ui.clip_rect());
        ui.allocate_rect(ui.max_rect(), Sense::hover());
        ui.painter().rect_filled(full, 0.0, t.gutter);
        let list_rect = Rect::from_min_size(full.min, vec2(LIST_W, full.height()));
        let main_rect = Rect::from_min_max(pos2(list_rect.right() + GAP, full.top()), full.max);
        let mut pane = |rect: Rect, salt: &str| {
            ui.painter().rect_filled(rect, 0.0, t.list);
            let mut child = ui.new_child(
                egui::UiBuilder::new()
                    .id_salt(salt)
                    .max_rect(rect)
                    .layout(egui::Layout::top_down(egui::Align::Min)),
            );
            child.set_clip_rect(rect);
            child.spacing_mut().item_spacing = vec2(0.0, 0.0);
            child
        };
        let mut list_ui = pane(list_rect, "views_list");
        let mut main_ui = pane(main_rect, "views_main");

        self.views_list_pane(&mut list_ui, &snap);
        self.view_pane(&mut main_ui, &snap);
        self.add_view_modal(ctx, &snap);
        self.delete_view_modal(ctx, &snap);
    }

    /// The Views list: header with Add, one row per view with its scene
    /// count, the delete icon on the selected row (never on the last view).
    fn views_list_pane(&mut self, ui: &mut Ui, snap: &Snapshot) {
        let mut add = false;
        studio::pane_header(ui, None, "Views", true, |ui| {
            add = studio::mini_button(ui, "Add", false, TARGET_SHOW)
                .on_hover_text("Add a view")
                .clicked();
        });
        if add && studio::open_modal(ui.ctx(), add_dialog_id()) {
            self.view_ui_state.add_name =
                free_view_name(snap.views.iter().map(|(_, n, _)| n.as_str()));
            self.view_ui_state.add_focus = true;
        }
        let can_delete = snap.views.len() > 1;
        egui::ScrollArea::vertical()
            .id_salt("views_list_scroll")
            .auto_shrink([false, false])
            .show(ui, |ui| {
                ui.spacing_mut().item_spacing = vec2(0.0, 0.0);
                if snap.views.is_empty() {
                    empty_hint(ui, "No views yet. Add one to stack scenes.");
                }
                for (id, name, count) in &snap.views {
                    let on = self.view_ui_state.selected_view_id == Some(*id);
                    let with_delete = on && can_delete;
                    let row = ListRow::new(name)
                        .count(*count)
                        .state(if on {
                            ListRowState::Active
                        } else {
                            ListRowState::Idle
                        })
                        .reserve(if with_delete { DELETE_W + 6.0 } else { 0.0 })
                        .show(ui);
                    let mut deleted = false;
                    if with_delete {
                        let r = Rect::from_center_size(
                            pos2(row.rect.right() - 8.0 - DELETE_W / 2.0, row.rect.center().y),
                            vec2(DELETE_W, DELETE_W),
                        );
                        let del = ui
                            .interact(r, ui.id().with(("view_delete", *id)), Sense::click())
                            .on_hover_text("Delete view");
                        let hovered = del.hovered();
                        let p = ui.painter();
                        if hovered {
                            p.rect_filled(r, 0.0, RED);
                        }
                        let t = studio_tokens(ui.ctx());
                        let color = if hovered {
                            Color32::WHITE
                        } else {
                            t.strong.gamma_multiply(0.7)
                        };
                        p.text(
                            r.center(),
                            Align2::CENTER_CENTER,
                            egui_phosphor::regular::TRASH,
                            FontId::new(13.0, FontFamily::Proportional),
                            color,
                        );
                        del.widget_info(|| {
                            egui::WidgetInfo::labeled(
                                egui::WidgetType::Button,
                                true,
                                format!("Delete view {name}"),
                            )
                        });
                        if del.clicked() && studio::open_modal(ui.ctx(), delete_dialog_id()) {
                            self.view_ui_state.delete_view_id = Some(*id);
                            deleted = true;
                        }
                    }
                    if row.clicked() && !deleted && !on {
                        self.view_ui_state.selected_view_id = Some(*id);
                        self.view_ui_state.name_edit = None;
                    }
                }
            });
    }

    /// The selected view: title bar (name field, Add overlay, Apply) and
    /// its scenes in folder groups.
    fn view_pane(&mut self, ui: &mut Ui, snap: &Snapshot) {
        let Some(view) = &snap.view else {
            studio::pane_header(ui, None, "View", true, |_| {});
            empty_hint(ui, "Select or add a view.");
            return;
        };
        let Some(view_id) = self.view_ui_state.selected_view_id else {
            return;
        };
        let t = studio_tokens(ui.ctx());

        // Title bar: the name field left, the actions right.
        let mut apply = false;
        let mut picked = None;
        let bar = studio::pane_header(ui, None, "", true, |ui| {
            let add_id = ui.make_persistent_id("view_add_overlay");
            let enabled = !snap.addable.is_empty();
            let resp = ui
                .add_enabled_ui(enabled, |ui| {
                    studio::mini_button(ui, "Add overlay", false, TARGET_SHOW)
                })
                .inner;
            let resp = if enabled {
                resp.on_hover_text("Stack a scene on this view")
            } else {
                resp.on_disabled_hover_text("Every scene is in this view")
            };
            let dd = add_id.with("dd");
            if resp.clicked() && enabled {
                dropdown::toggle(ui.ctx(), dd);
            }
            let items: Vec<Item> = snap
                .addable
                .iter()
                .map(|(id, name)| Item::new(name.clone()).meta(format!("#{id}")))
                .collect();
            if let dropdown::Outcome::Picked(i) = (Popover {
                id: dd,
                anchor: resp.rect,
                anchor_id: Some(resp.id),
                items: &items,
                options: Options {
                    width: Some(220.0),
                    placeholder: "Filter scenes".into(),
                    ..Options::default()
                },
            })
            .show_single(ui.ctx(), None)
            {
                picked = snap.addable.get(i).map(|(id, _)| *id);
            }
            apply = studio::mini_button(ui, "Apply", true, TARGET_SHOW)
                .on_hover_text("Play this view's scenes")
                .clicked();
        });
        self.view_name_field(ui, bar, view_id, &view.name, &snap.views);

        if let Some(scene) = picked {
            self.send_command(
                HostCommand::AddViewOverlay {
                    view: view_id,
                    scene,
                },
                |_, _| {},
            );
        }
        if apply {
            self.client.send(ControlEventMessage::new(
                EventOriginator::Web,
                ControlEvent::Transaction(view.apply_events()),
            ));
        }

        let mut remove = None;
        let mut toggle = None;
        egui::ScrollArea::vertical()
            .id_salt("view_scenes_scroll")
            .auto_shrink([false, false])
            .show(ui, |ui| {
                ui.spacing_mut().item_spacing = vec2(0.0, 0.0);
                if snap.groups.is_empty() {
                    empty_hint(ui, "No scenes in this view yet. Add overlay stacks one.");
                }
                for (index, (folder, scenes)) in snap.groups.iter().enumerate() {
                    let open = match folder {
                        Some(fid) => {
                            let open = !self.view_ui_state.closed_folders.contains(fid);
                            let name = snap
                                .folder_names
                                .iter()
                                .find(|(id, _)| id == fid)
                                .map_or("", |(_, n)| n.as_str());
                            if studio::folder_band(ui, name, Some(scenes.len()), open, index == 0)
                                .clicked()
                            {
                                toggle = Some(*fid);
                            }
                            open
                        }
                        None => true,
                    };
                    if !open {
                        continue;
                    }
                    for scene in scenes {
                        let name = snap
                            .scene_names
                            .iter()
                            .find(|(id, _)| id == scene)
                            .map_or("Unknown", |(_, n)| n.as_str());
                        let place = view.overlays.iter().position(|s| s == scene).unwrap_or(0) + 1;
                        if self.view_scene_row(
                            ui,
                            t,
                            view_id,
                            *scene,
                            name,
                            place,
                            view,
                            folder.is_some(),
                        ) {
                            remove = Some(*scene);
                        }
                    }
                }
            });
        if let Some(fid) = toggle {
            let closed = &mut self.view_ui_state.closed_folders;
            if !closed.remove(&fid) {
                closed.insert(fid);
            }
        }
        if let Some(scene) = remove {
            self.send_command(
                HostCommand::RemoveViewOverlay {
                    view: view_id,
                    scene,
                },
                |_, _| {},
            );
        }
    }

    /// The view's name as an editable field in the title bar (`bar`), with
    /// a pencil. Enter or leaving the field saves; Esc restores the name.
    fn view_name_field(
        &mut self,
        ui: &mut Ui,
        bar: Rect,
        view_id: u8,
        name: &str,
        views: &[(u8, String, usize)],
    ) {
        let t = studio_tokens(ui.ctx());
        let id = Id::new(("view_name_field", view_id));
        let rect = Rect::from_min_size(
            pos2(bar.left() + 8.0, bar.center().y - TARGET_SHOW / 2.0),
            vec2(
                NAME_FIELD_W.min((bar.width() * 0.5).max(120.0)),
                TARGET_SHOW,
            ),
        );
        let editing = self
            .view_ui_state
            .name_edit
            .as_ref()
            .is_some_and(|(v, _)| *v == view_id);
        let mut text = match &self.view_ui_state.name_edit {
            Some((v, s)) if *v == view_id => s.clone(),
            _ => name.to_string(),
        };
        let focused = ui.memory(|m| m.has_focus(id));
        let hovered = ui.rect_contains_pointer(rect);
        let p = ui.painter();
        p.rect_filled(rect, 0.0, t.well);
        let line = if focused {
            t.blue
        } else if hovered {
            t.muted
        } else {
            t.line
        };
        p.rect_stroke(rect, 0.0, Stroke::new(1.0_f32, line), StrokeKind::Inside);
        p.text(
            pos2(rect.right() - 15.0, rect.center().y),
            Align2::CENTER_CENTER,
            egui_phosphor::regular::PENCIL_SIMPLE,
            FontId::new(14.0, FontFamily::Proportional),
            if focused || hovered {
                t.strong
            } else {
                t.muted
            },
        );
        let edit_rect = Rect::from_min_max(
            pos2(rect.left() + 8.0, rect.top()),
            pos2(rect.right() - 28.0, rect.bottom()),
        );
        let mut child = ui.new_child(
            egui::UiBuilder::new()
                .max_rect(edit_rect)
                .layout(egui::Layout::left_to_right(egui::Align::Center)),
        );
        let resp = child
            .add(
                egui::TextEdit::singleline(&mut text)
                    .id(id)
                    .frame(egui::Frame::NONE)
                    .font(font(ui, Weight::Medium, 14.0))
                    .text_color(t.strong)
                    .desired_width(edit_rect.width())
                    .margin(egui::Margin::ZERO),
            )
            .on_hover_text("Rename view");
        resp.widget_info(|| {
            egui::WidgetInfo::text_edit(true, name.to_string(), text.clone(), "View name")
        });
        if resp.changed() || (resp.has_focus() && !editing) {
            self.view_ui_state.name_edit = Some((view_id, text.clone()));
        }
        if resp.lost_focus() {
            // egui drops the focus on Esc before the field runs, so read
            // the key here.
            let esc = ui.input(|i| i.key_pressed(Key::Escape));
            self.view_ui_state.name_edit = None;
            let new = text.trim();
            if esc || new.is_empty() || new == name {
                return;
            }
            if views.iter().any(|(v, n, _)| *v != view_id && n == new) {
                notify_error(ui.ctx(), format!("A view named {new} already exists"));
                return;
            }
            self.send_command(
                HostCommand::RenameView {
                    view: view_id,
                    name: new.to_string(),
                },
                |_, _| {},
            );
        }
    }

    /// One 72px scene row: name with "#id · N in stack", the Intensity and
    /// Speed knobs (the view's masters for the scene), Remove flush right.
    /// Returns whether Remove was clicked.
    #[allow(clippy::too_many_arguments)]
    fn view_scene_row(
        &mut self,
        ui: &mut Ui,
        t: &crate::app::ui_tokens::Studio,
        view_id: u8,
        scene_id: u8,
        name: &str,
        place: usize,
        view: &View,
        in_folder: bool,
    ) -> bool {
        let (rect, _) = ui.allocate_exact_size(vec2(ui.available_width(), ROW_H), Sense::hover());
        let p = ui.painter();
        p.hline(
            rect.x_range(),
            rect.bottom() - 0.5,
            Stroke::new(1.0_f32, t.hair),
        );
        if in_folder {
            p.vline(
                rect.left() + 12.5,
                rect.y_range(),
                Stroke::new(1.0_f32, t.line),
            );
        }
        let left = rect.left() + if in_folder { LEAF_PAD } else { 8.0 };
        let right = rect.right() - 10.0;
        // Columns: name, two knobs, Remove (the rest, flush right). The
        // name column gives way first when the pane is narrow.
        let remove_w = 64.0;
        let fixed = 2.0 * KNOB_W + 3.0 * COL_GAP + remove_w;
        let name_w = (right - left - fixed).clamp(NAME_MIN_W, NAME_W);
        let cy = rect.center().y;
        studio::paint_truncated(
            ui,
            pos2(left, cy - 8.0),
            name,
            font(ui, Weight::Regular, TEXT + 1.0),
            t.strong,
            name_w,
        );
        studio::paint_truncated(
            ui,
            pos2(left, cy + 8.0),
            &format!("#{scene_id} \u{b7} {place} in stack"),
            font(ui, Weight::Regular, TEXT_MIN),
            t.muted,
            name_w,
        );
        if ui.rect_contains_pointer(Rect::from_min_size(
            pos2(left, rect.top()),
            vec2(name_w, ROW_H),
        )) {
            ui.interact(
                Rect::from_min_size(pos2(left, rect.top()), vec2(name_w, ROW_H)),
                ui.id().with(("view_scene_name", scene_id)),
                Sense::hover(),
            )
            .on_hover_text(name);
        }

        let key = (view_id, scene_id);
        let engine = view.masters_for(scene_id);
        let mut masters = engine;
        let mut changed = false;
        let knob_top = rect.top() + (ROW_H - 67.0) / 2.0;
        let k1 = Rect::from_min_size(pos2(left + name_w + COL_GAP, knob_top), vec2(KNOB_W, 67.0));
        let k2 = k1.translate(vec2(KNOB_W + COL_GAP, 0.0));
        let mut knob_ui = |r: Rect| {
            ui.new_child(
                egui::UiBuilder::new()
                    .id_salt(("view_knob", scene_id, r.left() as i32))
                    .max_rect(r)
                    .layout(egui::Layout::top_down(egui::Align::Center)),
            )
        };
        let mut a_ui = knob_ui(k1);
        let held = &mut self.view_ui_state.alpha_held;
        let mut alpha = held.shown(key, engine.master_alpha as f32);
        let resp = studio::knob(
            &mut a_ui,
            &mut alpha,
            0.0..=100.0,
            "Intensity",
            KnobSize::Row,
        );
        if resp.changed() {
            held.hold(key, alpha);
            masters.master_alpha = alpha.round() as u8;
            changed = true;
        } else {
            masters.master_alpha = alpha.round() as u8;
        }
        held.settle_row(resp.dragged(), key, |v| {
            v.round() as u8 == engine.master_alpha
        });
        let mut s_ui = knob_ui(k2);
        let held = &mut self.view_ui_state.speed_held;
        masters.master_speed = held.shown(key, engine.master_speed);
        let resp = studio::speed_knob(&mut s_ui, &mut masters.master_speed, "Speed", KnobSize::Row);
        if resp.changed() {
            held.hold(key, masters.master_speed);
            changed = true;
        }
        held.settle_row(resp.dragged(), key, |v| v == engine.master_speed);
        if changed {
            self.client.send(HostCommand::SetViewSceneMasters {
                view: view_id,
                scene: scene_id,
                masters,
            });
        }

        // Remove: a 28px mini button that turns red under the pointer.
        let f = font(ui, Weight::Regular, TEXT);
        let label = "Remove";
        let w = ui
            .painter()
            .layout_no_wrap(label.into(), f.clone(), Color32::WHITE)
            .size()
            .x
            + 20.0;
        let r = Rect::from_min_size(
            pos2(right - w, cy - TARGET_SHOW / 2.0),
            vec2(w, TARGET_SHOW),
        );
        let resp = ui.interact(r, ui.id().with(("view_remove", scene_id)), Sense::click());
        let hovered = resp.hovered();
        let p = ui.painter();
        p.rect_filled(r, 0.0, if hovered { RED } else { t.ctl });
        if resp.has_focus() {
            p.rect_stroke(r, 0.0, Stroke::new(1.0_f32, t.blue), StrokeKind::Inside);
        }
        p.text(
            r.center(),
            Align2::CENTER_CENTER,
            label,
            f,
            if hovered { Color32::WHITE } else { t.text },
        );
        resp.widget_info(|| {
            egui::WidgetInfo::labeled(egui::WidgetType::Button, true, format!("Remove {name}"))
        });
        resp.on_hover_text("Remove from this view").clicked()
    }

    /// Add view: a name (a free one prefilled and selected); a name that is
    /// already taken is refused.
    fn add_view_modal(&mut self, ctx: &Context, snap: &Snapshot) {
        let id = add_dialog_id();
        if studio::open_modal_of(ctx) != Some(id) {
            return;
        }
        let mut name = std::mem::take(&mut self.view_ui_state.add_name);
        let focus = std::mem::take(&mut self.view_ui_state.add_focus);
        let mut enter = false;
        let outcome = studio::modal(ctx, id, "Add view", "Add view", false, |ui| {
            let t = studio_tokens(ui.ctx());
            ui.horizontal(|ui| {
                // A label column, then the field (`.dlgf`).
                ui.allocate_ui_with_layout(
                    vec2(100.0, TARGET_SHOW - 4.0),
                    egui::Layout::left_to_right(egui::Align::Center),
                    |ui| {
                        ui.set_min_width(100.0);
                        ui.label(egui::RichText::new("Name").size(TEXT));
                    },
                );
                let field_id = Id::new("views_add_name");
                let resp = ui.add(
                    egui::TextEdit::singleline(&mut name)
                        .id(field_id)
                        .desired_width(ui.available_width()),
                );
                if focus {
                    resp.request_focus();
                    if let Some(mut state) = egui::TextEdit::load_state(ui.ctx(), field_id) {
                        let all = egui::text::CCursorRange::two(
                            egui::text::CCursor::new(0),
                            egui::text::CCursor::new(name.chars().count()),
                        );
                        state.cursor.set_char_range(Some(all));
                        state.store(ui.ctx(), field_id);
                    }
                }
                if resp.lost_focus() && ui.input(|i| i.key_pressed(Key::Enter)) {
                    enter = true;
                }
            });
            ui.label(
                egui::RichText::new("Add scenes to it afterwards with Add overlay.")
                    .color(t.muted)
                    .size(TEXT),
            );
        });
        // The field's selection needs the edit state of the first frame.
        if focus {
            self.view_ui_state.add_focus = !ctx.memory(|m| m.has_focus(Id::new("views_add_name")))
                || egui::TextEdit::load_state(ctx, Id::new("views_add_name")).is_none();
        }
        let confirmed = outcome == ModalOutcome::Confirmed || enter;
        if !confirmed {
            if outcome == ModalOutcome::Open {
                self.view_ui_state.add_name = name;
            }
            return;
        }
        let free = free_view_name(snap.views.iter().map(|(_, n, _)| n.as_str()));
        let name = match name.trim() {
            "" => free,
            n => n.to_string(),
        };
        if snap.views.iter().any(|(_, n, _)| *n == name) {
            notify_error(ctx, format!("A view named {name} already exists"));
            // Keep the dialog open with the name for another try.
            studio::open_modal(ctx, id);
            self.view_ui_state.add_name = name;
            self.view_ui_state.add_focus = true;
            return;
        }
        studio::close_modal(ctx, id);
        let shown = name.clone();
        self.send_command(
            HostCommand::CreateEmptyView { name },
            move |app, created| {
                if let Some(CreatedId::View(id)) = created {
                    app.view_ui_state.selected_view_id = Some(id);
                }
                app.desk_notes.info(format!("Added view {shown}"));
            },
        );
    }

    /// Delete view: asks first.
    fn delete_view_modal(&mut self, ctx: &Context, snap: &Snapshot) {
        let id = delete_dialog_id();
        if studio::open_modal_of(ctx) != Some(id) {
            return;
        }
        let outcome = studio::modal(ctx, id, "Delete this view?", "Delete view", true, |ui| {
            ui.label("Its overlay list is lost.");
        });
        if outcome != ModalOutcome::Confirmed {
            if outcome == ModalOutcome::Cancelled {
                self.view_ui_state.delete_view_id = None;
            }
            return;
        }
        let Some(view) = self.view_ui_state.delete_view_id.take() else {
            return;
        };
        // Select the view after it (or before it, for the last one).
        let pos = snap.views.iter().position(|(v, _, _)| *v == view);
        let next = pos.and_then(|i| {
            snap.views
                .get(i + 1)
                .or_else(|| i.checked_sub(1).and_then(|j| snap.views.get(j)))
                .map(|(v, _, _)| *v)
        });
        self.send_command(HostCommand::DeleteView(view), move |app, _| {
            app.view_ui_state.selected_view_id = next;
            app.view_ui_state.name_edit = None;
        });
    }
}

/// The muted empty-state line of a pane.
fn empty_hint(ui: &mut Ui, text: &str) {
    let t = studio_tokens(ui.ctx());
    ui.add_space(10.0);
    ui.horizontal(|ui| {
        ui.add_space(10.0);
        ui.label(egui::RichText::new(text).color(t.muted).size(TEXT));
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn free_names_skip_taken_ones() {
        let names = ["New view", "New view 2", "Chorus"];
        assert_eq!(free_view_name(names.iter().copied()), "New view 3");
        assert_eq!(free_view_name(["Chorus"].iter().copied()), "New view");
    }

    #[test]
    fn row_fits_list_row_height_rule() {
        // The 72px scene row holds the 67px row knob.
        const _: () = assert!(ROW_H >= 67.0);
    }
}
