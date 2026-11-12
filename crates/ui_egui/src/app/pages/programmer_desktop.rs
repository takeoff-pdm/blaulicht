//! The desktop Programmer (`design/studio/programmer.html`, ticket 075):
//! the page bar with the scene picker and its masters, Blackout, Reset
//! layout and Exclusive preview; Groups, Fixtures, the fader bank, Colour
//! and Pan / Tilt; Palettes and the Scene pane (animations and Scene
//! changes). Panes resize and fold as laid out by [`super::programmer_layout`].
//!
//! Desktop only ([`Density::Compact`]); the kiosk keeps
//! [`BlaulichtApp::programmer_ui`]'s touch layout.
//!
//! [`Density::Compact`]: crate::app::ui_tokens::Density::Compact

use super::programmer_layout::{
    self as layout, clamp_list_split, clamp_pal_w, clamp_side_w, Fold, Geometry, GripKind, Mode,
    ModeLayout, Pane, PaneLayout, STRIP,
};
use crate::app::{
    components::{
        self, dropdown, studio,
        studio::{ListRow, ListRowState},
        Dialog, TreeAction, TreeKey,
    },
    ui_tokens::{
        studio as studio_tokens, Density, Weight, LIST_ROW_HEIGHT, TARGET_MIN, TARGET_SHOW, TEXT,
        TEXT_MIN,
    },
    BlaulichtApp,
};
use blaulicht_engine::{client::EngineClient, dmx::EngineState};
use blaulicht_shared::{
    fixture::state::ResolvedFixtureState,
    palette::PaletteKind,
    scene::{FixtureSelection, FixtureSelector, BLANK_SCENE_ID},
    ControlEvent, ControlEventMessage, EventOriginator, FixtureProperty, RGBColor,
};
use egui::{
    pos2, vec2, Align2, Color32, Context, FontFamily, FontId, Id, Key, Modifiers, Rect, Sense,
    Stroke, StrokeKind, Ui,
};
use std::collections::{BTreeMap, HashMap, HashSet};

/// The faders in bank order, with their labels and ranges.
const FADERS: [(FixtureProperty, &str, f32); 8] = [
    (FixtureProperty::Alpha, "Intensity", 255.0),
    (FixtureProperty::Strobe, "Strobe", 255.0),
    (FixtureProperty::Focus, "Focus", 255.0),
    (FixtureProperty::Tilt, "Tilt", 255.0),
    (FixtureProperty::Pan, "Pan", 255.0),
    (FixtureProperty::ColorHue, "Hue", 360.0),
    (FixtureProperty::ColorSaturation, "Sat", 255.0),
    (FixtureProperty::ColorValue, "Value", 255.0),
];

/// Collapse state of the desktop Programmer's group list.
const GROUP_TREE: &str = "programmer_desktop_groups";
/// The XY pad's side.
const PAD: f32 = 150.0;

/// What a fixture's scene looked like when this session first showed it,
/// so Backspace on a fader can undo this session's changes.
#[derive(Debug, Clone, Default)]
pub struct SceneBase {
    values: HashMap<(u8, u8, FixtureProperty), f32>,
    changes: HashSet<FixtureSelector>,
}

/// A removal that removes more than one thing, waiting for confirmation.
#[derive(Debug, Clone)]
pub struct PendingRemove {
    title: String,
    body: String,
    action: String,
    event: ControlEvent,
}

/// An active divider drag.
#[derive(Debug, Clone)]
struct GripDrag {
    kind: GripKind,
    mode: Mode,
    /// The layout before the drag, restored by Esc.
    before: ModeLayout,
    /// Widths of the panes left and right of the divider at the start.
    a: f32,
    b: f32,
    start_x: f32,
}

/// The desktop Programmer's own state.
#[derive(Default)]
pub struct ProgrammerDesktop {
    /// Loaded on first use.
    layout: Option<PaneLayout>,
    grip: Option<GripDrag>,
    fixture_filter: String,
    /// The fader Backspace and Delete act on.
    picked: Option<FixtureProperty>,
    /// A fader being dragged and its value, shown until it is let go.
    dragging: Option<(FixtureProperty, f32)>,
    bases: HashMap<u8, SceneBase>,
    /// Open rows of the Scene changes tree: a group, or a group's fixture.
    changes_open: HashSet<(u8, Option<u8>)>,
    confirm: Option<PendingRemove>,
}

/// The value of `property` in a resolved fixture state, on the fader's
/// scale (hue in degrees, the rest 0-255).
pub(crate) fn prop_value(r: &ResolvedFixtureState, property: FixtureProperty) -> f32 {
    match property {
        FixtureProperty::Alpha => r.alpha as f32,
        FixtureProperty::Strobe => r.strobe_speed as f32,
        FixtureProperty::Focus => r.focus as f32,
        FixtureProperty::ColorHue => r.color.h as f32,
        FixtureProperty::ColorSaturation => (r.color.s * 255.0).round() as f32,
        FixtureProperty::ColorValue => (r.color.v * 255.0).round() as f32,
        FixtureProperty::Tilt => r.orientation.tilt as f32,
        FixtureProperty::Pan => r.orientation.pan as f32,
    }
}

/// The event that sets `property` to `value` on the selection.
pub(crate) fn set_event(property: FixtureProperty, value: f32) -> ControlEvent {
    let v8 = value.round().clamp(0.0, 255.0) as u8;
    match property {
        FixtureProperty::Alpha => ControlEvent::SetAlpha(v8),
        FixtureProperty::Strobe => ControlEvent::SetStrobeSpeed(v8),
        FixtureProperty::Focus => ControlEvent::SetFocus(v8),
        FixtureProperty::ColorHue => {
            ControlEvent::SetColorHue(value.round().clamp(0.0, 360.0) as u16)
        }
        FixtureProperty::ColorSaturation => ControlEvent::SetColorSaturation(v8),
        FixtureProperty::ColorValue => ControlEvent::SetColorValue(v8),
        FixtureProperty::Tilt => ControlEvent::SetTilt(v8),
        FixtureProperty::Pan => ControlEvent::SetPan(v8),
    }
}

/// Runs `event` on `fixtures` only and puts the selection back: per group,
/// select the group, limit it to its fixtures, apply.
fn on_fixtures(fixtures: &[(u8, u8)], event: ControlEvent) -> Vec<ControlEvent> {
    let mut by_group: BTreeMap<u8, Vec<u8>> = BTreeMap::new();
    for (g, f) in fixtures {
        by_group.entry(*g).or_default().push(*f);
    }
    let mut out = vec![ControlEvent::PushSelection];
    for (g, fids) in by_group {
        out.push(ControlEvent::RemoveAllSelection);
        out.push(ControlEvent::SelectGroup(g));
        for f in fids {
            out.push(ControlEvent::LimitSelectionToFixtureInCurrentGroup(f));
        }
        out.push(event.clone());
    }
    out.push(ControlEvent::PopSelection);
    out
}

/// `RemoveChange` for `changes`, one per property.
fn remove_changes_event(changes: &[FixtureSelector]) -> Option<ControlEvent> {
    let mut by_property: BTreeMap<u8, (FixtureProperty, Vec<(u8, u8)>)> = BTreeMap::new();
    for c in changes {
        by_property
            .entry(c.property as u8)
            .or_insert_with(|| (c.property, Vec::new()))
            .1
            .push((c.gid, c.fid));
    }
    if by_property.is_empty() {
        return None;
    }
    Some(ControlEvent::Transaction(
        by_property
            .into_values()
            .map(|(property, fixtures)| ControlEvent::RemoveChange {
                selection: FixtureSelection { fixtures },
                property,
            })
            .collect(),
    ))
}

/// What Backspace on the fader for `property` does for the `selected`
/// fixtures (`RULES.md`, State honesty): undo this session's changes
/// (put back the value the scene had, or drop a change this session added);
/// with nothing left to undo, remove the scene's earlier changes for it.
#[derive(Debug, Clone, PartialEq)]
pub(crate) enum FaderUndo {
    /// Values to put back per fixture, and this session's changes to drop.
    Session {
        restore: Vec<((u8, u8), f32)>,
        drop: Vec<FixtureSelector>,
    },
    /// The scene's earlier changes to remove.
    Earlier(Vec<FixtureSelector>),
    Nothing,
}

pub(crate) fn fader_undo(
    base: &SceneBase,
    now_values: &HashMap<(u8, u8, FixtureProperty), f32>,
    now_changes: &HashSet<FixtureSelector>,
    selected: &[(u8, u8)],
    property: FixtureProperty,
) -> FaderUndo {
    let key = |g: u8, f: u8| FixtureSelector {
        gid: g,
        fid: f,
        property,
    };
    let mut restore = Vec::new();
    let mut drop = Vec::new();
    for &(g, f) in selected {
        let k = key(g, f);
        let was = base.values.get(&(g, f, property)).copied();
        let now = now_values.get(&(g, f, property)).copied();
        let in_base = base.changes.contains(&k);
        let in_now = now_changes.contains(&k);
        let edited = (now != was && (in_base || in_now)) || (!in_base && in_now);
        if !edited {
            continue;
        }
        if in_base {
            restore.push(((g, f), was.unwrap_or(0.0)));
        } else {
            drop.push(k);
        }
    }
    if !restore.is_empty() || !drop.is_empty() {
        return FaderUndo::Session { restore, drop };
    }
    let earlier: Vec<_> = selected
        .iter()
        .map(|&(g, f)| key(g, f))
        .filter(|k| base.changes.contains(k) && now_changes.contains(k))
        .collect();
    if earlier.is_empty() {
        FaderUndo::Nothing
    } else {
        FaderUndo::Earlier(earlier)
    }
}

/// The events behind a [`FaderUndo`].
fn fader_undo_events(undo: &FaderUndo, property: FixtureProperty) -> Vec<ControlEvent> {
    match undo {
        FaderUndo::Session { restore, drop } => {
            let mut events = Vec::new();
            // Fixtures that go back to the same value share one step.
            let mut by_value: BTreeMap<i64, Vec<(u8, u8)>> = BTreeMap::new();
            for (k, v) in restore {
                by_value.entry(v.round() as i64).or_default().push(*k);
            }
            for (v, fixtures) in by_value {
                events.extend(on_fixtures(&fixtures, set_event(property, v as f32)));
            }
            if let Some(e) = remove_changes_event(drop) {
                events.push(e);
            }
            events
        }
        FaderUndo::Earlier(changes) => remove_changes_event(changes).into_iter().collect(),
        FaderUndo::Nothing => Vec::new(),
    }
}

/// Every fixture's values in `scene`, resolved with `palettes`.
fn scene_values(dmx_engine: &EngineState, scene_id: u8) -> HashMap<(u8, u8, FixtureProperty), f32> {
    let mut out = HashMap::new();
    let Some(scene) = dmx_engine.0.scenes.get(&scene_id) else {
        return out;
    };
    for ((g, f), state) in &scene.sink.fixture_states {
        let r = state.resolve(&dmx_engine.0.palettes);
        for (p, _, _) in FADERS {
            out.insert((*g, *f, p), prop_value(&r, p));
        }
    }
    out
}

fn font(ui: &Ui, weight: Weight, size: f32) -> FontId {
    Density::of(ui.ctx()).font(weight, size)
}

fn mono(size: f32) -> FontId {
    FontId::new(size, FontFamily::Monospace)
}

fn text_w(ui: &Ui, text: &str, f: FontId) -> f32 {
    ui.painter()
        .layout_no_wrap(text.to_owned(), f, Color32::WHITE)
        .size()
        .x
}

/// A child `Ui` over `rect`, clipped to it, top-down without spacing.
fn child(ui: &mut Ui, rect: Rect, salt: impl std::hash::Hash) -> Ui {
    let mut c = ui.new_child(
        egui::UiBuilder::new()
            .id_salt(salt)
            .max_rect(rect)
            .layout(egui::Layout::top_down(egui::Align::Min)),
    );
    c.set_clip_rect(rect.intersect(ui.clip_rect()));
    c.spacing_mut().item_spacing = vec2(0.0, 0.0);
    c
}

/// A 26px square icon button (`.pfold`, `.ib.quiet`).
fn icon_button(ui: &mut Ui, rect: Rect, id: Id, glyph: &str, tip: &str) -> bool {
    let t = studio_tokens(ui.ctx());
    let r = ui.interact(rect, id, Sense::click());
    if r.hovered() {
        ui.painter().rect_filled(rect, 0.0, t.head);
    }
    ui.painter().text(
        rect.center(),
        Align2::CENTER_CENTER,
        glyph,
        FontId::proportional(13.0),
        if r.hovered() { t.strong } else { t.muted },
    );
    r.on_hover_text(tip).clicked()
}

/// The caret on a pane's fold button.
fn fold_glyph(fold: Fold, bar: bool) -> &'static str {
    use egui_phosphor::regular as ph;
    match (fold, bar) {
        (Fold::Open, true) => ph::CARET_UP,
        (Fold::Open, false) => ph::CARET_LEFT,
        (Fold::Bar, _) => ph::CARET_DOWN,
        (Fold::Strip, _) => ph::CARET_RIGHT,
    }
}

/// A pane title row, 26px: a title-bar grey band (`head`) or plain on the
/// pane (`h3`), the title, a muted note after it, and room on the right for
/// the fold button. Returns the row.
fn title_row(ui: &mut Ui, title: &str, note: &str, band: bool, right_pad: f32) -> Rect {
    let t = studio_tokens(ui.ctx());
    let (rect, _) = ui.allocate_exact_size(vec2(ui.available_width(), STRIP), Sense::hover());
    if band {
        ui.painter().rect_filled(rect, 0.0, t.head);
    }
    let f = font(ui, Weight::Medium, TEXT + 1.0);
    let x = rect.left() + 8.0;
    let max = rect.right() - right_pad - x;
    studio::paint_truncated(
        ui,
        pos2(x, rect.center().y),
        title,
        f.clone(),
        t.strong,
        max,
    );
    if !note.is_empty() {
        let nx = x + text_w(ui, title, f) + 6.0;
        ui.painter().text(
            pos2(nx, rect.center().y),
            Align2::LEFT_CENTER,
            note,
            font(ui, Weight::Regular, TEXT),
            t.muted,
        );
    }
    rect
}

/// Paints a folded pane and returns whether it was clicked (to open it).
fn folded_pane(ui: &mut Ui, pane: Pane, rect: Rect, fold: Fold) -> bool {
    let t = studio_tokens(ui.ctx());
    let id = Id::new(("programmer_fold", pane));
    let r = ui.interact(rect, id.with("body"), Sense::click());
    let p = ui.painter();
    p.rect_filled(rect, 0.0, t.head);
    let glyph = fold_glyph(fold, false);
    let button = Rect::from_min_size(pos2(rect.right() - STRIP, rect.top()), vec2(STRIP, STRIP));
    p.text(
        button.center(),
        Align2::CENTER_CENTER,
        glyph,
        FontId::proportional(13.0),
        if r.hovered() { t.strong } else { t.muted },
    );
    let f = font(ui, Weight::Medium, TEXT + 1.0);
    match fold {
        Fold::Strip => {
            let galley = ui
                .painter()
                .layout_no_wrap(pane.title().to_owned(), f, t.strong);
            // Reads top to bottom (`writing-mode: vertical-rl`).
            let pos = pos2(rect.center().x + galley.size().y / 2.0, rect.top() + 30.0);
            ui.painter().add(
                egui::epaint::TextShape::new(pos, galley, t.strong)
                    .with_angle(std::f32::consts::FRAC_PI_2),
            );
        }
        _ => {
            ui.painter().text(
                pos2(rect.left() + 8.0, rect.top() + STRIP / 2.0),
                Align2::LEFT_CENTER,
                pane.title(),
                f,
                t.strong,
            );
        }
    }
    r.on_hover_text(format!("Open {}", pane.title())).clicked()
}

impl BlaulichtApp {
    /// The saved pane layout, loaded on first use.
    fn programmer_layout(&mut self) -> &mut PaneLayout {
        let path = self.config_dir.as_deref().map(PaneLayout::path);
        self.programmer_ui_state
            .desktop
            .layout
            .get_or_insert_with(|| PaneLayout::load(path.as_deref()))
    }

    fn save_programmer_layout(&mut self) {
        let path = self.config_dir.as_deref().map(PaneLayout::path);
        if let Some(l) = &self.programmer_ui_state.desktop.layout {
            l.save(path.as_deref());
        }
    }

    /// The desktop Programmer.
    pub(super) fn programmer_desktop_ui(
        &mut self,
        ui: &mut Ui,
        ctx: &Context,
        dmx_engine: &EngineState,
    ) {
        let t = studio_tokens(ctx);
        let scene_id = dmx_engine.0.current_scene_focus;
        // The scene as this session first saw it.
        self.programmer_ui_state
            .desktop
            .bases
            .entry(scene_id)
            .or_insert_with(|| SceneBase {
                values: scene_values(dmx_engine, scene_id),
                changes: dmx_engine.curr_scene().sink.changeset.clone(),
            });

        self.render_pending_remove(ctx);
        self.desktop_shortcuts(ctx, dmx_engine);

        // Edge to edge over the page frame's margin, gutter between panes.
        let full = ui.max_rect().expand(8.0).intersect(ui.clip_rect());
        ui.allocate_rect(ui.max_rect(), Sense::hover());
        ui.painter().rect_filled(full, 0.0, t.gutter);
        let mode = Mode::for_width(ctx.content_rect().width());
        let geo = layout::geometry(self.programmer_layout().mode(mode), mode, full);

        let mut bar_ui = child(ui, geo.bar, "programmer_bar");
        self.programmer_page_bar(&mut bar_ui, dmx_engine);

        let mut toggle: Option<Pane> = None;
        for (pane, rect, fold) in geo.panes.clone() {
            if fold != Fold::Open {
                if folded_pane(ui, pane, rect, fold) {
                    toggle = Some(pane);
                }
                continue;
            }
            let fill = match pane {
                Pane::Groups | Pane::Fixtures => t.list,
                _ => t.pane,
            };
            ui.painter().rect_filled(rect, 0.0, fill);
            let mut pui = child(ui, rect, ("programmer_pane", pane));
            match pane {
                Pane::Groups => self.groups_pane(&mut pui, dmx_engine),
                Pane::Fixtures => self.fixtures_pane(&mut pui, dmx_engine),
                Pane::Faders => self.faders_pane(&mut pui, dmx_engine),
                Pane::Colour => self.colour_pane(&mut pui, dmx_engine),
                Pane::PanTilt => self.pan_tilt_pane(&mut pui, dmx_engine),
                Pane::Palettes => self.palettes_pane(&mut pui, dmx_engine),
                Pane::Scene => self.scene_pane(&mut pui, dmx_engine),
            }
            if mode.can_fold(pane) {
                let button =
                    Rect::from_min_size(pos2(rect.right() - STRIP, rect.top()), vec2(STRIP, STRIP));
                let glyph = fold_glyph(Fold::Open, mode.folds_to_bar(pane));
                if icon_button(
                    ui,
                    button,
                    Id::new(("programmer_fold", pane)),
                    glyph,
                    &format!("Fold {}", pane.title()),
                ) {
                    toggle = Some(pane);
                }
            }
        }
        if let Some(pane) = toggle {
            self.programmer_layout()
                .mode_mut(mode)
                .toggle_fold(mode, pane);
            self.save_programmer_layout();
        }

        self.programmer_grips(ui, &geo);
    }

    /// Backspace or Delete on the picked fader; Esc cancels a divider drag
    /// or clears the selection.
    fn desktop_shortcuts(&mut self, ctx: &Context, dmx_engine: &EngineState) {
        let modal_open = ctx.memory(|m| m.top_modal_layer().is_some());
        let typing = ctx.text_edit_focused();
        let popover = dropdown::open_id(ctx).is_some();
        if let Some(drag) = &self.programmer_ui_state.desktop.grip {
            if ctx.input_mut(|i| i.consume_key(Modifiers::NONE, Key::Escape)) {
                let (mode, before) = (drag.mode, drag.before.clone());
                *self.programmer_layout().mode_mut(mode) = before;
                self.programmer_ui_state.desktop.grip = None;
            }
        }
        if !modal_open && !typing && !popover {
            if !dmx_engine.selection().is_empty()
                && ctx.input_mut(|i| i.consume_key(Modifiers::NONE, Key::Escape))
            {
                self.client.send(ControlEventMessage::new(
                    EventOriginator::Web,
                    ControlEvent::RemoveAllSelection,
                ));
            }
            if let Some(property) = self.programmer_ui_state.desktop.picked {
                let pressed = ctx.input_mut(|i| {
                    i.consume_key(Modifiers::NONE, Key::Backspace)
                        | i.consume_key(Modifiers::NONE, Key::Delete)
                });
                if pressed {
                    self.undo_fader(ctx, dmx_engine, property);
                }
            }
        }
    }

    fn undo_fader(&mut self, ctx: &Context, dmx_engine: &EngineState, property: FixtureProperty) {
        let selected = dmx_engine.get_selection().fixtures;
        let label = FADERS
            .iter()
            .find(|(p, _, _)| *p == property)
            .map_or("Fader", |(_, l, _)| *l);
        if selected.is_empty() {
            return;
        }
        let scene_id = dmx_engine.0.current_scene_focus;
        let Some(base) = self.programmer_ui_state.desktop.bases.get(&scene_id) else {
            return;
        };
        let undo = fader_undo(
            base,
            &scene_values(dmx_engine, scene_id),
            &dmx_engine.curr_scene().sink.changeset,
            &selected,
            property,
        );
        let plural = |n: usize, what: &str| format!("{n} {what}{}", if n == 1 { "" } else { "s" });
        let note = match &undo {
            FaderUndo::Session { restore, drop } => format!(
                "{label}: undid the change on {}",
                plural(restore.len() + drop.len(), "fixture")
            ),
            FaderUndo::Earlier(c) => format!(
                "{label}: removed {} from Scene changes",
                plural(c.len(), "change")
            ),
            FaderUndo::Nothing => format!("{label} has no change to undo"),
        };
        let events = fader_undo_events(&undo, property);
        if !events.is_empty() {
            self.client.send(ControlEventMessage::new(
                EventOriginator::Web,
                ControlEvent::Transaction(events),
            ));
        }
        components::notify_info(ctx, note);
    }

    // -----------------------------------------------------------------------
    // Page bar
    // -----------------------------------------------------------------------

    fn programmer_page_bar(&mut self, ui: &mut Ui, dmx_engine: &EngineState) {
        let black = self.client.grand_master_percent() == 0;
        let mut blackout = false;
        let mut reset = false;
        let mut preview = false;
        let owned = dmx_engine.live_owner() == Some(self.client.session());
        let any = dmx_engine.0.live_mode;
        studio::page_bar(
            ui,
            |ui| {
                ui.spacing_mut().item_spacing.x = 14.0;
                self.scene_picker(ui, dmx_engine);
                ui.spacing_mut().item_spacing.x = 16.0;
                self.scene_masters(ui, dmx_engine);
            },
            |ui| {
                ui.spacing_mut().item_spacing.x = 14.0;
                blackout = studio::blackout_button(ui, black).clicked();
                reset = studio::mini_button(ui, "Reset layout", false, TARGET_SHOW)
                    .on_hover_text("Restore every pane's size and open every folded pane")
                    .clicked();
                preview = exclusive_preview_button(ui, owned, any);
            },
        );
        if preview {
            self.client.send(ControlEventMessage::new(
                EventOriginator::Web,
                ControlEvent::ToggleLiveMode,
            ));
        }
        if blackout {
            self.toggle_blackout();
        }
        if reset {
            *self.programmer_layout() = PaneLayout::default();
            self.save_programmer_layout();
            components::notify_info(ui.ctx(), "Layout reset");
        }
    }

    /// The scene being edited (`.scenepick`) and its list over the page
    /// dimmed by 55%.
    fn scene_picker(&mut self, ui: &mut Ui, dmx_engine: &EngineState) {
        let t = studio_tokens(ui.ctx());
        let state = &dmx_engine.0;
        let focus = state.current_scene_focus;
        let name = state
            .scenes
            .get(&focus)
            .map_or_else(|| format!("#{focus}"), |s| s.name.clone());
        let sid = format!("#{focus}");
        let name_font = font(ui, Weight::Medium, 14.0);
        let w = (8.0
            + text_w(ui, &name, name_font.clone())
            + 8.0
            + text_w(ui, &sid, mono(TEXT))
            + 8.0
            + 24.0)
            .clamp(180.0, 320.0);
        let id = Id::new("programmer_scene_picker");
        let dd = id.with("dd");
        let (rect, _) = ui.allocate_exact_size(vec2(w, 30.0), Sense::hover());
        let resp = ui.interact(rect, id, Sense::click());
        let open = dropdown::is_open(ui.ctx(), dd);
        let sid_w = text_w(ui, &sid, mono(TEXT));
        let caret_x = rect.right() - 14.0;
        let max_name = caret_x - 10.0 - sid_w - 8.0 - (rect.left() + 8.0);
        let mut job = egui::text::LayoutJob::simple_singleline(name.clone(), name_font, t.strong);
        job.wrap = egui::text::TextWrapping::truncate_at_width(max_name.max(0.0));
        let galley = ui.painter().layout_job(job);
        // Open, the button stays above the dimmed page (`.pickwrap.open`).
        let painter = if open {
            ui.ctx().layer_painter(egui::LayerId::new(
                egui::Order::Foreground,
                id.with("button"),
            ))
        } else {
            ui.painter().clone()
        };
        let cy = rect.center().y;
        painter.rect_filled(
            rect,
            0.0,
            if resp.hovered() || open {
                t.ctl_hi
            } else {
                t.ctl
            },
        );
        if resp.has_focus() {
            painter.rect_stroke(rect, 0.0, Stroke::new(1.0_f32, t.blue), StrokeKind::Inside);
        }
        painter.text(
            pos2(caret_x, cy),
            Align2::CENTER_CENTER,
            egui_phosphor::regular::CARET_DOWN,
            FontId::proportional(11.0),
            t.muted,
        );
        let name_w = galley.size().x;
        painter.galley(
            pos2(rect.left() + 8.0, cy - galley.size().y / 2.0),
            galley,
            t.strong,
        );
        painter.text(
            pos2(rect.left() + 8.0 + name_w + 8.0, cy),
            Align2::LEFT_CENTER,
            &sid,
            mono(TEXT),
            t.muted,
        );
        let resp = resp.on_hover_text(format!("Scene being edited: {name}"));
        let alt_down =
            resp.has_focus() && ui.input_mut(|i| i.consume_key(Modifiers::ALT, Key::ArrowDown));
        if resp.clicked() || alt_down {
            dropdown::toggle(ui.ctx(), dd);
        }

        // Scenes without a folder first, then one band per folder.
        let items_tree = components::scene_tree_items(state, |id| id != BLANK_SCENE_ID);
        let rows = components::visible_rows(&items_tree, &Default::default());
        let folder_name = |f: blaulicht_shared::folder::FolderId| {
            state
                .folders
                .get(&f)
                .map(|f| f.name.clone())
                .unwrap_or_default()
        };
        let mut loose = Vec::new();
        let mut grouped = Vec::new();
        for row in rows {
            let item = &items_tree[row.item];
            if let TreeKey::Leaf(sid) = item.key {
                let entry = dropdown::Item::new(item.label.clone()).meta(format!("#{sid}"));
                match item.parent {
                    Some(f) => grouped.push((sid, entry.group(folder_name(f)))),
                    None => loose.push((sid, entry)),
                }
            }
        }
        loose.extend(grouped);
        let ids: Vec<u8> = loose.iter().map(|(id, _)| *id).collect();
        let items: Vec<dropdown::Item> = loose.into_iter().map(|(_, i)| i).collect();
        let selected = ids.iter().position(|id| *id == focus);
        let outcome = dropdown::Popover {
            id: dd,
            anchor: rect,
            anchor_id: Some(id),
            items: &items,
            options: dropdown::Options {
                width: Some(300.0),
                filter: Some(true),
                placeholder: "Filter scenes".into(),
                empty: "No scenes match.".into(),
                dim: true,
            },
        }
        .show_single(ui.ctx(), selected);
        if let dropdown::Outcome::Picked(i) = outcome {
            if let Some(&scene) = ids.get(i) {
                if scene != focus {
                    self.client.send(ControlEventMessage::new(
                        EventOriginator::Web,
                        ControlEvent::SetSceneFocus(scene),
                    ));
                }
            }
        }
    }

    /// The edited scene's master intensity and speed as small knobs.
    fn scene_masters(&mut self, ui: &mut Ui, dmx_engine: &EngineState) {
        let scene_id = dmx_engine.0.current_scene_focus;
        let sink = &dmx_engine.curr_scene().sink;
        let held = &mut self.programmer_ui_state.master_alpha_held;
        let mut alpha = held.shown(scene_id, sink.master_alpha_fader as f32);
        let response = studio::knob(
            ui,
            &mut alpha,
            0.0..=100.0,
            "Intensity",
            studio::KnobSize::Small,
        );
        if response.changed() {
            held.hold(scene_id, alpha);
            self.client.send_event(ControlEvent::SetSceneMasterAlpha(
                scene_id,
                alpha.round() as u8,
            ));
        }
        held.settle(response.dragged(), scene_id, |v| {
            v.round() as u8 == sink.master_alpha_fader
        });

        let held = &mut self.programmer_ui_state.master_speed_held;
        let mut speed = held.shown(scene_id, sink.master_speed);
        let response = studio::speed_knob(ui, &mut speed, "Speed", studio::KnobSize::Small);
        if response.changed() {
            held.hold(scene_id, speed);
            self.client
                .send_event(ControlEvent::SetSceneMasterSpeed(scene_id, speed));
        }
        held.settle(response.dragged(), scene_id, |v| v == sink.master_speed);
    }

    // -----------------------------------------------------------------------
    // Dividers
    // -----------------------------------------------------------------------

    fn programmer_grips(&mut self, ui: &mut Ui, geo: &Geometry) {
        let t = studio_tokens(ui.ctx());
        let mode = geo.mode;
        let mut changed = false;
        let mut save = false;
        for grip in &geo.grips {
            let id = Id::new(("programmer_grip", grip.kind));
            let r = ui
                .interact(grip.rect, id, Sense::click_and_drag())
                .on_hover_cursor(egui::CursorIcon::ResizeColumn)
                .on_hover_text("Drag to resize, double-click to restore");
            let (left, right) = match grip.kind {
                GripKind::Lists => (Pane::Groups, Pane::Fixtures),
                GripKind::Palettes => (Pane::Palettes, Pane::Scene),
                GripKind::Side => (Pane::PanTilt, Pane::Scene),
            };
            let wa = geo.pane(left).map_or(0.0, |(r, _)| r.width());
            let wb = geo.pane(right).map_or(0.0, |(r, _)| r.width());
            if r.double_clicked() {
                let l = self.programmer_layout().mode_mut(mode);
                let d = ModeLayout::default();
                match grip.kind {
                    GripKind::Lists => {
                        let avg = (l.groups_w + l.fixtures_w) / 2.0;
                        l.groups_w = avg;
                        l.fixtures_w = avg;
                    }
                    GripKind::Palettes => l.pal_w = d.pal_w,
                    GripKind::Side => l.side_w = d.side_w,
                }
                save = true;
            } else if r.drag_started() {
                let before = self.programmer_layout().mode(mode).clone();
                let start_x = ui
                    .input(|i| i.pointer.press_origin())
                    .or(r.interact_pointer_pos())
                    .map_or(grip.rect.center().x, |p| p.x);
                self.programmer_ui_state.desktop.grip = Some(GripDrag {
                    kind: grip.kind,
                    mode,
                    before,
                    a: wa,
                    b: wb,
                    start_x,
                });
            }
            let active = self
                .programmer_ui_state
                .desktop
                .grip
                .as_ref()
                .filter(|d| d.kind == grip.kind && d.mode == mode)
                .cloned();
            if let Some(drag) = active {
                if r.dragged() || r.drag_stopped() {
                    let x = ui
                        .input(|i| i.pointer.latest_pos())
                        .map_or(drag.start_x, |p| p.x);
                    let d = x - drag.start_x;
                    let work_w = geo.bar.width();
                    let lower_w = geo.lower_w;
                    let l = self.programmer_layout().mode_mut(mode);
                    match grip.kind {
                        GripKind::Lists => {
                            let a = clamp_list_split(drag.a + d, drag.a + drag.b, mode);
                            l.groups_w = a;
                            l.fixtures_w = drag.a + drag.b - a;
                        }
                        GripKind::Palettes => l.pal_w = clamp_pal_w(drag.a + d, lower_w),
                        GripKind::Side => {
                            let w = clamp_side_w(drag.b - d, l, work_w);
                            l.side_w = w;
                        }
                    }
                    changed = true;
                }
                if r.drag_stopped() {
                    self.programmer_ui_state.desktop.grip = None;
                    save = true;
                }
            }
            let lit = r.hovered() || r.dragged();
            if lit {
                ui.painter()
                    .rect_filled(grip.rect.shrink2(vec2(3.0, 0.0)), 0.0, t.blue);
            }
        }
        if save {
            self.save_programmer_layout();
        }
        if changed {
            ui.ctx().request_repaint();
        }
    }

    // -----------------------------------------------------------------------
    // Panes
    // -----------------------------------------------------------------------

    fn groups_pane(&mut self, ui: &mut Ui, dmx_engine: &EngineState) {
        let t = studio_tokens(ui.ctx());
        let rect = ui.max_rect();
        ui.painter().rect_filled(
            Rect::from_min_size(rect.min, vec2(rect.width(), STRIP)),
            0.0,
            t.head,
        );
        title_row(ui, "Groups", "", false, STRIP);
        let state = &dmx_engine.0;
        let selection = dmx_engine.selection();
        if state.groups.is_empty() {
            studio_note(ui, "No groups yet. Add them in Setup › Groups.");
            return;
        }
        let mut items = components::group_tree_items(state);
        components::desktop_folder_order(&mut items, &state.folders);
        for item in &mut items {
            if let TreeKey::Leaf(id) = item.key {
                item.detail = state.groups.get(&id).map(|g| g.fixtures.len().to_string());
            }
        }
        let selected = selection.group_ids.iter().map(|id| TreeKey::Leaf(*id));
        let action = egui::ScrollArea::vertical()
            .id_salt("programmer_desktop_groups")
            .auto_shrink([false, false])
            .show(ui, |ui| {
                components::Tree::new(GROUP_TREE, &items)
                    .selected_keys(selected)
                    .list_look(true)
                    .show(ui)
            })
            .inner;
        let msg = match action {
            Some(TreeAction::SelectLeaf(group_id)) => {
                Some(if selection.group_ids.contains(&group_id) {
                    ControlEvent::DeSelectGroup(group_id)
                } else {
                    ControlEvent::SelectGroup(group_id)
                })
            }
            Some(TreeAction::TapFolder(folder)) => {
                components::toggle_folder(ui.ctx(), GROUP_TREE, folder);
                None
            }
            _ => None,
        };
        if let Some(msg) = msg {
            self.client
                .send(ControlEventMessage::new(EventOriginator::Web, msg));
        }
    }

    fn fixtures_pane(&mut self, ui: &mut Ui, dmx_engine: &EngineState) {
        let t = studio_tokens(ui.ctx());
        let groups = dmx_engine.groups();
        let selection = dmx_engine.selection();
        let mut group_ids: Vec<u8> = selection.group_ids.iter().copied().collect();
        group_ids.sort_unstable();
        let total: usize = group_ids
            .iter()
            .filter_map(|g| groups.get(g))
            .map(|g| g.fixtures.len())
            .sum();
        let limited = selection.fixtures_in_group.len();

        // Title with the count right before the fold button.
        let rect = ui.max_rect();
        ui.painter().rect_filled(
            Rect::from_min_size(rect.min, vec2(rect.width(), STRIP)),
            0.0,
            t.head,
        );
        let row = title_row(ui, "Fixtures", "", false, STRIP + 30.0);
        let count = if limited > 0 {
            format!("{limited}/{total}")
        } else if total > 0 {
            total.to_string()
        } else {
            String::new()
        };
        ui.painter().text(
            pos2(row.right() - STRIP - 4.0, row.center().y),
            Align2::RIGHT_CENTER,
            count,
            font(ui, Weight::Regular, TEXT),
            t.muted,
        );

        // Filter field.
        let (frect, _) = ui.allocate_exact_size(vec2(ui.available_width(), 32.0), Sense::hover());
        let field = frect.shrink(4.0);
        ui.painter().rect_filled(field, 0.0, t.well);
        ui.painter()
            .rect_stroke(field, 0.0, Stroke::new(1.0_f32, t.line), StrokeKind::Inside);
        let mut fui = child(ui, field.shrink2(vec2(6.0, 0.0)), "programmer_fx_filter");
        fui.add_sized(
            vec2(field.width() - 12.0, field.height()),
            egui::TextEdit::singleline(&mut self.programmer_ui_state.desktop.fixture_filter)
                .hint_text("Filter fixtures")
                .frame(egui::Frame::NONE)
                .font(font(ui, Weight::Regular, TEXT))
                .vertical_align(egui::Align::Center),
        );

        // Clear, Limit, Limit all.
        let (trect, _) = ui.allocate_exact_size(vec2(ui.available_width(), 28.0), Sense::hover());
        ui.painter().rect_filled(trect, 0.0, t.pane);
        let mut tools = ui.new_child(
            egui::UiBuilder::new()
                .max_rect(Rect::from_min_max(
                    pos2(trect.left() + 4.0, trect.top()),
                    pos2(trect.right() - 4.0, trect.bottom() - 4.0),
                ))
                .layout(egui::Layout::left_to_right(egui::Align::Center)),
        );
        tools.spacing_mut().item_spacing.x = 2.0;
        let has_selection = !selection.is_empty();
        if studio::mini_button(&mut tools, "Clear", false, TARGET_MIN)
            .on_hover_text(if has_selection {
                "Clear the selection (Esc)"
            } else {
                "Nothing selected"
            })
            .clicked()
            && has_selection
        {
            self.client.send(ControlEventMessage::new(
                EventOriginator::Web,
                ControlEvent::RemoveAllSelection,
            ));
        }
        // Placeholders until their behaviour is decided (`RULES.md`).
        studio::mini_button(&mut tools, "Limit", false, TARGET_MIN)
            .on_hover_text("Placeholder, no action yet");
        studio::mini_button(&mut tools, "Limit all", false, TARGET_MIN)
            .on_hover_text("Placeholder, no action yet");

        let filter = self
            .programmer_ui_state
            .desktop
            .fixture_filter
            .trim()
            .to_lowercase();
        let many = group_ids.len() > 1;
        let mut clicked = None;
        egui::ScrollArea::vertical()
            .id_salt("programmer_desktop_fixtures")
            .auto_shrink([false, false])
            .show(ui, |ui| {
                ui.spacing_mut().item_spacing.y = 0.0;
                if group_ids.is_empty() {
                    studio_note(ui, "Select a group to list its fixtures.");
                }
                for gid in &group_ids {
                    let Some(group) = groups.get(gid) else {
                        continue;
                    };
                    if many {
                        let (r, _) = ui.allocate_exact_size(
                            vec2(ui.available_width(), LIST_ROW_HEIGHT),
                            Sense::hover(),
                        );
                        ui.painter().text(
                            pos2(r.left() + 8.0, r.center().y),
                            Align2::LEFT_CENTER,
                            &group.name,
                            font(ui, Weight::Medium, TEXT),
                            t.muted,
                        );
                    }
                    for (fid, fixture) in &group.fixtures {
                        if !filter.is_empty() && !fixture.name.to_lowercase().contains(&filter) {
                            continue;
                        }
                        let on = selection.fixtures_in_group.contains(fid);
                        let state = if on {
                            ListRowState::Included
                        } else {
                            ListRowState::Idle
                        };
                        if ListRow::new(&fixture.name).state(state).show(ui).clicked() {
                            clicked = Some((*gid, *fid, on));
                        }
                    }
                }
            });
        // Limiting works within one selected group (engine rule).
        if let Some((_, fid, on)) = clicked {
            if group_ids.len() == 1 {
                let msg = if on {
                    ControlEvent::UnLimitSelectionToFixtureInCurrentGroup(fid)
                } else {
                    ControlEvent::LimitSelectionToFixtureInCurrentGroup(fid)
                };
                self.client
                    .send(ControlEventMessage::new(EventOriginator::Web, msg));
            }
        }
    }

    /// The selection's name for the fader bank: "MAC250E (all 6 fixtures)".
    pub(crate) fn selection_name(dmx_engine: &EngineState) -> String {
        let groups = dmx_engine.groups();
        let selection = dmx_engine.selection();
        if selection.is_empty() {
            return "Nothing selected".into();
        }
        let mut ids: Vec<u8> = selection.group_ids.iter().copied().collect();
        ids.sort_unstable();
        let names: Vec<&str> = ids
            .iter()
            .filter_map(|g| groups.get(g).map(|g| g.name.as_str()))
            .collect();
        let total: usize = ids
            .iter()
            .filter_map(|g| groups.get(g))
            .map(|g| g.fixtures.len())
            .sum();
        let n = selection.fixtures_in_group.len();
        if n > 0 {
            format!("{} ({n} of {total} fixtures)", names.join(", "))
        } else {
            format!("{} (all {total} fixtures)", names.join(", "))
        }
    }

    fn faders_pane(&mut self, ui: &mut Ui, dmx_engine: &EngineState) {
        let t = studio_tokens(ui.ctx());
        let rect = ui.max_rect();
        ui.painter().rect_filled(
            Rect::from_min_size(rect.min, vec2(rect.width(), STRIP)),
            0.0,
            t.head,
        );
        title_row(ui, &Self::selection_name(dmx_engine), "", false, STRIP);

        let selected = dmx_engine.get_selection().fixtures;
        let scene = dmx_engine.curr_scene();
        let palettes = &dmx_engine.0.palettes;
        // The edited scene's values of the first selected fixture; the
        // programmer buffer without a selection.
        let first = selected
            .iter()
            .find_map(|k| scene.sink.fixture_states.get(k))
            .cloned()
            .unwrap_or_else(|| dmx_engine.0.control_buffer.clone());
        let resolved = first.resolve(palettes);
        let bank = Rect::from_min_max(
            pos2(rect.left() + 8.0, rect.top() + STRIP + 10.0),
            pos2(rect.right() - 8.0, rect.bottom() - 8.0),
        );
        let strip_w = bank.width() / FADERS.len() as f32;
        let desk = &mut self.programmer_ui_state.desktop;
        let mut send = Vec::new();
        for (i, (property, label, max)) in FADERS.iter().enumerate() {
            let changed = selected.iter().any(|(g, f)| {
                scene.sink.changeset.contains(&FixtureSelector {
                    gid: *g,
                    fid: *f,
                    property: *property,
                })
            });
            let frozen = selected.iter().any(|k| {
                scene
                    .sink
                    .fixture_states
                    .get(k)
                    .is_some_and(|s| s.slot(*property).is_frozen())
            });
            let mut value = match desk.dragging {
                Some((p, v)) if p == *property => v,
                _ => prop_value(&resolved, *property),
            };
            let x = bank.left() + i as f32 * strip_w + (strip_w - 52.0) / 2.0;
            let srect = Rect::from_min_size(pos2(x, bank.top()), vec2(52.0, bank.height()));
            let mut sui = child(ui, srect, ("programmer_fader", i));
            let r = sui
                .add_enabled_ui(!frozen, |ui| {
                    studio::fader(ui, &mut value, 0.0..=*max, label, changed, srect.height())
                })
                .inner;
            let r = if frozen {
                r.on_hover_text(format!("{label} is set by a palette"))
            } else {
                r
            };
            if r.clicked() || r.drag_started() || r.gained_focus() || r.has_focus() {
                desk.picked = Some(*property);
            }
            if r.dragged() {
                desk.dragging = Some((*property, value));
            } else if matches!(desk.dragging, Some((p, _)) if p == *property) {
                desk.dragging = None;
            }
            if r.changed() {
                send.push(set_event(*property, value));
            }
        }
        for e in send {
            self.client
                .send(ControlEventMessage::new(EventOriginator::Web, e));
        }
    }

    fn colour_pane(&mut self, ui: &mut Ui, dmx_engine: &EngineState) {
        let t = studio_tokens(ui.ctx());
        title_row(ui, "Colour", "", false, STRIP);
        let resolved = self.first_selected_state(dmx_engine);
        let rgb: RGBColor = resolved.color.into();
        let rect = ui.max_rect();
        let (h, sat, val) = (
            resolved.color.h as f32,
            resolved.color.s as f32,
            resolved.color.v as f32,
        );
        let frozen = |p: FixtureProperty| {
            let scene = dmx_engine.curr_scene();
            dmx_engine.get_selection().fixtures.iter().any(|k| {
                scene
                    .sink
                    .fixture_states
                    .get(k)
                    .is_some_and(|s| s.slot(p).is_frozen())
            })
        };
        let hue_ok = !frozen(FixtureProperty::ColorHue);
        let sv_ok =
            !frozen(FixtureProperty::ColorSaturation) && !frozen(FixtureProperty::ColorValue);
        // Wheel under the title, centred; the readout right below it.
        let wheel = Rect::from_min_size(
            pos2(
                rect.center().x - components::HSV_TRIANGLE_SIZE / 2.0,
                rect.top() + STRIP + 4.0,
            ),
            vec2(components::HSV_TRIANGLE_SIZE, components::HSV_TRIANGLE_SIZE),
        );
        let mut wui = ui.new_child(egui::UiBuilder::new().max_rect(wheel));
        let (wr, change) = components::hsv_triangle(&mut wui, h, sat, val, hue_ok, sv_ok);
        if !hue_ok || !sv_ok {
            wr.on_hover_text("Colour is set by a palette");
        }
        match change {
            Some(components::HsvChange::Hue(nh)) if (nh.round() - h).abs() >= 1.0 => {
                self.client.send(ControlEventMessage::new(
                    EventOriginator::Web,
                    set_event(FixtureProperty::ColorHue, nh),
                ));
            }
            Some(components::HsvChange::SatValue { s, v }) => {
                for (p, old, new) in [
                    (FixtureProperty::ColorSaturation, sat, s),
                    (FixtureProperty::ColorValue, val, v),
                ] {
                    if ((new * 255.0).round() - (old * 255.0).round()).abs() >= 1.0 {
                        self.client.send(ControlEventMessage::new(
                            EventOriginator::Web,
                            set_event(p, new * 255.0),
                        ));
                    }
                }
            }
            _ => {}
        }
        let w = rect.width().min(196.0) - 16.0;
        let row = Rect::from_min_size(
            pos2(rect.center().x - w / 2.0, wheel.bottom() + 6.0),
            vec2(w, 40.0),
        );
        let mut cui = ui.new_child(
            egui::UiBuilder::new()
                .max_rect(row)
                .layout(egui::Layout::left_to_right(egui::Align::Center)),
        );
        cui.spacing_mut().item_spacing.x = 2.0;
        let mut new: Option<(u8, u8, u8)> = None;
        let (sw, _) = cui.allocate_exact_size(vec2(36.0, 40.0), Sense::hover());
        cui.painter()
            .rect_filled(sw, 0.0, Color32::from_rgb(rgb.r, rgb.g, rgb.b));
        cui.painter()
            .rect_stroke(sw, 0.0, Stroke::new(1.0_f32, t.line), StrokeKind::Inside);
        let cell_w = ((w - 36.0 - 6.0) / 3.0).floor();
        let mut channels = [rgb.r, rgb.g, rgb.b];
        for (i, label) in ["R", "G", "B"].iter().enumerate() {
            let (cell, _) = cui.allocate_exact_size(vec2(cell_w, 40.0), Sense::hover());
            cui.painter().rect_filled(cell, 0.0, t.well);
            cui.painter().text(
                pos2(cell.center().x, cell.top() + 9.0),
                Align2::CENTER_CENTER,
                label,
                font(&cui, Weight::Regular, TEXT_MIN),
                t.muted,
            );
            let mut v = channels[i] as f64;
            let field = Rect::from_center_size(
                pos2(cell.center().x, cell.bottom() - 12.0),
                vec2(40.0, 20.0),
            );
            let mut fui = cui.new_child(egui::UiBuilder::new().max_rect(field));
            if studio::num_field(&mut fui, &mut v, 1.0, 0.0..=255.0, 0, 40.0).changed() {
                channels[i] = v.round() as u8;
                new = Some((channels[0], channels[1], channels[2]));
            }
        }
        if let Some(c) = new {
            if RGBColor::from(c) != rgb {
                self.client.send(ControlEventMessage::new(
                    EventOriginator::Web,
                    ControlEvent::SetColor(c),
                ));
            }
        }
    }

    /// The edited scene's state of the first selected fixture, or the
    /// programmer buffer.
    fn first_selected_state(&self, dmx_engine: &EngineState) -> ResolvedFixtureState {
        let scene = dmx_engine.curr_scene();
        dmx_engine
            .get_selection()
            .fixtures
            .iter()
            .find_map(|k| scene.sink.fixture_states.get(k))
            .cloned()
            .unwrap_or_else(|| dmx_engine.0.control_buffer.clone())
            .resolve(&dmx_engine.0.palettes)
    }

    fn pan_tilt_pane(&mut self, ui: &mut Ui, dmx_engine: &EngineState) {
        let t = studio_tokens(ui.ctx());
        title_row(ui, "Pan / Tilt", "", false, STRIP);
        let r = self.first_selected_state(dmx_engine);
        let (pan, tilt) = (r.orientation.pan, r.orientation.tilt);
        let rect = ui.max_rect();
        let pad = Rect::from_min_size(
            pos2(rect.center().x - PAD / 2.0, rect.top() + STRIP + 4.0),
            vec2(PAD, PAD),
        );
        let id = Id::new("programmer_xy");
        let resp = ui.interact(pad, id, Sense::click_and_drag());
        let p = ui.painter();
        p.rect_filled(pad, 0.0, t.well);
        p.rect_stroke(pad, 0.0, Stroke::new(1.0_f32, t.line), StrokeKind::Outside);
        p.vline(pad.center().x, pad.y_range(), Stroke::new(1.0_f32, t.line));
        p.hline(pad.x_range(), pad.center().y, Stroke::new(1.0_f32, t.line));
        let dot = pos2(
            pad.left() + pan as f32 / 255.0 * PAD,
            pad.top() + tilt as f32 / 255.0 * PAD,
        );
        p.rect_filled(Rect::from_center_size(dot, vec2(6.0, 6.0)), 0.0, t.blue);
        if resp.has_focus() {
            p.rect_stroke(
                pad.expand(1.0),
                0.0,
                Stroke::new(1.0_f32, t.blue),
                StrokeKind::Outside,
            );
        }
        let mut target: Option<(u8, u8)> = None;
        if resp.clicked() || resp.dragged() {
            resp.request_focus();
            if let Some(pos) = resp.interact_pointer_pos() {
                let x = ((pos.x - pad.left()) / PAD).clamp(0.0, 1.0);
                let y = ((pos.y - pad.top()) / PAD).clamp(0.0, 1.0);
                target = Some(((x * 255.0).round() as u8, (y * 255.0).round() as u8));
            }
        }
        if resp.has_focus() {
            let (dx, dy) = ui.input_mut(|i| {
                let mut dx = 0i32;
                let mut dy = 0i32;
                if i.consume_key(Modifiers::NONE, Key::ArrowLeft) {
                    dx -= 1;
                }
                if i.consume_key(Modifiers::NONE, Key::ArrowRight) {
                    dx += 1;
                }
                if i.consume_key(Modifiers::NONE, Key::ArrowUp) {
                    dy -= 1;
                }
                if i.consume_key(Modifiers::NONE, Key::ArrowDown) {
                    dy += 1;
                }
                (dx, dy)
            });
            if dx != 0 || dy != 0 {
                target = Some((
                    (pan as i32 + dx).clamp(0, 255) as u8,
                    (tilt as i32 + dy).clamp(0, 255) as u8,
                ));
            }
        }
        resp.on_hover_text("Pan and tilt. Drag, or use the arrow keys.");

        // Readout and Centre.
        let ry = pad.bottom() + 14.0;
        let f = font(ui, Weight::Regular, TEXT);
        let parts = [("Pan ", pan), ("   Tilt ", tilt)];
        let mut w = 0.0;
        for (l, v) in parts {
            w += text_w(ui, l, f.clone()) + text_w(ui, &v.to_string(), mono(TEXT));
        }
        let mut x = pad.center().x - w / 2.0;
        for (l, v) in parts {
            ui.painter()
                .text(pos2(x, ry), Align2::LEFT_CENTER, l, f.clone(), t.muted);
            x += text_w(ui, l, f.clone());
            let vs = v.to_string();
            ui.painter()
                .text(pos2(x, ry), Align2::LEFT_CENTER, &vs, mono(TEXT), t.strong);
            x += text_w(ui, &vs, mono(TEXT));
        }
        let cw = text_w(ui, "Centre", f) + 16.0;
        let crect = Rect::from_min_size(
            pos2(pad.center().x - cw / 2.0, ry + 14.0),
            vec2(cw, TARGET_MIN),
        );
        let mut bui = ui.new_child(egui::UiBuilder::new().max_rect(crect));
        if studio::mini_button(&mut bui, "Centre", false, TARGET_MIN).clicked() {
            target = Some((128, 128));
        }
        if let Some((p, tl)) = target {
            if (p, tl) != (pan, tilt) {
                self.client.send(ControlEventMessage::new(
                    EventOriginator::Web,
                    ControlEvent::Transaction(vec![
                        ControlEvent::SetPan(p),
                        ControlEvent::SetTilt(tl),
                    ]),
                ));
            }
        }
    }

    fn palettes_pane(&mut self, ui: &mut Ui, dmx_engine: &EngineState) {
        let t = studio_tokens(ui.ctx());
        let state = &dmx_engine.0;
        let selection = dmx_engine.get_selection();
        let active: Vec<u8> = self
            .programmer_ui_state
            .active_palettes_cache
            .get(&selection)
            .cloned()
            .unwrap_or_default();
        title_row(
            ui,
            "Palettes",
            &format!("{} active", active.len()),
            false,
            STRIP,
        );
        let rect = ui.max_rect();
        let mut body = child(
            ui,
            Rect::from_min_max(
                pos2(rect.left() + 8.0, rect.top() + STRIP),
                pos2(rect.right() - 8.0, rect.bottom() - 8.0),
            ),
            "programmer_palettes",
        );
        if state.palettes.is_empty() {
            studio_note(&mut body, "No palettes. Create them in Setup › Palettes.");
            return;
        }
        // One section per kind, in palette order.
        let mut kinds: Vec<(&'static str, Vec<u8>)> = Vec::new();
        for (id, palette) in &state.palettes {
            let label = palette_section(state, *id, palette, 0);
            match kinds.iter_mut().find(|(k, _)| *k == label) {
                Some((_, ids)) => ids.push(*id),
                None => kinds.push((label, vec![*id])),
            }
        }
        let mut clicked = None;
        egui::ScrollArea::vertical()
            .id_salt("programmer_desktop_palettes")
            .auto_shrink([false, false])
            .show(&mut body, |ui| {
                ui.spacing_mut().item_spacing = vec2(2.0, 2.0);
                for (kind, ids) in &kinds {
                    ui.add_space(4.0);
                    ui.label(
                        egui::RichText::new(*kind)
                            .font(font(ui, Weight::Regular, TEXT))
                            .color(t.muted),
                    );
                    ui.add_space(4.0);
                    let cols = 3usize;
                    let w =
                        ((ui.available_width() - 2.0 * (cols as f32 - 1.0)) / cols as f32).floor();
                    for chunk in ids.chunks(cols) {
                        ui.horizontal(|ui| {
                            for id in chunk {
                                let order = active.iter().position(|a| a == id);
                                if palette_chip(ui, state, *id, order, w) {
                                    clicked = Some(*id);
                                }
                            }
                        });
                    }
                }
            });
        if let (Some(id), false) = (clicked, selection.fixtures.is_empty()) {
            self.client.send(ControlEventMessage::new(
                EventOriginator::Web,
                super::programmer::palette_toggle_event(&active, id),
            ));
        }
    }

    fn scene_pane(&mut self, ui: &mut Ui, dmx_engine: &EngineState) {
        let t = studio_tokens(ui.ctx());
        let scene = dmx_engine.curr_scene();
        let groups = dmx_engine.groups();
        let rect = ui.max_rect();

        // Animations.
        let anims: Vec<_> = {
            let mut v: Vec<_> = scene
                .sink
                .active_animations
                .iter()
                .flat_map(|(sel, a)| {
                    a.iter()
                        .map(move |(id, anim)| (sel.clone(), *id, anim.clone()))
                })
                .collect();
            v.sort_by_key(|(_, id, _)| *id);
            v
        };
        let head = title_row(
            ui,
            "Animations",
            &anims.len().to_string(),
            true,
            STRIP + 4.0,
        );
        let add_rect = Rect::from_min_size(
            pos2(head.right() - STRIP - 4.0 - 34.0, head.top() + 1.0),
            vec2(34.0, TARGET_MIN),
        );
        let mut aui = ui.new_child(egui::UiBuilder::new().max_rect(add_rect));
        if studio::mini_button(
            &mut aui,
            "Add",
            self.programmer_ui_state.animation_presets.open,
            TARGET_MIN,
        )
        .on_hover_text("Add an animation to the selection")
        .clicked()
        {
            self.programmer_ui_state.animation_presets.open =
                !self.programmer_ui_state.animation_presets.open;
        }
        if anims.is_empty() {
            studio_note(ui, "No animations in this scene.");
        }
        let mut action: Option<(FixtureSelection, u8, &'static str)> = None;
        for (row_i, (sel, id, anim)) in anims.iter().enumerate() {
            let (row, _) = ui.allocate_exact_size(vec2(ui.available_width(), 40.0), Sense::hover());
            ui.painter().hline(
                row.x_range(),
                row.bottom() - 0.5,
                Stroke::new(1.0_f32, t.line),
            );
            let play = Rect::from_min_size(
                pos2(row.left() + 6.0, row.center().y - 13.0),
                vec2(26.0, 26.0),
            );
            let pr = ui.interact(play, Id::new(("anim_play", row_i)), Sense::click());
            ui.painter()
                .rect_filled(play, 0.0, if pr.hovered() { t.ctl_hi } else { t.ctl });
            let (glyph, color) = if anim.enabled {
                (egui_phosphor::fill::PAUSE, t.ok)
            } else {
                (egui_phosphor::fill::PLAY, t.text)
            };
            ui.painter().text(
                play.center(),
                Align2::CENTER_CENTER,
                glyph,
                FontId::new(
                    14.0,
                    FontFamily::Name(crate::app::ui_tokens::PHOSPHOR_FILL_FAMILY.into()),
                ),
                color,
            );
            if pr
                .on_hover_text(if anim.enabled { "Pause" } else { "Play" })
                .clicked()
            {
                action = Some((sel.clone(), *id, "play"));
            }
            let edit = Rect::from_min_size(
                pos2(row.right() - 56.0, row.center().y - 13.0),
                vec2(26.0, 26.0),
            );
            let rm = Rect::from_min_size(
                pos2(row.right() - 28.0, row.center().y - 13.0),
                vec2(26.0, 26.0),
            );
            if icon_button(
                ui,
                edit,
                Id::new(("anim_edit", row_i)),
                egui_phosphor::regular::PENCIL_SIMPLE,
                "Edit",
            ) {
                action = Some((sel.clone(), *id, "edit"));
            }
            if icon_button(
                ui,
                rm,
                Id::new(("anim_rm", row_i)),
                egui_phosphor::regular::X,
                "Remove",
            ) {
                action = Some((sel.clone(), *id, "remove"));
            }
            let mut gnames: Vec<&str> = Vec::new();
            for (g, _) in &sel.fixtures {
                if let Some(group) = groups.get(g) {
                    if !gnames.contains(&group.name.as_str()) {
                        gnames.push(&group.name);
                    }
                }
            }
            let x = play.right() + 6.0;
            let maxw = edit.left() - 6.0 - x;
            let name_color = if anim.enabled { t.strong } else { t.muted };
            studio::paint_truncated(
                ui,
                pos2(x, row.center().y - 7.0),
                &anim.spec_cloned.name,
                font(ui, Weight::Regular, TEXT + 1.0),
                name_color,
                maxw,
            );
            studio::paint_truncated(
                ui,
                pos2(x, row.center().y + 8.0),
                &gnames.join(", "),
                font(ui, Weight::Regular, TEXT),
                t.muted,
                maxw,
            );
        }
        if let Some((sel, id, what)) = action {
            self.animation_action(dmx_engine, sel, id, what);
        }

        // Scene changes.
        ui.add_space(2.0);
        let changes = super::programmer::sorted_changes_pub(&scene.sink.changeset);
        let head = title_row(ui, "Scene changes", &changes.len().to_string(), true, 4.0);
        let clear_w = text_w(ui, "Clear all", font(ui, Weight::Regular, TEXT)) + 16.0;
        let crect = Rect::from_min_size(
            pos2(head.right() - 4.0 - clear_w, head.top() + 1.0),
            vec2(clear_w, TARGET_MIN),
        );
        let mut cui = ui.new_child(egui::UiBuilder::new().max_rect(crect));
        if studio::mini_button(&mut cui, "Clear all", false, TARGET_MIN).clicked()
            && !changes.is_empty()
        {
            self.programmer_ui_state.undo_all_changes_confirm_open = true;
        }
        let list_rect = Rect::from_min_max(ui.cursor().min, rect.max);
        ui.painter().rect_filled(list_rect, 0.0, t.list);
        let selected_groups = &dmx_engine.selection().group_ids;
        let values = scene_values(dmx_engine, dmx_engine.0.current_scene_focus);
        let mut by_group: BTreeMap<u8, BTreeMap<u8, Vec<FixtureSelector>>> = BTreeMap::new();
        for c in &changes {
            if groups.contains_key(&c.gid) {
                by_group
                    .entry(c.gid)
                    .or_default()
                    .entry(c.fid)
                    .or_default()
                    .push(c.clone());
            }
        }
        let mut toggle_open: Option<(u8, Option<u8>)> = None;
        let mut remove: Option<Vec<FixtureSelector>> = None;
        let open = self.programmer_ui_state.desktop.changes_open.clone();
        egui::ScrollArea::vertical()
            .id_salt("programmer_desktop_changes")
            .auto_shrink([false, false])
            .show(ui, |ui| {
                ui.spacing_mut().item_spacing.y = 0.0;
                if by_group.is_empty() {
                    studio_note(ui, "No changes in this scene.");
                }
                for (gid, fixtures) in &by_group {
                    let group = &groups[gid];
                    let all: Vec<FixtureSelector> = fixtures.values().flatten().cloned().collect();
                    let g_open = open.contains(&(*gid, None));
                    let (clicked, x) = change_row(
                        ui,
                        Id::new(("chg", *gid)),
                        0.0,
                        Some(g_open),
                        &group.name,
                        &all.len().to_string(),
                        true,
                        selected_groups.contains(gid),
                    );
                    if x {
                        remove = Some(all.clone());
                    } else if clicked {
                        toggle_open = Some((*gid, None));
                    }
                    if !g_open {
                        continue;
                    }
                    for (fid, props) in fixtures {
                        let f_open = open.contains(&(*gid, Some(*fid)));
                        let name = group
                            .fixtures
                            .get(fid)
                            .map_or_else(|| format!("#{fid}"), |f| f.name.clone());
                        let (clicked, x) = change_row(
                            ui,
                            Id::new(("chg", *gid, *fid)),
                            20.0,
                            Some(f_open),
                            &name,
                            &props.len().to_string(),
                            false,
                            false,
                        );
                        if x {
                            remove = Some(props.clone());
                        } else if clicked {
                            toggle_open = Some((*gid, Some(*fid)));
                        }
                        if !f_open {
                            continue;
                        }
                        for c in props {
                            let v = values
                                .get(&(c.gid, c.fid, c.property))
                                .map_or(String::new(), |v| format!("{}", v.round() as i64));
                            let (_, x) = change_row(
                                ui,
                                Id::new(("chg", *gid, *fid, c.property as u8)),
                                45.0,
                                None,
                                property_label(c.property),
                                &v,
                                false,
                                false,
                            );
                            if x {
                                remove = Some(vec![c.clone()]);
                            }
                        }
                    }
                }
            });
        if let Some(k) = toggle_open {
            let set = &mut self.programmer_ui_state.desktop.changes_open;
            if !set.remove(&k) {
                set.insert(k);
            }
        }
        if let Some(changes) = remove {
            if let Some(event) = remove_changes_event(&changes) {
                if changes.len() > 1 {
                    self.programmer_ui_state.desktop.confirm = Some(PendingRemove {
                        title: format!("Remove {} changes?", changes.len()),
                        body:
                            "These values go back to the scene's saved state. This can't be undone."
                                .into(),
                        action: "Remove changes".into(),
                        event,
                    });
                } else {
                    self.client
                        .send(ControlEventMessage::new(EventOriginator::Web, event));
                }
            }
        }
    }

    fn animation_action(
        &mut self,
        dmx_engine: &EngineState,
        sel: FixtureSelection,
        id: u8,
        what: &str,
    ) {
        let scene = dmx_engine.curr_scene();
        let Some(anim) = scene
            .sink
            .active_animations
            .get(&sel)
            .and_then(|a| a.get(&id))
        else {
            return;
        };
        let wrap = |event: ControlEvent| {
            let mut instr = sel.generate_instructions();
            instr.push_front(ControlEvent::PushSelection);
            instr.push_back(event);
            instr.push_back(ControlEvent::PopSelection);
            ControlEvent::Transaction(instr.into_iter().collect())
        };
        match what {
            "play" => {
                let event = if anim.enabled {
                    ControlEvent::PauseAnimation(id)
                } else {
                    ControlEvent::PlayAnimation(id)
                };
                self.client
                    .send(ControlEventMessage::new(EventOriginator::Web, wrap(event)));
            }
            "edit" => {
                self.current_scene_animations_dialog_open = true;
                self.programmer_ui_state
                    .scene_overview_animation_selection_edit = Some((sel, id));
                self.programmer_ui_state
                    .scene_overview_animation_selection_edit_need_to_load = true;
                self.programmer_ui_state.close_scene_overview_after_child = true;
            }
            _ => {
                self.programmer_ui_state.desktop.confirm = Some(PendingRemove {
                    title: format!("Remove {}?", anim.spec_cloned.name),
                    body: "It stops and its settings are lost. This can't be undone.".into(),
                    action: "Remove animation".into(),
                    event: wrap(ControlEvent::RemoveAnimation(id)),
                });
            }
        }
    }

    fn render_pending_remove(&mut self, ctx: &Context) {
        let Some(pending) = self.programmer_ui_state.desktop.confirm.clone() else {
            return;
        };
        let mut close = false;
        let response = Dialog::new(pending.title.clone(), vec2(380.0, 150.0))
            .with_backdrop()
            .show(ctx, |ui| {
                ui.heading(&pending.title);
                ui.add_space(4.0);
                ui.label(&pending.body);
                ui.add_space(12.0);
                ui.horizontal(|ui| {
                    if studio::mini_button(ui, "Cancel", false, TARGET_SHOW).clicked() {
                        close = true;
                    }
                    if studio::mini_button(ui, &pending.action, false, TARGET_SHOW).clicked() {
                        self.client.send(ControlEventMessage::new(
                            EventOriginator::Web,
                            pending.event.clone(),
                        ));
                        close = true;
                    }
                });
            });
        if close || response.cancel_requested {
            self.programmer_ui_state.desktop.confirm = None;
        }
    }
}

/// A property's name in the Scene changes list, as on its fader.
fn property_label(property: FixtureProperty) -> &'static str {
    match property {
        FixtureProperty::ColorSaturation => "Saturation",
        p => FADERS
            .iter()
            .find(|(q, _, _)| *q == p)
            .map_or("", |(_, l, _)| *l),
    }
}

/// Exclusive preview (`.livebtn`): lit while this session previews.
/// Returns whether it was clicked.
pub(crate) fn exclusive_preview_button(ui: &mut Ui, owned: bool, any: bool) -> bool {
    let response = studio::live_button(ui, "Exclusive preview", owned, TARGET_SHOW);
    let clicked = response.clicked();
    response.on_hover_text(match (owned, any) {
        (true, _) => "Only the selected scene renders. Click to return to the live rig.",
        (false, true) => {
            "Another operator is previewing their scene; overlays are suppressed. Click to preview yours instead."
        }
        (false, false) => {
            "Exclusive preview: shows only this scene and suppresses overlays. Off by default."
        }
    });
    clicked
}

/// A muted line of help text with the list padding.
fn studio_note(ui: &mut Ui, text: &str) {
    let t = studio_tokens(ui.ctx());
    let (r, _) = ui.allocate_exact_size(
        vec2(ui.available_width(), LIST_ROW_HEIGHT + 4.0),
        Sense::hover(),
    );
    studio::paint_truncated(
        ui,
        pos2(r.left() + 8.0, r.center().y),
        text,
        font(ui, Weight::Regular, TEXT),
        t.muted,
        r.width() - 16.0,
    );
}

/// One row of the Scene changes tree. `open` is `None` for a property row
/// (no caret). Returns (clicked, remove clicked).
#[allow(clippy::too_many_arguments)]
fn change_row(
    ui: &mut Ui,
    id: Id,
    indent: f32,
    open: Option<bool>,
    name: &str,
    value: &str,
    group: bool,
    selected: bool,
) -> (bool, bool) {
    let t = studio_tokens(ui.ctx());
    if group {
        ui.add_space(2.0);
    }
    let (rect, _) =
        ui.allocate_exact_size(vec2(ui.available_width(), LIST_ROW_HEIGHT), Sense::hover());
    let r = ui.interact(rect, id, Sense::click());
    let p = ui.painter();
    if group && !selected {
        p.rect_filled(rect, 0.0, if r.hovered() { t.head } else { t.band });
    }
    if selected {
        p.rect_filled(rect, 0.0, t.blue_dim);
        p.rect_filled(
            Rect::from_min_size(rect.min, vec2(2.0, rect.height())),
            0.0,
            t.blue,
        );
    } else if r.hovered() {
        p.rect_filled(rect, 0.0, t.head);
    }
    p.hline(
        rect.x_range(),
        rect.bottom() - 0.5,
        Stroke::new(1.0_f32, t.hair),
    );
    let cy = rect.center().y;
    let mut x = rect.left() + 8.0 + indent;
    if let Some(open) = open {
        p.text(
            pos2(x + 5.0, cy),
            Align2::CENTER_CENTER,
            if open {
                egui_phosphor::regular::CARET_DOWN
            } else {
                egui_phosphor::regular::CARET_RIGHT
            },
            FontId::proportional(10.0),
            t.muted,
        );
        x += 15.0;
    }
    let xr = Rect::from_min_size(
        pos2(rect.right() - 22.0, rect.top()),
        vec2(18.0, rect.height()),
    );
    let vf = mono(TEXT);
    let vw = text_w(ui, value, vf.clone());
    let vx = xr.left() - 4.0 - vw;
    let (nf, nc) = if group {
        (font(ui, Weight::Medium, TEXT + 1.0), t.strong)
    } else if open.is_none() {
        (font(ui, Weight::Regular, TEXT), t.muted)
    } else {
        (font(ui, Weight::Regular, TEXT), t.text)
    };
    studio::paint_truncated(ui, pos2(x, cy), name, nf, nc, vx - 6.0 - x);
    let vc = if open.is_none() || selected {
        t.strong
    } else {
        t.muted
    };
    ui.painter()
        .text(pos2(vx, cy), Align2::LEFT_CENTER, value, vf, vc);
    let mut removed = false;
    let hovered_row = r.hovered() || ui.rect_contains_pointer(rect);
    if hovered_row {
        let xresp = ui.interact(xr, id.with("x"), Sense::click());
        ui.painter().text(
            xr.center(),
            Align2::CENTER_CENTER,
            egui_phosphor::regular::X,
            FontId::proportional(12.0),
            if xresp.hovered() { t.strong } else { t.muted },
        );
        removed = xresp.on_hover_text("Remove").clicked();
    }
    (r.clicked() && !removed, removed)
}

/// A palette chip: swatch, name, link icon for a palette that follows
/// another, and its apply order when active. Returns whether it was clicked.
fn palette_chip(
    ui: &mut Ui,
    state: &blaulicht_shared::EngineState,
    id: u8,
    order: Option<usize>,
    width: f32,
) -> bool {
    let t = studio_tokens(ui.ctx());
    let Some(palette) = state.palettes.get(&id) else {
        return false;
    };
    let (rect, r) = ui.allocate_exact_size(vec2(width, TARGET_MIN), Sense::click());
    let on = order.is_some();
    let p = ui.painter();
    p.rect_filled(
        rect,
        0.0,
        if on {
            t.blue_dim
        } else if r.hovered() {
            t.ctl_hi
        } else {
            t.ctl
        },
    );
    if on {
        p.rect_stroke(rect, 0.0, Stroke::new(1.0_f32, t.blue), StrokeKind::Inside);
    }
    let mut x = rect.left() + 6.0;
    if let Some(c) = palette_colour(state, id, 0) {
        let sw = Rect::from_min_size(pos2(x, rect.center().y - 6.0), vec2(12.0, 12.0));
        p.rect_filled(sw, 0.0, c);
        x += 18.0;
    }
    let mut right = rect.right() - 6.0;
    if let Some(n) = order {
        let s = (n + 1).to_string();
        right -= text_w(ui, &s, mono(TEXT_MIN));
        ui.painter().text(
            pos2(right, rect.center().y),
            Align2::LEFT_CENTER,
            &s,
            mono(TEXT_MIN),
            t.blue,
        );
        right -= 6.0;
    }
    if matches!(palette.kind, PaletteKind::Pointer { .. }) {
        ui.painter().text(
            pos2(right - 6.0, rect.center().y),
            Align2::CENTER_CENTER,
            egui_phosphor::regular::LINK,
            FontId::proportional(12.0),
            t.muted,
        );
        right -= 16.0;
    }
    let name = components::item_name(state, blaulicht_shared::folder::FolderItem::Palette(id));
    let cut = studio::paint_truncated(
        ui,
        pos2(x, rect.center().y),
        &name,
        font(ui, Weight::Regular, TEXT),
        if on { t.strong } else { t.text },
        right - x,
    );
    let tip = match &palette.kind {
        PaletteKind::Pointer { target, .. } => Some(format!(
            "{name}: follows {}",
            components::item_name(
                state,
                blaulicht_shared::folder::FolderItem::Palette(*target)
            )
        )),
        _ if cut => Some(name.clone()),
        _ => None,
    };
    let r = match tip {
        Some(tip) => r.on_hover_text(tip),
        None => r,
    };
    r.clicked()
}

/// The Palettes section a palette is listed under: its kind, or for a
/// palette that follows another, the kind of the one it follows.
fn palette_section(
    state: &blaulicht_shared::EngineState,
    id: u8,
    palette: &blaulicht_shared::palette::Palette,
    depth: u8,
) -> &'static str {
    match &palette.kind {
        PaletteKind::Pointer { target, .. } if depth < 8 && *target != id => {
            match state.palettes.get(target) {
                Some(t) => palette_section(state, *target, t, depth + 1),
                None => "Pointer",
            }
        }
        PaletteKind::Color(_) => "Colour",
        kind => super::palette_kind_label(kind),
    }
}

/// The colour a palette shows: its own colour, or the colour of the palette
/// it follows.
fn palette_colour(state: &blaulicht_shared::EngineState, id: u8, depth: u8) -> Option<Color32> {
    if depth > 8 {
        return None;
    }
    match &state.palettes.get(&id)?.kind {
        PaletteKind::Color(hsv) => {
            let c: RGBColor = (*hsv).into();
            Some(Color32::from_rgb(c.r, c.g, c.b))
        }
        PaletteKind::Pointer { target, .. } => palette_colour(state, *target, depth + 1),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sel(g: u8, f: u8, p: FixtureProperty) -> FixtureSelector {
        FixtureSelector {
            gid: g,
            fid: f,
            property: p,
        }
    }

    #[test]
    fn backspace_undoes_this_sessions_change_then_removes_earlier_ones() {
        let p = FixtureProperty::Alpha;
        let mut base = SceneBase::default();
        // (0,0) had a change at 100 before this session; (0,1) had none.
        base.values.insert((0, 0, p), 100.0);
        base.values.insert((0, 1, p), 0.0);
        base.changes.insert(sel(0, 0, p));
        let mut now = base.values.clone();
        now.insert((0, 0, p), 200.0);
        now.insert((0, 1, p), 50.0);
        let mut changes = base.changes.clone();
        changes.insert(sel(0, 1, p));
        let undo = fader_undo(&base, &now, &changes, &[(0, 0), (0, 1)], p);
        assert_eq!(
            undo,
            FaderUndo::Session {
                restore: vec![((0, 0), 100.0)],
                drop: vec![sel(0, 1, p)],
            }
        );

        // Back at the session's start: the earlier change goes.
        let undo = fader_undo(&base, &base.values, &base.changes, &[(0, 0), (0, 1)], p);
        assert_eq!(undo, FaderUndo::Earlier(vec![sel(0, 0, p)]));

        // Nothing left.
        let undo = fader_undo(&base, &base.values, &HashSet::new(), &[(0, 0)], p);
        assert_eq!(undo, FaderUndo::Nothing);
    }

    #[test]
    fn undo_events_restore_per_fixture_and_drop_new_changes() {
        let p = FixtureProperty::Pan;
        let undo = FaderUndo::Session {
            restore: vec![((1, 2), 40.0), ((3, 4), 40.0)],
            drop: vec![sel(1, 5, p)],
        };
        let events = fader_undo_events(&undo, p);
        // One push/pop block for value 40, then the removal.
        assert!(matches!(events.first(), Some(ControlEvent::PushSelection)));
        assert!(events.iter().any(|e| matches!(e, ControlEvent::SetPan(40))));
        assert!(matches!(events.last(), Some(ControlEvent::Transaction(_))));
        let sets = events
            .iter()
            .filter(|e| matches!(e, ControlEvent::SetPan(_)))
            .count();
        assert_eq!(sets, 2, "once per group");
    }

    #[test]
    fn set_events_use_the_fader_scale() {
        assert!(matches!(
            set_event(FixtureProperty::ColorHue, 300.0),
            ControlEvent::SetColorHue(300)
        ));
        assert!(matches!(
            set_event(FixtureProperty::Alpha, 300.0),
            ControlEvent::SetAlpha(255)
        ));
    }
}
