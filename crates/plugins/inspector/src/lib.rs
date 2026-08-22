//! Inspector: a loopback-only UDP command port for dumping and manipulating
//! the running engine. Driven by `scripts/blctl.py`.
//!
//! Protocol: one JSON object per request datagram, e.g.
//!   {"id": 1, "cmd": "audio"}
//! Replies go to the datagram's source address, chunked as
//!   {"id": 1, "seq": 0, "total": 2, "data": "<partial JSON string>"}
//! Concatenating `data` over `seq` 0..total yields the reply JSON.

use blaulicht_plugin_framework as bpf;
use blaulicht_plugin_framework::{ui, Plugin, UdpPort};
use blaulicht_shared::{ControlEvent, LogLevel, TickInput};
use serde_json::{json, Value};
use std::collections::VecDeque;

const BIND_PORT: u16 = 9099;
/// Loopback datagrams: comfortably below the 64 KiB datagram ceiling while
/// keeping chunk counts low for full state dumps.
const CHUNK_BYTES: usize = 8_000;
const AUDIO_HISTORY: usize = 64;

struct InspectorPlugin {
    udp: Option<UdpPort>,
    error: String,
    request_count: u64,
    /// Address currently receiving the ControlEvent tap, if any.
    tap_target: Option<String>,
    audio_history: VecDeque<Value>,
}

impl Default for InspectorPlugin {
    fn default() -> Self {
        Self {
            udp: None,
            error: String::new(),
            request_count: 0,
            tap_target: None,
            audio_history: VecDeque::with_capacity(AUDIO_HISTORY),
        }
    }
}

fn audio_json(input: &TickInput) -> Value {
    // CollectedAudioSnapshot derives Serialize.
    serde_json::to_value(input.audio_data.clone()).unwrap_or(Value::Null)
}

fn reply(target: &str, request_id: u64, payload: &Value) {
    let body = payload.to_string();
    let bytes = body.as_bytes();
    let total = bytes.len().div_ceil(CHUNK_BYTES).max(1);
    let mut start = 0;
    for seq in 0..total {
        // Split on a char boundary at or below the chunk size.
        let mut end = (start + CHUNK_BYTES).min(bytes.len());
        while end < bytes.len() && !body.is_char_boundary(end) {
            end -= 1;
        }
        let frame = json!({
            "id": request_id,
            "seq": seq,
            "total": total,
            "data": &body[start..end],
        });
        bpf::send_udp(target, frame.to_string().as_bytes());
        start = end;
    }
}

/// Narrows a JSON value by a dotted path ("scenes.1.sink.master_speed").
fn narrow<'a>(mut value: &'a Value, path: &str) -> Option<&'a Value> {
    for segment in path.split('.').filter(|s| !s.is_empty()) {
        value = match value {
            Value::Object(map) => map.get(segment)?,
            Value::Array(list) => list.get(segment.parse::<usize>().ok()?)?,
            _ => return None,
        };
    }
    Some(value)
}

impl InspectorPlugin {
    fn beat_digest(&self, input: &TickInput) -> Value {
        let state = bpf::get_dmx();
        let focused = state.current_scene_focus;
        let mut animations = Vec::new();
        if let Some(scene) = state.scenes.get(&focused) {
            for (selection, scene_animations) in &scene.sink.active_animations {
                for (animation_id, animation) in scene_animations {
                    let timers: Value = animation
                        .fixture_timers
                        .iter()
                        .map(|(key, timer)| {
                            (
                                format!("{}:{}", key.0, key.1),
                                json!({
                                    "phase": timer.timer,
                                    "last_tick_time": timer.last_tick_time,
                                    "needs_reset_on_beat": timer.needs_reset_on_beat,
                                }),
                            )
                        })
                        .collect::<serde_json::Map<_, _>>()
                        .into();
                    animations.push(json!({
                        "animation_id": animation_id,
                        "name": animation.spec_cloned.name,
                        "selection": selection.fixtures,
                        "enabled": animation.enabled,
                        "speed_factor": animation.speed_factor.as_str(),
                        "iteration_count": animation.iteration_count,
                        "reversed": animation.reversed,
                        "timers": timers,
                    }));
                }
            }
        }
        json!({
            "focused_scene": focused,
            "overlay_scenes": state.current_overlay_scenes,
            "audio": audio_json(input),
            "animations": animations,
        })
    }

    fn handle_command(&mut self, request: &Value, src_addr: &str, input: &TickInput) -> Value {
        let cmd = request.get("cmd").and_then(Value::as_str).unwrap_or("");
        match cmd {
            "ping" => json!({
                "ok": true,
                "abi": blaulicht_shared::PLUGIN_ABI_VERSION,
                "plugin_id": input.id,
                "clock": input.clock,
                "requests": self.request_count,
            }),
            "audio" => json!({
                "current": audio_json(input),
                "history": self.audio_history,
            }),
            "beat" => self.beat_digest(input),
            "state" => {
                let state = bpf::get_dmx();
                let value = serde_json::to_value(&state)
                    .unwrap_or_else(|e| json!({"error": format!("serialize: {e}")}));
                match request.get("path").and_then(Value::as_str) {
                    Some(path) if !path.is_empty() => narrow(&value, path)
                        .cloned()
                        .unwrap_or_else(|| json!({"error": format!("no such path: {path}")})),
                    _ => value,
                }
            }
            "event" => match request.get("body") {
                Some(body) => match serde_json::from_value::<ControlEvent>(body.clone()) {
                    Ok(event) => {
                        bpf::send_event(event);
                        json!({"ok": true})
                    }
                    Err(e) => json!({"error": format!("bad ControlEvent: {e}")}),
                },
                None => json!({"error": "missing 'body'"}),
            },
            "override" => {
                let (u, c, v) = (
                    request.get("universe").and_then(Value::as_u64),
                    request.get("channel").and_then(Value::as_u64),
                    request.get("value").and_then(Value::as_u64),
                );
                match (u, c, v) {
                    (Some(u), Some(c), Some(v)) => {
                        bpf::send_event(ControlEvent::SetChannelOverride(
                            u as u16, c as u16, v as u8,
                        ));
                        json!({"ok": true})
                    }
                    _ => json!({"error": "need universe/channel/value"}),
                }
            }
            "clear" => {
                let (u, c) = (
                    request.get("universe").and_then(Value::as_u64),
                    request.get("channel").and_then(Value::as_u64),
                );
                match (u, c) {
                    (Some(u), Some(c)) => {
                        bpf::send_event(ControlEvent::RemoveChannelOverride(u as u16, c as u16));
                        json!({"ok": true})
                    }
                    _ => json!({"error": "need universe/channel"}),
                }
            }
            "tap" => {
                let on = request.get("on").and_then(Value::as_bool).unwrap_or(true);
                self.tap_target = on.then(|| src_addr.to_string());
                json!({"ok": true, "tapping": on})
            }
            other => json!({"error": format!("unknown cmd: {other}")}),
        }
    }
}

impl Plugin for InspectorPlugin {
    fn initialize(&mut self, _input: TickInput) {
        // NOTE: engine state is not available on the init tick; all state
        // reads happen in run().
        match UdpPort::open_loopback(BIND_PORT) {
            Ok(udp) => {
                self.udp = Some(udp);
                bpf::bl_log(
                    &format!("[inspector] listening on 127.0.0.1:{BIND_PORT}"),
                    LogLevel::Info,
                );
            }
            Err(e) => {
                self.error = format!("bind failed: {e}");
                bpf::bl_log(&format!("[inspector] {}", self.error), LogLevel::Err);
            }
        }
    }

    fn run(&mut self, input: TickInput) {
        if self.audio_history.len() >= AUDIO_HISTORY {
            self.audio_history.pop_front();
        }
        self.audio_history.push_back(audio_json(&input));

        // Event tap: stream everything on the bus to the subscribed client.
        if let Some(target) = self.tap_target.clone() {
            for ev in &input.events.events {
                let frame = json!({
                    "tap": true,
                    "clock": input.clock,
                    "originator": format!("{:?}", ev.originator()),
                    "event": serde_json::to_value(ev.body()).unwrap_or(Value::Null),
                });
                bpf::send_udp(&target, frame.to_string().as_bytes());
            }
        }

        if let Some(ref udp) = self.udp {
            for packet in udp.poll() {
                self.request_count += 1;
                let (request_id, response) = match serde_json::from_slice::<Value>(&packet.body) {
                    Ok(request) => {
                        let id = request.get("id").and_then(Value::as_u64).unwrap_or(0);
                        let response = self.handle_command(&request, &packet.src_addr, &input);
                        (id, response)
                    }
                    Err(e) => (0, json!({"error": format!("bad request JSON: {e}")})),
                };
                reply(&packet.src_addr, request_id, &response);
            }
        }

        ui::begin();
        ui::label_styled("Inspector", 18, false);
        if self.udp.is_some() {
            ui::label(&format!(
                "127.0.0.1:{BIND_PORT} | {} requests | tap: {}",
                self.request_count,
                self.tap_target.as_deref().unwrap_or("off"),
            ));
        } else {
            ui::label(&format!("NOT BOUND: {}", self.error));
        }
    }
}

#[no_mangle]
extern "C" fn main() {
    bpf::hook_plugin(Box::new(InspectorPlugin::default()));
}
