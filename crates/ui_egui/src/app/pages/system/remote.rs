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

/// This machine's address on the network, for the Web UI pane: the local
/// address the system would send from (a UDP `connect` sends nothing).
#[cfg(not(target_arch = "wasm32"))]
fn local_ip() -> Option<std::net::IpAddr> {
    static IP: std::sync::OnceLock<Option<std::net::IpAddr>> = std::sync::OnceLock::new();
    *IP.get_or_init(|| {
        let socket = std::net::UdpSocket::bind("0.0.0.0:0").ok()?;
        socket.connect("192.168.0.1:9").ok()?;
        let ip = socket.local_addr().ok()?.ip();
        (!ip.is_unspecified()).then_some(ip)
    })
}

#[cfg(target_arch = "wasm32")]
fn local_ip() -> Option<std::net::IpAddr> {
    None
}

/// The address a browser opens for the web UI.
fn web_address(status: &RemoteServerStatus, default_port: Option<u16>) -> String {
    let addr = status.listening;
    let port = addr.map(|a| a.port()).or(default_port);
    let host = match addr.map(|a| a.ip()) {
        Some(ip) if !ip.is_unspecified() && !ip.is_loopback() => Some(ip),
        _ => local_ip(),
    };
    let host = host.map_or_else(|| "<this device>".to_string(), |ip| ip.to_string());
    match port {
        Some(port) => format!("http://{host}:{port}/"),
        None => format!("http://{host}/"),
    }
}

/// Column starts of the Connected clients table (`system.html` `.rows`).
const CLIENT_COLS: [f32; 3] = [0.0, 0.445, 0.668];

impl BlaulichtApp {
    /// System › Remote UI on the desktop (086, `system.html#remote`): the
    /// Web UI address and Access side by side over the connected clients.
    /// Only the desk changes Access; a remote UI sees it read-only.
    pub(super) fn render_remote_desktop(&mut self, ui: &mut Ui) {
        use super::desk::{self, State};
        use crate::app::components::studio;
        use crate::app::pages::setup::duo;
        use crate::app::ui_tokens::{studio as tokens, Density, Weight, TARGET_MIN, TEXT};
        use egui::{pos2, vec2, Align2, Rect, Sense};

        let t = tokens(ui.ctx());
        let status = self.client.with_remote_server(RemoteServerStatus::clone);
        let remote = self.client.is_remote();
        let warn = status.enabled && !status.passcode_set;
        let full = desk::page_rect(ui);
        // Header, padding, two rows and their gap, padding; the warning
        // adds a line.
        let top_h = (26.0 + 8.0 + 2.0 * TARGET_MIN + 6.0 + 8.0 + if warn { 22.0 } else { 0.0 })
            .max(26.0 + 8.0 + 36.0 + 6.0 + 16.0 + 8.0);
        let half = ((full.width() - 1.0) / 2.0).floor();
        let addr_rect = Rect::from_min_size(full.min, vec2(half, top_h));
        let access_rect = Rect::from_min_max(
            pos2(addr_rect.right() + 1.0, full.top()),
            pos2(full.right(), full.top() + top_h),
        );
        let clients_rect =
            Rect::from_min_max(pos2(full.left(), addr_rect.bottom() + 1.0), full.max);

        // Web UI.
        let mut addr = desk::pane_at(ui, addr_rect, t.pane, "remote-addr");
        let (state, state_text) = if status.error.is_some() {
            (State::Bad, "Error")
        } else if !status.enabled {
            (State::Off, "Off")
        } else if status.listening.is_some() {
            (State::Ok, "Listening")
        } else {
            (State::Warn, "Starting")
        };
        studio::pane_header(&mut addr, None, "Web UI", false, |ui| {
            desk::state_label(ui, state, state_text);
        });
        if let Some(addr) = status.listening {
            self.system_ui_state.remote_port = Some(addr.port());
        }
        let address = web_address(&status, self.system_ui_state.remote_port);
        duo::body(&mut addr, None, |ui| {
            let f = egui::FontId::monospace(16.0);
            let (r, _) = ui.allocate_exact_size(vec2(ui.available_width(), 36.0), Sense::hover());
            ui.painter().rect_filled(r, 0.0, t.well);
            studio::paint_truncated(
                ui,
                pos2(r.left() + 8.0, r.center().y),
                &address,
                f,
                t.strong,
                r.width() - 16.0,
            );
            match &status.error {
                Some(error) => desk::error_text(ui, error),
                None => duo::note(
                    ui,
                    if status.enabled {
                        "Open this address on any device on the network."
                    } else {
                        "Remote connections are off."
                    },
                ),
            }
        });

        // Access.
        let mut access = desk::pane_at(ui, access_rect, t.pane, "remote-access");
        studio::pane_header(&mut access, None, "Access", false, |_| {});
        let mut enabled = status.enabled;
        let mut new_passcode = status.passcode.clone();
        let mut changed = false;
        duo::body(&mut access, None, |ui| {
            let f = Density::of(ui.ctx()).font(Weight::Regular, TEXT);
            access_row(ui, "Remote control", |ui| {
                if remote {
                    duo::text(ui, if status.enabled { "Allowed" } else { "Off" });
                    return;
                }
                changed |= studio::switch(ui, &mut enabled).changed();
                ui.add_space(4.0);
                duo::note(ui, if enabled { "Allowed" } else { "Off" });
            });
            access_row(ui, "Passcode", |ui| {
                if remote {
                    duo::text(ui, if status.passcode_set { "Set" } else { "None" });
                    return;
                }
                let current = status.passcode.clone().unwrap_or_default();
                let id = ui.id().with("remote-passcode");
                let mut buf: String = ui
                    .data(|d| d.get_temp::<String>(id))
                    .unwrap_or_else(|| current.clone());
                let shown = self.system_ui_state.remote_passcode_shown;
                let response = ui.add_sized(
                    vec2(150.0, TARGET_MIN),
                    egui::TextEdit::singleline(&mut buf)
                        .id(id.with("edit"))
                        .password(!shown)
                        .hint_text("None")
                        .font(f.clone())
                        .vertical_align(egui::Align::Center),
                );
                if response.lost_focus() {
                    let escaped = ui.input(|i| i.key_pressed(egui::Key::Escape));
                    let typed = buf.trim();
                    if !escaped && typed != current {
                        new_passcode = (!typed.is_empty()).then(|| typed.to_string());
                        changed = true;
                    }
                    ui.data_mut(|d| d.remove::<String>(id));
                } else if response.has_focus() {
                    ui.data_mut(|d| d.insert_temp(id, buf));
                } else {
                    ui.data_mut(|d| d.remove::<String>(id));
                }
                ui.spacing_mut().item_spacing.x = 2.0;
                ui.add_space(2.0);
                if duo::chip(ui, if shown { "Hide" } else { "Show" }) {
                    self.system_ui_state.remote_passcode_shown = !shown;
                }
                if ui
                    .add_enabled_ui(status.passcode_set, |ui| duo::chip(ui, "Remove"))
                    .inner
                {
                    new_passcode = None;
                    changed = true;
                }
            });
            if warn {
                desk::error_text(
                    ui,
                    "Without a passcode, anyone on the network can operate the show.",
                );
            }
        });
        if changed && !remote {
            self.client.send(HostCommand::SetRemoteAccess {
                enabled,
                passcode: new_passcode,
            });
        }

        // Connected clients.
        let mut clients = desk::pane_at(ui, clients_rect, t.pane, "remote-clients");
        studio::pane_header(&mut clients, None, "Connected clients", false, |ui| {
            let n = status.sessions.len().to_string();
            let w = studio::text_width(ui, &n, duo::mono_font());
            let (r, _) = ui.allocate_exact_size(vec2(w, 24.0), Sense::hover());
            ui.painter().text(
                pos2(r.left(), r.center().y),
                Align2::LEFT_CENTER,
                n,
                duo::mono_font(),
                t.strong,
            );
        });
        // `.body` (table or empty note) is the darker list colour.
        clients
            .painter()
            .rect_filled(clients.available_rect_before_wrap(), 0.0, t.list);
        if status.sessions.is_empty() {
            duo::body(&mut clients, None, |ui| {
                duo::note(ui, "No remote UI is connected.");
            });
            return;
        }
        let me = self.client.remote().map(|_| self.client.client());
        let rows: Vec<Vec<duo::Cell>> = status
            .sessions
            .iter()
            .map(|s| {
                let mut client = "Remote UI".to_string();
                if me == Some(s.client) {
                    client.push_str(" (this UI)");
                }
                if s.backlogged {
                    client.push_str(", backlogged");
                }
                vec![
                    (s.peer.ip().to_string(), duo::Look::Mono),
                    (client, duo::Look::Text),
                    (desk::duration(s.connected_secs), duo::Look::Muted),
                ]
            })
            .collect();
        duo::flush(&mut clients, |ui| {
            duo::table(
                ui,
                &["Address", "Client", "Connected for"],
                &CLIENT_COLS,
                &rows,
            );
        });
    }
}

/// A row of the Access list: a muted term in the 128px column, then `add`,
/// 24px tall for its controls.
fn access_row(ui: &mut Ui, term: &str, add: impl FnOnce(&mut Ui)) {
    let t = crate::app::ui_tokens::studio(ui.ctx());
    let f = crate::app::ui_tokens::Density::of(ui.ctx())
        .font(crate::app::ui_tokens::Weight::Regular, 12.0);
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = 0.0;
        let (r, _) = ui.allocate_exact_size(
            egui::vec2(128.0, crate::app::ui_tokens::TARGET_MIN),
            egui::Sense::hover(),
        );
        ui.painter().text(
            egui::pos2(r.left(), r.center().y),
            egui::Align2::LEFT_CENTER,
            term,
            f,
            t.muted,
        );
        add(ui);
    });
}
