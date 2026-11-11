//! The desktop layout's status bar (059): one strip along the bottom of the
//! window, identical on every page. Cells in this order: Blackout (only while
//! Grand Master is 0), showfile and save state, the engine connection when it
//! is not healthy, outputs, audio, BPM, engine load, plugin load, then on the
//! right the LIVE preview, problems and Remote UI. A healthy cell stays
//! quiet; a problem colours its cell, and clicking it opens the page that
//! fixes it. Desktop only: the kiosk never draws this.

use std::collections::VecDeque;
use std::time::Duration;

use blaulicht_engine::persistence::SaveStatus;
use blaulicht_shared::AppSubPage;
use egui::{pos2, vec2, Align2, Color32, CursorIcon, FontId, Rect, Sense, Stroke, Ui};
use egui_phosphor::regular as icons;
use web_time::Instant;

use crate::app::ui_tokens::{self, Density, Studio, Weight};

/// Height of the status bar (the mockup's 26px row).
pub const STATUS_BAR_HEIGHT: f32 = 26.0;

/// The Blackout cell's red (`.cell.black`), the same in both themes.
const BLACKOUT_RED: Color32 = Color32::from_rgb(0xb8, 0x0f, 0x0f);
const BLACKOUT_RED_HOVER: Color32 = Color32::from_rgb(0xd4, 0x0f, 0x0f);

/// Samples kept per sparkline, one every [`SPARK_INTERVAL`].
const SPARK_SAMPLES: usize = 30;
const SPARK_INTERVAL: Duration = Duration::from_millis(200);

/// Recent engine loop and plugin tick times for the status bar's
/// sparklines. Stops taking samples while the engine is offline.
#[derive(Default)]
pub struct StatusSparks {
    engine: VecDeque<f32>,
    plugins: VecDeque<f32>,
    last: Option<Instant>,
}

impl StatusSparks {
    /// Adds a sample when the last one is old enough.
    pub fn sample(&mut self, engine_ms: f32, plugins_ms: f32) {
        if self.last.is_some_and(|at| at.elapsed() < SPARK_INTERVAL) {
            return;
        }
        self.last = Some(Instant::now());
        for (series, value) in [
            (&mut self.engine, engine_ms),
            (&mut self.plugins, plugins_ms),
        ] {
            if series.len() >= SPARK_SAMPLES {
                series.pop_front();
            }
            series.push_back(value);
        }
    }
}

/// Where the engine connection stands, for the status bar.
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum EngineLink {
    Online,
    Connecting,
    Disconnected,
}

impl EngineLink {
    pub fn is_online(self) -> bool {
        self == EngineLink::Online
    }

    /// "disconnected" / "connecting", for "… not sent: engine …".
    pub fn word(self) -> &'static str {
        match self {
            EngineLink::Online => "online",
            EngineLink::Connecting => "connecting",
            EngineLink::Disconnected => "disconnected",
        }
    }
}

/// What the status bar shows, gathered once per frame from the client.
pub struct StatusBarInfo<'a> {
    /// `Some(level to restore)` while Grand Master is at 0.
    pub blackout: Option<u8>,
    /// The showfile's display name; `None` without a showfile.
    pub show_name: Option<String>,
    pub save_status: &'a SaveStatus,
    pub link: EngineLink,
    /// Health of each hardware DMX output, then Art-Net (`None`: not used).
    pub dmx_outputs: Vec<bool>,
    pub artnet: Option<bool>,
    /// Audio input level, 0..1.
    pub audio_level: f32,
    pub bpm: f32,
    pub sparks: &'a StatusSparks,
    /// Some session previews its scene (LIVE): overlays are suppressed.
    pub live: bool,
    pub errors: usize,
    pub warnings: usize,
    /// Connected Remote UI sessions.
    pub remote_sessions: usize,
}

/// What a click on the status bar asks for.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum StatusBarAction {
    ToggleBlackout,
    Open(AppSubPage),
}

/// Engine loop time shown as the sparkline's full height (ms).
const ENGINE_SPARK_MAX: f32 = 20.0;
const PLUGINS_SPARK_MAX: f32 = 10.0;

/// Draws the status bar as a bottom panel of `ui`.
pub fn status_bar(ui: &mut Ui, info: &StatusBarInfo<'_>) -> Option<StatusBarAction> {
    let t = ui_tokens::studio(ui.ctx());
    let mut action = None;
    egui::Panel::bottom("desktop_status_bar")
        .resizable(false)
        .exact_size(STATUS_BAR_HEIGHT)
        .show_separator_line(false)
        .frame(egui::Frame::NONE.fill(t.head))
        .show_inside(ui, |ui| {
            let full = ui.max_rect();
            // The gutter line above the bar.
            ui.painter().hline(
                full.x_range(),
                full.top() + 0.5,
                Stroke::new(1.0_f32, t.gutter),
            );
            let mut bar = Bar {
                ui,
                t,
                x: full.left(),
                right: full.right(),
                rect: full,
            };
            bar.left_cells(info, &mut action);
            bar.right_cells(info, &mut action);
        });
    action
}

struct Bar<'u> {
    ui: &'u mut Ui,
    t: &'static Studio,
    /// Where the next left cell starts / where the right cells end.
    x: f32,
    right: f32,
    rect: Rect,
}

/// One cell's contents: text runs and small shapes, laid out left to right.
enum Part {
    Text(String, FontId, Color32),
    Square(Color32),
    Meter(f32),
    Gauge(&'static str, Vec<f32>, f32),
    Space(f32),
}

impl Bar<'_> {
    fn font(&self, weight: Weight, size: f32) -> FontId {
        Density::of(self.ui.ctx()).font(weight, size)
    }

    fn icon(&self, icon: &str, color: Color32, size: f32) -> Part {
        Part::Text(icon.to_owned(), self.font(Weight::Regular, size), color)
    }

    fn label(&self, text: impl Into<String>, color: Color32) -> Part {
        Part::Text(text.into(), self.font(Weight::Regular, 12.0), color)
    }

    fn part_width(&self, part: &Part) -> f32 {
        match part {
            Part::Text(text, font, _) => {
                self.ui
                    .painter()
                    .layout_no_wrap(text.clone(), font.clone(), Color32::WHITE)
                    .size()
                    .x
            }
            Part::Square(_) => 7.0,
            Part::Meter(_) | Part::Gauge(..) => 50.0,
            Part::Space(w) => *w,
        }
    }

    /// Lays out one cell at the left (`from_right` false) or right edge.
    /// Returns its response (click/hover) when `sense` is not hover-only.
    #[allow(clippy::too_many_arguments)]
    fn cell(
        &mut self,
        id: &str,
        parts: Vec<Part>,
        from_right: bool,
        fill: Option<(Color32, Color32)>,
        clickable: bool,
        stale: bool,
        hover: Option<String>,
    ) -> bool {
        const PAD: f32 = 10.0;
        const GAP: f32 = 6.0;
        let gauge = parts
            .iter()
            .any(|p| matches!(p, Part::Gauge(..) | Part::Meter(_)));
        let pad = if gauge { 7.0 } else { PAD };
        let content: f32 = parts.iter().map(|p| self.part_width(p)).sum::<f32>()
            + GAP * parts.len().saturating_sub(1) as f32;
        let width = (content + 2.0 * pad).max(if gauge { 44.0 } else { 0.0 });
        let rect = if from_right {
            let r = Rect::from_min_max(
                pos2(self.right - width, self.rect.top()),
                pos2(self.right, self.rect.bottom()),
            );
            self.right -= width + 1.0;
            r
        } else {
            let r = Rect::from_min_max(
                pos2(self.x, self.rect.top()),
                pos2(self.x + width, self.rect.bottom()),
            );
            self.x += width + 1.0;
            r
        };
        let sense = if clickable {
            Sense::click()
        } else {
            Sense::hover()
        };
        let response = self
            .ui
            .interact(rect, self.ui.id().with(("status_cell", id)), sense);
        let mut painter = self.ui.painter().clone();
        if stale {
            painter.multiply_opacity(0.45);
        }
        if let Some((normal, hovered)) = fill {
            let fill = if response.hovered() { hovered } else { normal };
            painter.rect_filled(rect, 0.0, fill);
        } else if clickable && response.hovered() {
            painter.rect_filled(rect, 0.0, self.t.ctl_hi);
        }
        // The 1px gutter between cells.
        let edge = if from_right {
            rect.left() - 0.5
        } else {
            rect.right() + 0.5
        };
        if fill.is_none() {
            painter.vline(edge, rect.y_range(), Stroke::new(1.0_f32, self.t.gutter));
        }
        let cy = rect.center().y;
        if gauge {
            // Gauge cells stack the graphic over a small label.
            if let Some(Part::Gauge(name, data, max)) = parts.first() {
                let g = Rect::from_center_size(pos2(rect.center().x, cy - 5.5), vec2(50.0, 11.0));
                painter.rect_filled(g, 0.0, self.t.well);
                let color = spark_color(self.t, data, *max, stale);
                if data.len() > 1 {
                    let n = (data.len() - 1) as f32;
                    let points: Vec<_> = data
                        .iter()
                        .enumerate()
                        .map(|(i, v)| {
                            pos2(
                                g.left() + i as f32 * g.width() / n,
                                g.bottom() - 0.5 - (v / max).clamp(0.0, 1.0) * (g.height() - 1.0),
                            )
                        })
                        .collect();
                    painter.add(egui::Shape::line(points, Stroke::new(1.0_f32, color)));
                }
                painter.text(
                    pos2(rect.center().x, cy + 6.0),
                    Align2::CENTER_CENTER,
                    *name,
                    self.font(Weight::Regular, 11.0),
                    self.t.text,
                );
            }
            if let Some(Part::Meter(level)) = parts.first() {
                let g = Rect::from_center_size(pos2(rect.center().x, cy - 5.5), vec2(50.0, 8.0));
                painter.rect_filled(g, 0.0, self.t.well);
                let mut lit = g;
                lit.set_width(g.width() * level.clamp(0.0, 1.0));
                painter.rect_filled(lit, 0.0, self.t.ok);
                painter.text(
                    pos2(rect.center().x, cy + 6.0),
                    Align2::CENTER_CENTER,
                    "Audio",
                    self.font(Weight::Regular, 11.0),
                    self.t.text,
                );
            }
        } else {
            let mut x = rect.left() + pad;
            for part in &parts {
                let w = self.part_width(part);
                match part {
                    Part::Text(text, font, color) => {
                        painter.text(pos2(x, cy), Align2::LEFT_CENTER, text, font.clone(), *color);
                    }
                    Part::Square(color) => {
                        painter.rect_filled(
                            Rect::from_center_size(pos2(x + 3.5, cy), vec2(7.0, 7.0)),
                            0.0,
                            *color,
                        );
                    }
                    Part::Meter(_) | Part::Gauge(..) | Part::Space(_) => {}
                }
                x += w + GAP;
            }
        }
        if clickable {
            response.clone().on_hover_cursor(CursorIcon::PointingHand);
        }
        let clicked = response.clicked();
        if let Some(text) = hover {
            response.on_hover_text(text);
        }
        clicked
    }

    fn left_cells(&mut self, info: &StatusBarInfo<'_>, action: &mut Option<StatusBarAction>) {
        let t = self.t;
        let online = info.link.is_online();
        let stale = !online;

        if let Some(restore) = info.blackout {
            let label = if online {
                format!("BLACKOUT, press to restore {restore}%")
            } else {
                "BLACKOUT, last known".to_owned()
            };
            let parts = vec![
                Part::Text(
                    icons::PROHIBIT.to_owned(),
                    self.font(Weight::Regular, 14.0),
                    Color32::WHITE,
                ),
                Part::Text(label, self.font(Weight::SemiBold, 12.0), Color32::WHITE),
            ];
            if self.cell(
                "blackout",
                parts,
                false,
                Some((BLACKOUT_RED, BLACKOUT_RED_HOVER)),
                true,
                false,
                Some("Restore Grand Master (Cmd/Ctrl+B)".to_owned()),
            ) {
                *action = Some(StatusBarAction::ToggleBlackout);
            }
        }

        // Showfile and save state.
        let mut parts = vec![self.icon(icons::FILE, t.text, 14.0)];
        let mut hover = None;
        match &info.show_name {
            Some(name) => parts.push(Part::Text(
                name.clone(),
                self.font(Weight::Medium, 12.0),
                t.strong,
            )),
            None => parts.push(self.label("No showfile", t.muted)),
        }
        let save_problem = !matches!(info.save_status, SaveStatus::Clean | SaveStatus::NoShowfile);
        match info.save_status {
            SaveStatus::NoShowfile => {}
            SaveStatus::Clean => {
                parts.push(Part::Square(t.ok));
                parts.push(self.label("Saved", t.text));
            }
            SaveStatus::Dirty => parts.push(self.label("Unsaved changes", t.changed)),
            SaveStatus::Saving => parts.push(self.label("Saving…", t.changed)),
            SaveStatus::Failed(error) => {
                parts.push(self.icon(icons::WARNING_OCTAGON, t.red_text, 13.0));
                parts.push(self.label("Save failed", t.red_text));
                hover = Some(format!("Save failed: {error}"));
            }
        }
        if self.cell("show", parts, false, None, save_problem, false, hover) {
            *action = Some(StatusBarAction::Open(AppSubPage::System_System));
        }

        if !online {
            let parts = vec![
                Part::Square(t.red),
                self.label(format!("Engine {}", info.link.word()), t.red_text),
            ];
            self.cell(
                "engine_link",
                parts,
                false,
                None,
                false,
                false,
                Some("Values are the last known and may not match the lights.".to_owned()),
            );
        }

        // Outputs: a square per hardware DMX universe, then Art-Net.
        if !info.dmx_outputs.is_empty() || info.artnet.is_some() {
            let bad = info.dmx_outputs.iter().any(|ok| !ok) || info.artnet == Some(false);
            let mut parts =
                vec![self.label("Outputs", if bad && online { t.red_text } else { t.text })];
            let mut lines = Vec::new();
            for (i, ok) in info.dmx_outputs.iter().enumerate() {
                parts.push(Part::Square(if !online {
                    t.muted
                } else if *ok {
                    t.ok
                } else {
                    t.red
                }));
                lines.push(format!("DMX {i}: {}", if *ok { "OK" } else { "offline" }));
            }
            if let Some(ok) = info.artnet {
                parts.push(Part::Square(if !online {
                    t.muted
                } else if ok {
                    t.ok
                } else {
                    t.red
                }));
                lines.push(format!("Art-Net: {}", if ok { "OK" } else { "offline" }));
            }
            let target = if info.dmx_outputs.iter().any(|ok| !ok) || info.artnet.is_none() {
                AppSubPage::System_Dmx
            } else {
                AppSubPage::System_ArtNet
            };
            if self.cell(
                "outputs",
                parts,
                false,
                None,
                bad && online,
                stale,
                Some(lines.join("\n")),
            ) {
                *action = Some(StatusBarAction::Open(target));
            }
        }

        let level = if online { info.audio_level } else { 0.0 };
        self.cell(
            "audio",
            vec![Part::Meter(level)],
            false,
            None,
            false,
            stale,
            Some("Audio input level".to_owned()),
        );

        let bpm = if online {
            format!("{:.1}", info.bpm)
        } else {
            "---".to_owned()
        };
        let parts = vec![
            Part::Text(bpm, FontId::monospace(14.0), t.strong),
            self.label("BPM", t.text),
        ];
        self.cell("bpm", parts, false, None, false, stale, None);

        for (id, name, data, max) in [
            ("engine", "Engine", &info.sparks.engine, ENGINE_SPARK_MAX),
            (
                "plugins",
                "Plugins",
                &info.sparks.plugins,
                PLUGINS_SPARK_MAX,
            ),
        ] {
            let last = data.back().copied().unwrap_or_default();
            let hover = if online {
                format!("{name}: {last:.1} ms of {max:.0} ms")
            } else {
                "Not updating, engine offline".to_owned()
            };
            self.cell(
                id,
                vec![Part::Gauge(name, data.iter().copied().collect(), max)],
                false,
                None,
                false,
                stale,
                Some(hover),
            );
        }
    }

    fn right_cells(&mut self, info: &StatusBarInfo<'_>, action: &mut Option<StatusBarAction>) {
        let t = self.t;
        let online = info.link.is_online();

        let remote = if online {
            info.remote_sessions.to_string()
        } else {
            "--".to_owned()
        };
        let parts = vec![self.label(format!("Remote UI {remote}"), t.text)];
        if self.cell(
            "remote",
            parts,
            true,
            None,
            true,
            !online,
            Some("Connected Remote UI sessions".to_owned()),
        ) {
            *action = Some(StatusBarAction::Open(AppSubPage::System_Remote));
        }

        let total = info.errors + info.warnings;
        if total > 0 {
            let mut parts = Vec::new();
            if info.errors > 0 {
                parts.push(self.icon(icons::X_CIRCLE, t.red_text, 13.0));
            }
            if info.warnings > 0 {
                parts.push(self.icon(icons::WARNING, t.warn, 13.0));
            }
            parts.push(Part::Space(-2.0));
            parts.push(self.label(
                format!("{total} issue{}", if total == 1 { "" } else { "s" }),
                t.strong,
            ));
            let plural =
                |n: usize, word: &str| format!("{n} {word}{}", if n == 1 { "" } else { "s" });
            let split: Vec<_> = [
                (info.errors > 0).then(|| plural(info.errors, "error")),
                (info.warnings > 0).then(|| plural(info.warnings, "warning")),
            ]
            .into_iter()
            .flatten()
            .collect();
            if self.cell(
                "issues",
                parts,
                true,
                None,
                true,
                false,
                Some(format!("{}. Open the problem list.", split.join(", "))),
            ) {
                *action = Some(StatusBarAction::Open(AppSubPage::System_System));
            }
        }

        if info.live {
            let parts = vec![Part::Text(
                "LIVE".to_owned(),
                self.font(Weight::SemiBold, 11.0),
                Color32::WHITE,
            )];
            self.cell(
                "live",
                parts,
                true,
                Some((BLACKOUT_RED, BLACKOUT_RED)),
                false,
                false,
                Some("A scene is previewed live: overlays are suppressed.".to_owned()),
            );
        }
    }
}

/// Green, amber past 60% and red past 85% of `max`; muted while offline.
fn spark_color(t: &Studio, data: &[f32], max: f32, stale: bool) -> Color32 {
    if stale {
        return t.muted;
    }
    let v = data.last().copied().unwrap_or_default() / max;
    if v > 0.85 {
        t.red
    } else if v > 0.6 {
        t.warn
    } else {
        t.ok
    }
}
