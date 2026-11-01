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
        };
        // Keep the countdown moving and notice the socket's callbacks.
        ctx.request_repaint_after(Duration::from_millis(250));

        let since_attempt = self
            .connection_ui
            .last_attempt
            .map(|at| at.elapsed())
            .unwrap_or(RECONNECT_INTERVAL);
        let mut reconnect = reason.is_some() && since_attempt >= RECONNECT_INTERVAL;

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
