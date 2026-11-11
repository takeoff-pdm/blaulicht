//! Desktop notifications (059, minimal form of `chrome.js` `BL.toast`):
//! cards bottom right above the status bar, newest at the bottom, at most 4.
//! Info closes after 5 s (paused while hovered), errors stay until closed,
//! and a repeated message counts up instead of adding a card.
//!
//! Desktop layout only; the kiosk keeps its popups. Ticket 070 owns the full
//! notification stack (actions, raising from dialogs) and builds on this.

use super::STATUS_BAR_HEIGHT;
use crate::app::ui_tokens::{studio, Density, Weight, PHOSPHOR_FILL_FAMILY};
use egui::{vec2, Align2, Context, Id, Order, Sense, Stroke, StrokeKind};
use std::time::Duration;
use web_time::Instant;

const MAX_NOTES: usize = 4;
const INFO_LIFE: Duration = Duration::from_secs(5);
const WIDTH: f32 = 360.0;

#[derive(Debug, Clone)]
struct Note {
    id: u64,
    text: String,
    error: bool,
    count: u32,
    /// When the info timer (re)started; `None` while hovered.
    since: Option<Instant>,
}

#[derive(Debug, Default)]
pub struct Notes {
    notes: Vec<Note>,
    next_id: u64,
}

impl Notes {
    pub fn info(&mut self, text: impl Into<String>) {
        self.push(text.into(), false);
    }

    pub fn error(&mut self, text: impl Into<String>) {
        self.push(text.into(), true);
    }

    fn push(&mut self, text: String, error: bool) {
        if let Some(same) = self.notes.iter_mut().find(|n| n.text == text) {
            same.count += 1;
            same.since = Some(Instant::now());
            return;
        }
        self.next_id += 1;
        self.notes.push(Note {
            id: self.next_id,
            text,
            error,
            count: 1,
            since: Some(Instant::now()),
        });
        if self.notes.len() > MAX_NOTES {
            let drop = self.notes.len() - MAX_NOTES;
            self.notes.drain(..drop);
        }
    }

    /// Draws the stack. Call once per frame on the desktop layout.
    pub fn show(&mut self, ctx: &Context) {
        self.notes
            .retain(|n| n.error || n.since.map_or(true, |at| at.elapsed() < INFO_LIFE));
        if self.notes.is_empty() {
            return;
        }
        let t = studio(ctx);
        let density = Density::of(ctx);
        let font = density.font(Weight::Regular, 12.0);
        let icon_font =
            egui::FontId::new(16.0, egui::FontFamily::Name(PHOSPHOR_FILL_FAMILY.into()));
        let screen = ctx.content_rect();
        let mut closed = None;
        egui::Area::new(Id::new("desk_notes"))
            .order(Order::Foreground)
            .pivot(Align2::RIGHT_BOTTOM)
            .fixed_pos(screen.right_bottom() - vec2(12.0, STATUS_BAR_HEIGHT + 10.0))
            .show(ctx, |ui| {
                ui.set_width(WIDTH);
                ui.spacing_mut().item_spacing.y = 6.0;
                for note in &mut self.notes {
                    let mut text = note.text.clone();
                    if note.count > 1 {
                        text.push_str(&format!("  \u{00d7}{}", note.count));
                    }
                    let galley = ui.painter().layout(
                        text,
                        font.clone(),
                        t.strong,
                        WIDTH - 12.0 - 18.0 - 30.0,
                    );
                    let h = galley.size().y.max(18.0) + 20.0;
                    let (rect, response) = ui.allocate_exact_size(vec2(WIDTH, h), Sense::hover());
                    let p = ui.painter();
                    p.rect_filled(rect, 0.0, t.ctl);
                    p.rect_stroke(rect, 0.0, Stroke::new(1.0_f32, t.line), StrokeKind::Outside);
                    let edge = if note.error { t.red } else { t.blue };
                    p.rect_filled(
                        egui::Rect::from_min_size(rect.min, vec2(3.0, rect.height())),
                        0.0,
                        edge,
                    );
                    let icon = if note.error {
                        egui_phosphor::fill::X_CIRCLE
                    } else {
                        egui_phosphor::fill::INFO
                    };
                    p.text(
                        rect.min + vec2(12.0, 10.0),
                        Align2::LEFT_TOP,
                        icon,
                        icon_font.clone(),
                        edge,
                    );
                    p.galley(rect.min + vec2(38.0, 10.0), galley, t.strong);
                    // Close: always visible on errors, on hover for info.
                    let hovered = response.contains_pointer();
                    let x_rect = egui::Rect::from_min_size(
                        egui::pos2(rect.right() - 30.0, rect.top() + 8.0),
                        vec2(22.0, 22.0),
                    );
                    let x = ui.interact(x_rect, Id::new(("desk_note_x", note.id)), Sense::click());
                    if note.error || hovered || x.hovered() {
                        let p = ui.painter();
                        if x.hovered() {
                            p.rect_filled(x_rect, 0.0, t.ctl_hi);
                        }
                        p.text(
                            x_rect.center(),
                            Align2::CENTER_CENTER,
                            egui_phosphor::regular::X,
                            egui::FontId::new(14.0, egui::FontFamily::Proportional),
                            if x.hovered() { t.strong } else { t.muted },
                        );
                    }
                    let x_hovered = x.hovered();
                    if x.on_hover_text("Close").clicked() {
                        closed = Some(note.id);
                    }
                    // Hover pauses the info timer; leaving restarts it.
                    if !note.error {
                        if hovered || x_hovered {
                            note.since = None;
                        } else if note.since.is_none() {
                            note.since = Some(Instant::now());
                        }
                    }
                }
            });
        if let Some(id) = closed {
            self.notes.retain(|n| n.id != id);
        }
        if self.notes.iter().any(|n| !n.error) {
            ctx.request_repaint_after(Duration::from_millis(250));
        }
    }
}
