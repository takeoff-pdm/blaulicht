//! A remote frontend's connection (the web UI): which streams it receives,
//! and a notice that covers the UI while it connects or after the connection
//! dropped, with automatic and manual reconnects. Nothing here runs for the
//! desk's local client.

use std::time::Duration;
use web_time::Instant;

use blaulicht_engine::client::{EngineClient, RemoteStatus, ReplyHandlers};
use blaulicht_shared::AppPage;
use egui::{Color32, Context, RichText};

use crate::app::{
    components::{self, ButtonSize, Dialog},
    BlaulichtApp,
};
use crate::client::remote_streams;

/// How long after a dropped connection (or a failed attempt) the UI tries
/// again on its own.
const RECONNECT_INTERVAL: Duration = Duration::from_secs(3);

/// A remote client's reconnect bookkeeping.
#[derive(Default)]
pub(crate) struct ConnectionUi {
    /// When the current connection attempt started (`None`: the first one).
    last_attempt: Option<Instant>,
    /// Why the last reconnect couldn't even start (the browser refused the
    /// URL), if it couldn't.
    error: Option<String>,
    /// What is typed into the passcode prompt.
    passcode_input: String,
}

impl BlaulichtApp {
    /// Keeps a remote client's streams to what the pages need: the
    /// spectrogram only while the Audio page shows (it is the biggest).
    pub(crate) fn sync_remote_streams(&mut self) {
        let Some(remote) = self.client.remote() else {
            return;
        };
        let wanted = remote_streams(self.navbar.page() == AppPage::Audio);
        if remote.streams() != wanted {
            remote.subscribe(&wanted);
        }
    }

    /// Covers the UI while a remote client connects, or after its connection
    /// dropped (then it reconnects every few seconds, or on a click).
    /// Returns `true` while it does; the page is not drawn then.
    pub(crate) fn render_connection_notice(&mut self, ctx: &Context) -> bool {
        let reason = match self.client.status() {
            RemoteStatus::Connected => return false,
            RemoteStatus::Connecting => None,
            RemoteStatus::Disconnected(reason) => Some(reason),
            RemoteStatus::PasscodeRequired(reason) => {
                self.render_passcode_prompt(ctx, &reason);
                return true;
            }
        };
        // Keep the countdown moving and notice the socket's callbacks.
        ctx.request_repaint_after(Duration::from_millis(250));

        let since_attempt = self
            .connection_ui
            .last_attempt
            .map(|at| at.elapsed())
            .unwrap_or(RECONNECT_INTERVAL);
        let mut reconnect = reason.is_some() && since_attempt >= RECONNECT_INTERVAL;

        // Desktop (059): the strip above the page shows this instead, so
        // nothing is covered; the page stays visible, dimmed and paused.
        if self.layout.is_desktop() {
            if reconnect {
                self.reconnect_remote();
            }
            return false;
        }

        let title = match reason {
            None => "Connecting",
            Some(_) => "Connection lost",
        };
        Dialog::new(title.to_string(), egui::vec2(380.0, 190.0))
            .with_backdrop()
            .show(ctx, |ui| match &reason {
                None => {
                    ui.heading("Connecting to the engine...");
                    ui.add_space(8.0);
                    ui.spinner();
                }
                Some(reason) => {
                    ui.heading(RichText::new("Connection to the engine lost").strong());
                    ui.label(RichText::new(reason).color(Color32::from_gray(150)));
                    if let Some(error) = &self.connection_ui.error {
                        ui.label(RichText::new(error).color(Color32::LIGHT_RED));
                    }
                    ui.add_space(6.0);
                    ui.label(
                        "Reconnecting opens a new session: the selection and \
                             programmer values start empty. The show is unaffected.",
                    );
                    let left = RECONNECT_INTERVAL.saturating_sub(since_attempt);
                    ui.label(format!("Trying again in {} s", left.as_secs() + 1));
                    ui.add_space(6.0);
                    ui.horizontal(|ui| {
                        if components::button(ui, true, "Reconnect", ButtonSize::Medium) {
                            reconnect = true;
                        }
                        #[cfg(target_arch = "wasm32")]
                        if components::button(ui, false, "Reload Page", ButtonSize::Medium) {
                            reload_page();
                        }
                    });
                }
            });

        if reconnect {
            self.reconnect_remote();
        }
        true
    }

    /// Desktop: the red strip on its own row above the page while the engine
    /// is offline. Returns `true` while it shows (the page is paused then).
    pub(crate) fn desktop_offline_strip(&mut self, ui: &mut egui::Ui) -> bool {
        let reason = match self.client.status() {
            RemoteStatus::Connected | RemoteStatus::PasscodeRequired(_) => return false,
            RemoteStatus::Connecting => None,
            RemoteStatus::Disconnected(reason) => Some(reason),
        };
        let (title, body) = match &reason {
            None => (
                "Engine connecting",
                "Values are the last known and may not match the lights.".to_string(),
            ),
            Some(_) => {
                let since = self
                    .connection_ui
                    .last_attempt
                    .map(|at| at.elapsed())
                    .unwrap_or(RECONNECT_INTERVAL);
                let left = RECONNECT_INTERVAL.saturating_sub(since).as_secs() + 1;
                (
                    "Engine disconnected",
                    format!(
                        "Reconnecting in {left} s. Values are the last known and may not match the lights."
                    ),
                )
            }
        };
        let density = crate::app::ui_tokens::Density::of(ui.ctx());
        let mut reconnect = false;
        egui::Panel::top("desktop_offline_strip")
            .resizable(false)
            .exact_size(28.0)
            .show_separator_line(false)
            .frame(
                egui::Frame::NONE
                    .fill(Color32::from_rgb(0xb8, 0x0f, 0x0f))
                    .inner_margin(egui::Margin::symmetric(10, 0)),
            )
            .show_inside(ui, |ui| {
                ui.horizontal_centered(|ui| {
                    ui.spacing_mut().item_spacing.x = 12.0;
                    let strong = crate::app::ui_tokens::Weight::SemiBold;
                    ui.label(
                        RichText::new(format!(
                            "{} {title}",
                            egui_phosphor::regular::WARNING_OCTAGON
                        ))
                        .font(density.font(strong, 12.0))
                        .color(Color32::WHITE),
                    );
                    ui.label(
                        RichText::new(body)
                            .font(density.font(crate::app::ui_tokens::Weight::Regular, 12.0))
                            .color(Color32::WHITE),
                    );
                    if let Some(error) = &self.connection_ui.error {
                        ui.label(RichText::new(error).color(Color32::WHITE));
                    }
                    if reason.is_some() {
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            if crate::app::components::studio::mini_button(
                                ui,
                                "Reconnect now",
                                false,
                                crate::app::ui_tokens::TARGET_MIN,
                            )
                            .clicked()
                            {
                                reconnect = true;
                            }
                        });
                    }
                });
            });
        if reconnect {
            self.reconnect_remote();
        }
        true
    }

    /// Asks for the engine's passcode, and connects with what is entered.
    fn render_passcode_prompt(&mut self, ctx: &Context, reason: &str) {
        let mut submit = false;
        let input = &mut self.connection_ui.passcode_input;
        Dialog::new("Passcode required".to_string(), egui::vec2(380.0, 190.0))
            .with_backdrop()
            .show(ctx, |ui| {
                ui.heading(RichText::new("Enter the engine's passcode").strong());
                ui.label(RichText::new(reason).color(Color32::from_gray(150)));
                ui.add_space(6.0);
                let response = components::TextInput::new(240.0)
                    .with_hint_text("Passcode")
                    .with_password(true)
                    .ui(ui, input);
                // Read before taking the focus back, which clears `lost_focus`.
                submit = response.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));
                if ui.memory(|memory| memory.focused().is_none()) {
                    response.request_focus();
                }
                ui.add_space(6.0);
                submit |= components::button(ui, true, "Connect", ButtonSize::Medium);
            });
        let passcode = self.connection_ui.passcode_input.trim().to_string();
        if submit && !passcode.is_empty() {
            self.connection_ui.passcode_input.clear();
            if let Some(remote) = self.client.remote() {
                remote.set_passcode(Some(passcode));
            }
            self.reconnect_remote();
        }
    }

    /// For System > General: this remote client's session and how long its
    /// recent commands took to come back. `None` for the desk.
    pub(crate) fn remote_status_line(&self) -> Option<String> {
        let remote = self.client.remote()?;
        let stats = remote.stats();
        let mut round_trips: Vec<_> = stats.recent_round_trips.iter().copied().collect();
        round_trips.sort();
        let timing = match (round_trips.get(round_trips.len() / 2), round_trips.last()) {
            (Some(median), Some(slowest)) => format!(
                "round trip {:.0} ms median, {:.0} ms slowest (last {})",
                median.as_secs_f64() * 1000.0,
                slowest.as_secs_f64() * 1000.0,
                round_trips.len()
            ),
            _ => "no commands yet".to_string(),
        };
        Some(format!(
            "Connected as {} ({}): {timing}",
            self.client.client(),
            self.client.session()
        ))
    }

    /// Replaces the dropped connection with a new one (a new session).
    fn reconnect_remote(&mut self) {
        // The old connection refused every command still waiting; run those
        // closures now, because the new client numbers its commands from 1
        // again.
        let results = self.client.take_results();
        if !results.is_empty() {
            ReplyHandlers::dispatch(self, |app| &mut app.reply_handlers, &results);
        }
        self.reply_handlers = ReplyHandlers::default();
        self.connection_ui.last_attempt = Some(Instant::now());
        match self.client.reconnect() {
            Ok(()) => {
                self.connection_ui.error = None;
                // The new connection replays the engine's recent log lines
                // and announces the show again.
                self.log_window.clear_logs();
                self.show_generation = 0;
                self.published_frontend_section = None;
                self.reset_show_view_state();
            }
            Err(err) => self.connection_ui.error = Some(format!("{err:#}")),
        }
    }
}

/// Reloads the page, which also fetches the engine's current web bundle.
#[cfg(target_arch = "wasm32")]
fn reload_page() {
    if let Some(window) = eframe::web_sys::window() {
        let _ = window.location().reload();
    }
}
