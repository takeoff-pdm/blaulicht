use blaulicht_shared::{ControlEvent, ControlEventMessage, EventOriginator, MainUiEvent};
use egui::Context;

use crate::app::{
    components::{self, NavChange, NavbarStatus},
    BlaulichtApp,
};
use blaulicht_engine::client::{EngineClient, RemoteStatus};
use blaulicht_engine::hub::HostCommand;

impl BlaulichtApp {
    /// The mobile layout's icon rail.
    pub fn show_navbar(&mut self, ctx: &Context) {
        let summary = self.startup_progress.summary();
        let status = self
            .startup_progress
            .indicator()
            .map(|indicator| NavbarStatus {
                indicator,
                summary: &summary,
            });
        let Some(page_change) = self.navbar.ui(ctx, status) else {
            return;
        };

        // self.current_page = page_change.clone();

        // Send event to notify plugins.
        self.notify_navigation(MainUiEvent::NavigatePage(page_change));
    }

    /// The desktop layout's navigation sidebar, as a left panel of `ui`.
    pub fn show_desktop_sidebar(&mut self, ui: &mut egui::Ui) {
        let summary = self.startup_progress.summary();
        let status = self
            .startup_progress
            .indicator()
            .map(|indicator| NavbarStatus {
                indicator,
                summary: &summary,
            });
        self.navbar.sync_sections();
        let Some(click) = components::desktop_sidebar(ui, &self.navbar, status) else {
            return;
        };
        match self.navbar.sidebar_click(click) {
            None => {}
            Some(NavChange::Page(page)) => {
                self.notify_navigation(MainUiEvent::NavigatePage(page));
            }
            Some(NavChange::SubPage {
                subpage,
                page_changed,
            }) => {
                if page_changed {
                    self.notify_navigation(MainUiEvent::NavigatePage(subpage.page()));
                }
                self.notify_navigation(MainUiEvent::NavigateSubPage(subpage));
            }
        }
    }

    /// Where the engine connection stands; the desk's own engine is always
    /// online.
    pub(crate) fn engine_link(&self) -> components::EngineLink {
        match self.client.status() {
            RemoteStatus::Connected => components::EngineLink::Online,
            RemoteStatus::Connecting => components::EngineLink::Connecting,
            RemoteStatus::Disconnected(_) | RemoteStatus::PasscodeRequired(_) => {
                components::EngineLink::Disconnected
            }
        }
    }

    /// The desktop layout's status bar, as a bottom panel of `ui`. Only cheap
    /// reads of state the client already holds; clicks open the page that
    /// fixes a problem, or toggle Blackout.
    pub fn show_desktop_status_bar(&mut self, ui: &mut egui::Ui) {
        let link = self.engine_link();
        components::value_entry::set_engine_link(ui.ctx(), link);
        let grand_master = self.client.grand_master_percent();
        // Remember the level Blackout restores: the last non-zero one, also
        // when another client changed Grand Master.
        if link.is_online() && grand_master > 0 {
            self.blackout_restore = grand_master;
        }
        let loop_ms = self.tick_speeds.loop_total.as_secs_f32() * 1000.0;
        let plugins_ms = self.tick_speeds.plugins.as_secs_f32() * 1000.0;
        if link.is_online() {
            self.status_sparks.sample(loop_ms, plugins_ms);
        }
        let show_name = self
            .client
            .showfile_path()
            .as_deref()
            .map(blaulicht_engine::config::showfile_display_name);
        let live = self.client.with_show(|show| show.0.live_mode);
        let health = self.health_summary();
        let remote_sessions = self
            .client
            .with_remote_server(|server| server.sessions.len());
        let info = components::StatusBarInfo {
            blackout: (grand_master == 0).then_some(self.blackout_restore),
            show_name,
            save_status: &self.save_status,
            link,
            dmx_outputs: health.dmx_outputs,
            artnet: health.artnet,
            audio_level: (self.collector_snapshot.volume as f32 / 255.0).clamp(0.0, 1.0),
            bpm: self.collector_snapshot.bpm,
            sparks: &self.status_sparks,
            live,
            errors: health.errors,
            warnings: health.warnings,
            remote_sessions,
            routing_pending: self.routing_pending,
        };
        match components::status_bar(ui, &info) {
            None => {}
            Some(components::StatusBarAction::ToggleBlackout) => self.toggle_blackout(),
            Some(components::StatusBarAction::Open(subpage)) => self.open_subpage(subpage),
        }
    }

    /// Blackout (desktop, 059): Grand Master to 0, or back to the level it
    /// had (`blackout_restore`). Offline, nothing is sent or queued; a
    /// notice says so.
    pub(crate) fn toggle_blackout(&mut self) {
        let link = self.engine_link();
        let black = self.client.grand_master_percent() == 0;
        if !link.is_online() {
            let what = if black { "Restore" } else { "Blackout" };
            let why = format!("{what} not sent: engine {}", link.word());
            if self.layout.is_desktop() {
                self.desk_notes.error(why);
            } else {
                self.show_refusal(why);
            }
            return;
        }
        let level = if black {
            self.blackout_restore.max(1)
        } else {
            0
        };
        self.send_command(HostCommand::SetGrandMaster(level), |_, _| {});
        // The mockup's messages (`chrome.js` toggleBlack).
        if self.layout.is_desktop() {
            if level == 0 {
                self.desk_notes.error("Blackout on");
            } else {
                self.desk_notes
                    .info(format!("Grand Master restored to {level}%"));
            }
        }
    }

    /// `Cmd/Ctrl+B` toggles Blackout from any page, inside text fields and
    /// behind dialogs. Key repeat is ignored, so holding it toggles once.
    /// The event is taken out of the input, so a focused field never sees
    /// it; a plain `b` is left alone. Desktop only.
    pub(crate) fn handle_blackout_shortcut(&mut self, ctx: &Context) {
        let mut pressed = false;
        ctx.input_mut(|input| {
            input.events.retain(|event| match event {
                egui::Event::Key {
                    key: egui::Key::B,
                    pressed: down,
                    repeat,
                    modifiers,
                    ..
                } if modifiers.command && !modifiers.alt && !modifiers.shift => {
                    if *down && !*repeat {
                        pressed = true;
                    }
                    false
                }
                _ => true,
            });
        });
        if pressed {
            self.toggle_blackout();
        }
    }

    /// Tells plugins about a navigation in this UI. Sent as `Web`, so it
    /// doesn't loop back into the UI.
    pub(crate) fn notify_navigation(&self, event: MainUiEvent) {
        self.client.send(ControlEventMessage::new(
            EventOriginator::Web,
            ControlEvent::MainUi(event),
        ));
    }
}
