//! The desktop System › General page (ticket 084, `system.html#general`):
//! two columns, Show and Problems on the left, Engine, About and
//! Application on the right; one column below [`STACK_BELOW`] points.
//!
//! Desktop only ([`Density::Compact`]). The kiosk keeps
//! `render_system_general` in `misc.rs` unchanged.
//!
//! [`Density::Compact`]: crate::app::ui_tokens::Density::Compact

use super::super::setup::duo::{kv_row, type_label, KV_GAP};
use crate::app::components::studio;
use crate::app::ui_tokens::{
    studio as studio_tokens, Density, Studio, Weight, TARGET_MIN, TEXT, TEXT_MIN,
};
use crate::app::{BlaulichtApp, GuardedLifecycleAction, PopupSpec};
use blaulicht_engine::{
    audio::defs::AudioThreadControlSignal,
    client::EngineClient,
    hub::HostCommand,
    persistence::SaveStatus,
    state::{AppHealthState, MidiDeviceState, SerialDeviceState},
};
use blaulicht_shared::{AppSubPage, EngineGroups, EngineState};
use egui::{pos2, vec2, Align2, Color32, FontFamily, FontId, Rect, Sense, Stroke, Ui};
use std::{collections::BTreeMap, path::Path, time::Duration};

/// Below this page width the two columns stack (`system.css`, 1000px
/// window minus the sidebar).
const STACK_BELOW: f32 = 820.0;
/// Share of the page the left column takes.
const LEFT_SHARE: f32 = 0.6;
/// Height of a problem row (`.pbrow`).
const PROBLEM_ROW: f32 = 44.0;
/// Width of a problem's action button (`.pact`).
const ACTION_W: f32 = 132.0;
/// Height of a speed graph (`.graph`).
const GRAPH_H: f32 = 64.0;

/// How bad a problem is: red when output is at risk, amber when it needs
/// attention (`problems()` in the mockup).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Severity {
    Error,
    Warning,
}

/// What a problem's button does.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum ProblemAction {
    Open(AppSubPage),
    ReloadPlugins,
}

/// One row of the Problems pane.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct Problem {
    pub severity: Severity,
    pub text: String,
    pub sub: String,
    pub action_label: String,
    pub action: ProblemAction,
}

/// Two fixtures on the same DMX channels.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Overlap {
    pub universe: usize,
    /// First shared channel (1-based).
    pub channel: usize,
    pub a: String,
    pub b: String,
}

/// The patch at a glance: every fixture's channels per universe.
#[derive(Debug, Default, PartialEq)]
pub(crate) struct PatchSummary {
    pub fixtures: usize,
    pub universes: usize,
    /// Channels used by more than one fixture.
    pub overlapping_channels: usize,
    pub overlaps: Vec<Overlap>,
}

/// Fixtures that share channels, and the patch counts.
pub(crate) fn patch_summary(groups: &EngineGroups) -> PatchSummary {
    // (universe) -> [(start, end exclusive, name)]
    let mut by_universe: BTreeMap<usize, Vec<(usize, usize, String)>> = BTreeMap::new();
    let mut fixtures = 0;
    for group in groups.values() {
        for fixture in group.fixtures.values() {
            fixtures += 1;
            let start = fixture.start_addr;
            let end = start + fixture.type_.footprint().max(1);
            by_universe.entry(fixture.universe_no).or_default().push((
                start,
                end,
                fixture.name.clone(),
            ));
        }
    }
    let mut overlaps = Vec::new();
    let mut overlapping_channels = 0;
    for (universe, list) in &mut by_universe {
        list.sort_by_key(|(start, _, _)| *start);
        let mut used: BTreeMap<usize, usize> = BTreeMap::new();
        for (start, end, _) in list.iter() {
            for channel in *start..*end {
                *used.entry(channel).or_default() += 1;
            }
        }
        overlapping_channels += used.values().filter(|n| **n > 1).count();
        for (i, (start_a, end_a, name_a)) in list.iter().enumerate() {
            for (start_b, _, name_b) in &list[i + 1..] {
                if start_b >= end_a {
                    break;
                }
                overlaps.push(Overlap {
                    universe: *universe,
                    channel: (*start_b).max(*start_a) + 1,
                    a: name_a.clone(),
                    b: name_b.clone(),
                });
            }
        }
    }
    PatchSummary {
        fixtures,
        universes: by_universe.len(),
        overlapping_channels,
        overlaps,
    }
}

/// The Problems list, errors first, from the same health data as the
/// status bar's issues cell.
pub(crate) fn collect_problems(
    health: &AppHealthState,
    artnet_enabled: bool,
    crashed_plugins: &[String],
    overlaps: &[Overlap],
) -> Vec<Problem> {
    let mut problems = Vec::new();
    for (universe, port) in health.dmx_universes_healthy.iter().take(2).enumerate() {
        if port.is_healthy() {
            continue;
        }
        let name = match port.port.trim() {
            "" => format!("DMX output {universe}"),
            port => port.to_string(),
        };
        problems.push(Problem {
            severity: Severity::Error,
            text: format!("{name} is offline"),
            sub: format!("Universe {universe} isn't reaching this output"),
            action_label: format!("Open universe {universe}"),
            action: ProblemAction::Open(AppSubPage::System_Dmx),
        });
    }
    if artnet_enabled && !health.artnet_health_state {
        problems.push(Problem {
            severity: Severity::Error,
            text: "Art-Net is offline".to_string(),
            sub: "No Art-Net receiver is getting output".to_string(),
            action_label: "Open Art-Net".to_string(),
            action: ProblemAction::Open(AppSubPage::System_ArtNet),
        });
    }
    for name in crashed_plugins {
        problems.push(Problem {
            severity: Severity::Warning,
            text: format!("{name} crashed"),
            sub: "The plugin stopped and needs a reload".to_string(),
            action_label: "Reload plugin".to_string(),
            action: ProblemAction::ReloadPlugins,
        });
    }
    for overlap in overlaps {
        problems.push(Problem {
            severity: Severity::Warning,
            text: format!("Patch overlap at {}", overlap.channel),
            sub: format!(
                "{} and {} share a channel on universe {}",
                overlap.a, overlap.b, overlap.universe
            ),
            action_label: "Show overlap".to_string(),
            action: ProblemAction::Open(AppSubPage::System_Dmx),
        });
    }
    let mut midi: Vec<_> = health
        .midi_health
        .devices
        .iter()
        .filter(|(_, state)| matches!(state, MidiDeviceState::Error(_)))
        .map(|(name, _)| name.clone())
        .collect();
    midi.sort();
    for name in midi {
        problems.push(Problem {
            severity: Severity::Warning,
            text: format!("{name} disconnected"),
            sub: "MIDI device".to_string(),
            action_label: "Open MIDI".to_string(),
            action: ProblemAction::Open(AppSubPage::System_Midi),
        });
    }
    let mut serial: Vec<_> = health
        .serial_health
        .devices
        .iter()
        .filter(|(_, state)| matches!(state, SerialDeviceState::Error(_)))
        .map(|(name, _)| name.clone())
        .collect();
    serial.sort();
    for name in serial {
        problems.push(Problem {
            severity: Severity::Warning,
            text: format!("{name} disconnected"),
            sub: "Serial device".to_string(),
            action_label: "Open Serial".to_string(),
            action: ProblemAction::Open(AppSubPage::System_Serial),
        });
    }
    problems.sort_by_key(|p| p.severity == Severity::Warning);
    problems
}

/// What the Show pane lists, gathered before drawing.
struct ShowInfo {
    file: Option<String>,
    location: Option<String>,
    modified: Option<String>,
    size: Option<String>,
    room: String,
    contents: Vec<(&'static str, usize)>,
    types: Vec<(String, usize)>,
}

fn show_info(
    state: &blaulicht_engine::dmx::EngineState,
    path: Option<&Path>,
    remote: bool,
) -> ShowInfo {
    let shared: &EngineState = &state.0;
    let (file, location, modified, size) = match path {
        Some(path) => {
            let absolute = if remote {
                path.to_path_buf()
            } else {
                blaulicht_engine::config::absolute_showfile_path(path)
            };
            // A remote UI can't read the engine host's file.
            let meta = (!remote)
                .then(|| std::fs::metadata(&absolute).ok())
                .flatten();
            let modified = meta.as_ref().and_then(|m| m.modified().ok()).map(|t| {
                chrono::DateTime::<chrono::Local>::from(t)
                    .format("%Y-%m-%d %H:%M")
                    .to_string()
            });
            let size = meta.as_ref().map(|m| format_size(m.len()));
            (
                path.file_name().map(|n| n.to_string_lossy().into_owned()),
                Some(absolute.to_string_lossy().into_owned()),
                modified,
                size,
            )
        }
        None => (None, None, None, None),
    };
    let stage = state.stage();
    let room = format!(
        "{} x {} x {} m, {} stage objects",
        trim_float(stage.room.width),
        trim_float(stage.room.depth),
        trim_float(stage.room.height),
        stage.objects.len()
    );
    let fixtures: usize = shared.groups.values().map(|g| g.fixtures.len()).sum();
    // Scene 0 is the empty scene and `u8::MAX` the blank base scene.
    let scenes = shared
        .scenes
        .keys()
        .filter(|id| **id != 0 && **id != u8::MAX)
        .count();
    let contents = vec![
        ("Scenes", scenes),
        ("Groups", shared.groups.len()),
        ("Fixtures", fixtures),
        ("Animations", shared.animation_templates.len()),
        ("Palettes", shared.palettes.len()),
        ("Views", shared.views.len()),
        ("Folders", shared.folders.len()),
    ];
    let mut types: BTreeMap<String, usize> = BTreeMap::new();
    for group in shared.groups.values() {
        for fixture in group.fixtures.values() {
            *types
                .entry(type_label(&fixture.type_.model_string()))
                .or_default() += 1;
        }
    }
    let mut types: Vec<_> = types.into_iter().collect();
    types.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(&b.0)));
    ShowInfo {
        file,
        location,
        modified,
        size,
        room,
        contents,
        types,
    }
}

fn trim_float(v: f32) -> String {
    if (v - v.round()).abs() < 0.05 {
        format!("{:.0}", v)
    } else {
        format!("{:.1}", v)
    }
}

fn format_size(bytes: u64) -> String {
    if bytes >= 1024 * 1024 {
        format!("{:.1} MB", bytes as f64 / (1024.0 * 1024.0))
    } else if bytes >= 1024 {
        format!("{} KB", bytes / 1024)
    } else {
        format!("{bytes} B")
    }
}

fn font(ui: &Ui, weight: Weight, size: f32) -> FontId {
    Density::of(ui.ctx()).font(weight, size)
}

fn mono(size: f32) -> FontId {
    FontId::new(size, FontFamily::Monospace)
}

/// Paints a label at `pos` (left centre).
fn label(ui: &Ui, pos: egui::Pos2, text: &str, f: FontId, color: Color32) -> Rect {
    ui.painter().text(pos, Align2::LEFT_CENTER, text, f, color)
}

/// A kv value in the text colour (mono for paths and numbers).
fn value(ui: &mut Ui, text: &str, mono_text: bool) {
    let t = studio_tokens(ui.ctx());
    let f = if mono_text {
        mono(TEXT)
    } else {
        font(ui, Weight::Regular, TEXT)
    };
    let color = if mono_text { t.strong } else { t.text };
    let (r, resp) = ui.allocate_exact_size(vec2(ui.available_width(), 16.0), Sense::hover());
    let cut = studio::paint_truncated(ui, pos2(r.left(), r.center().y), text, f, color, r.width());
    if cut {
        resp.on_hover_text(text);
    }
}

/// The severity mark: a red square or an amber triangle (`.d.bad`, `.d.wrn`).
fn paint_severity(ui: &Ui, t: &Studio, center: egui::Pos2, severity: Severity) {
    match severity {
        Severity::Error => {
            ui.painter()
                .rect_filled(Rect::from_center_size(center, vec2(8.0, 8.0)), 0.0, t.red);
        }
        Severity::Warning => {
            let points = vec![
                pos2(center.x, center.y - 5.0),
                pos2(center.x + 5.5, center.y + 4.5),
                pos2(center.x - 5.5, center.y + 4.5),
            ];
            ui.painter()
                .add(egui::Shape::convex_polygon(points, t.warn, Stroke::NONE));
        }
    }
}

/// A 24px studio button; disabled ones are dimmed and never clicked.
fn chip(ui: &mut Ui, label: &str, pressed: bool, enabled: bool, tip: Option<&str>) -> bool {
    let inner = ui.add_enabled_ui(enabled, |ui| {
        if !enabled {
            ui.set_opacity(0.4);
        }
        studio::mini_button(ui, label, pressed, TARGET_MIN)
    });
    let response = inner.inner;
    let response = match (enabled, tip) {
        (false, Some(tip)) => response.on_disabled_hover_text(tip),
        _ => response,
    };
    enabled && response.clicked()
}

/// The danger button (`.mini.danger`): red text on the control fill.
fn danger_button(ui: &mut Ui, label: &str) -> bool {
    let t = studio_tokens(ui.ctx());
    let f = font(ui, Weight::Regular, TEXT);
    let w = studio::text_width(ui, label, f.clone()) + 16.0;
    let (rect, response) = ui.allocate_exact_size(vec2(w, TARGET_MIN), Sense::click());
    let fill = if response.hovered() { t.ctl_hi } else { t.ctl };
    ui.painter().rect_filled(rect, 0.0, fill);
    ui.painter()
        .text(rect.center(), Align2::CENTER_CENTER, label, f, t.red_text);
    response.widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::Button, true, label));
    response.clicked()
}

/// A pane: `pane` fill under its 26px title bar and body. `fill` stretches
/// it to `min_height`.
fn pane(
    ui: &mut Ui,
    title: &str,
    min_height: f32,
    actions: impl FnOnce(&mut Ui),
    body: impl FnOnce(&mut Ui),
) {
    let t = studio_tokens(ui.ctx());
    let top = ui.cursor().min;
    let width = ui.available_width();
    let bg = ui.painter().add(egui::Shape::Noop);
    let mut child = ui.new_child(
        egui::UiBuilder::new()
            .max_rect(Rect::from_min_size(top, vec2(width, f32::INFINITY)))
            .layout(egui::Layout::top_down(egui::Align::Min)),
    );
    child.spacing_mut().item_spacing = vec2(0.0, 0.0);
    studio::pane_header(&mut child, None, title, false, actions);
    body(&mut child);
    let height = (child.min_rect().height()).max(min_height);
    let rect = Rect::from_min_size(top, vec2(width, height));
    ui.painter()
        .set(bg, egui::Shape::rect_filled(rect, 0.0, t.pane));
    ui.allocate_rect(rect, Sense::hover());
}

/// A padded pane body (`.pad`: 8px).
fn pad(ui: &mut Ui, add: impl FnOnce(&mut Ui)) {
    egui::Frame::new()
        .inner_margin(egui::Margin::same(8))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.spacing_mut().item_spacing = vec2(6.0, KV_GAP);
            // Text rows are 16px, not the 20px `interact_size` minimum.
            ui.spacing_mut().interact_size.y = 16.0;
            add(ui);
        });
}

impl BlaulichtApp {
    /// System › General on the desktop.
    pub(super) fn render_system_general_desktop(&mut self, ui: &mut Ui) {
        let t = studio_tokens(ui.ctx());
        let full = ui.max_rect().expand(8.0).intersect(ui.clip_rect());
        ui.allocate_rect(ui.max_rect(), Sense::hover());
        ui.painter().rect_filled(full, 0.0, t.gutter);
        let mut page = ui.new_child(
            egui::UiBuilder::new()
                .max_rect(full)
                .layout(egui::Layout::top_down(egui::Align::Min)),
        );
        page.set_clip_rect(full);
        page.spacing_mut().item_spacing = vec2(0.0, 0.0);

        if full.width() < STACK_BELOW {
            egui::ScrollArea::vertical()
                .id_salt("system-general")
                .auto_shrink([false, false])
                .show(&mut page, |ui| {
                    ui.spacing_mut().item_spacing = vec2(0.0, 0.0);
                    self.general_left(ui, 0.0);
                    ui.add_space(1.0);
                    self.general_right(ui, 0.0);
                });
            return;
        }

        let left_w = (full.width() * LEFT_SHARE).round();
        let left_rect = Rect::from_min_size(full.min, vec2(left_w, full.height()));
        let right_rect = Rect::from_min_max(pos2(left_rect.right() + 1.0, full.top()), full.max);
        for (rect, left) in [(left_rect, true), (right_rect, false)] {
            let mut col = page.new_child(
                egui::UiBuilder::new()
                    .id_salt(("general-col", left))
                    .max_rect(rect)
                    .layout(egui::Layout::top_down(egui::Align::Min)),
            );
            col.set_clip_rect(rect);
            col.spacing_mut().item_spacing = vec2(0.0, 0.0);
            if left {
                self.general_left(&mut col, rect.bottom());
            } else {
                self.general_right(&mut col, rect.bottom());
            }
        }
    }

    /// Show, then Problems down to `bottom` (0: natural height).
    fn general_left(&mut self, ui: &mut Ui, bottom: f32) {
        let t = studio_tokens(ui.ctx());
        let remote = self.client.is_remote();
        let path = self.client.showfile_path();
        let info = self
            .client
            .with_show(|state| show_info(state, path.as_deref(), remote));
        let showfile_open = path.is_some();

        let (mut load, mut save_as, mut save, mut close) = (false, false, false, false);
        pane(
            ui,
            "Show",
            0.0,
            |ui| {
                ui.spacing_mut().item_spacing.x = 2.0;
                let saving = matches!(self.save_status, SaveStatus::Saving);
                // The file dialogs browse this machine: no use to a remote UI.
                if cfg!(feature = "file-dialog") && !remote {
                    load = chip(ui, "Load", false, true, None);
                    save_as = chip(ui, "Save as", false, true, None);
                }
                save = chip(
                    ui,
                    "Save",
                    true,
                    showfile_open && !saving,
                    Some("Open or create a showfile before saving"),
                );
                if !remote {
                    // The divider (`.hsep`).
                    let (r, _) = ui.allocate_exact_size(vec2(9.0, TARGET_MIN), Sense::hover());
                    ui.painter().vline(
                        r.center().x,
                        r.shrink2(vec2(0.0, 4.0)).y_range(),
                        Stroke::new(1.0_f32, t.line),
                    );
                    close = chip(
                        ui,
                        "Close",
                        false,
                        showfile_open,
                        Some("No showfile is currently open"),
                    );
                }
            },
            |ui| {
                pad(ui, |ui| {
                    kv_row(ui, "File", |ui| {
                        value(ui, info.file.as_deref().unwrap_or("No showfile"), false)
                    });
                    kv_row(ui, "State", |ui| self.save_state_value(ui));
                    if let Some(location) = &info.location {
                        kv_row(ui, "Location", |ui| value(ui, location, true));
                    }
                    if let Some(modified) = &info.modified {
                        kv_row(ui, "Modified", |ui| value(ui, modified, false));
                    }
                    if let Some(size) = &info.size {
                        kv_row(ui, "Size", |ui| value(ui, size, false));
                    }
                    kv_row(ui, "Room", |ui| value(ui, &info.room, false));
                    if let Some(line) = self.remote_status_line() {
                        kv_row(ui, "Remote", |ui| value(ui, &line, true));
                    }
                    ui.add_space(12.0);
                    let w = ((ui.available_width() - 24.0) / 2.0).max(0.0);
                    ui.horizontal_top(|ui| {
                        ui.spacing_mut().item_spacing.x = 24.0;
                        let contents: Vec<(String, usize)> = info
                            .contents
                            .iter()
                            .map(|(k, v)| (k.to_string(), *v))
                            .collect();
                        count_column(ui, w, "Contents", &contents);
                        count_column(ui, w, "Fixture types", &info.types);
                    });
                });
            },
        );
        if load {
            #[cfg(feature = "file-dialog")]
            self.open_load_showfile_dialog();
        }
        if save_as {
            #[cfg(feature = "file-dialog")]
            self.open_save_showfile_dialog();
        }
        if save {
            self.save_showfile();
        }
        if close {
            self.system_ui_state.close_showfile_confirm_open = true;
        }

        ui.add_space(1.0);

        let patch = self
            .client
            .with_show(|state| patch_summary(&state.0.groups));
        let artnet_enabled = self
            .client
            .with_artnet_receivers(|receivers| receivers.iter().any(|r| r.enabled));
        let crashed: Vec<String> = self.client.with_plugins(|plugins| {
            let mut list: Vec<_> = plugins
                .iter()
                .filter(|(_, p)| p.is_enabled() && p.has_errored())
                .map(|(id, p)| {
                    (
                        *id,
                        Path::new(p.path.as_ref())
                            .file_stem()
                            .map_or_else(|| p.path.to_string(), |s| s.to_string_lossy().into()),
                    )
                })
                .collect();
            list.sort();
            list.into_iter().map(|(_, name)| name).collect()
        });
        let problems = self.client.with_health(|health| {
            collect_problems(health, artnet_enabled, &crashed, &patch.overlaps)
        });

        let min_height = if bottom > 0.0 {
            bottom - ui.cursor().top()
        } else {
            0.0
        };
        let count = problems.len();
        let mut action = None;
        pane(
            ui,
            "Problems",
            min_height,
            |ui| {
                if count > 0 {
                    let (r, _) = ui.allocate_exact_size(vec2(24.0, TARGET_MIN), Sense::hover());
                    ui.painter().text(
                        pos2(r.right(), r.center().y),
                        Align2::RIGHT_CENTER,
                        count.to_string(),
                        mono(TEXT),
                        t.strong,
                    );
                }
            },
            |ui| {
                if problems.is_empty() {
                    let (r, _) =
                        ui.allocate_exact_size(vec2(ui.available_width(), 52.0), Sense::hover());
                    ui.painter().text(
                        pos2(r.left() + 10.0, r.center().y),
                        Align2::LEFT_CENTER,
                        egui_phosphor::regular::CHECK_CIRCLE,
                        FontId::proportional(20.0),
                        t.ok,
                    );
                    label(
                        ui,
                        pos2(r.left() + 38.0, r.center().y - 8.0),
                        "No problems",
                        font(ui, Weight::SemiBold, TEXT + 1.0),
                        t.strong,
                    );
                    label(
                        ui,
                        pos2(r.left() + 38.0, r.center().y + 8.0),
                        "Outputs, plugins and devices are all healthy",
                        font(ui, Weight::Regular, TEXT_MIN),
                        t.muted,
                    );
                }
                for (i, problem) in problems.iter().enumerate() {
                    let (r, _) = ui.allocate_exact_size(
                        vec2(ui.available_width(), PROBLEM_ROW),
                        Sense::hover(),
                    );
                    ui.painter()
                        .hline(r.x_range(), r.bottom() - 0.5, Stroke::new(1.0_f32, t.line));
                    paint_severity(
                        ui,
                        t,
                        pos2(r.left() + 12.0, r.center().y - 6.0),
                        problem.severity,
                    );
                    let text_w = r.width() - 27.0 - ACTION_W - 16.0;
                    studio::paint_truncated(
                        ui,
                        pos2(r.left() + 24.0, r.center().y - 7.0),
                        &problem.text,
                        font(ui, Weight::SemiBold, TEXT + 1.0),
                        t.strong,
                        text_w,
                    );
                    studio::paint_truncated(
                        ui,
                        pos2(r.left() + 24.0, r.center().y + 8.0),
                        &problem.sub,
                        font(ui, Weight::Regular, TEXT_MIN),
                        t.muted,
                        text_w,
                    );
                    let button = Rect::from_min_size(
                        pos2(r.right() - 8.0 - ACTION_W, r.center().y - TARGET_MIN / 2.0),
                        vec2(ACTION_W, TARGET_MIN),
                    );
                    let mut b = ui.new_child(egui::UiBuilder::new().max_rect(button).id_salt(i));
                    if studio::mini_button_w(&mut b, &problem.action_label, ACTION_W, TARGET_MIN)
                        .clicked()
                    {
                        action = Some(problem.action);
                    }
                }
                // The patch line (`.gfoot`).
                let (r, _) =
                    ui.allocate_exact_size(vec2(ui.available_width(), 36.0), Sense::hover());
                ui.painter().rect_filled(r, 0.0, t.band);
                let overlaps = match patch.overlapping_channels {
                    0 => "no overlaps".to_string(),
                    1 => "1 overlapping channel".to_string(),
                    n => format!("{n} overlapping channels"),
                };
                let summary = format!(
                    "Patch: {} fixtures on {} {}, {overlaps}",
                    patch.fixtures,
                    patch.universes,
                    if patch.universes == 1 {
                        "universe"
                    } else {
                        "universes"
                    },
                );
                let button_label = "Open patch map";
                let bw =
                    studio::text_width(ui, button_label, font(ui, Weight::Regular, TEXT)) + 16.0;
                studio::paint_truncated(
                    ui,
                    pos2(r.left() + 8.0, r.center().y),
                    &summary,
                    font(ui, Weight::Regular, TEXT),
                    t.muted,
                    r.width() - bw - 24.0,
                );
                let button = Rect::from_min_size(
                    pos2(r.right() - 8.0 - bw, r.center().y - TARGET_MIN / 2.0),
                    vec2(bw, TARGET_MIN),
                );
                let mut b = ui.new_child(egui::UiBuilder::new().max_rect(button).id_salt("patch"));
                if studio::mini_button(&mut b, button_label, false, TARGET_MIN).clicked() {
                    action = Some(ProblemAction::Open(AppSubPage::System_Dmx));
                }
            },
        );
        match action {
            Some(ProblemAction::Open(subpage)) => self.open_subpage(subpage),
            Some(ProblemAction::ReloadPlugins) => self.reload_plugins(),
            None => {}
        }
    }

    fn reload_plugins(&mut self) {
        self.send_command(HostCommand::ReloadPlugins, |app, _| {
            app.show_popup(PopupSpec::with_duration(
                Duration::from_secs(2),
                "Reload in progress...".to_string(),
            ));
        });
    }

    /// The save state with its square (`.state`), and why a save failed.
    fn save_state_value(&self, ui: &mut Ui) {
        let t = studio_tokens(ui.ctx());
        let (text, color) = match &self.save_status {
            SaveStatus::NoShowfile => ("Not saved to a file".to_string(), t.muted),
            SaveStatus::Clean => ("Saved".to_string(), t.ok),
            SaveStatus::Dirty => ("Unsaved changes".to_string(), t.changed),
            SaveStatus::Saving => ("Saving…".to_string(), t.changed),
            SaveStatus::Failed(error) => (format!("Save failed: {error}"), t.red),
        };
        let (r, _) = ui.allocate_exact_size(vec2(14.0, 16.0), Sense::hover());
        ui.painter().rect_filled(
            Rect::from_center_size(pos2(r.left() + 4.0, r.center().y), vec2(8.0, 8.0)),
            0.0,
            color,
        );
        value(ui, &text, false);
    }

    /// Engine, About, then Application down to `bottom`.
    fn general_right(&mut self, ui: &mut Ui, bottom: f32) {
        let t = studio_tokens(ui.ctx());
        let remote = self.client.is_remote();
        let signal = self.client.mainloop_state();
        let reload_enabled = matches!(signal, AudioThreadControlSignal::CONTINUE);
        let (mut debug, mut reload) = (false, false);
        let debug_open = self.system_ui_state.debug_open;
        let engine: Vec<f32> = self.status_sparks.engine().iter().copied().collect();
        let plugins: Vec<f32> = self.status_sparks.plugins().iter().copied().collect();
        pane(
            ui,
            "Engine",
            0.0,
            |ui| {
                ui.spacing_mut().item_spacing.x = 2.0;
                debug = chip(ui, "Debug", debug_open, true, None);
                reload = chip(
                    ui,
                    "Reload",
                    false,
                    reload_enabled,
                    Some("The engine is already reloading or unavailable"),
                );
            },
            |ui| {
                pad(ui, |ui| {
                    ui.spacing_mut().item_spacing.y = 0.0;
                    speed_graph(ui, t, "Loop", &engine);
                    ui.add_space(12.0);
                    speed_graph(ui, t, "Plugins", &plugins);
                });
            },
        );
        if debug {
            self.system_ui_state.debug_open = !debug_open;
        }
        if reload {
            self.reload_plugins();
        }

        ui.add_space(1.0);

        let uptime = self.system_ui_state.started.elapsed().as_secs();
        let uptime = format!(
            "{}:{:02}:{:02}",
            uptime / 3600,
            (uptime / 60) % 60,
            uptime % 60
        );
        let port = self.client.with_remote_server(|server| {
            server
                .listening
                .map_or_else(|| "Off".to_string(), |addr| addr.port().to_string())
        });
        pane(
            ui,
            "About",
            0.0,
            |_| {},
            |ui| {
                pad(ui, |ui| {
                    kv_row(ui, "Uptime", |ui| value(ui, &uptime, true));
                    kv_row(ui, "Version", |ui| {
                        value(
                            ui,
                            &format!(
                                "{}, plugin ABI {}",
                                env!("CARGO_PKG_VERSION"),
                                blaulicht_shared::PLUGIN_ABI_VERSION
                            ),
                            false,
                        )
                    });
                    kv_row(ui, "Network port", |ui| value(ui, &port, true));
                });
            },
        );
        // The clock ticks every second.
        ui.ctx().request_repaint_after(Duration::from_secs(1));

        ui.add_space(1.0);

        let min_height = if bottom > 0.0 {
            bottom - ui.cursor().top()
        } else {
            0.0
        };
        let dark = ui.ctx().global_style().visuals.dark_mode;
        let (mut theme, mut restart, mut quit, mut shutdown) = (None, false, false, false);
        pane(
            ui,
            "Application",
            min_height,
            |_| {},
            |ui| {
                pad(ui, |ui| {
                    kv_row(ui, "Theme", |ui| {
                        ui.spacing_mut().item_spacing.x = 2.0;
                        if chip(ui, "Light", !dark, true, None) {
                            theme = Some(egui::ThemePreference::Light);
                        }
                        if chip(ui, "Dark", dark, true, None) {
                            theme = Some(egui::ThemePreference::Dark);
                        }
                    });
                    // Quitting, restarting and shutting down are the desk's.
                    if remote {
                        return;
                    }
                    ui.add_space(4.0);
                    kv_row(ui, "Program", |ui| {
                        ui.spacing_mut().item_spacing.x = 2.0;
                        restart = chip(ui, "Restart", false, true, None);
                        quit = chip(ui, "Quit", false, true, None);
                    });
                    ui.add_space(12.0);
                    let (line, _) =
                        ui.allocate_exact_size(vec2(ui.available_width(), 1.0), Sense::hover());
                    ui.painter().hline(
                        line.x_range(),
                        line.center().y,
                        Stroke::new(1.0_f32, t.line),
                    );
                    ui.add_space(8.0);
                    let (r, _) =
                        ui.allocate_exact_size(vec2(ui.available_width(), 36.0), Sense::hover());
                    label(
                        ui,
                        pos2(r.left(), r.center().y - 8.0),
                        "Shut down this device",
                        font(ui, Weight::SemiBold, TEXT + 1.0),
                        t.strong,
                    );
                    label(
                        ui,
                        pos2(r.left(), r.center().y + 9.0),
                        "Powers off the computer running the engine. Asks first.",
                        font(ui, Weight::Regular, TEXT_MIN),
                        t.muted,
                    );
                    let bw =
                        studio::text_width(ui, "Shut down", font(ui, Weight::Regular, TEXT)) + 16.0;
                    let button = Rect::from_min_size(
                        pos2(r.right() - bw, r.center().y - TARGET_MIN / 2.0),
                        vec2(bw, TARGET_MIN),
                    );
                    let mut b =
                        ui.new_child(egui::UiBuilder::new().max_rect(button).id_salt("shutdown"));
                    shutdown = danger_button(&mut b, "Shut down");
                });
            },
        );
        if let Some(theme) = theme {
            ui.ctx().set_theme(theme);
        }
        if restart {
            self.request_guarded_lifecycle(GuardedLifecycleAction::Restart);
        }
        if quit {
            self.request_guarded_lifecycle(GuardedLifecycleAction::Quit);
        }
        if shutdown {
            self.system_ui_state.confirm_shutdown_open = true;
        }
    }
}

/// A subhead and name/count rows (`.cnt`), `width` wide.
fn count_column(ui: &mut Ui, width: f32, title: &str, rows: &[(String, usize)]) {
    let t = studio_tokens(ui.ctx());
    ui.vertical(|ui| {
        ui.set_width(width);
        ui.spacing_mut().item_spacing.y = KV_GAP;
        let (r, _) = ui.allocate_exact_size(vec2(width, 22.0), Sense::hover());
        ui.painter()
            .hline(r.x_range(), r.top() + 0.5, Stroke::new(1.0_f32, t.line));
        label(
            ui,
            pos2(r.left(), r.bottom() - 6.0),
            title,
            font(ui, Weight::Medium, TEXT),
            t.text,
        );
        if rows.is_empty() {
            let (r, _) = ui.allocate_exact_size(vec2(width, 16.0), Sense::hover());
            label(
                ui,
                pos2(r.left(), r.center().y),
                "None",
                font(ui, Weight::Regular, TEXT),
                t.muted,
            );
        }
        for (name, count) in rows {
            let (r, resp) = ui.allocate_exact_size(vec2(width, 16.0), Sense::hover());
            let text = count.to_string();
            let cut = studio::paint_truncated(
                ui,
                pos2(r.left(), r.center().y),
                name,
                font(ui, Weight::Regular, TEXT),
                t.text,
                r.width() - 40.0,
            );
            if cut {
                resp.on_hover_text(name);
            }
            ui.painter().text(
                pos2(r.right(), r.center().y),
                Align2::RIGHT_CENTER,
                text,
                mono(TEXT),
                t.strong,
            );
        }
    });
}

/// A speed graph (`.graph`): the name and latest value over a well with the
/// recent samples as a line, scaled to their peak.
fn speed_graph(ui: &mut Ui, t: &Studio, name: &str, samples: &[f32]) {
    let w = ui.available_width();
    let (head, _) = ui.allocate_exact_size(vec2(w, 18.0), Sense::hover());
    label(
        ui,
        pos2(head.left(), head.center().y),
        name,
        font(ui, Weight::Regular, TEXT),
        t.muted,
    );
    let latest = samples.last().copied();
    ui.painter().text(
        pos2(head.right(), head.center().y),
        Align2::RIGHT_CENTER,
        latest.map_or_else(|| "--".to_string(), |v| format!("{v:.1} ms")),
        mono(TEXT),
        t.strong,
    );
    let (r, _) = ui.allocate_exact_size(vec2(w, GRAPH_H), Sense::hover());
    ui.painter().rect_filled(r, 0.0, t.well);
    if samples.len() < 2 {
        return;
    }
    let peak = samples.iter().copied().fold(0.0_f32, f32::max).max(1.0) * 1.25;
    let inner = r.shrink2(vec2(0.0, 4.0));
    let step = inner.width() / (samples.len() - 1) as f32;
    let points: Vec<_> = samples
        .iter()
        .enumerate()
        .map(|(i, v)| {
            pos2(
                inner.left() + i as f32 * step,
                inner.bottom() - (v / peak).clamp(0.0, 1.0) * inner.height(),
            )
        })
        .collect();
    ui.painter()
        .add(egui::Shape::line(points, Stroke::new(1.0_f32, t.ok)));
}

#[cfg(test)]
mod tests {
    use super::*;
    use blaulicht_engine::state::DmxHealth;
    use blaulicht_shared::fixture::state::{Fixture, FixtureGroup};

    #[test]
    fn overlapping_fixtures_are_found_once_per_pair() {
        let mut groups = EngineGroups::new();
        let dimmer = || {
            blaulicht_shared::fixture::FixtureType::Dimmer(
                blaulicht_shared::fixture::dimmer::Dimmer::DimmerSingle,
            )
        };
        let mut fixtures = BTreeMap::new();
        fixtures.insert(0, Fixture::new(0, 0, "A".into(), dimmer()));
        fixtures.insert(1, Fixture::new(0, 0, "B".into(), dimmer()));
        fixtures.insert(2, Fixture::new(1, 10, "C".into(), dimmer()));
        groups.insert(
            0,
            FixtureGroup {
                name: "G".into(),
                fixtures,
                folder: None,
            },
        );
        let s = patch_summary(&groups);
        assert_eq!(s.fixtures, 3);
        assert_eq!(s.universes, 2);
        assert_eq!(s.overlaps.len(), 1);
        assert_eq!(s.overlaps[0].a, "A");
        assert!(s.overlapping_channels >= 1);
    }

    #[test]
    fn errors_come_before_warnings() {
        let mut health = AppHealthState::default();
        health.dmx_universes_healthy[0] = DmxHealth::error("/dev/x".into(), "gone".into());
        let p = collect_problems(&health, false, &["setlist".to_string()], &[]);
        assert_eq!(p[0].severity, Severity::Error);
        assert_eq!(p[0].text, "/dev/x is offline");
        assert_eq!(p.last().unwrap().text, "setlist crashed");
    }
}
