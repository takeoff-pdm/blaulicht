use crate::{
    app::{
        components::{self, clickable, ButtonSize, Dialog},
        ui::FileDialogOpenOrigin,
        BlaulichtApp, PopupSpec,
    },
    audio::defs::AudioThreadControlSignal,
    config,
    mainloop::DMX_TICK_TIME,
    msg::{FromFrontend, SystemMessage},
    plugin::midi::MidiError,
    state::{
        ArtNetReceiver, DmxHealthState, MidiDeviceState, PluginOpenState, ScreenId,
        SerialDeviceState, NUM_DMX_UNIVERSES,
    },
};
use blaulicht_assets::icons;
use blaulicht_shared::{
    ControlEvent, ControlEventMessage, EventOriginator, LogLevel, MainUiEvent, SaveEngineState,
    Showfile, ShowfileArtNetReceiver, ShowfileArtNetState,
};
use egui::{
    Color32, Context, FontFamily, FontId, Frame, Label, Margin, RichText, ThemePreference, Ui,
    Vec2, Widget,
};
use egui_extras::{Column, TableBuilder};
use egui_file::FileDialog;
use std::{
    ffi::OsStr,
    mem,
    net::SocketAddr,
    path::{Path, PathBuf},
    process::Command,
    str::FromStr,
    time::Duration,
};

impl BlaulichtApp {
    //
    // TODO: i need to remove this
    //
    pub fn save_showfile(&mut self) {
        let mut config_mut = self.data.config.lock().unwrap();

        match config_mut.last_open_showfile.clone() {
            Some(path) => {
                let engine_snapshot = {
                    let dmx = self.data.state.dmx_engine.read().unwrap();
                    dmx.0.clone()
                };

                let plugin_state = { self.data.state.plugin_state_storage.lock().unwrap().clone() };

                let artnet_state = {
                    let artnet_output = self.data.state.artnet_output.read().unwrap();
                    ShowfileArtNetState {
                        receivers: artnet_output
                            .receivers
                            .iter()
                            .map(|receiver| ShowfileArtNetReceiver {
                                address: receiver.address,
                                enabled: receiver.enabled,
                            })
                            .collect(),
                    }
                };

                let showfile = Showfile {
                    engine: SaveEngineState::from(engine_snapshot),
                    artnet: artnet_state,
                    plugin_state,
                };

                let serialized =
                    serde_json::to_string_pretty(&showfile).expect("Failed to serialize showfile");
                std::fs::write(&path, &serialized).expect("Failed to write showfile");

                config_mut.last_open_showfile = Some(path.clone());

                let config_path = PathBuf::from_str(&self.data.config_path).unwrap();
                config::write_config(config_path, config_mut.clone()).unwrap();

                self.data
                    .system_message_sender
                    .send(SystemMessage::Log(
                        format!("Saved showfile to {path:?}"),
                        LogLevel::Info,
                    ))
                    .unwrap();

                mem::drop(config_mut);

                self.show_popup(PopupSpec::with_duration(
                    Duration::from_secs(2),
                    "Saved Showfile".to_string(),
                ));
            }
            None => {
                self.data
                    .system_message_sender
                    .send(SystemMessage::Log(
                        "No opened showfile, not saving".to_string(),
                        LogLevel::Err,
                    ))
                    .unwrap();

                mem::drop(config_mut);

                self.show_popup(PopupSpec::with_duration(
                    Duration::from_secs(2),
                    "No Showfile".to_string(),
                ));
            }
        }
    }

    pub fn render_showfile_buttons(&mut self, ui: &mut Ui, button_size: ButtonSize) {
        ui.horizontal(|ui| {
            if components::button(ui, false, "Load Showfile", button_size) {
                let filter = Box::new({
                    let ext = Some(OsStr::new("json"));
                    move |path: &Path| -> bool { path.extension() == ext }
                });

                let config = self.data.config.lock().unwrap();

                let mut dialog = FileDialog::open_file(config.last_open_showfile.clone())
                    .show_files_filter(filter);

                dialog.open();
                self.system_ui_state.open_file_dialog = Some(dialog);
                self.system_ui_state.file_dialog_open_origin = FileDialogOpenOrigin::Load;
            }

            if components::button(ui, false, "Save to Showfile", button_size) {
                let filter = Box::new({
                    let ext = Some(OsStr::new("json"));
                    move |path: &Path| -> bool { path.extension() == ext }
                });

                let config = self.data.config.lock().unwrap();

                let mut dialog = FileDialog::open_file(config.last_open_showfile.clone())
                    .show_files_filter(filter);

                dialog.open();
                self.system_ui_state.open_file_dialog = Some(dialog);
                self.system_ui_state.file_dialog_open_origin = FileDialogOpenOrigin::Save;
            }

            {
                let mut conf = self.data.config.lock().unwrap();
                let button_enabled = conf.last_open_showfile.is_some();
                if components::button(ui, button_enabled, "Close Showfile", button_size)
                    && button_enabled
                {
                    conf.last_open_showfile = None;
                    let path = PathBuf::from_str(&self.data.config_path).unwrap();
                    config::write_config(path, conf.clone()).unwrap();
                    let mut dmx = self.data.state.dmx_engine.write().unwrap();
                    let mut artnet = self.data.state.artnet_output.write().unwrap();
                    config::close_showfile(
                        &mut dmx,
                        &mut artnet,
                        &self.data.state.plugin_state_storage,
                    );
                    mem::drop(artnet);
                    mem::drop(dmx);
                }
            }

            let (label, allowed) = match &self
                .data
                .config
                .lock()
                .unwrap()
                .last_open_showfile
                .is_some()
            {
                true => ("Save Showfile", true),
                false => ("Save Showfile", false),
            };

            if components::button(ui, allowed, label, button_size) {
                self.save_showfile();
            }
        });
    }

    pub fn render_showfile_dialog(&mut self, ctx: &Context) {
        let Some(dialog) = &mut self.system_ui_state.open_file_dialog else {
            return;
        };

        if dialog.show(ctx).selected() {
            let mut config = self.data.config.lock().unwrap();

            if let Some(file) = dialog.path() {
                match self.system_ui_state.file_dialog_open_origin {
                    FileDialogOpenOrigin::Save => {
                        // let dmx = self.data.state.dmx_engine.read().unwrap();
                        // let serialized = postcard::to_allocvec(&dmx.clone()).unwrap();
                        // std::fs::write(file, serialized).unwrap();

                        config.last_open_showfile = Some(file.to_path_buf());
                        //
                        // let config_path = PathBuf::from_str(&self.data.config_path).unwrap();
                        // config::write_config(config_path, config.clone()).unwrap();
                        //
                        // self.data
                        //     .system_message_sender
                        //     .send(SystemMessage::Log(
                        //         format!("Saved showfile to {file:?}"),
                        //         LogLevel::Info,
                        //     ))
                        //     .unwrap();

                        mem::drop(config);

                        self.save_showfile();
                    }
                    FileDialogOpenOrigin::Load => {
                        config.last_open_showfile = Some(file.to_path_buf());

                        // let mut f = File::open(file).expect("no file found");
                        // let metadata = fs::metadata(file).expect("unable to read metadata");
                        // let mut buffer = vec![0; metadata.len() as usize];
                        // f.read(&mut buffer).expect("buffer overflow");
                        //
                        // let decoded: blaulicht_shared::EngineState =
                        //     postcard::from_bytes(&buffer).unwrap();
                        //
                        // {
                        //     let mut plugin_state =
                        //         self.data.state.plugin_state_storage.lock().unwrap();
                        //     *plugin_state = decoded.plugin_state.clone();
                        // }
                        //
                        // let mut dmx = self.data.state.dmx_engine.write().unwrap();
                        // // dmx.overwrite(decoded);
                        // dmx.load_showfile(decoded);
                        // mem::drop(dmx);

                        let mut dmx = self.data.state.dmx_engine.write().unwrap();
                        let mut artnet = self.data.state.artnet_output.write().unwrap();
                        config::read_showfile(
                            file.to_path_buf(),
                            &mut dmx,
                            &mut artnet,
                            &self.data.state.plugin_state_storage,
                            self.data.system_message_sender.clone(),
                        );
                        mem::drop(artnet);
                        mem::drop(dmx);

                        config.last_open_showfile = Some(file.to_path_buf());

                        let config_path = PathBuf::from_str(&self.data.config_path).unwrap();
                        config::write_config(config_path, config.clone()).unwrap();

                        self.data
                            .system_message_sender
                            .send(SystemMessage::Log(
                                format!("Loaded showfile from {file:?}"),
                                LogLevel::Info,
                            ))
                            .unwrap();

                        mem::drop(config);

                        self.show_popup(PopupSpec::with_duration(
                            Duration::from_secs(2),
                            "Loaded Showfile".to_string(),
                        ));
                    }
                }
            }
        }
    }
}
