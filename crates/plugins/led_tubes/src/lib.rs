use blaulicht_plugin_framework as bpf;
use blaulicht_plugin_framework::{CommandPollResult, Plugin};
use blaulicht_shared::{ControlEvent, ControlEventMessage, PluginUiEvent, TickInput};
use serde::{Deserialize, Serialize};

const DEFAULT_HOST: &str = "192.168.50.19";
const STRIP_COUNT: usize = 5;
const ARTNET_PORT: u16 = 6454;

const UI_STRIP_SELECT: u8 = 1;
const UI_MODE: u8 = 2;
const UI_BRIGHTNESS: u8 = 3;
const UI_R: u8 = 4;
const UI_G: u8 = 5;
const UI_B: u8 = 6;
const UI_COUNT: u8 = 7;
const UI_INDEX: u8 = 8;
const UI_SPAN: u8 = 9;
const UI_REFRESH: u8 = 10;
const UI_ALL_OFF: u8 = 11;
const UI_APPLY: u8 = 12;
const UI_HOST: u8 = 13;
const UI_ARTNET_ENABLED: u8 = 14;

const MODES: &[&str] = &["off", "solid", "chase", "pixel", "window", "ruler"];

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
struct StripState {
    mode: u8,
    brightness: u8,
    r: u8,
    g: u8,
    b: u8,
    count: u8,
    index: u8,
    span: u8,
}

impl Default for StripState {
    fn default() -> Self {
        Self {
            mode: 0,
            brightness: 16,
            r: 255,
            g: 96,
            b: 0,
            count: 99,
            index: 0,
            span: 1,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
struct SaveState {
    host: String,
    strips: [StripState; STRIP_COUNT],
    selected_strip: u8,
    artnet_enabled: bool,
}

impl Default for SaveState {
    fn default() -> Self {
        Self {
            host: DEFAULT_HOST.to_string(),
            strips: Default::default(),
            selected_strip: 0,
            artnet_enabled: true,
        }
    }
}

enum PendingHttp {
    Refresh { handle: u32 },
    SendStrip { handle: u32, strip_id: usize },
    AllOff { handle: u32 },
}

#[derive(Default)]
pub struct LedTubesPlugin {
    state: SaveState,
    last_error: Option<String>,
    status_text: Option<String>,
    // RAII handle; drop = unregister. Reassign on host change to swap atomically.
    artnet: Option<bpf::ArtNetReceiverHandle>,
    pending: Option<PendingHttp>,
}

impl LedTubesPlugin {
    fn selected(&self) -> &StripState {
        &self.state.strips[self.state.selected_strip as usize]
    }

    fn selected_mut(&mut self) -> &mut StripState {
        &mut self.state.strips[self.state.selected_strip as usize]
    }

    fn spawn_http_get(&self, path: &str) -> u32 {
        let url = format!("http://{}{}", self.state.host, path);
        let cmd = format!("curl -sS --max-time 3 '{}'", url);
        bpf::command_spawn_bg(&cmd)
    }

    fn spawn_send_strip(&mut self, strip_id: usize) {
        let s = &self.state.strips[strip_id];
        let mode = MODES.get(s.mode as usize).unwrap_or(&"off");
        let path = format!(
            "/led?strip={}&mode={}&brightness={}&r={}&g={}&b={}&count={}&index={}&span={}",
            strip_id + 1,
            mode,
            s.brightness,
            s.r,
            s.g,
            s.b,
            s.count,
            s.index,
            s.span,
        );
        let handle = self.spawn_http_get(&path);
        self.pending = Some(PendingHttp::SendStrip { handle, strip_id });
    }

    fn spawn_all_off(&mut self) {
        let handle = self.spawn_http_get("/led?strip=all&mode=off");
        self.pending = Some(PendingHttp::AllOff { handle });
    }

    fn spawn_refresh_status(&mut self) {
        let handle = self.spawn_http_get("/status");
        self.pending = Some(PendingHttp::Refresh { handle });
    }

    fn poll_pending(&mut self) {
        let pending = self.pending.take().unwrap();
        match pending {
            PendingHttp::Refresh { handle } => self.poll_refresh(handle),
            PendingHttp::SendStrip { handle, strip_id } => self.poll_send_strip(handle, strip_id),
            PendingHttp::AllOff { handle } => self.poll_all_off(handle),
        }
    }

    fn poll_refresh(&mut self, handle: u32) {
        match bpf::command_poll_result(handle) {
            CommandPollResult::Running => {
                self.pending = Some(PendingHttp::Refresh { handle });
            }
            CommandPollResult::Finished(body) => {
                let body = body.trim().to_string();
                if body.is_empty() {
                    self.last_error = Some("no response from /status".to_string());
                } else {
                    self.status_text = Some(body.clone());
                    if let Ok(status) = serde_json::from_str::<StatusResponse>(&body) {
                        for strip in &status.strips {
                            let idx = (strip.id as usize).saturating_sub(1);
                            if idx < STRIP_COUNT {
                                let s = &mut self.state.strips[idx];
                                s.brightness = strip.brightness.clamp(0, 31) as u8;
                                s.r = strip.rgb[0].clamp(0, 255) as u8;
                                s.g = strip.rgb[1].clamp(0, 255) as u8;
                                s.b = strip.rgb[2].clamp(0, 255) as u8;
                                s.count = strip.active_segments.clamp(1, 200) as u8;
                                s.index = strip.index.clamp(0, 199) as u8;
                                s.span = strip.span.clamp(1, 20) as u8;
                                s.mode = mode_str_to_index(&strip.mode);
                            }
                        }
                        self.last_error = None;
                    } else {
                        self.last_error = Some("failed to parse /status".to_string());
                    }
                }
                self.save_state();
            }
            CommandPollResult::Failed(err) => {
                self.last_error = Some(format!("refresh failed: {err}"));
                self.save_state();
            }
        }
    }

    fn poll_send_strip(&mut self, handle: u32, strip_id: usize) {
        match bpf::command_poll_result(handle) {
            CommandPollResult::Running => {
                self.pending = Some(PendingHttp::SendStrip { handle, strip_id });
            }
            CommandPollResult::Finished(_) => {
                self.last_error = None;
                self.save_state();
            }
            CommandPollResult::Failed(err) => {
                self.last_error = Some(format!("send strip {} failed: {err}", strip_id + 1));
                self.save_state();
            }
        }
    }

    fn poll_all_off(&mut self, handle: u32) {
        match bpf::command_poll_result(handle) {
            CommandPollResult::Running => {
                self.pending = Some(PendingHttp::AllOff { handle });
            }
            CommandPollResult::Finished(_) => {
                for s in &mut self.state.strips {
                    s.mode = 0;
                }
                self.last_error = None;
                self.save_state();
            }
            CommandPollResult::Failed(err) => {
                self.last_error = Some(format!("all-off failed: {err}"));
                self.save_state();
            }
        }
    }

    /// Reconciles the in-memory Art-Net registration with the desired state
    /// (`artnet_enabled` + current `host`). Idempotent: safe to call on any change.
    fn sync_artnet(&mut self) {
        let desired = if self.state.artnet_enabled {
            Some(format!("{}:{}", self.state.host.trim(), ARTNET_PORT))
        } else {
            None
        };

        let current = self.artnet.as_ref().map(|h| h.addr().to_string());

        if current == desired {
            self.sanity_check_artnet();
            return;
        }

        // Drop the old handle first (unregister) so re-registration at the same
        // address with a different host doesn't trip the duplicate-address guard.
        self.artnet = None;

        if let Some(addr) = desired {
            match bpf::ArtNetReceiverHandle::register(&addr) {
                Some(h) => self.artnet = Some(h),
                None => self.last_error = Some(format!("artnet: register {addr} failed")),
            }
        }

        self.sanity_check_artnet();
    }

    /// Cross-checks our local view of the Art-Net registration against the
    /// host's authoritative list. Logs a diagnostic into `last_error` on
    /// mismatch — duplicate self-owned entries, stale handles, address drift,
    /// etc. Cheap; safe to run after every `sync_artnet`.
    fn sanity_check_artnet(&mut self) {
        let all = bpf::ArtNetReceiverHandle::enumerate_all();
        let my_id = unsafe { bpf::PLUGIN_ID };
        let owned: Vec<_> = all
            .iter()
            .filter(|r| r.owner_plugin_id == Some(my_id))
            .collect();

        match (&self.artnet, owned.as_slice()) {
            (None, []) => {}
            (None, extras) => {
                self.last_error = Some(format!(
                    "artnet sanity: host still holds {} self-owned receiver(s) after unregister",
                    extras.len()
                ));
            }
            (Some(_), []) => {
                self.last_error =
                    Some("artnet sanity: host has no self-owned receiver (stale handle?)".into());
            }
            (Some(h), [only]) => {
                if only.handle != h.handle() || only.address != h.addr() {
                    self.last_error = Some(format!(
                        "artnet sanity: host entry ({}, handle={}) doesn't match local ({}, handle={})",
                        only.address,
                        only.handle,
                        h.addr(),
                        h.handle()
                    ));
                }
            }
            (Some(_), extras) => {
                self.last_error = Some(format!(
                    "artnet sanity: {} self-owned receivers registered, expected 1",
                    extras.len()
                ));
            }
        }
    }

    fn save_state(&self) {
        if let Ok(json) = serde_json::to_string(&self.state) {
            bpf::save_plugin_state(blaulicht_shared::PluginStateLocation::Showfile, &json);
        }
    }

    fn load_state(&mut self) {
        if let Some(data) =
            bpf::load_plugin_state(blaulicht_shared::PluginStateLocation::Showfile)
        {
            if let Ok(saved) = serde_json::from_str::<SaveState>(&data) {
                self.state = saved;
            }
        }
    }

    fn process_events(&mut self, input: &TickInput) {
        for message in &input.events.events {
            self.dispatch_event(message, input.id);
        }
    }

    fn dispatch_event(&mut self, event: &ControlEventMessage, plugin_id: u8) {
        match event.body() {
            ControlEvent::PluginUi(ui_event, pid) if pid == plugin_id => {
                self.handle_ui_event(&ui_event);
            }
            _ => {}
        }
    }

    fn handle_ui_event(&mut self, event: &PluginUiEvent) {
        match event {
            PluginUiEvent::Slider { id, value } | PluginUiEvent::HFader { id, value } => {
                match *id {
                    UI_STRIP_SELECT => {
                        self.state.selected_strip = (*value).min((STRIP_COUNT - 1) as u8);
                    }
                    UI_MODE => {
                        self.selected_mut().mode = (*value).min((MODES.len() - 1) as u8);
                    }
                    UI_BRIGHTNESS => {
                        self.selected_mut().brightness = (*value).min(31);
                    }
                    UI_R => {
                        self.selected_mut().r = *value;
                    }
                    UI_G => {
                        self.selected_mut().g = *value;
                    }
                    UI_B => {
                        self.selected_mut().b = *value;
                    }
                    UI_COUNT => {
                        let v = (*value).max(1);
                        self.selected_mut().count = v;
                    }
                    UI_INDEX => {
                        self.selected_mut().index = *value;
                    }
                    UI_SPAN => {
                        let v = (*value).clamp(1, 20);
                        self.selected_mut().span = v;
                    }
                    _ => {}
                }
            }
            PluginUiEvent::Button { id } => match *id {
                UI_REFRESH => self.spawn_refresh_status(),
                UI_ALL_OFF => self.spawn_all_off(),
                UI_APPLY => {
                    let idx = self.state.selected_strip as usize;
                    self.spawn_send_strip(idx);
                }
                _ => {}
            },
            PluginUiEvent::Switch { id, value } if *id == UI_ARTNET_ENABLED => {
                self.state.artnet_enabled = *value;
                self.sync_artnet();
                self.save_state();
            }
            PluginUiEvent::Text { id, text } if *id == UI_HOST => {
                let trimmed = text.trim().to_string();
                if !trimmed.is_empty() && trimmed != self.state.host {
                    self.state.host = trimmed;
                    self.sync_artnet();
                    self.save_state();
                }
            }
            _ => {}
        }
    }

    fn render_ui(&self) {
        bpf::ui::begin();
        bpf::ui::begin_frame_styled(10, "LED Tubes", 8, 8, 4, 4);

        if let Some(err) = &self.last_error {
            bpf::ui::label(&format!("Error: {}", err));
        }

        bpf::ui::text_edit("Host", UI_HOST, &self.state.host);

        let artnet_status = match &self.artnet {
            Some(h) => format!("Art-Net: {} (handle {})", h.addr(), h.handle()),
            None if self.state.artnet_enabled => "Art-Net: REGISTER FAILED".to_string(),
            None => "Art-Net: disabled".to_string(),
        };
        bpf::ui::label(&artnet_status);
        bpf::ui::switch("Art-Net", UI_ARTNET_ENABLED, self.state.artnet_enabled);

        bpf::ui::hfader(
            "Strip",
            UI_STRIP_SELECT,
            0,
            (STRIP_COUNT - 1) as u8,
            self.state.selected_strip,
        );

        let strip_num = self.state.selected_strip + 1;
        let s = self.selected();
        let mode_name = MODES.get(s.mode as usize).unwrap_or(&"off");

        bpf::ui::label(&format!("--- Strip {} ---", strip_num));
        bpf::ui::label(&format!("Mode: {}", mode_name));
        bpf::ui::hfader("Mode", UI_MODE, 0, (MODES.len() - 1) as u8, s.mode);

        bpf::ui::label(&format!("Brightness: {}", s.brightness));
        bpf::ui::hfader("Brightness", UI_BRIGHTNESS, 0, 31, s.brightness);

        bpf::ui::label(&format!("R: {} G: {} B: {}", s.r, s.g, s.b));
        bpf::ui::hfader("R", UI_R, 0, 255, s.r);
        bpf::ui::hfader("G", UI_G, 0, 255, s.g);
        bpf::ui::hfader("B", UI_B, 0, 255, s.b);

        bpf::ui::label(&format!("Count: {}", s.count));
        bpf::ui::hfader("Count", UI_COUNT, 1, 200, s.count);

        bpf::ui::label(&format!("Index: {}", s.index));
        bpf::ui::hfader("Index", UI_INDEX, 0, s.count.saturating_sub(1), s.index);

        bpf::ui::label(&format!("Span: {}", s.span));
        bpf::ui::hfader("Span", UI_SPAN, 1, 20, s.span);

        bpf::ui::button("Apply", UI_APPLY);
        bpf::ui::button("All Off", UI_ALL_OFF);
        bpf::ui::button("Refresh Status", UI_REFRESH);

        bpf::ui::end_frame();
    }
}

impl Plugin for LedTubesPlugin {
    fn initialize(&mut self, input: TickInput) {
        self.load_state();
        self.sync_artnet();
        self.process_events(&input);
        self.spawn_refresh_status();
    }

    fn run(&mut self, input: TickInput) {
        if self.pending.is_some() {
            self.poll_pending();
        }
        self.process_events(&input);
        self.render_ui();
    }
}

#[no_mangle]
extern "C" fn main() {
    bpf::hook_plugin(Box::new(LedTubesPlugin::default()));
}

fn mode_str_to_index(mode: &str) -> u8 {
    match mode.to_lowercase().as_str() {
        "off" => 0,
        "solid" => 1,
        "chase" => 2,
        "pixel" => 3,
        "window" => 4,
        "ruler" => 5,
        _ => 0,
    }
}

#[derive(Debug, Deserialize)]
struct StatusResponse {
    #[serde(default)]
    strips: Vec<StripStatus>,
}

#[derive(Debug, Deserialize)]
struct StripStatus {
    #[serde(default)]
    id: u16,
    #[serde(default)]
    mode: String,
    #[serde(default)]
    active_segments: u16,
    #[serde(default)]
    brightness: u16,
    #[serde(default)]
    index: u16,
    #[serde(default)]
    span: u16,
    #[serde(default)]
    rgb: Vec<u16>,
}
