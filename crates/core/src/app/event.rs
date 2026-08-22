use crate::{app::BlaulichtApp, config, msg::SystemMessage};
use blaulicht_shared::{ControlEvent, EventOriginator, LogLevel, MainUiEvent, PluginStateLocation};
use crossbeam_channel::TryRecvError;
use std::collections::hash_map::Entry;
use std::path::PathBuf;

#[cfg(feature = "audio")]
use cpal::traits::DeviceTrait;

impl BlaulichtApp {
    pub(crate) fn handle_events(&mut self) {
        let mut drained: usize = 0;
        loop {
            // Reset per iteration: break only when BOTH channels are empty in
            // the same pass. The old cross-iteration counter stopped after ~3
            // messages per frame, letting the unbounded system channel grow
            // without limit under log bursts.
            let mut empty = 0;
            match self.data.event_bus_connection.try_recv() {
                Some(control_event) if control_event.originator() == EventOriginator::Web => {
                    if control_event_marks_showfile_dirty(&control_event.body()) {
                        self.mark_showfile_dirty();
                    }
                }
                // Don't handle web to avoid infinite loopbacks.
                Some(control_event) if control_event.originator() != EventOriginator::Web => {
                    let body = control_event.body();
                    if control_event_marks_showfile_dirty(&body) {
                        self.mark_showfile_dirty();
                    }
                    if let ControlEvent::MainUi(main_ui_event) = body {
                        match main_ui_event {
                            MainUiEvent::NavigatePage(app_page) => {
                                self.navbar.navigate_to(app_page)
                            }
                            MainUiEvent::SetPluginUIOpen { plugin_id, open } => {
                                tracing::debug!(
                                    "[UI] Set plugin <{plugin_id}> visibility to: {open}"
                                );
                                let mut map = self.data.state.plugin_ui_visibility.write().unwrap();
                                if let Entry::Occupied(ref mut entry) = map.entry(plugin_id) {
                                    entry.get_mut().open = open;
                                };
                            }
                            MainUiEvent::CreateExternalScreen { width, height } => {
                                self.add_external_screen_with_dimensions(egui::vec2(
                                    width as f32,
                                    height as f32,
                                ));
                            }
                            MainUiEvent::RemoveExternalScreen { index } => {
                                self.remove_external_screen(index as usize);
                            }
                            MainUiEvent::CreateOwnedExternalScreen {
                                owner_plugin_id,
                                width,
                                height,
                            } => {
                                self.upsert_external_screen_for_owner(
                                    owner_plugin_id,
                                    egui::vec2(width as f32, height as f32),
                                );
                            }
                            MainUiEvent::RemoveOwnedExternalScreen { owner_plugin_id } => {
                                self.remove_external_screen_for_owner(owner_plugin_id);
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
                                .filter_map(|(_, dev)| dev.name().ok())
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
                                tracing::error!(
                                    "Failed to write config after global state save: {err}"
                                );
                            }
                        }
                    }
                    SystemMessage::PluginAlert {
                        plugin_id: _plugin_id,
                        label,
                        duration_ms,
                    } => {
                        self.show_popup(crate::app::PopupSpec::with_duration(
                            std::time::Duration::from_millis(duration_ms.max(1) as u64),
                            label,
                        ));
                    }
                },
                Err(TryRecvError::Empty) => {
                    empty += 1;
                }
                Err(TryRecvError::Disconnected) => {
                    tracing::warn!("System message channel disconnected");
                    break;
                }
            }

            if empty >= 2 {
                break;
            }

            // Safety cap so one frame can't stall on a pathological flood.
            drained += 1;
            if drained >= 10_000 {
                break;
            }
        }
    }
}

fn control_event_marks_showfile_dirty(event: &ControlEvent) -> bool {
    match event {
        ControlEvent::SelectGroup(_)
        | ControlEvent::DeSelectGroup(_)
        | ControlEvent::LimitSelectionToFixtureInCurrentGroup(_)
        | ControlEvent::UnLimitSelectionToFixtureInCurrentGroup(_)
        | ControlEvent::RemoveSelection
        | ControlEvent::RemoveAllSelection
        | ControlEvent::PushSelection
        | ControlEvent::PopSelection
        | ControlEvent::MainUi(_)
        | ControlEvent::PluginUi(_, _)
        | ControlEvent::MiscEvent { .. } => false,
        ControlEvent::Transaction(events) => events.iter().any(control_event_marks_showfile_dirty),
        _ => true,
    }
}
