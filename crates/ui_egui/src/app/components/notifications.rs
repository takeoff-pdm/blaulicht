//! Stacked notifications (070, `chrome.js` `BL.toast`, `RULES.md`
//! Notifications): cards bottom right above the status bar, newest at the
//! bottom, at most 4 (the oldest closes). Each card has a type icon and left
//! edge (blue info, red error), the message, an optional action drawn as a
//! link, and a close button (always visible on errors).
//!
//! Info closes after 5 s, or 8 s with an action, paused while hovered.
//! Errors stay until closed. A repeated message counts up (×2) and restarts
//! its timer instead of adding a card.
//!
//! The stack belongs to the app, not to a page or a dialog, so a notification
//! raised from a dialog survives the dialog closing. Code that only has a
//! [`Context`] (dialogs, components) raises one with [`notify_info`] /
//! [`notify_error`]; the app takes those in on the next [`Notifications::show`].
//!
//! Desktop layout only: on the kiosk (`Density::Touch`) the free functions do
//! nothing and the app keeps its popups.

use super::STATUS_BAR_HEIGHT;
use crate::app::ui_tokens::{studio, Density, Weight, PHOSPHOR_FILL_FAMILY};
use crate::app::BlaulichtApp;
use egui::text::{LayoutJob, TextFormat};
use egui::{vec2, Align2, Context, FontFamily, FontId, Id, Order, Rect, Sense, Stroke, StrokeKind};
use std::sync::{Arc, Mutex};
use std::time::Duration;
use web_time::Instant;

pub const MAX_NOTIFICATIONS: usize = 4;
const INFO_LIFE: Duration = Duration::from_secs(5);
const ACTION_LIFE: Duration = Duration::from_secs(8);
const WIDTH: f32 = 360.0;
/// `.note` padding: 10px top/bottom, 12px left, 8px right.
const PAD_TOP: f32 = 10.0;
const PAD_LEFT: f32 = 12.0;
const PAD_RIGHT: f32 = 8.0;
const ICON_W: f32 = 18.0;
const GAP: f32 = 8.0;
const CLOSE: f32 = 22.0;
const LINE: f32 = 18.0;

/// What a notification's action link does once clicked. It runs with the app
/// on the frame after the click, and the card closes.
pub type NotificationAction = Box<dyn FnOnce(&mut BlaulichtApp, &Context) + Send>;

/// One notification to raise.
pub struct Notice {
    text: String,
    error: bool,
    action: Option<(String, NotificationAction)>,
}

impl Notice {
    pub fn info(text: impl Into<String>) -> Self {
        Self {
            text: text.into(),
            error: false,
            action: None,
        }
    }

    pub fn error(text: impl Into<String>) -> Self {
        Self {
            error: true,
            ..Self::info(text)
        }
    }

    /// Adds an action link. Never an "Undo": there is no undo support.
    pub fn action(
        mut self,
        label: impl Into<String>,
        run: impl FnOnce(&mut BlaulichtApp, &Context) + Send + 'static,
    ) -> Self {
        self.action = Some((label.into(), Box::new(run)));
        self
    }
}

struct Card {
    id: u64,
    text: String,
    error: bool,
    count: u32,
    action: Option<(String, NotificationAction)>,
    /// When the timer (re)started; `None` while hovered.
    since: Option<Instant>,
}

impl Card {
    /// How long the card stays; `None` for errors, which stay until closed.
    fn life(&self) -> Option<Duration> {
        match (self.error, self.action.is_some()) {
            (true, _) => None,
            (false, true) => Some(ACTION_LIFE),
            (false, false) => Some(INFO_LIFE),
        }
    }

    fn expired(&self, now: Instant) -> bool {
        match (self.life(), self.since) {
            (Some(life), Some(at)) => now.saturating_duration_since(at) >= life,
            _ => false,
        }
    }
}

/// The app's notification stack.
#[derive(Default)]
pub struct Notifications {
    cards: Vec<Card>,
    next_id: u64,
}

impl std::fmt::Debug for Notifications {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Notifications")
            .field("cards", &self.cards.len())
            .finish()
    }
}

type Queue = Arc<Mutex<Vec<Notice>>>;

fn queue_id() -> Id {
    Id::new("desk_notifications_queue")
}

/// Raises `notice` from code that only has a [`Context`], such as a dialog.
/// Does nothing on the kiosk.
pub fn notify(ctx: &Context, notice: Notice) {
    if !Density::of(ctx).is_compact() {
        return;
    }
    let queue: Queue = ctx.data_mut(|d| d.get_temp_mut_or_default::<Queue>(queue_id()).clone());
    if let Ok(mut q) = queue.lock() {
        q.push(notice);
    }
    ctx.request_repaint();
}

pub fn notify_info(ctx: &Context, text: impl Into<String>) {
    notify(ctx, Notice::info(text));
}

pub fn notify_error(ctx: &Context, text: impl Into<String>) {
    notify(ctx, Notice::error(text));
}

impl Notifications {
    pub fn info(&mut self, text: impl Into<String>) {
        self.push(Notice::info(text));
    }

    pub fn error(&mut self, text: impl Into<String>) {
        self.push(Notice::error(text));
    }

    pub fn len(&self) -> usize {
        self.cards.len()
    }

    pub fn is_empty(&self) -> bool {
        self.cards.is_empty()
    }

    pub fn push(&mut self, notice: Notice) {
        self.push_at(notice, Instant::now());
    }

    fn push_at(&mut self, notice: Notice, now: Instant) {
        if let Some(same) = self.cards.iter_mut().find(|c| c.text == notice.text) {
            same.count += 1;
            same.since = Some(now);
            return;
        }
        self.next_id += 1;
        self.cards.push(Card {
            id: self.next_id,
            text: notice.text,
            error: notice.error,
            count: 1,
            action: notice.action,
            since: Some(now),
        });
        if self.cards.len() > MAX_NOTIFICATIONS {
            let drop = self.cards.len() - MAX_NOTIFICATIONS;
            self.cards.drain(..drop);
        }
    }

    fn expire(&mut self, now: Instant) {
        self.cards.retain(|c| !c.expired(now));
    }

    /// Takes in what [`notify`] queued, draws the stack and returns the
    /// actions clicked this frame (run them with the app). Call once per
    /// frame on the desktop layout, after dialogs, so the cards draw on top.
    #[must_use]
    pub fn show(&mut self, ctx: &Context) -> Vec<NotificationAction> {
        let queued: Option<Queue> = ctx.data(|d| d.get_temp::<Queue>(queue_id()));
        if let Some(queue) = queued {
            if let Ok(mut q) = queue.lock() {
                for notice in q.drain(..) {
                    self.push(notice);
                }
            }
        }
        self.expire(Instant::now());
        let mut run = Vec::new();
        if self.cards.is_empty() {
            return run;
        }
        let t = studio(ctx);
        let density = Density::of(ctx);
        let font = density.font(Weight::Regular, 13.0);
        let link_font = density.font(Weight::Medium, 13.0);
        let mono = FontId::new(12.0, FontFamily::Monospace);
        let icon_font = FontId::new(16.0, FontFamily::Name(PHOSPHOR_FILL_FAMILY.into()));
        let msg_x = PAD_LEFT + ICON_W + GAP;
        let msg_w = WIDTH - msg_x - GAP - CLOSE - PAD_RIGHT;
        let screen = ctx.content_rect();
        let mut closed = None;
        egui::Area::new(Id::new("desk_notifications"))
            // Above dialogs and their backdrop, like the mockup's stack
            // inside an open <dialog>.
            .order(Order::Tooltip)
            .pivot(Align2::RIGHT_BOTTOM)
            .fixed_pos(screen.right_bottom() - vec2(12.0, STATUS_BAR_HEIGHT + 10.0))
            .show(ctx, |ui| {
                ui.set_width(WIDTH);
                ui.spacing_mut().item_spacing.y = 6.0;
                for card in &mut self.cards {
                    let mut job = LayoutJob::default();
                    job.wrap.max_width = msg_w;
                    job.append(
                        &card.text,
                        0.0,
                        TextFormat {
                            font_id: font.clone(),
                            color: t.strong,
                            line_height: Some(LINE),
                            ..Default::default()
                        },
                    );
                    if card.count > 1 {
                        job.append(
                            &format!("\u{00d7}{}", card.count),
                            6.0,
                            TextFormat {
                                font_id: mono.clone(),
                                color: t.muted,
                                line_height: Some(LINE),
                                ..Default::default()
                            },
                        );
                    }
                    let galley = ui.fonts_mut(|f| f.layout_job(job));
                    let action_h = if card.action.is_some() { 6.0 + LINE } else { 0.0 };
                    let h = galley.size().y.max(LINE) + action_h + 2.0 * PAD_TOP;
                    let (rect, _) = ui.allocate_exact_size(vec2(WIDTH, h), Sense::hover());
                    let hovered = ui.rect_contains_pointer(rect);
                    let p = ui.painter();
                    p.rect_filled(rect, 0.0, t.ctl);
                    p.rect_stroke(rect, 0.0, Stroke::new(1.0_f32, t.line), StrokeKind::Outside);
                    let edge = if card.error { t.red } else { t.blue };
                    p.rect_filled(
                        Rect::from_min_size(rect.min, vec2(3.0, rect.height())),
                        0.0,
                        edge,
                    );
                    let icon = if card.error {
                        egui_phosphor::fill::X_CIRCLE
                    } else {
                        egui_phosphor::fill::INFO
                    };
                    p.text(
                        rect.min + vec2(PAD_LEFT + ICON_W / 2.0, PAD_TOP + LINE / 2.0),
                        Align2::CENTER_CENTER,
                        icon,
                        icon_font.clone(),
                        edge,
                    );
                    let text_h = galley.size().y.max(LINE);
                    p.galley(rect.min + vec2(msg_x, PAD_TOP), galley, t.strong);

                    // The action, as a blue link under the message.
                    if let Some((label, _)) = &card.action {
                        let g = ui.painter().layout_no_wrap(label.clone(), link_font.clone(), t.blue);
                        let at = rect.min + vec2(msg_x, PAD_TOP + text_h + 6.0);
                        let link_rect = Rect::from_min_size(at, vec2(g.size().x, LINE));
                        let link =
                            ui.interact(link_rect, Id::new(("desk_note_act", card.id)), Sense::click());
                        let y = link_rect.center().y - g.size().y / 2.0;
                        let p = ui.painter();
                        p.galley(egui::pos2(at.x, y), g.clone(), t.blue);
                        if link.hovered() {
                            let base = y + g.size().y - 1.0;
                            p.hline(at.x..=at.x + g.size().x, base, Stroke::new(1.0_f32, t.blue));
                        }
                        if link.clicked() {
                            closed = Some((card.id, true));
                        }
                    }

                    // Close: always visible on errors, on hover for info.
                    let x_rect = Rect::from_min_size(
                        egui::pos2(rect.right() - PAD_RIGHT - CLOSE, rect.top() + PAD_TOP),
                        vec2(CLOSE, CLOSE),
                    );
                    let x = ui.interact(x_rect, Id::new(("desk_note_x", card.id)), Sense::click());
                    if card.error || hovered {
                        let p = ui.painter();
                        if x.hovered() {
                            p.rect_filled(x_rect, 0.0, t.ctl_hi);
                        }
                        p.text(
                            x_rect.center(),
                            Align2::CENTER_CENTER,
                            egui_phosphor::regular::X,
                            FontId::new(14.0, FontFamily::Proportional),
                            if x.hovered() { t.strong } else { t.muted },
                        );
                    }
                    if x.on_hover_text("Close").clicked() {
                        closed = Some((card.id, false));
                    }
                    // Hover pauses the timer; leaving restarts it.
                    if hovered {
                        card.since = None;
                    } else if card.since.is_none() {
                        card.since = Some(Instant::now());
                    }
                }
            });
        if let Some((id, act)) = closed {
            if let Some(i) = self.cards.iter().position(|c| c.id == id) {
                let card = self.cards.remove(i);
                // The link runs its action; the X only closes.
                if let (true, Some((_, action))) = (act, card.action) {
                    run.push(action);
                }
            }
        }
        if self.cards.iter().any(|c| !c.error) {
            ctx.request_repaint_after(Duration::from_millis(250));
        }
        run
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn texts(n: &Notifications) -> Vec<(&str, u32)> {
        n.cards.iter().map(|c| (c.text.as_str(), c.count)).collect()
    }

    #[test]
    fn five_quick_notifications_keep_the_newest_four() {
        let mut n = Notifications::default();
        let now = Instant::now();
        for i in 1..=5 {
            n.push_at(Notice::info(format!("n{i}")), now);
        }
        assert_eq!(
            texts(&n),
            vec![("n2", 1), ("n3", 1), ("n4", 1), ("n5", 1)]
        );
    }

    #[test]
    fn a_repeat_counts_up_and_restarts_its_timer() {
        let mut n = Notifications::default();
        let t0 = Instant::now();
        n.push_at(Notice::info("Saved"), t0);
        n.push_at(Notice::info("Saved"), t0 + Duration::from_secs(4));
        assert_eq!(texts(&n), vec![("Saved", 2)]);
        n.expire(t0 + Duration::from_secs(6));
        assert_eq!(n.len(), 1, "the repeat restarted the 5 s timer");
        n.expire(t0 + Duration::from_secs(9));
        assert!(n.is_empty());
    }

    #[test]
    fn info_lives_5_s_or_8_s_with_an_action_and_errors_stay() {
        let mut n = Notifications::default();
        let t0 = Instant::now();
        n.push_at(Notice::info("plain"), t0);
        n.push_at(Notice::info("linked").action("Open", |_, _| {}), t0);
        n.push_at(Notice::error("broken"), t0);
        n.expire(t0 + Duration::from_millis(5100));
        assert_eq!(texts(&n), vec![("linked", 1), ("broken", 1)]);
        n.expire(t0 + Duration::from_millis(8100));
        assert_eq!(texts(&n), vec![("broken", 1)]);
        n.expire(t0 + Duration::from_secs(600));
        assert_eq!(texts(&n), vec![("broken", 1)]);
    }

    #[test]
    fn hovered_cards_do_not_expire() {
        let mut n = Notifications::default();
        let t0 = Instant::now();
        n.push_at(Notice::info("held"), t0);
        n.cards[0].since = None;
        n.expire(t0 + Duration::from_secs(60));
        assert_eq!(n.len(), 1);
    }

    #[test]
    fn notify_is_a_no_op_on_the_kiosk() {
        let ctx = Context::default();
        notify_info(&ctx, "kiosk");
        assert!(ctx
            .data(|d| d.get_temp::<Queue>(queue_id()))
            .is_none());
        Density::Compact.set(&ctx);
        notify_info(&ctx, "desk");
        let q = ctx.data(|d| d.get_temp::<Queue>(queue_id())).unwrap();
        assert_eq!(q.lock().unwrap().len(), 1);
    }
}
