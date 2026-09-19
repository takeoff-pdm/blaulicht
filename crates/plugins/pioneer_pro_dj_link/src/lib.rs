//! Pioneer Pro DJ Link plugin, built on the `prolink` crate.
//!
//! The plugin binds UDP 50000-50002 and tracks every CDJ / XDJ / DJM on the
//! LINK network. Beat packets are broadcast, so listening alone yields beats
//! and pitch-adjusted tempo without transmitting anything. Detailed status
//! (play state and master) requires explicitly joining as a virtual CDJ.
//! Startup, reload and Rebind always return to passive listening.
//!
//! The interface to join through is detected automatically (via `ip -j addr`
//! on the host); IP and MAC can still be entered by hand.

mod iface;
mod transport;

use std::collections::BTreeMap;
use std::net::Ipv4Addr;

use blaulicht_plugin_framework as bpf;
use blaulicht_plugin_framework::{ui, Plugin};
use blaulicht_shared::{ControlEvent, LogLevel, PluginUiEvent, TickInput};
use iface::{choose_iface, discover_ifaces, Iface};
use prolink::packet::StateFlags;
use prolink::prelude::*;
use serde::{Deserialize, Serialize};
use transport::{BlaulichtTransport, Identity};

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
const CHK_AUTO_IDENTITY: u8 = 11;
const BTN_REFRESH_IFACE: u8 = 12;
const CHK_AUDIO_SYNC: u8 = 13;

const MAX_LOG_LINES: usize = 12;
const PANEL_MAX_WIDTH: i32 = 360;
/// Small strip of four boxes; capped so the host cannot stretch it.
const BEAT_CANVAS_MAX_WIDTH: i32 = 160;
/// Prefix length assumed for a manually entered address.
const MANUAL_PREFIX_LEN: u8 = 24;

fn default_true() -> bool {
    true
}

#[derive(Serialize, Deserialize)]
struct SaveState {
    log_beats: bool,
    #[serde(default)]
    audio_sync: bool,
    /// Interface address used when joining by hand (`auto_identity == false`).
    ip: String,
    mac: String,
    // Legacy `auto_join` settings are deliberately ignored. Joining is
    // session-only and must always be requested explicitly.
    /// Pick the LINK interface from the host's interface list.
    #[serde(default = "default_true")]
    auto_identity: bool,
}

impl Default for SaveState {
    fn default() -> Self {
        Self {
            log_beats: false,
            audio_sync: false,
            ip: String::new(),
            mac: String::new(),
            auto_identity: true,
        }
    }
}

/// What a player told us in its last beat packet.
#[derive(Default, Clone, Copy)]
struct BeatMark {
    /// Engine clock at which the beat arrived.
    at: Millis,
    beat: u8,
    next_beat_ms: u32,
    /// Effective tempo (pitch applied), straight from the beat packet.
    bpm: Option<Bpm>,
    pitch: Pitch,
}

/// Everything the tempo header shows.
struct TempoView {
    player_number: u8,
    model: String,
    bpm: Bpm,
    pitch: Pitch,
    master: bool,
    /// No status packet from this player; tempo comes from beats alone.
    beats_only: bool,
}

struct ProDjLinkPlugin {
    state: SaveState,
    session: Session<BlaulichtTransport>,
    bind_errors: Vec<String>,
    beats: BTreeMap<u8, BeatMark>,
    received_beats: Vec<(u8, f32, u8)>,
    sync_player: Option<u8>,
    /// Engine clock of the last status packet per player.
    last_status: BTreeMap<u8, Millis>,
    log: Vec<String>,
    packet_count: u64,
    /// Monotonic milliseconds; the engine clock is u32 and wraps.
    now: Millis,
    last_clock: u32,
    ip_text: String,
    mac_text: String,
    join_error: String,
    ifaces: Vec<Iface>,
    active_iface: Option<String>,
    selected_identity: Option<Identity>,
    identity_error: String,
}

impl ProDjLinkPlugin {
    fn new() -> Self {
        let (transport, bind_errors) = BlaulichtTransport::bind();
        Self {
            state: SaveState::default(),
            session: Session::new(transport, Self::config()),
            bind_errors,
            beats: BTreeMap::new(),
            received_beats: Vec::new(),
            sync_player: None,
            last_status: BTreeMap::new(),
            log: Vec::new(),
            packet_count: 0,
            now: Millis::ZERO,
            last_clock: 0,
            ip_text: String::new(),
            mac_text: String::new(),
            join_error: String::new(),
            ifaces: Vec::new(),
            active_iface: None,
            selected_identity: None,
            identity_error: String::new(),
        }
    }

    fn config() -> VcdjConfig {
        // Even after an explicit Join, only announce our presence to receive
        // status. Do not publish simulated CDJ playback/status packets.
        VcdjConfig::new("blaulicht").without_status_publishing()
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

    fn is_joined(&self) -> bool {
        !matches!(self.session.state(), VcdjState::Idle | VcdjState::Failed(_))
    }

    /// Re-binds the ports. The session is rebuilt because it owns the
    /// transport; devices are re-learned from the next keepalives.
    fn rebind(&mut self) {
        let (transport, errors) = BlaulichtTransport::bind();
        self.bind_errors = errors;
        self.session = Session::new(transport, Self::config());
        self.beats.clear();
        self.last_status.clear();
        if self.bind_errors.is_empty() {
            self.log_msg("Listening on UDP 50000-50002");
        } else {
            for err in self.bind_errors.clone() {
                bpf::bl_log(&format!("Pro DJ Link bind failed: {err}"), LogLevel::Err);
                self.log_msg(&format!("Bind failed: {err}"));
            }
        }
        self.sync_player = None;
        self.log_msg("Passive listening; Join is required to transmit again.");
    }

    fn parse_identity(&self) -> Result<Identity, String> {
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
        // If the address belongs to a known interface, reuse its real subnet.
        let known = self.ifaces.iter().find(|i| i.identity.ip == ip);
        Ok(match known {
            Some(i) => Identity { mac, ..i.identity },
            None => Identity::new(ip, MANUAL_PREFIX_LEN, mac, None),
        })
    }

    /// The only entry point that opts into network transmission.
    fn join(&mut self) {
        if self.is_joined() {
            return;
        }
        let identity = if self.state.auto_identity {
            self.refresh_identity();
            let Some(identity) = self.selected_identity else {
                self.join_error = self.identity_error.clone();
                return;
            };
            identity
        } else {
            match self.parse_identity() {
                Ok(identity) => identity,
                Err(error) => {
                    self.join_error = error;
                    return;
                }
            }
        };
        self.state.ip = self.ip_text.trim().to_string();
        self.state.mac = self.mac_text.trim().to_string();
        self.save();
        self.join_with(identity);
    }

    /// Rebuilds the session with `identity` and starts claiming a number.
    fn join_with(&mut self, identity: Identity) {
        self.join_error.clear();
        // The session copies the identity at construction time, so rebuild it
        // with the transport now reporting the configured address.
        let mut old = std::mem::replace(
            &mut self.session,
            Session::new(BlaulichtTransport::detached(), Self::config()),
        );
        old.stop();
        let mut transport = old.into_transport();
        transport.set_identity(Some(identity));
        transport.set_transmit_enabled(true);
        self.session = Session::new(transport, Self::config());

        match self.session.start(self.now) {
            Ok(()) => self.log_msg(&format!("Joining as virtual CDJ from {identity}")),
            Err(e) => {
                self.session.transport_mut().set_transmit_enabled(false);
                self.join_error = format!("{e}");
            }
        }
    }

    fn leave(&mut self) {
        self.session.stop();
        self.session.transport_mut().set_transmit_enabled(false);
        self.last_status.clear();
        self.sync_player = None;
        self.log_msg("Passive listening; no outgoing LINK packets.");
    }

    /// Selects an interface for a future Join; discovery never transmits.
    fn refresh_identity(&mut self) {
        if !self.state.auto_identity {
            return;
        }
        self.ifaces = discover_ifaces();

        let device_ips: Vec<Ipv4Addr> = self.session.devices().map(|d| d.ip).collect();
        let Some(chosen) = choose_iface(&self.ifaces, &device_ips).cloned() else {
            self.selected_identity = None;
            self.active_iface = None;
            self.identity_error = "no ethernet interface with an IPv4 address".to_string();
            return;
        };
        let stranded = device_ips
            .iter()
            .find(|ip| !self.ifaces.iter().any(|i| i.identity.contains(**ip)));
        self.identity_error = match stranded {
            Some(ip) => format!("player at {ip} is on no local subnet"),
            None => String::new(),
        };

        self.active_iface = Some(chosen.name.clone());
        self.selected_identity = Some(chosen.identity);
        self.ip_text = chosen.identity.ip.to_string();
        self.mac_text = chosen.identity.mac.to_string();
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
            Event::DeviceAppeared {
                player_number,
                device_type,
                model,
                ip,
                ..
            } => {
                let device_type = classify_device(model.as_ref(), device_type);
                let msg = format!("Found {model} #{player_number} ({device_type:?}) at {ip}");
                bpf::bl_log(&msg, LogLevel::Info);
                self.log_msg(&msg);
            }
            Event::DeviceLost { player_number } => {
                self.beats.remove(&player_number);
                self.last_status.remove(&player_number);
                self.log_msg(&format!("Lost player {player_number}"));
            }
            Event::DeviceStatus { player_number, .. } => {
                self.last_status.insert(player_number, self.now);
            }
            Event::MasterChanged { player_number } => {
                self.log_msg(&format!("Player {player_number} is now tempo master"));
            }
            Event::TrackLoaded {
                player_number,
                track_id,
                ..
            } => {
                self.log_msg(&format!("Player {player_number} loaded track {track_id}"));
            }
            Event::Beat {
                player_number,
                beat,
                bpm,
                pitch,
                next_beat_ms,
            } => {
                if let Some(bpm) = bpm {
                    self.received_beats.push((player_number, bpm.0, beat));
                }
                self.beats.insert(
                    player_number,
                    BeatMark {
                        at: self.now,
                        beat,
                        next_beat_ms,
                        bpm,
                        pitch,
                    },
                );
                if self.state.log_beats {
                    let bpm = bpm.map(|b| format!("{b}")).unwrap_or_else(|| "--".into());
                    self.log_msg(&format!(
                        "#{player_number} beat {beat}/4  {bpm} BPM  {pitch}"
                    ));
                }
            }
            Event::EventsDropped { count } => {
                bpf::bl_log(
                    &format!("Pro DJ Link: {count} events dropped"),
                    LogLevel::Warn,
                );
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
                BTN_REFRESH_IFACE => self.refresh_identity(),
                _ => {}
            },
            PluginUiEvent::Checkbox { id, checked } => match *id {
                CHK_AUDIO_SYNC => {
                    self.state.audio_sync = *checked;
                    self.save();
                }
                CHK_LOG_BEATS => {
                    self.state.log_beats = *checked;
                    self.save();
                }
                CHK_AUTO_IDENTITY => {
                    self.leave();
                    self.state.auto_identity = *checked;
                    self.save();
                    self.active_iface = None;
                    self.selected_identity = None;
                    if *checked {
                        self.refresh_identity();
                    }
                }
                _ => {}
            },
            PluginUiEvent::Text { id, text } => match *id {
                TXT_IP => self.ip_text = text.clone(),
                TXT_MAC => self.mac_text = text.clone(),
                _ => {}
            },
            _ => {}
        }
    }

    /// Prefer a playing master, then a stable CDJ source, then mixer beats.
    /// Every choice requires fresh beat packets; master status alone is never
    /// enough to hold the source after that deck stops.
    fn tempo_view(&self) -> Option<TempoView> {
        // DeviceStatus events report changes, not every status datagram, so
        // their age is not a reliable liveness timeout. Require fresh beats
        // below, and use status only while joined.
        let has_status = |player: u8| self.is_joined() && self.last_status.contains_key(&player);
        let master = self
            .session
            .master()
            .filter(|d| d.is_master())
            .map(|d| d.player_number);
        let sources = self.beats.iter().filter_map(|(&player_number, &mark)| {
            let device = self.session.device(player_number);
            let live_status = has_status(player_number);
            // A fresh pause/stop report takes effect immediately. Otherwise
            // beat freshness is the authority, including in passive mode.
            if live_status
                && self
                    .last_status
                    .get(&player_number)
                    .is_some_and(|at| at.0 >= mark.at.0)
                && device
                    .and_then(|d| d.player)
                    .is_some_and(|p| !p.play_state.is_playing())
            {
                return None;
            }
            Some(BeatSource {
                player_number,
                mark,
                mixer: device.is_some_and(|d| {
                    classify_device(d.model.as_ref(), d.device_type) == DeviceType::Djm
                }),
                master: live_status && master == Some(player_number),
            })
        });
        let source = select_beat_source(sources, self.now, self.sync_player)?;
        Some(TempoView {
            player_number: source.player_number,
            model: self
                .session
                .device(source.player_number)
                .map(|d| d.model.to_string())
                .unwrap_or_else(|| "player".to_string()),
            bpm: source.mark.bpm?,
            pitch: source.mark.pitch,
            master: source.master,
            beats_only: !has_status(source.player_number),
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
            ui::label(&format!(
                "{} | {} packets",
                if self.is_joined() {
                    "Active LINK"
                } else {
                    "Passive listening"
                },
                self.packet_count,
            ));
        } else {
            ui::label(&format!("ERROR: {}", self.bind_errors.join(", ")));
        }
        ui::button("Rebind", BTN_REBIND);
        ui::checkbox("Log beats", CHK_LOG_BEATS, self.state.log_beats);
        ui::end_horizontal();

        ui::checkbox(
            "Sync audio BPM / beats",
            CHK_AUDIO_SYNC,
            self.state.audio_sync,
        );
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
        let Some(view) = self.tempo_view() else {
            ui::label("No player is sending a tempo.");
            return;
        };
        let mark = self
            .beats
            .get(&view.player_number)
            .copied()
            .unwrap_or_default();

        // Position inside the current beat, 0 on the beat → 1 just before the next.
        let phase = if mark.next_beat_ms > 0 {
            (self.now.since(mark.at) as f32 / mark.next_beat_ms as f32).clamp(0.0, 1.0)
        } else {
            0.0
        };

        ui::begin_horizontal();
        ui::label_styled(&format!("{:.1} BPM", view.bpm.0), 28, true);
        ui::begin_vertical();
        ui::label(&format!(
            "{} #{}{}{}",
            view.model,
            view.player_number,
            if view.master { " (master)" } else { "" },
            if view.beats_only { " (beats only)" } else { "" },
        ));
        ui::label(&format!("beat {}/4, pitch {}", mark.beat, view.pitch));
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
            let brightness = if active {
                255 - (phase * 180.0) as u8
            } else {
                50
            };
            let (r, g, b) = if i == 0 {
                (brightness, 60, 60)
            } else {
                (brightness, brightness, brightness)
            };
            ui::painter_rect(x, 0, box_w, H, r, g, b, 255);
        }
        ui::painter_end();
        ui::end_vertical();
    }

    fn render_devices(&self) {
        ui::begin_frame_styled(FRAME_DEVICES, "Devices", 6, 4, 0, 4);
        let mut any_player = false;
        let mut any = false;
        for d in self.session.devices() {
            any = true;
            let mut flags = Vec::new();
            let bpm = match d.player {
                Some(p) => {
                    if p.play_state.is_playing() {
                        flags.push("PLAY")
                    }
                    if p.state.contains(StateFlags::MASTER) {
                        flags.push("MASTER")
                    }
                    if p.state.contains(StateFlags::SYNC) {
                        flags.push("SYNC")
                    }
                    if p.state.contains(StateFlags::ON_AIR) {
                        flags.push("ON AIR")
                    }
                    p.effective_bpm()
                        .map(|b| format!("{:.2} BPM", b.0))
                        .unwrap_or_else(|| format!("{:?}", p.play_state))
                }
                None => match d.mixer {
                    Some(m) => {
                        if m.state.contains(StateFlags::MASTER) {
                            flags.push("MASTER")
                        }
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
            // Mixers report through a different packet; only players get the
            // "have we received status" note.
            let device_type = classify_device(d.model.as_ref(), d.device_type);
            let status = if device_type == DeviceType::Djm {
                String::new()
            } else {
                any_player = true;
                match self.last_status.get(&d.player_number) {
                    _ if !self.is_joined() => String::new(),
                    Some(t) => format!("  status {:.1}s ago", self.now.since(*t) as f32 / 1000.0),
                    None => "  status: never (not joined?)".to_string(),
                }
            };
            let kind = match device_type {
                DeviceType::Djm => "Mixer".to_string(),
                other => format!("{other:?}"),
            };
            ui::label_styled(&format!("    {kind} at {}{status}", d.ip), 11, true);
        }
        if !any {
            ui::label("None found. Is this machine on the LINK network?");
        } else if any_player && !self.is_joined() {
            ui::label("Passive mode: master/play status unavailable.");
        }
        ui::end_frame();
    }

    fn render_join(&self) {
        ui::begin_collapsing(COLLAPSE_JOIN, "Join as virtual CDJ", false);
        let state = self.session.state();
        match state {
            VcdjState::Idle => ui::label("Passive: receives beats/BPM, sends no LINK packets."),
            VcdjState::Failed(reason) => ui::label(&format!("Join failed: {reason:?}")),
            s if s.is_online() => ui::label(&format!(
                "Online as player {}",
                s.player_number().unwrap_or(0)
            )),
            _ => ui::label("Negotiating player number..."),
        }
        ui::checkbox(
            "Auto-detect interface",
            CHK_AUTO_IDENTITY,
            self.state.auto_identity,
        );
        if self.state.auto_identity {
            ui::begin_horizontal();
            match (&self.active_iface, self.selected_identity) {
                (Some(name), Some(id)) => ui::label(&format!("{name}: {id}")),
                _ => ui::label("No interface selected yet."),
            }
            ui::button("Refresh", BTN_REFRESH_IFACE);
            ui::end_horizontal();
            if !self.identity_error.is_empty() {
                ui::label(&format!("WARNING: {}", self.identity_error));
            }
        } else {
            ui::begin_horizontal();
            ui::text_edit("IP", TXT_IP, &self.ip_text);
            ui::text_edit("MAC", TXT_MAC, &self.mac_text);
            ui::end_horizontal();
        }
        ui::begin_horizontal();
        ui::button_styled("Join (sends to CDJs)", BTN_JOIN, !self.is_joined());
        ui::button_styled("Leave", BTN_LEAVE, self.is_joined());
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
        self.log_msg("Passive listening; Join is optional for detailed status.");
    }

    fn run(&mut self, input: TickInput) {
        self.advance_clock(input.clock);
        self.received_beats.clear();

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

        // Use only beat packets from the same source as the tempo header.
        // Do not renew the host lease from stale status or synthesize beats.
        if self.state.audio_sync {
            if let Some(view) = self.tempo_view() {
                self.sync_player = Some(view.player_number);
                for &(player, bpm, beat) in &self.received_beats {
                    if player == view.player_number {
                        bpf::audio_tempo_bar(Some(bpm), true, Some(beat), player);
                    }
                }
            } else {
                bpf::audio_tempo(None, false);
            }
        } else {
            self.sync_player = None;
            bpf::audio_tempo(None, false);
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

// The DJM-900nexus announces the player type on this LINK network. Its
// explicit model name is a stronger identity hint than that shared wire byte.
fn classify_device(model: &str, reported: DeviceType) -> DeviceType {
    if model
        .get(..4)
        .is_some_and(|prefix| prefix.eq_ignore_ascii_case("DJM-"))
    {
        DeviceType::Djm
    } else {
        reported
    }
}

#[derive(Clone, Copy)]
struct BeatSource {
    player_number: u8,
    mark: BeatMark,
    mixer: bool,
    master: bool,
}

// Stay on one active deck, but do not let the mixer's continuous beat stream
// prevent a CDJ from taking over. A silent or invalid master never blocks one.
fn select_beat_source(
    sources: impl Iterator<Item = BeatSource>,
    now: Millis,
    preferred: Option<u8>,
) -> Option<BeatSource> {
    sources
        .filter(|source| {
            let Some(Bpm(bpm)) = source.mark.bpm else {
                return false;
            };
            // Match the host's supported tempos and lease length. next_beat_ms can
            // contain the protocol's 0xffffffff sentinel near the end of a track.
            bpm.is_finite()
                && (20.0..=400.0).contains(&bpm)
                && now.since(source.mark.at) <= (180_000.0 / bpm).clamp(250.0, 3_000.0) as u64
        })
        .max_by_key(|source| {
            (
                source.master,
                !source.mixer,
                Some(source.player_number) == preferred,
                source.mark.at.0,
            )
        })
}

#[cfg(test)]
mod sync_tests {
    use super::*;

    fn source(player_number: u8, at: u64, bpm: f32) -> BeatSource {
        BeatSource {
            player_number,
            mark: BeatMark {
                at: Millis(at),
                bpm: Some(Bpm(bpm)),
                ..Default::default()
            },
            mixer: false,
            master: false,
        }
    }

    #[test]
    fn beats_only_sync_keeps_one_deck_until_it_stops() {
        let sources = [source(1, 100, 120.0), source(2, 300, 130.0)];
        assert_eq!(
            select_beat_source(sources.into_iter(), Millis(300), None)
                .unwrap()
                .player_number,
            2
        );
        assert_eq!(
            select_beat_source(sources.into_iter(), Millis(300), Some(1))
                .unwrap()
                .player_number,
            1
        );
        assert_eq!(
            select_beat_source(sources.into_iter(), Millis(1650), Some(1))
                .unwrap()
                .player_number,
            2
        );
        assert!(select_beat_source(sources.into_iter(), Millis(2400), Some(2)).is_none());
    }

    #[test]
    fn cdj_takes_over_from_a_continuously_beating_mixer_without_a_master() {
        let mixer = BeatSource {
            mixer: true,
            ..source(33, 400, 128.0)
        };
        let cdj = source(2, 300, 120.0);
        assert_eq!(
            select_beat_source([mixer, cdj].into_iter(), Millis(400), Some(33))
                .unwrap()
                .player_number,
            2
        );
        assert_eq!(
            select_beat_source([mixer].into_iter(), Millis(400), Some(2))
                .unwrap()
                .player_number,
            33
        );
    }

    #[test]
    fn only_a_fresh_valid_master_has_priority() {
        let mut master = BeatSource {
            master: true,
            ..source(1, 100, 120.0)
        };
        let cdj = source(2, 1500, 125.0);
        assert_eq!(
            select_beat_source([master, cdj].into_iter(), Millis(1500), Some(2))
                .unwrap()
                .player_number,
            1
        );
        // The next-beat sentinel must not keep a stopped master alive.
        master.mark.next_beat_ms = u32::MAX;
        assert_eq!(
            select_beat_source([master, cdj].into_iter(), Millis(2000), Some(1))
                .unwrap()
                .player_number,
            2
        );
        for bpm in [0.0, f32::NAN, f32::INFINITY] {
            master.mark.bpm = Some(Bpm(bpm));
            assert_eq!(
                select_beat_source([master, cdj].into_iter(), Millis(1500), Some(1))
                    .unwrap()
                    .player_number,
                2
            );
        }
    }

    #[test]
    fn djm_model_overrides_player_announcement_without_reclassifying_players() {
        assert_eq!(
            classify_device("DJM-900nexus", DeviceType::Cdj),
            DeviceType::Djm
        );
        assert_eq!(
            classify_device("djm-900nxs2", DeviceType::Cdj),
            DeviceType::Djm
        );
        assert_eq!(
            classify_device("CDJ-2000nexus", DeviceType::Cdj),
            DeviceType::Cdj
        );
        assert_eq!(
            classify_device("CDJ-3000", DeviceType::Rekordbox),
            DeviceType::Rekordbox
        );
        assert_eq!(classify_device("Unknown", DeviceType::Djm), DeviceType::Djm);
    }

    #[test]
    fn explicit_join_does_not_publish_cdj_status() {
        let config = ProDjLinkPlugin::config();
        assert!(!config.publish_status);
        assert!(!config.unicast_keepalives);
    }

    #[test]
    fn old_auto_join_settings_are_not_restored_or_saved() {
        let state: SaveState = serde_json::from_str(
            r#"{"log_beats":false,"audio_sync":true,"ip":"10.10.25.95","mac":"38:f3:ab:bc:44:e1","auto_join":true,"auto_identity":true}"#,
        ).unwrap();
        let saved = serde_json::to_value(state).unwrap();
        assert!(saved.get("auto_join").is_none());
        assert_eq!(saved["auto_identity"], true);
        assert_eq!(saved["audio_sync"], true);
    }

    #[test]
    fn existing_saved_settings_leave_audio_sync_disabled() {
        let state: SaveState =
            serde_json::from_str(r#"{"log_beats":false,"ip":"","mac":"","auto_join":true}"#)
                .unwrap();
        assert!(!state.audio_sync);
    }
}
