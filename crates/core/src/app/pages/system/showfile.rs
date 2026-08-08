use crate::{
    app::{
        components::{self, ButtonSize, Dialog},
        ui::FileDialogOpenOrigin,
        BlaulichtApp, PopupSpec,
    },
    config,
    msg::SystemMessage,
};
use blaulicht_shared::{LogLevel, SaveEngineState, ShowfileArtNetReceiver, ShowfileArtNetState};
use egui::{Context, FontFamily, FontId, Ui};
use egui_file_dialog::{FileDialog, Filter};
use egui_phosphor::regular as ph;
use std::{collections::HashSet, mem, path::PathBuf, str::FromStr, time::Duration};
use sysinfo::Disks;

fn places_quick_access_entries(showfile_home: Option<PathBuf>) -> Vec<(String, PathBuf)> {
    let disks = Disks::new_with_refreshed_list();
    let mut seen = HashSet::new();
    let mut entries = Vec::new();
    let mut disk_count = 0;
    let mut usb_count = 0;
    let mut sd_count = 0;

    if let Some(path) = showfile_home {
        if !path.as_os_str().is_empty() {
            entries.push((format!("{}  Workspace", ph::FOLDER_SIMPLE), path));
        }
    }

    for disk in disks.iter() {
        let mount = disk.mount_point().to_path_buf();
        if mount.as_os_str().is_empty() || !seen.insert(mount.clone()) {
            continue;
        }

        let name = disk.name().to_string_lossy().to_lowercase();
        let is_sd = disk.is_removable()
            && (name.contains("mmc")
                || name.contains("sdcard")
                || name.contains("sdhc")
                || name.contains("sdxc"));

        let label = if is_sd {
            sd_count += 1;
            format!("{}  SD {sd_count}", ph::SIM_CARD)
        } else if disk.is_removable() {
            usb_count += 1;
            format!("{}  USB {usb_count}", ph::USB)
        } else {
            disk_count += 1;
            format!("{}  Disk {disk_count}", ph::HARD_DRIVE)
        };

        entries.push((label, mount));
    }

    entries
}

impl BlaulichtApp {
    fn close_showfile(&mut self) {
        let mut conf = self.data.config.lock().unwrap();
        conf.last_open_showfile = None;
        let path = PathBuf::from(&self.data.config_path);
        if let Err(err) = config::write_config(path, conf.clone()) {
            tracing::error!("Failed to persist closed showfile config: {err}");
        }
        drop(conf);

        let mut dmx = self.data.state.dmx_engine.write().unwrap();
        let mut artnet = self.data.state.artnet_output.write().unwrap();
        config::close_showfile(
            &mut dmx,
            &mut artnet,
            &self.data.state.plugin_state_storage,
        );
    }

    fn load_showfile_path(&mut self, file: PathBuf) {
        let mut config = self.data.config.lock().unwrap();
        let mut dmx = self.data.state.dmx_engine.write().unwrap();
        let mut artnet = self.data.state.artnet_output.write().unwrap();
        let ui_state = config::read_showfile(
            file.clone(),
            &mut dmx,
            &mut artnet,
            &self.data.state.plugin_state_storage,
            self.data.system_message_sender.clone(),
        );
        mem::drop(artnet);
        mem::drop(dmx);

        config.last_open_showfile = Some(file.clone());
        let config_path = PathBuf::from(&self.data.config_path);
        if let Err(err) = config::write_config(config_path, config.clone()) {
            tracing::error!("Failed to persist loaded showfile config: {err}");
        }

        let _ = self.data.system_message_sender.send(SystemMessage::Log(
            format!("Loaded showfile from {file:?}"),
            LogLevel::Info,
        ));
        mem::drop(config);

        if let Some(ui_state) = ui_state {
            self.apply_showfile_ui_state(ui_state);
        }
        self.show_popup(PopupSpec::with_duration(
            Duration::from_secs(2),
            "Loaded Showfile".to_string(),
        ));
    }

    fn render_pending_showfile_load_dialog(&mut self, ctx: &Context) {
        let Some(file) = self.system_ui_state.pending_load_file.clone() else {
            return;
        };

        Dialog::new("Replace Showfile".to_string(), egui::vec2(320.0, 150.0))
            .with_backdrop()
            .show(ctx, |ui| {
                ui.label("Loading this showfile will replace the current show.");
                ui.add_space(12.0);
                ui.horizontal(|ui| {
                    if components::button(ui, false, "Load", ButtonSize::Medium) {
                        self.system_ui_state.pending_load_file = None;
                        self.load_showfile_path(file.clone());
                    }
                    if components::button(ui, true, "Cancel", ButtonSize::Medium) {
                        self.system_ui_state.pending_load_file = None;
                    }
                });
            });
    }

    fn render_close_showfile_dialog(&mut self, ctx: &Context) {
        if !self.system_ui_state.close_showfile_confirm_open {
            return;
        }

        Dialog::new("Close Showfile".to_string(), egui::vec2(320.0, 150.0))
            .with_backdrop()
            .show(ctx, |ui| {
                ui.label("Close the current showfile? Unsaved changes will be lost.");
                ui.add_space(12.0);
                ui.horizontal(|ui| {
                    if components::button(ui, false, "Close", ButtonSize::Medium) {
                        self.system_ui_state.close_showfile_confirm_open = false;
                        self.close_showfile();
                    }
                    if components::button(ui, true, "Cancel", ButtonSize::Medium) {
                        self.system_ui_state.close_showfile_confirm_open = false;
                    }
                });
            });
    }

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
                            .filter(|receiver| receiver.owner_plugin_id.is_none())
                            .map(|receiver| ShowfileArtNetReceiver {
                                address: receiver.address,
                                enabled: receiver.enabled,
                            })
                            .collect(),
                    }
                };

                let showfile = config::CoreShowfile {
                    engine: SaveEngineState::from(engine_snapshot),
                    artnet: artnet_state,
                    plugin_state,
                    ui: Some(self.showfile_ui_state()),
                };

                let serialized = match serde_json::to_string_pretty(&showfile) {
                    Ok(serialized) => serialized,
                    Err(err) => {
                        tracing::error!("Failed to serialize showfile: {err}");
                        drop(config_mut);
                        self.show_popup(PopupSpec::with_duration(
                            Duration::from_secs(3),
                            format!("Failed to save showfile: {err}"),
                        ));
                        return;
                    }
                };
                if let Err(err) = std::fs::write(&path, &serialized) {
                    tracing::error!("Failed to write showfile {path:?}: {err}");
                    drop(config_mut);
                    self.show_popup(PopupSpec::with_duration(
                        Duration::from_secs(3),
                        format!("Failed to save showfile: {err}"),
                    ));
                    return;
                }
                self.last_save_time = Some(std::time::Instant::now());

                config_mut.last_open_showfile = Some(path.clone());

                let config_path = PathBuf::from(&self.data.config_path);
                if let Err(err) = config::write_config(config_path, config_mut.clone()) {
                    tracing::error!("Failed to persist showfile config: {err}");
                }

                let _ = self.data.system_message_sender.send(SystemMessage::Log(
                    format!("Saved showfile to {path:?}"),
                    LogLevel::Info,
                ));

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
                let config = self.data.config.lock().unwrap();
                let last_dir = config
                    .last_open_showfile
                    .as_ref()
                    .and_then(|path| path.parent())
                    .map(|path| path.to_path_buf());
                let config_dir = PathBuf::from_str(&self.data.config_path)
                    .ok()
                    .and_then(|path| path.parent().map(|p| p.to_path_buf()));
                let preferred_dir = self
                    .showfile_home
                    .clone()
                    .or(last_dir.clone())
                    .or(config_dir.clone());
                let places_entries = places_quick_access_entries(self.showfile_home.clone());

                let mut dialog = FileDialog::new()
                    .add_file_filter_extensions("Showfiles", vec!["json"])
                    .default_file_filter("Showfiles")
                    .set_file_icon(
                        ph::FILE_CODE,
                        Filter::new(|path: &std::path::Path| {
                            path.extension()
                                .and_then(|ext| ext.to_str())
                                .map(|ext| ext.eq_ignore_ascii_case("json"))
                                .unwrap_or(false)
                        }),
                    )
                    .default_file_icon(ph::FILE_TEXT)
                    .default_folder_icon(ph::FOLDER)
                    .device_icon(ph::HARD_DRIVE)
                    .removable_device_icon(ph::USB)
                    .parent_directory_icon(ph::ARROW_UP)
                    .back_icon(ph::ARROW_LEFT)
                    .forward_icon(ph::ARROW_RIGHT)
                    .new_folder_icon(ph::FOLDER_PLUS)
                    .menu_icon(ph::LIST_BULLETS)
                    .search_icon(ph::MAGNIFYING_GLASS)
                    .path_edit_icon(ph::PENCIL)
                    .directory_separator("›")
                    .add_quick_access("Places", move |qa| {
                        for (label, path) in places_entries {
                            qa.add_path(&label, path);
                        }
                    })
                    .as_modal(true)
                    .modal_overlay_color(egui::Color32::from_black_alpha(150))
                    .default_size(egui::vec2(900.0, 560.0))
                    .min_size(egui::vec2(700.0, 420.0))
                    .resizable(true)
                    .show_left_panel(true)
                    .show_search(false)
                    .show_path_edit_button(true)
                    .show_menu_button(true)
                    .show_new_folder_button(false)
                    .show_pinned_folders(false)
                    .show_places(false)
                    .show_devices(false)
                    .show_removable_devices(false);

                if let Some(dir) = preferred_dir.as_ref() {
                    dialog = dialog.initial_directory(dir.to_path_buf());
                }

                let mut dialog = dialog;
                dialog.pick_file();
                self.system_ui_state.open_file_dialog = Some(dialog);
                self.system_ui_state.file_dialog_open_origin = FileDialogOpenOrigin::Load;
            }

            if components::button(ui, false, "Save to Showfile", button_size) {
                let config = self.data.config.lock().unwrap();
                let last_dir = config
                    .last_open_showfile
                    .as_ref()
                    .and_then(|path| path.parent())
                    .map(|path| path.to_path_buf());
                let config_dir = PathBuf::from_str(&self.data.config_path)
                    .ok()
                    .and_then(|path| path.parent().map(|p| p.to_path_buf()));
                let preferred_dir = self
                    .showfile_home
                    .clone()
                    .or(last_dir.clone())
                    .or(config_dir.clone());
                let places_entries = places_quick_access_entries(self.showfile_home.clone());

                let mut dialog = FileDialog::new()
                    .add_file_filter_extensions("Showfiles", vec!["json"])
                    .default_file_filter("Showfiles")
                    .set_file_icon(
                        ph::FILE_CODE,
                        Filter::new(|path: &std::path::Path| {
                            path.extension()
                                .and_then(|ext| ext.to_str())
                                .map(|ext| ext.eq_ignore_ascii_case("json"))
                                .unwrap_or(false)
                        }),
                    )
                    .default_file_icon(ph::FILE_TEXT)
                    .default_folder_icon(ph::FOLDER)
                    .device_icon(ph::HARD_DRIVE)
                    .removable_device_icon(ph::USB)
                    .parent_directory_icon(ph::ARROW_UP)
                    .back_icon(ph::ARROW_LEFT)
                    .forward_icon(ph::ARROW_RIGHT)
                    .new_folder_icon(ph::FOLDER_PLUS)
                    .menu_icon(ph::LIST_BULLETS)
                    .search_icon(ph::MAGNIFYING_GLASS)
                    .path_edit_icon(ph::PENCIL)
                    .directory_separator("›")
                    .add_quick_access("Places", move |qa| {
                        for (label, path) in places_entries {
                            qa.add_path(&label, path);
                        }
                    })
                    .as_modal(true)
                    .modal_overlay_color(egui::Color32::from_black_alpha(150))
                    .default_size(egui::vec2(900.0, 560.0))
                    .min_size(egui::vec2(700.0, 420.0))
                    .resizable(true)
                    .show_left_panel(true)
                    .show_search(false)
                    .show_path_edit_button(true)
                    .show_menu_button(true)
                    .show_new_folder_button(true)
                    .show_pinned_folders(false)
                    .show_places(false)
                    .show_devices(false)
                    .show_removable_devices(false);

                if let Some(dir) = preferred_dir.as_ref() {
                    dialog = dialog.initial_directory(dir.to_path_buf());
                }

                let mut dialog = dialog;
                dialog.save_file();
                self.system_ui_state.open_file_dialog = Some(dialog);
                self.system_ui_state.file_dialog_open_origin = FileDialogOpenOrigin::Save;
            }

            {
                let conf = self.data.config.lock().unwrap();
                let button_enabled = conf.last_open_showfile.is_some();
                if components::button(ui, button_enabled, "Close Showfile", button_size)
                    && button_enabled
                {
                    self.system_ui_state.close_showfile_confirm_open = true;
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

            if components::button(ui, allowed, label, button_size) && allowed {
                self.save_showfile();
            }
        });
    }

    pub fn render_showfile_dialog(&mut self, ctx: &Context) {
        self.render_pending_showfile_load_dialog(ctx);
        self.render_close_showfile_dialog(ctx);

        let mut dialog = match self.system_ui_state.open_file_dialog.take() {
            Some(dialog) => dialog,
            None => return,
        };
        let popup_dialog = !self.desktop_mode;

        if popup_dialog {
            let viewport = ctx.viewport_rect().size();
            let max_size = egui::vec2((viewport.x - 32.0).max(0.0), (viewport.y - 32.0).max(0.0));
            let desired = egui::vec2(900.0, 560.0);
            let size = egui::vec2(desired.x.min(max_size.x), desired.y.min(max_size.y));
            let config = dialog.config_mut();
            config.title_bar = false;
            config.resizable = false;
            config.movable = false;
            config.anchor = Some((egui::Align2::CENTER_CENTER, egui::vec2(0.0, 0.0)));
            config.default_size = size;
            config.min_size = size;
            config.max_size = Some(size);
        }

        let old_style = ctx.global_style().clone();
        let mut dialog_style = (*old_style).clone();
        dialog_style.visuals = egui::Visuals::dark();
        dialog_style.visuals.window_fill = egui::Color32::from_rgb(16, 18, 22);
        dialog_style.visuals.panel_fill = egui::Color32::from_rgb(19, 22, 26);
        dialog_style.visuals.faint_bg_color = egui::Color32::from_rgb(28, 32, 38);
        dialog_style.visuals.extreme_bg_color = egui::Color32::from_rgb(11, 13, 17);
        dialog_style.visuals.override_text_color = Some(egui::Color32::from_gray(235));
        dialog_style.visuals.window_corner_radius = egui::CornerRadius::same(8);
        dialog_style.spacing.interact_size.y = 30.0;
        dialog_style.spacing.item_spacing = egui::vec2(6.0, 8.0);
        dialog_style.spacing.button_padding = egui::vec2(6.0, 6.0);
        dialog_style.spacing.window_margin = egui::Margin::same(10);
        dialog_style.text_styles.insert(
            egui::TextStyle::Body,
            FontId::new(18.0, FontFamily::Proportional),
        );
        dialog_style.text_styles.insert(
            egui::TextStyle::Button,
            FontId::new(15.0, FontFamily::Proportional),
        );
        ctx.set_global_style(dialog_style);

        dialog.update(ctx);

        let picked = dialog.take_picked();

        ctx.set_global_style(old_style);

        if picked.is_none() {
            self.system_ui_state.open_file_dialog = Some(dialog);
        }

        if let Some(file) = picked {
            let mut config = self.data.config.lock().unwrap();

            self.system_ui_state.open_file_dialog = None;

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
                    mem::drop(config);
                    self.system_ui_state.pending_load_file = Some(file.to_path_buf());
                }
            }
        }
    }
}
