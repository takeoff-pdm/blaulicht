//! Pioneer Pro DJ Link plugin, built on the `prolink` crate.
//!
//! By default the plugin only listens: it binds UDP 50000-50002, tracks every
//! CDJ / XDJ / DJM on the LINK network and shows tempo, beat position and
//! play / master / sync / on-air state in the plugin UI.
//!
//! Optionally it can join the network as a virtual CDJ. That requires the IP
//! and MAC address of the interface connected to the players, because a WASM
//! plugin cannot discover them itself. Joining is what allows later features
//! such as metadata queries; for tempo and beats listening is enough.

mod transport;

use std::collections::BTreeMap;
use std::net::Ipv4Addr;

use blaulicht_plugin_framework as bpf;
use blaulicht_plugin_framework::{ui, Plugin};
use blaulicht_shared::{ControlEvent, LogLevel, PluginUiEvent, TickInput};
use prolink::packet::StateFlags;
use prolink::prelude::*;
use serde::{Deserialize, Serialize};
use transport::BlaulichtTransport;

const BTN_REBIND: u8 = 1;
const CHK_LOG_BEATS: u8 = 2;
const PAINTER_BEAT: u8 = 3;
const FRAME_DEVICES: u8 = 4;
const FRAME_LOG: u8 = 5;
const COLLAPSE_JOIN: u8 = 6;
const TXT_IP: u8 = 7;
const TXT_MAC: u8 = 8;
const BTN_JOIN: u8 = 9;
const BTN_LEAVE: u8 = 10;

const MAX_LOG_LINES: usize = 12;
const PANEL_MAX_WIDTH: i32 = 360;
/// Small strip of four boxes; capped so the host cannot stretch it.
const BEAT_CANVAS_MAX_WIDTH: i32 = 160;

#[derive(Serialize, Deserialize, Default)]
struct SaveState {
    log_beats: bool,
    /// Interface address used when joining as a virtual CDJ.
    ip: String,
    mac: String,
    /// Whether to join automatically on startup.
    auto_join: bool,
}

/// Engine clock (ms) at which each player last crossed a beat, for the UI.
#[derive(Default, Clone, Copy)]
struct BeatMark {
    at: Millis,
    beat: u8,
    next_beat_ms: u32,
}

struct ProDjLinkPlugin {
    state: SaveState,
    session: Session<BlaulichtTransport>,
    bind_errors: Vec<String>,
    beats: BTreeMap<u8, BeatMark>,
    log: Vec<String>,
    packet_count: u64,
    /// Monotonic milliseconds; the engine clock is u32 and wraps.
    now: Millis,
    last_clock: u32,
    ip_text: String,
    mac_text: String,
    join_error: String,
}

impl ProDjLinkPlugin {
    fn new() -> Self {
        let (transport, bind_errors) = BlaulichtTransport::bind();
        Self {
            state: SaveState::default(),
            session: Session::new(transport, Self::config()),
            bind_errors,
            beats: BTreeMap::new(),
            log: Vec::new(),
            packet_count: 0,
            now: Millis::ZERO,
            last_clock: 0,
            ip_text: String::new(),
            mac_text: String::new(),
            join_error: String::new(),
        }
    }

    fn config() -> VcdjConfig {
        VcdjConfig::new("blaulicht")
    }

    fn save(&self) {
        if let Ok(json) = serde_json::to_string(&self.state) {
            bpf::save_plugin_state(bpf::PluginStateLocation::Global, &json);
        }
    }

    fn advance_clock(&mut self, clock: u32) {
        let delta = clock.wrapping_sub(self.last_clock);
        self.last_clock = clock;
        self.now = self.now.add(delta as u64);
    }

    fn log_msg(&mut self, msg: &str) {
        if self.log.len() >= MAX_LOG_LINES {
            self.log.remove(0);
        }
        self.log.push(msg.to_string());
    }

    /// Re-binds the ports. The session is rebuilt because it owns the
    /// transport; devices are re-learned from the next keepalives.
    fn rebind(&mut self) {
        let was_online = self.session.state() != VcdjState::Idle;
        let (transport, errors) = BlaulichtTransport::bind();
        self.bind_errors = errors;
        self.session = Session::new(transport, Self::config());
        self.beats.clear();
        if self.bind_errors.is_empty() {
            self.log_msg("Listening on UDP 50000-50002");
        } else {
            for err in self.bind_errors.clone() {
                bpf::bl_log(&format!("Pro DJ Link bind failed: {err}"), LogLevel::Err);
                self.log_msg(&format!("Bind failed: {err}"));
            }
        }
        if was_online {
            self.join();
        }
    }

    fn parse_identity(&self) -> Result<(Ipv4Addr, MacAddr), String> {
        let ip = self
            .ip_text
            .trim()
            .parse::<Ipv4Addr>()
            .map_err(|_| "invalid IPv4 address".to_string())?;
        let mac = self
            .mac_text
            .trim()
            .parse::<MacAddr>()
            .map_err(|e| format!("invalid MAC address: {e}"))?;
        Ok((ip, mac))
    }

    fn join(&mut self) {
        let identity = match self.parse_identity() {
            Ok(id) => id,
            Err(e) => {
                self.join_error = e;
                return;
            }
        };
        self.join_error.clear();
        self.state.ip = self.ip_text.trim().to_string();
        self.state.mac = self.mac_text.trim().to_string();
        self.state.auto_join = true;
        self.save();

        // The session copies the identity at construction time, so rebuild it
        // with the transport now reporting the configured address.
        let mut old = std::mem::replace(
            &mut self.session,
            Session::new(BlaulichtTransport::detached(), Self::config()),
        );
        old.stop();
        let mut transport = old.into_transport();
        transport.set_identity(Some(identity));
        self.session = Session::new(transport, Self::config());

        match self.session.start(self.now) {
            Ok(()) => self.log_msg("Joining network as virtual CDJ"),
            Err(e) => self.join_error = format!("{e}"),
        }
    }

    fn leave(&mut self) {
        self.session.stop();
        self.state.auto_join = false;
        self.save();
        self.log_msg("Left the network (listen only)");
    }

    fn handle_event(&mut self, event: Event) {
        match event {
            Event::Online { player_number } => {
                let msg = format!("Online as player {player_number}");
                bpf::bl_log(&msg, LogLevel::Info);
                self.log_msg(&msg);
            }
            Event::NegotiationFailed(reason) => {
                self.join_error = format!("{reason:?}");
                self.log_msg(&format!("Could not join: {reason:?}"));
            }
            Event::DeviceAppeared { player_number, device_type, model, ip, .. } => {
                let msg = format!("Found {model} #{player_number} ({device_type:?}) at {ip}");
                bpf::bl_log(&msg, LogLevel::Info);
                self.log_msg(&msg);
            }
            Event::DeviceLost { player_number } => {
                self.beats.remove(&player_number);
                self.log_msg(&format!("Lost player {player_number}"));
            }
            Event::MasterChanged { player_number } => {
                self.log_msg(&format!("Player {player_number} is now tempo master"));
            }
            Event::TrackLoaded { player_number, track_id, .. } => {
                self.log_msg(&format!("Player {player_number} loaded track {track_id}"));
            }
            Event::Beat { player_number, beat, bpm, pitch, next_beat_ms } => {
                self.beats.insert(
                    player_number,
                    BeatMark { at: self.now, beat, next_beat_ms },
                );
                if self.state.log_beats {
                    let bpm = bpm.map(|b| format!("{b}")).unwrap_or_else(|| "--".into());
                    self.log_msg(&format!("#{player_number} beat {beat}/4  {bpm} BPM  {pitch}"));
                }
            }
            Event::EventsDropped { count } => {
                bpf::bl_log(&format!("Pro DJ Link: {count} events dropped"), LogLevel::Warn);
            }
            _ => {}
        }
    }

    fn handle_ui_event(&mut self, event: &PluginUiEvent, plugin_id: u8, my_id: u8) {
        if plugin_id != my_id {
            return;
        }
        match event {
            PluginUiEvent::Button { id } => match *id {
                BTN_REBIND => self.rebind(),
                BTN_JOIN => self.join(),
                BTN_LEAVE => self.leave(),
                _ => {}
            },
            PluginUiEvent::Checkbox { id, checked } if *id == CHK_LOG_BEATS => {
                self.state.log_beats = *checked;
                self.save();
            }
            PluginUiEvent::Text { id, text } => match *id {
                TXT_IP => self.ip_text = text.clone(),
                TXT_MAC => self.mac_text = text.clone(),
                _ => {}
            },
            _ => {}
        }
    }

    /// The device whose tempo the lights should follow: the tempo master, or
    /// failing that any playing deck with a known BPM.
    fn tempo_source(&self) -> Option<&Device> {
        self.session
            .master()
            .filter(|d| d.effective_bpm().is_some())
            .or_else(|| {
                self.session.devices().find(|d| {
                    d.player.is_some_and(|p| p.play_state.is_playing()) && d.effective_bpm().is_some()
                })
            })
    }

    fn render_ui(&self) {
        ui::begin();
        // The host renders plugin windows in a scroll area that scrolls both
        // ways, so separators and frames expand past the visible width once a
        // vertical scrollbar appears. Bounding the root keeps everything inside.
        ui::set_max_width(PANEL_MAX_WIDTH);

        ui::begin_horizontal();
        if self.bind_errors.is_empty() {
            ui::label(&format!("Listening | {} packets", self.packet_count));
        } else {
            ui::label(&format!("ERROR: {}", self.bind_errors.join(", ")));
        }
        ui::button("Rebind", BTN_REBIND);
        ui::checkbox("Log beats", CHK_LOG_BEATS, self.state.log_beats);
        ui::end_horizontal();

        self.render_tempo();
        self.render_devices();
        self.render_join();

        ui::begin_frame_styled(FRAME_LOG, "Log", 6, 4, 0, 4);
        for line in self.log.iter().rev() {
            ui::label(line);
        }
        ui::end_frame();
    }

    fn render_tempo(&self) {
        let Some(device) = self.tempo_source() else {
            ui::label("No player is sending a tempo.");
            return;
        };
        let bpm = device.effective_bpm().expect("filtered above");
        let mark = self.beats.get(&device.player_number).copied().unwrap_or_default();
        let pitch = device.player.map(|p| p.actual_pitch).unwrap_or_default();

        // Position inside the current beat, 0 on the beat → 1 just before the next.
        let phase = if mark.next_beat_ms > 0 {
            (self.now.since(mark.at) as f32 / mark.next_beat_ms as f32).clamp(0.0, 1.0)
        } else {
            0.0
        };

        ui::begin_horizontal();
        ui::label_styled(&format!("{:.1} BPM", bpm.0), 28, true);
        ui::begin_vertical();
        ui::label(&format!(
            "{} #{}{}",
            device.model,
            device.player_number,
            if device.is_master() { " (master)" } else { "" }
        ));
        ui::label(&format!("beat {}/4, pitch {pitch}", mark.beat));
        ui::end_vertical();
        ui::end_horizontal();

        const W: i32 = BEAT_CANVAS_MAX_WIDTH;
        const H: i32 = 12;
        const GAP: i32 = 4;
        let box_w = (W - 3 * GAP) / 4;
        // The host stretches a canvas to the full available width; cap it so
        // the strip stays small.
        ui::begin_vertical();
        ui::set_max_width(BEAT_CANVAS_MAX_WIDTH);
        ui::painter_begin(PAINTER_BEAT, W, H);
        for i in 0..4u8 {
            let x = i as i32 * (box_w + GAP);
            let active = mark.beat == i + 1;
            let brightness = if active { 255 - (phase * 180.0) as u8 } else { 50 };
            let (r, g, b) = if i == 0 { (brightness, 60, 60) } else { (brightness, brightness, brightness) };
            ui::painter_rect(x, 0, box_w, H, r, g, b, 255);
        }
        ui::painter_end();
        ui::end_vertical();
    }

    fn render_devices(&self) {
        ui::begin_frame_styled(FRAME_DEVICES, "Devices", 6, 4, 0, 4);
        let mut any = false;
        for d in self.session.devices() {
            any = true;
            let mut flags = Vec::new();
            let bpm = match d.player {
                Some(p) => {
                    if p.play_state.is_playing() { flags.push("PLAY") }
                    if p.state.contains(StateFlags::MASTER) { flags.push("MASTER") }
                    if p.state.contains(StateFlags::SYNC) { flags.push("SYNC") }
                    if p.state.contains(StateFlags::ON_AIR) { flags.push("ON AIR") }
                    p.effective_bpm().map(|b| format!("{:.2} BPM", b.0)).unwrap_or_else(|| format!("{:?}", p.play_state))
                }
                None => match d.mixer {
                    Some(m) => {
                        if m.state.contains(StateFlags::MASTER) { flags.push("MASTER") }
                        m.bpm.map(|b| format!("{:.2} BPM", b.0)).unwrap_or_default()
                    }
                    None => String::new(),
                },
            };
            ui::label_styled(
                &format!(
                    "#{:<2} {:<14} {bpm:>10}  {}",
                    d.player_number,
                    d.model,
                    flags.join(" ")
                ),
                13,
                true,
            );
            ui::label_styled(&format!("    {:?} at {}", d.device_type, d.ip), 11, true);
        }
        if !any {
            ui::label("None found. Is this machine on the LINK network?");
        }
        ui::end_frame();
    }

    fn render_join(&self) {
        ui::begin_collapsing(COLLAPSE_JOIN, "Join as virtual CDJ", false);
        let state = self.session.state();
        match state {
            VcdjState::Idle => ui::label("Listen only. Enter the LINK interface address to join."),
            s if s.is_online() => ui::label(&format!(
                "Online as player {}",
                s.player_number().unwrap_or(0)
            )),
            _ => ui::label("Negotiating player number..."),
        }
        ui::begin_horizontal();
        ui::text_edit("IP", TXT_IP, &self.ip_text);
        ui::text_edit("MAC", TXT_MAC, &self.mac_text);
        ui::end_horizontal();
        ui::begin_horizontal();
        ui::button_styled("Join", BTN_JOIN, state == VcdjState::Idle);
        ui::button_styled("Leave", BTN_LEAVE, state != VcdjState::Idle);
        ui::end_horizontal();
        if !self.join_error.is_empty() {
            ui::label(&format!("ERROR: {}", self.join_error));
        }
        ui::end_collapsing();
    }
}

impl Plugin for ProDjLinkPlugin {
    fn initialize(&mut self, input: TickInput) {
        self.last_clock = input.clock;
        if let Some(json) = bpf::load_plugin_state(bpf::PluginStateLocation::Global) {
            if let Ok(saved) = serde_json::from_str::<SaveState>(&json) {
                self.state = saved;
                self.ip_text = self.state.ip.clone();
                self.mac_text = self.state.mac.clone();
            }
        }
        if self.bind_errors.is_empty() {
            self.log_msg("Listening on UDP 50000-50002");
        } else {
            for err in self.bind_errors.clone() {
                bpf::bl_log(&format!("Pro DJ Link bind failed: {err}"), LogLevel::Err);
                self.log_msg(&format!("Bind failed: {err}"));
            }
        }
        if self.state.auto_join && !self.ip_text.is_empty() {
            self.join();
        }
    }

    fn run(&mut self, input: TickInput) {
        self.advance_clock(input.clock);

        for ev in &input.events.events {
            if let ControlEvent::PluginUi(ui_event, plugin_id) = ev.body() {
                self.handle_ui_event(&ui_event, plugin_id, input.id);
            }
        }

        self.packet_count += self.session.transport_mut().collect() as u64;
        loop {
            match self.session.poll(self.now) {
                Ok(Some(event)) => self.handle_event(event),
                Ok(None) => break,
                Err(e) => {
                    bpf::bl_log(&format!("Pro DJ Link: {e}"), LogLevel::Err);
                    break;
                }
            }
        }

        self.render_ui();
    }
}

/// Native unit tests must not export a second `main` symbol.
#[cfg(not(test))]
#[no_mangle]
extern "C" fn main() {
    bpf::hook_plugin(Box::new(ProDjLinkPlugin::new()));
}
