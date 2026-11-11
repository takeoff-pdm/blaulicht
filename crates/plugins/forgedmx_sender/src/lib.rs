//! Sends BPM and beat-in-bar to forgeDMX over OSC/UDP (default port 9000) and
//! checks over its HTTP API (default port 3000) that the messages arrive.
//! Optionally sends the colors of a primary and a secondary palette.

mod clock;
mod color;
mod health;
mod osc;

use blaulicht_plugin_framework as bpf;
use blaulicht_plugin_framework::{ui, HttpRequest, Plugin};
use blaulicht_shared::{ControlEvent, PluginUiEvent, TickInput};
use clock::BeatTracker;
use color::Rgb;
use health::{Health, RemoteState};
use osc::Arg;
use serde::{Deserialize, Serialize};

const ID_HOST: u8 = 1;
const ID_PORT: u8 = 2;
const BTN_APPLY: u8 = 3;
const CHK_ENABLED: u8 = 4;
const BTN_RESYNC: u8 = 5;
const ID_API_PORT: u8 = 6;
const CMB_PRIMARY: u8 = 7;
const CMB_SECONDARY: u8 = 8;

const KEEPALIVE_MS: u32 = 5_000;
const CHECK_MS: u32 = 2_000;
const BPM_EPSILON: f32 = 0.5;

fn default_api_port() -> u16 {
    3000
}

#[derive(Serialize, Deserialize)]
struct SaveState {
    host: String,
    port: u16,
    #[serde(default = "default_api_port")]
    api_port: u16,
    enabled: bool,
    /// Palette ids whose colors are sent as forgeDMX color A and B.
    #[serde(default)]
    primary_palette: Option<u8>,
    #[serde(default)]
    secondary_palette: Option<u8>,
}

impl Default for SaveState {
    fn default() -> Self {
        Self {
            host: "127.0.0.1".into(),
            port: 9000,
            api_port: 3000,
            enabled: false,
            primary_palette: None,
            secondary_palette: None,
        }
    }
}

/// Reachability check state; reset whenever the target changes.
struct Check {
    last_started: Option<u32>,
    state_req: Option<HttpRequest>,
    sync_req: Option<HttpRequest>,
    /// Our message counter when the running/previous state request started.
    sent_at_start: u64,
    sent_at_prev: u64,
    remote: Option<RemoteState>,
    sync_source: Option<String>,
    health: Health,
}

impl Default for Check {
    fn default() -> Self {
        Self {
            last_started: None,
            state_req: None,
            sync_req: None,
            sent_at_start: 0,
            sent_at_prev: 0,
            remote: None,
            sync_source: None,
            health: Health::Unknown,
        }
    }
}

#[derive(Default)]
struct ForgeDmxSender {
    state: SaveState,
    host_text: String,
    port_text: String,
    api_port_text: String,
    error: String,
    tracker: BeatTracker,
    /// Clock of the last sync/keep-alive; None forces a sync on next tick.
    last_sync: Option<u32>,
    last_bpm_sent: Option<f32>,
    last_beat: Option<clock::Beat>,
    messages: u64,
    check: Check,
    /// Color palettes of the show, refreshed every tick, as (id, name).
    palette_options: Vec<(u8, String)>,
    /// Current colors of the selected palettes (None: none selected or gone).
    colors: (Option<Rgb>, Option<Rgb>),
    /// Colors last sent to forgeDMX.
    sent_colors: (Option<Rgb>, Option<Rgb>),
}

impl ForgeDmxSender {
    fn target(&self) -> String {
        format!("{}:{}", self.state.host.trim(), self.state.port)
    }

    fn api_url(&self, path: &str) -> String {
        format!("http://{}:{}/api/{path}", self.state.host.trim(), self.state.api_port)
    }

    fn send(&mut self, address: &str, args: &[Arg]) {
        bpf::send_udp(&self.target(), &osc::message(address, args));
        self.messages += 1;
    }

    fn send_bpm(&mut self, bpm: f32) {
        self.send("/forgedmx/bpm", &[Arg::Float(bpm)]);
        self.last_bpm_sent = Some(bpm);
    }

    fn save(&self) {
        if let Ok(json) = serde_json::to_string(&self.state) {
            bpf::save_plugin_state(bpf::PluginStateLocation::Global, &json);
        }
    }

    /// Target or enable changed: resync and judge reachability from scratch.
    fn restart(&mut self) {
        self.last_sync = None;
        self.check = Check::default();
        self.sent_colors = (None, None);
    }

    fn refresh_palettes(&mut self) {
        let (primary, secondary) = (self.state.primary_palette, self.state.secondary_palette);
        let (options, colors) = bpf::with_dmx(|dmx| {
            let rgb = |id: Option<u8>| id.and_then(|id| color::palette_rgb(&dmx.palettes, id));
            (color::color_palettes(&dmx.palettes), (rgb(primary), rgb(secondary)))
        });
        self.palette_options = options;
        self.colors = colors;
    }

    /// Combo index 0 is "None"; the rest follow `palette_options`.
    fn palette_for_index(&self, index: u8) -> Option<u8> {
        index
            .checked_sub(1)
            .and_then(|i| self.palette_options.get(i as usize))
            .map(|(id, _)| *id)
    }

    fn index_for_palette(&self, id: Option<u8>) -> u8 {
        id.and_then(|id| self.palette_options.iter().position(|(p, _)| *p == id))
            .map_or(0, |i| (i + 1).min(u8::MAX as usize) as u8)
    }

    /// Sends colors that changed since the last send (all of them when `force`).
    /// A stays as it is in forgeDMX when no primary is selected; B goes back to
    /// "derived from A" when the secondary is deselected.
    fn send_colors(&mut self, force: bool) {
        let (a, b) = self.colors;
        let (sent_a, sent_b) = self.sent_colors;
        let a_changed = force || a != sent_a;
        let b_changed = force || b != sent_b;
        match (a, b) {
            (Some(a), Some(b)) => {
                if a_changed || b_changed {
                    let (a, b) = (color::hex(a), color::hex(b));
                    self.send("/forgedmx/color", &[Arg::Str(&a), Arg::Str(&b)]);
                }
            }
            _ => {
                if let Some(a) = a.filter(|_| a_changed) {
                    self.send("/forgedmx/color/a", &[Arg::Str(&color::hex(a))]);
                }
                match b {
                    Some(b) if b_changed => self.send("/forgedmx/color/b", &[Arg::Str(&color::hex(b))]),
                    None if sent_b.is_some() => self.send("/forgedmx/color/b", &[]),
                    _ => {}
                }
            }
        }
        self.sent_colors = (a, b);
    }

    fn handle_ui(&mut self, event: &PluginUiEvent) {
        match event {
            PluginUiEvent::Text { id, text } if *id == ID_HOST => self.host_text = text.clone(),
            PluginUiEvent::Text { id, text } if *id == ID_PORT => self.port_text = text.clone(),
            PluginUiEvent::Text { id, text } if *id == ID_API_PORT => self.api_port_text = text.clone(),
            PluginUiEvent::Button { id } if *id == BTN_APPLY => {
                let host = self.host_text.trim();
                let port = self.port_text.trim().parse::<u16>();
                let api_port = self.api_port_text.trim().parse::<u16>();
                match (port, api_port) {
                    (Ok(port), Ok(api_port)) if port != 0 && api_port != 0 && !host.is_empty() => {
                        self.state.host = host.to_string();
                        self.state.port = port;
                        self.state.api_port = api_port;
                        self.error.clear();
                        self.save();
                        self.restart();
                    }
                    _ => self.error = "Invalid host or port".into(),
                }
            }
            PluginUiEvent::Checkbox { id, checked } if *id == CHK_ENABLED => {
                self.state.enabled = *checked;
                self.save();
                self.restart();
            }
            PluginUiEvent::Button { id } if *id == BTN_RESYNC => self.last_sync = None,
            PluginUiEvent::ComboBox { id, selected } if *id == CMB_PRIMARY || *id == CMB_SECONDARY => {
                let palette = self.palette_for_index(*selected);
                if *id == CMB_PRIMARY {
                    self.state.primary_palette = palette;
                } else {
                    self.state.secondary_palette = palette;
                }
                self.save();
                self.refresh_palettes();
            }
            _ => {}
        }
    }

    fn run_check(&mut self, now: u32) {
        if let Some(resp) = self.check.state_req.as_ref().and_then(HttpRequest::poll) {
            self.check.state_req = None;
            let sent_between = self.check.sent_at_start - self.check.sent_at_prev;
            self.check.health = if !resp.ok {
                let why = if resp.error.is_empty() {
                    format!("HTTP {} from {}", resp.status, self.api_url("state"))
                } else {
                    resp.error
                };
                self.check.remote = None;
                Health::Unreachable(why)
            } else {
                match health::parse_state(&resp.body) {
                    Ok(remote) => {
                        let h = health::evaluate(self.check.remote.as_ref(), &remote, sent_between);
                        self.check.remote = Some(remote);
                        h
                    }
                    Err(e) => {
                        self.check.remote = None;
                        Health::Unreachable(e)
                    }
                }
            };
            self.check.sent_at_prev = self.check.sent_at_start;
        }
        if let Some(resp) = self.check.sync_req.as_ref().and_then(HttpRequest::poll) {
            self.check.sync_req = None;
            self.check.sync_source = resp.ok.then(|| health::parse_sync_source(&resp.body)).flatten();
        }

        let in_flight = self.check.state_req.is_some() || self.check.sync_req.is_some();
        let due = self.check.last_started.is_none_or(|t| now.wrapping_sub(t) >= CHECK_MS);
        if !in_flight && due {
            self.check.last_started = Some(now);
            self.check.sent_at_start = self.messages;
            self.check.state_req = bpf::http_get(&self.api_url("state"));
            self.check.sync_req = bpf::http_get(&self.api_url("sync"));
        }
    }

    fn render_ui(&self) {
        ui::begin();
        ui::label_styled("forgeDMX Sender", 18, false);
        ui::label("Sends BPM and beats to forgeDMX (OSC over UDP).");
        ui::separator();
        ui::begin_horizontal();
        ui::text_edit("Host", ID_HOST, &self.host_text);
        ui::text_edit("OSC port", ID_PORT, &self.port_text);
        ui::text_edit("API port", ID_API_PORT, &self.api_port_text);
        ui::button("Apply", BTN_APPLY);
        ui::end_horizontal();
        let mut names = vec!["None".to_string()];
        names.extend(self.palette_options.iter().map(|(id, name)| format!("{id}: {name}")));
        ui::begin_horizontal();
        ui::combo_box("Primary (A)", CMB_PRIMARY, &names, self.index_for_palette(self.state.primary_palette));
        ui::combo_box("Secondary (B)", CMB_SECONDARY, &names, self.index_for_palette(self.state.secondary_palette));
        ui::end_horizontal();
        for (label, id) in [("Primary", self.state.primary_palette), ("Secondary", self.state.secondary_palette)] {
            if let Some(id) = id.filter(|id| !self.palette_options.iter().any(|(p, _)| p == id)) {
                ui::label(&format!("{label} palette {id} no longer exists or is not a color palette."));
            }
        }
        ui::begin_horizontal();
        ui::checkbox("Enabled", CHK_ENABLED, self.state.enabled);
        ui::button("Resend sync", BTN_RESYNC);
        ui::end_horizontal();
        if !self.error.is_empty() {
            ui::label(&format!("ERROR: {}", self.error));
        }
        ui::separator();
        if !self.state.enabled {
            ui::label("Disabled");
            return;
        }

        let health = &self.check.health;
        let prefix = if health.is_error() { "ERROR: " } else { "" };
        ui::label_styled(&format!("{prefix}{}", health.text()), 14, true);
        if let Some(r) = &self.check.remote {
            let bpm = r.bpm.map_or("-".into(), |b| format!("{b:.1}"));
            let beat = r.beat_in_bar.map_or("-".into(), |b| b.to_string());
            let age = r.last_beat_age.map_or("-".into(), |a| format!("{a:.2} s"));
            ui::label(&format!(
                "forgeDMX has: BPM {bpm} | beat {beat} | last beat {age} ago | {} messages",
                r.messages
            ));
        }
        if let Some(src) = &self.check.sync_source {
            if !src.eq_ignore_ascii_case("api") {
                ui::label(&format!(
                    "forgeDMX sync source is {src}, so its visuals don't follow this clock yet."
                ));
            }
        }

        let bpm = self.last_bpm_sent.map_or("-".into(), |b| format!("{b:.1}"));
        let beat = self.last_beat.map_or("-".into(), |b| {
            format!("{}{}", b.position, if b.estimated { " (counted)" } else { "" })
        });
        ui::label(&format!(
            "Sending to {}: BPM {bpm} | beat {beat} | {} messages",
            self.target(),
            self.messages
        ));
        let a = self.colors.0.map_or("unchanged".into(), color::hex);
        let b = self.colors.1.map_or("derived from A".into(), color::hex);
        ui::label(&format!("Colors: A {a} | B {b}"));
    }
}

impl Plugin for ForgeDmxSender {
    fn initialize(&mut self, _input: TickInput) {
        if let Some(json) = bpf::load_plugin_state(bpf::PluginStateLocation::Global) {
            if let Ok(saved) = serde_json::from_str::<SaveState>(&json) {
                self.state = saved;
            }
        }
        self.host_text = self.state.host.clone();
        self.port_text = self.state.port.to_string();
        self.api_port_text = self.state.api_port.to_string();
    }

    fn run(&mut self, input: TickInput) {
        // Before UI events: combo indices refer to the current palette list.
        self.refresh_palettes();
        for ev in &input.events.events {
            if let ControlEvent::PluginUi(ui_event, plugin_id) = ev.body() {
                if plugin_id == input.id {
                    self.handle_ui(&ui_event);
                }
            }
        }

        // Track always so beat ids stay current while disabled.
        let update = self.tracker.update(&input.audio_data);

        if self.state.enabled {
            let now = input.clock;
            // Beats first: they are time critical.
            if let (Some(beat), Some(bpm)) = (update.beat, update.bpm) {
                self.send("/forgedmx/beat", &[Arg::Int(beat.position as i32), Arg::Float(bpm)]);
                self.last_bpm_sent = Some(bpm);
                self.last_beat = Some(beat);
            }
            let due = self.last_sync.is_none_or(|t| now.wrapping_sub(t) >= KEEPALIVE_MS);
            // Keep-alive resends the colors too, so a restarted forgeDMX gets them.
            self.send_colors(due);
            if due {
                self.send("/forgedmx/sync", &[Arg::Str("api")]);
                self.last_sync = Some(now);
                if let Some(bpm) = update.bpm {
                    self.send_bpm(bpm);
                }
            } else if let Some(bpm) = update.bpm {
                if self.last_bpm_sent.is_none_or(|b| (b - bpm).abs() > BPM_EPSILON) {
                    self.send_bpm(bpm);
                }
            }
            self.run_check(now);
        }

        self.render_ui();
    }
}

#[cfg(not(test))]
#[no_mangle]
extern "C" fn main() {
    bpf::hook_plugin(Box::new(ForgeDmxSender::default()));
}
