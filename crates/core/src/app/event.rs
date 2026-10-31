use crate::{
    app::BlaulichtApp,
    client::{EngineClient, ReplyHandlers},
    config,
    hub::{session_scope, BusBody, SessionScope},
    msg::SystemMessage,
};
use blaulicht_shared::{ControlEvent, EventOriginator, LogLevel, MainUiEvent, PluginStateLocation};
use std::path::{Path, PathBuf};

#[cfg(feature = "audio")]
use cpal::traits::DeviceTrait;

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
        let plugins = self.data.state.plugins.read().unwrap();
        plugin_log_source(plugin_id, plugins.get(&plugin_id).map(|p| p.path.as_ref()))
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
            // don't move this desk's jog target (`concerns_desk`) and mark
            // the show dirty only if they edit it (`own_session`).
            let session = self.client.session();
            let (concerns_desk, own_session) = message.as_ref().map_or((false, false), |message| {
                (
                    message.concerns_session(session),
                    message.session == session,
                )
            });
            match message.map(|message| message.body) {
                Some(BusBody::Control(control_event))
                    if control_event.originator() == EventOriginator::Web =>
                {
                    let body = control_event.body();
                    if concerns_desk {
                        self.recent_fixture_property
                            .observe(&body, std::time::Instant::now());
                    }
                    if control_event_marks_showfile_dirty(&body, own_session) {
                        self.mark_showfile_dirty();
                    }
                }
                // Don't handle web to avoid infinite loopbacks.
                Some(BusBody::Control(control_event)) => {
                    let body = control_event.body();
                    if control_event.originator() == EventOriginator::Plugin && concerns_desk {
                        self.recent_fixture_property
                            .observe(&body, std::time::Instant::now());
                    }
                    if control_event_marks_showfile_dirty(&body, own_session) {
                        self.mark_showfile_dirty();
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
                                None,
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

/// `true` if `event` changes what the showfile saves. `own_session` is
/// `false` for events from another operator session: those change only that
/// session if they are [`SessionScope::Session`] (its selection or focus,
/// which the showfile doesn't save), but their control events edit scenes
/// like the desk's own.
fn control_event_marks_showfile_dirty(event: &ControlEvent, own_session: bool) -> bool {
    match event {
        ControlEvent::Transaction(events) => events
            .iter()
            .any(|event| control_event_marks_showfile_dirty(event, own_session)),
        _ if !own_session && session_scope(event) == SessionScope::Session => false,
        ControlEvent::SelectGroup(_)
        | ControlEvent::DeSelectGroup(_)
        | ControlEvent::SelectFolder(_)
        | ControlEvent::LimitSelectionToFixtureInCurrentGroup(_)
        | ControlEvent::UnLimitSelectionToFixtureInCurrentGroup(_)
        | ControlEvent::RemoveSelection
        | ControlEvent::RemoveAllSelection
        | ControlEvent::PushSelection
        | ControlEvent::PopSelection
        | ControlEvent::MainUi(_)
        | ControlEvent::PluginUi(_, _)
        | ControlEvent::SetLiveMode(_)
        | ControlEvent::ToggleLiveMode
        | ControlEvent::MiscEvent { .. } => false,
        _ => true,
    }
}

#[cfg(test)]
mod tests {
    use super::{control_event_marks_showfile_dirty, plugin_log_source};
    use blaulicht_shared::{
        folder::{FolderItem, FolderKind},
        ControlEvent,
    };

    #[test]
    fn selecting_a_folder_is_not_show_data_but_editing_folders_is() {
        assert!(!control_event_marks_showfile_dirty(
            &ControlEvent::SelectFolder(0),
            true
        ));
        // The Programmer's folder deselect and the SelectFolder rollback.
        assert!(!control_event_marks_showfile_dirty(
            &ControlEvent::Transaction(vec![
                ControlEvent::DeSelectGroup(0),
                ControlEvent::LimitSelectionToFixtureInCurrentGroup(1),
            ]),
            true
        ));
        for event in [
            ControlEvent::CreateFolder {
                id: 0,
                kind: FolderKind::Groups,
                parent: None,
                name: "Stage".to_string(),
            },
            ControlEvent::RenameFolder(0, "Front".to_string()),
            ControlEvent::DeleteFolder(0),
            ControlEvent::MoveToFolder {
                item: FolderItem::Group(1),
                folder: Some(0),
            },
            ControlEvent::ReorderFolder(0, 2),
        ] {
            assert!(
                control_event_marks_showfile_dirty(&event, true),
                "{event:?}"
            );
        }
    }

    /// Another operator's focus changes only their session (the showfile
    /// saves the desk's), but their control events edit the show.
    #[test]
    fn other_sessions_mark_the_show_dirty_only_when_they_edit_it() {
        for event in [
            ControlEvent::SetSceneFocus(1),
            ControlEvent::SetFocusedSceneGraph(Some(0)),
        ] {
            assert!(
                control_event_marks_showfile_dirty(&event, true),
                "{event:?}"
            );
            assert!(
                !control_event_marks_showfile_dirty(&event, false),
                "{event:?}"
            );
        }
        for event in [
            ControlEvent::SetColor((1, 2, 3)),
            ControlEvent::SetChannelOverride(0, 1, 255),
            ControlEvent::Transaction(vec![
                ControlEvent::SetSceneFocus(1),
                ControlEvent::SetAlpha(9),
            ]),
        ] {
            assert!(
                control_event_marks_showfile_dirty(&event, false),
                "{event:?}"
            );
        }
        assert!(!control_event_marks_showfile_dirty(
            &ControlEvent::Transaction(vec![
                ControlEvent::SetSceneFocus(1),
                ControlEvent::SelectGroup(0),
            ]),
            false
        ));
    }

    #[test]
    fn plugin_logs_are_attributed_to_the_plugin_file_stem() {
        assert_eq!(
            plugin_log_source(3, Some("./crates/plugins/bundle_debug/inspector.wasm")),
            "inspector"
        );
        assert_eq!(plugin_log_source(7, None), "Plugin 7");
    }
}
