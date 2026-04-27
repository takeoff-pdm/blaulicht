use crate::{app::BlaulichtApp, config, msg::SystemMessage};
use blaulicht_shared::{ControlEvent, EventOriginator, LogLevel, MainUiEvent, PluginStateLocation};
use crossbeam_channel::TryRecvError;
use std::collections::hash_map::Entry;
use std::path::PathBuf;

#[cfg(feature = "audio")]
use cpal::traits::DeviceTrait;

impl BlaulichtApp {
    pub(crate) fn handle_events(&mut self) {
        let mut empty = 0;
        loop {
            match self.data.event_bus_connection.try_recv() {
                // Don't handle web to avoid infinite loopbacks.
                Some(control_event) if control_event.originator() != EventOriginator::Web => {
                    if let ControlEvent::MainUi(main_ui_event) = control_event.body() {
                        match main_ui_event {
                            MainUiEvent::NavigatePage(app_page) => {
                                self.navbar.navigate_to(app_page)
                            }
                            MainUiEvent::SetPluginUIOpen { plugin_id, open } => {
                                tracing::debug!("[UI] Set plugin <{plugin_id}> visibility to: {open}");
                                let mut map = self.data.state.plugin_ui_visibility.write().unwrap();
                                if let Entry::Occupied(ref mut entry) = map.entry(plugin_id) {
                                    entry.get_mut().open = open;
                                };
                            }
                        }
                    }
                }
                _ => {
                    empty += 1;
                }
            }

            match self.data.system_message_receiver.try_recv() {
                Ok(sys) => match sys {
                    SystemMessage::Heartbeat(_) => {
                        self.last_heartbeat_frame = self.frame_count;
                    }
                    SystemMessage::Log(log_msg, level) => {
                        self.log_window
                            .add_log(level, log_msg, "System".to_string());
                    }
                    SystemMessage::WasmLog(wasm_log_body) => {
                        self.log_window.add_log(
                            wasm_log_body.level.clone(),
                            format!("PID: {} | {}", wasm_log_body.plugin_id, wasm_log_body.msg),
                            "WASM".to_string(),
                        );
                    }
                    SystemMessage::TickSpeeds(speeds) => {
                        self.tick_speeds = speeds;
                    }
                    SystemMessage::AudioSelected(_device) => {
                        // self.log_window.add_log(
                        //     LogLevel::Info,
                        //     format!("Audio device selected: {}", if device.is_some() { "Yes" } else { "No" }),
                        //     "Audio".to_string(),
                        // );
                    }
                    SystemMessage::AudioDevicesView(items) => {
                        // Check if number of devices changed.
                        //
                        if self.available_audio_devices.len() != items.len() {
                            self.log_window.add_log(
                                LogLevel::Debug,
                                format!("Available audio devices updated: {} devices", items.len()),
                                "Audio".to_string(),
                            );
                            let items_str = items
                                .into_iter()
                                .map(|(_, dev)| dev.name().unwrap())
                                .collect();
                            self.available_audio_devices = items_str;
                        }
                    }
                    SystemMessage::DMX(_dmx_msg) => {
                        // self.log_window.add_log(
                        //     LogLevel::Info,
                        //     format!("DMX message: {:?}", dmx_msg),
                        //     "DMX".to_string(),
                        // );
                    }
                    SystemMessage::SavePluginState {
                        plugin_name,
                        state_data,
                        location,
                    } => {
                        if location == PluginStateLocation::Global {
                            let mut config_mut = self.data.config.lock().unwrap();
                            config_mut
                                .plugin_state
                                .insert(plugin_name.clone(), state_data.clone());
                            let config_path = PathBuf::from(self.data.config_path.clone());
                            if let Err(err) = config::write_config(config_path, config_mut.clone())
                            {
                                tracing::error!("Failed to write config after global state save: {err}");
                            }
                        }
                    }
                },
                Err(TryRecvError::Empty) => {
                    empty += 1;
                }
                Err(TryRecvError::Empty) => {}
                Err(TryRecvError::Disconnected) => {
                    unreachable!("CANNOT REACH")
                }
            }

            if empty >= 3 {
                break;
            }
        }
    }
}
