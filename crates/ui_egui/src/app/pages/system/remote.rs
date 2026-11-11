//! System > Remote UI: whether the engine accepts remote frontends, their
//! passcode, and who is connected. Only the desk changes the settings
//! (`HostCommand::SetRemoteAccess` is desk-only); a remote UI sees the status.

use blaulicht_engine::{
    client::EngineClient, hub::HostCommand, server::protocol::RemoteServerStatus,
};
use egui::{Color32, RichText, Ui, Widget};

use crate::app::{
    components::{self, ButtonSize},
    BlaulichtApp,
};

const DIM: Color32 = Color32::from_gray(150);
const WARNING: Color32 = Color32::from_rgb(230, 170, 70);

impl BlaulichtApp {
    pub(super) fn render_remote_ui(&mut self, ui: &mut Ui) {
        let status = self.client.with_remote_server(RemoteServerStatus::clone);

        ui.heading("Status");
        ui.add_space(4.0);
        let (line, color) = status_line(&status);
        ui.label(RichText::new(line).color(color));
        if let Some(addr) = status.listening {
            ui.label(
                RichText::new(format!(
                    "Open http://<this device>:{}/ in a browser",
                    addr.port()
                ))
                .color(DIM),
            );
        }
        ui.add_space(12.0);

        if !self.client.is_remote() {
            self.render_remote_settings(ui, &status);
            ui.add_space(12.0);
        }

        ui.heading(format!(
            "Sessions ({} of {})",
            status.sessions.len(),
            status.max_clients
        ));
        ui.add_space(4.0);
        if status.sessions.is_empty() {
            ui.label("No remote UI is connected.");
        }
        let me = self.client.remote().map(|_| self.client.client());
        for session in &status.sessions {
            let mut line = format!(
                "{}  {}  {}  {}  (TX: {})",
                session.client,
                session.session,
                session.peer,
                format_duration(session.connected_secs),
                format_bytes(session.sent_bytes),
            );
            if me == Some(session.client) {
                line.push_str("  (this UI)");
            }
            if session.backlogged {
                line.push_str("  backlogged");
            }
            ui.label(RichText::new(line).monospace());
        }
    }

    fn render_remote_settings(&mut self, ui: &mut Ui, status: &RemoteServerStatus) {
        ui.heading("Access");
        ui.add_space(4.0);

        let mut enabled = status.enabled;
        let mut new_passcode = status.passcode.clone();
        let mut changed = false;
        ui.horizontal(|ui| {
            changed |= components::Switch::new(&mut enabled).ui(ui).changed();
            ui.label("Allow remote connections");
        });
        ui.add_space(6.0);

        ui.horizontal(|ui| match &status.passcode {
            None => {
                ui.label("Passcode: none");
            }
            Some(passcode) => {
                let shown = self.system_ui_state.remote_passcode_shown;
                ui.label(if shown {
                    format!("Passcode: {passcode}")
                } else {
                    "Passcode: ••••".to_string()
                });
                let toggle = if shown { "Hide" } else { "Show" };
                if components::button(ui, false, toggle, ButtonSize::Medium) {
                    self.system_ui_state.remote_passcode_shown = !shown;
                }
                if components::button(ui, false, "Remove", ButtonSize::Medium) {
                    new_passcode = None;
                    changed = true;
                }
            }
        });
        ui.horizontal(|ui| {
            components::TextInput::new(200.0)
                .with_hint_text("New passcode")
                .ui(ui, &mut self.system_ui_state.remote_passcode_input);
            let typed = self.system_ui_state.remote_passcode_input.trim();
            if components::button(ui, !typed.is_empty(), "Set", ButtonSize::Medium)
                && !typed.is_empty()
            {
                new_passcode = Some(typed.to_string());
                changed = true;
                self.system_ui_state.remote_passcode_input.clear();
            }
        });
        ui.add_space(4.0);
        if status.enabled && !status.passcode_set {
            ui.label(
                RichText::new("Without a passcode, anyone on the network can operate the show.")
                    .color(WARNING),
            );
        }
        ui.label(
            RichText::new(
                "Changing the passcode disconnects every remote UI. It is sent unencrypted, \
                 so use it on a network you trust.",
            )
            .color(DIM),
        );

        if changed {
            self.client.send(HostCommand::SetRemoteAccess {
                enabled,
                passcode: new_passcode,
            });
        }
    }
}

fn status_line(status: &RemoteServerStatus) -> (String, Color32) {
    if let Some(error) = &status.error {
        return (error.clone(), Color32::LIGHT_RED);
    }
    if !status.enabled {
        return ("Off: remote UIs can't connect".to_string(), DIM);
    }
    match status.listening {
        Some(addr) => (
            format!(
                "Running on port {}, {}",
                addr.port(),
                if status.passcode_set {
                    "passcode required"
                } else {
                    "no passcode"
                }
            ),
            Color32::LIGHT_GREEN,
        ),
        None => ("Starting...".to_string(), WARNING),
    }
}

fn format_duration(secs: u64) -> String {
    match secs {
        0..=59 => format!("{secs}s"),
        60..=3599 => format!("{}m", secs / 60),
        _ => format!("{}h {}m", secs / 3600, secs / 60 % 60),
    }
}

fn format_bytes(bytes: u64) -> String {
    const MB: u64 = 1 << 20;
    if bytes >= MB {
        format!("{:.1} MB", bytes as f64 / MB as f64)
    } else {
        format!("{} KB", bytes >> 10)
    }
}
