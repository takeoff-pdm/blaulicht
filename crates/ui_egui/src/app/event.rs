use crate::app::BlaulichtApp;
use blaulicht_engine::{
    client::{EngineClient, ReplyHandlers},
    hub::BusBody,
    msg::SystemMessage,
};
use blaulicht_shared::{ControlEvent, EventOriginator, LogLevel, MainUiEvent};
use std::path::Path;

/// Logs page source for a plugin log line: the plugin's file stem, so each
/// plugin can be filtered on its own.
fn plugin_log_source(plugin_id: u8, plugin_path: Option<&str>) -> String {
    plugin_path
        .and_then(|path| Path::new(path).file_stem())
        .map(|stem| stem.to_string_lossy().into_owned())
        .unwrap_or_else(|| format!("Plugin {plugin_id}"))
}

impl BlaulichtApp {
    fn plugin_log_source(&self, plugin_id: u8) -> String {
        self.client.with_plugins(|plugins| {
            plugin_log_source(plugin_id, plugins.get(&plugin_id).map(|p| p.path.as_ref()))
        })
    }

    pub(crate) fn handle_events(&mut self) {
        let mut drained: usize = 0;
        loop {
            // Reset per iteration: break only when BOTH channels are empty in
            // the same pass. The old cross-iteration counter stopped after ~3
            // messages per frame, letting the system queue fill up under log
            // bursts.
            let mut empty = 0;
            let message = self.client.try_recv_bus();
            // Other operator sessions' selection, focus and control events
            // don't move this desk's jog target. (The engine marks the show
            // edited; the desk learns it from `ShowfileStatus`.)
            let session = self.client.session();
            let concerns_desk = message
                .as_ref()
                .is_some_and(|message| message.concerns_session(session));
            match message.map(|message| message.body) {
                Some(BusBody::Control(control_event))
                    if control_event.originator() == EventOriginator::Web =>
                {
                    if concerns_desk {
                        self.recent_fixture_property
                            .observe(&control_event.body(), std::time::Instant::now());
                    }
                }
                // Don't handle web to avoid infinite loopbacks.
                Some(BusBody::Control(control_event)) => {
                    let body = control_event.body();
                    if control_event.originator() == EventOriginator::Plugin && concerns_desk {
                        self.recent_fixture_property
                            .observe(&body, std::time::Instant::now());
                    }
                    if let ControlEvent::MainUi(main_ui_event) = body {
                        match main_ui_event {
                            MainUiEvent::NavigatePage(app_page) => {
                                self.navbar.navigate_to(app_page)
                            }
                            MainUiEvent::SetPluginUIOpen { .. } => {
                                // Plugin requests reach the desk as
                                // `SystemMessage::DeskRequest` (routed by the
                                // plugin host); the bus copy is for plugins.
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
                            MainUiEvent::NumberpadAdjust { delta } => {
                                self.pending_numberpad_delta =
                                    self.pending_numberpad_delta.saturating_add(delta);
                            }
                            MainUiEvent::NavigateSubPage(subpage) => {
                                self.navbar.navigate_to_subpage(subpage)
                            }
                        }
                    }
                }
                // Host commands are applied by the engine; the desk only sees
                // its reply (`SystemMessage::CommandResult`).
                Some(BusBody::Host(_)) => {}
                None => {
                    empty += 1;
                }
            }

            match self.client.try_recv_system() {
                Some(sys) => match sys {
                    SystemMessage::Heartbeat(_) => {
                        self.last_heartbeat_frame = self.frame_count;
                    }
                    SystemMessage::EngineInitializationComplete => {
                        self.engine_initialization_complete = true;
                    }
                    SystemMessage::StartupStage { stage, status } => {
                        self.startup_progress.apply(stage, status);
                    }
                    SystemMessage::Log {
                        message,
                        level,
                        source,
                    } => {
                        self.log_window
                            .add_log(level, message, None, source.into_owned());
                    }
                    SystemMessage::WasmLog(wasm_log_body) => {
                        let source = self.plugin_log_source(wasm_log_body.plugin_id);
                        self.log_window.add_log(
                            wasm_log_body.level.clone(),
                            wasm_log_body.msg.into_owned(),
                            wasm_log_body.additional,
                            source,
                        );
                    }
                    SystemMessage::TickSpeeds(speeds) => {
                        self.tick_speeds = speeds;
                    }
                    SystemMessage::AudioSelected(name) => {
                        self.selected_audio_input = Some(name);
                    }
                    SystemMessage::AudioDevicesView(items) => {
                        let names: Vec<String> =
                            items.into_iter().map(|(_host, device)| device).collect();
                        if self.available_audio_devices != names {
                            self.log_window.add_log(
                                LogLevel::Debug,
                                format!("Available audio devices updated: {} devices", names.len()),
                                None,
                                "Audio".to_string(),
                            );
                            self.available_audio_devices = names;
                        }
                    }
                    SystemMessage::DMX(_dmx_msg) => {
                        // self.log_window.add_log(
                        //     LogLevel::Info,
                        //     format!("DMX message: {:?}", dmx_msg),
                        //     "DMX".to_string(),
                        // );
                    }
                    SystemMessage::ShowLoaded {
                        generation,
                        frontend_section,
                    } => self.apply_show_loaded(generation, frontend_section),
                    SystemMessage::ShowfileStatus(status) => {
                        self.save_status = status.save;
                        self.saved_at = status.saved_at;
                    }
                    SystemMessage::Exiting { .. } => self.exit_requested = true,
                    SystemMessage::DeskRequest(request) => {
                        self.view_state.apply_desk_request(request);
                    }
                    SystemMessage::CommandResult(_) => {
                        // Not in the client's system stream: results are
                        // taken with `take_results` below.
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
                None => {
                    empty += 1;
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

        // Results of the desk's own commands: run the closures waiting for
        // them (`ReplyHandlers::send_then`). Results nobody waits for are
        // dropped (refusals are logged by the engine).
        let results = self.client.take_results();
        if !results.is_empty() {
            ReplyHandlers::dispatch(self, |app| &mut app.reply_handlers, &results);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::plugin_log_source;

    #[test]
    fn plugin_logs_are_attributed_to_the_plugin_file_stem() {
        assert_eq!(
            plugin_log_source(3, Some("./crates/plugins/bundle_debug/inspector.wasm")),
            "inspector"
        );
        assert_eq!(plugin_log_source(7, None), "Plugin 7");
    }
}
