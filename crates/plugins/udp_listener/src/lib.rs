use blaulicht_plugin_framework as bpf;
use blaulicht_plugin_framework::{ui, Plugin, UdpPort, UdpReceived};
use blaulicht_shared::{ControlEvent, PluginUiEvent, TickInput};
use serde::{Deserialize, Serialize};

const ID_PORT_TEXT: u8 = 1;
const BTN_BIND: u8 = 2;
const MAX_LOG_LINES: usize = 20;

#[derive(Serialize, Deserialize)]
struct SaveState {
    port: u16,
}

impl Default for SaveState {
    fn default() -> Self {
        Self { port: 9000 }
    }
}

struct UdpListenerPlugin {
    state: SaveState,
    port_text: String,
    udp: Option<UdpPort>,
    log: Vec<String>,
    error: String,
    packet_count: u64,
}

impl Default for UdpListenerPlugin {
    fn default() -> Self {
        Self {
            state: SaveState::default(),
            port_text: "9000".to_string(),
            udp: None,
            log: Vec::new(),
            error: String::new(),
            packet_count: 0,
        }
    }
}

impl UdpListenerPlugin {
    fn save(&self) {
        if let Ok(json) = serde_json::to_string(&self.state) {
            bpf::save_plugin_state(bpf::PluginStateLocation::Global, &json);
        }
    }

    fn try_bind(&mut self) {
        let port = self.state.port;
        match UdpPort::open(port) {
            Ok(udp) => {
                self.udp = Some(udp);
                self.error.clear();
                self.log_msg(&format!("Bound to port {port}"));
            }
            Err(e) => {
                self.udp = None;
                self.error = format!("{e}");
            }
        }
    }

    fn log_msg(&mut self, msg: &str) {
        if self.log.len() >= MAX_LOG_LINES {
            self.log.remove(0);
        }
        self.log.push(msg.to_string());
    }

    fn handle_ui_event(&mut self, event: &PluginUiEvent, plugin_id: u8, my_id: u8) {
        if plugin_id != my_id {
            return;
        }
        match event {
            PluginUiEvent::Text { id, text } if *id == ID_PORT_TEXT => {
                self.port_text = text.clone();
            }
            PluginUiEvent::Button { id } if *id == BTN_BIND => {
                if let Ok(port) = self.port_text.trim().parse::<u16>() {
                    self.state.port = port;
                    self.save();
                    self.try_bind();
                } else {
                    self.error = "Invalid port number".to_string();
                }
            }
            _ => {}
        }
    }

    fn render_ui(&self) {
        ui::begin();
        ui::label_styled("UDP Listener", 18, false);
        ui::separator();

        ui::begin_horizontal();
        ui::text_edit("Port", ID_PORT_TEXT, &self.port_text);
        ui::button("Bind", BTN_BIND);
        ui::end_horizontal();

        if self.udp.is_some() {
            ui::label(&format!("Listening on port {} | {} packets received", self.state.port, self.packet_count));
        } else {
            ui::label("Not bound");
        }

        if !self.error.is_empty() {
            ui::label(&format!("ERROR: {}", self.error));
        }

        ui::separator();
        ui::label_styled("Log", 14, true);
        for line in self.log.iter().rev() {
            ui::label(line);
        }
    }

    fn format_packet(packet: &UdpReceived) -> String {
        let hex: String = packet
            .body
            .iter()
            .take(32)
            .map(|b| format!("{:02x}", b))
            .collect::<Vec<_>>()
            .join(" ");
        let suffix = if packet.body.len() > 32 { "..." } else { "" };
        format!("[{}B] {}{}", packet.body.len(), hex, suffix)
    }
}

impl Plugin for UdpListenerPlugin {
    fn initialize(&mut self, _input: TickInput) {
        if let Some(json) = bpf::load_plugin_state(bpf::PluginStateLocation::Global) {
            if let Ok(saved) = serde_json::from_str::<SaveState>(&json) {
                self.state = saved;
                self.port_text = self.state.port.to_string();
            }
        }
        self.try_bind();
    }

    fn run(&mut self, input: TickInput) {
        for ev in &input.events.events {
            if let ControlEvent::PluginUi(ui_event, plugin_id) = ev.body() {
                self.handle_ui_event(&ui_event, plugin_id, input.id);
            }
        }

        if let Some(ref udp) = self.udp {
            for packet in udp.poll() {
                self.packet_count += 1;
                let msg = Self::format_packet(&packet);
                bpf::bl_log(&msg, blaulicht_shared::LogLevel::Info);
                self.log_msg(&msg);
            }
        }

        self.render_ui();
    }
}

#[no_mangle]
extern "C" fn main() {
    bpf::hook_plugin(Box::new(UdpListenerPlugin::default()));
}
