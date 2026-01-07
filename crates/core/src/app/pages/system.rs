use std::{
    ffi::OsStr,
    mem,
    path::{Path, PathBuf},
    process::Command,
    str::FromStr,
    time::Duration,
};

use blaulicht_shared::LogLevel;
use egui::{Color32, Context, FontId, RichText, ThemePreference};
use egui_file::FileDialog;

use crate::{
    app::{
        components::{self, ButtonSize},
        ui::FileDialogOpenOrigin,
        BlaulichtApp, PopupSpec,
    },
    audio::defs::AudioThreadControlSignal,
    config,
    mainloop::DMX_TICK_TIME,
    msg::{FromFrontend, SystemMessage},
};

impl BlaulichtApp {
    //
    // TODO: i need to remove this
    //
    fn save_showfile(&mut self) {
        let mut config_mut = self.data.config.lock().unwrap();

        match config_mut.last_open_showfile.clone() {
            Some(ref path) => {
                let mut dmx = self.data.state.dmx_engine.write().unwrap();

                {
                    let plugin_state = self.data.state.plugin_state_storage.lock().unwrap();
                    dmx.0.plugin_state = plugin_state.clone();
                }

                // let string = ron::to_string(&dmx.clone()).unwrap();
                // let pretty = PrettyConfig::new()
                //     .indentor("    ".to_owned())
                //     .new_line("\n".to_owned());

                // let string = serde_json::to_string_pretty(&dmx.clone()).unwrap();

                // let string = ron::ser::to_string_pretty(&dmx.clone(), pretty).unwrap();
                // let string = ron::to_string(&dmx.clone()).unwrap();
                let string = blaulicht_shared::save::engine_state_to_json(dmx.0.clone()).unwrap();
                println!("STRING: {string}");

                // let serialized = postcard::to_allocvec(&dmx.clone()).unwrap();
                std::fs::write(path, &string).unwrap();

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
                mem::drop(dmx);

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

    pub fn system_ui(&mut self, ui: &mut egui::Ui, ctx: &Context) {
        if let Some(dialog) = &mut self.open_file_dialog {
            if dialog.show(ctx).selected() {
                let mut config = self.data.config.lock().unwrap();

                if let Some(file) = dialog.path() {
                    match self.file_dialog_open_origin {
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
                            config::read_showfile(
                                file.to_path_buf(),
                                &mut dmx,
                                &self.data.state.plugin_state_storage,
                                self.data.system_message_sender.clone(),
                            );
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

        let button_size = ButtonSize::Medium.with_width(110.0);

        ui.vertical_centered(|ui| {
            ui.horizontal(|ui| {
                let showfile = self
                    .data
                    .config
                    .lock()
                    .unwrap()
                    .last_open_showfile
                    .as_ref()
                    .map(|s| s.to_string_lossy().to_string())
                    .unwrap_or_else(|| "N/A".to_string());

                ui.label("showfile:");

                ui.label(
                    egui::RichText::new(showfile)
                        .strong()
                        .font(FontId::monospace(14.0)),
                );
            });

            ui.add_space(3.0);

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
                    self.open_file_dialog = Some(dialog);
                    self.file_dialog_open_origin = FileDialogOpenOrigin::Load;
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
                    self.open_file_dialog = Some(dialog);
                    self.file_dialog_open_origin = FileDialogOpenOrigin::Save;
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
                        config::close_showfile(&mut dmx, &self.data.state.plugin_state_storage);
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

            ui.add_space(15.0);
            ui.separator();
            ui.add_space(15.0);

            ui.horizontal(|ui| {
                if components::button(
                    ui,
                    !ui.ctx().style().visuals.dark_mode,
                    "LIGHT",
                    button_size,
                ) {
                    ui.ctx().set_theme(ThemePreference::Light);
                }

                if components::button(ui, ui.ctx().style().visuals.dark_mode, "DARK", button_size) {
                    ui.ctx().set_theme(ThemePreference::Dark);
                }
            });

            ui.add_space(15.0);
            ui.separator();
            ui.add_space(15.0);

            ui.horizontal(|ui| {
                if components::button(ui, false, "Quit", button_size) {
                    ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                }

                if components::button(ui, false, "Shutdown", button_size) {
                    self.confirm_shutdown_open = true;
                }

                if components::button(ui, self.debug_open, "Debug", button_size) {
                    self.debug_open = !self.debug_open;
                }
            });

            // --- Loop Speed & Tick Speed Graphs ---
            ui.vertical(|ui| {
                ui.horizontal(|ui| {
                    let graph_width = (ui.available_width() - 16.0) / 2.0;
                    let graph_height = 36.0;
                    // Loop Speed Graph
                    let (loop_resp, loop_painter) = ui.allocate_painter(
                        egui::vec2(graph_width, graph_height),
                        egui::Sense::hover(),
                    );
                    // Draw value and label above the bar, left-aligned
                    let loop_val_str = match self.loop_speed {
                        v if v > 1000 => format!("{:.0} ms", self.loop_speed / 1000),
                        _ => format!("{:.0} us", self.loop_speed),
                    };
                    let label_y = loop_resp.rect.top() + 2.0;
                    let label_x = loop_resp.rect.left() + 4.0;
                    loop_painter.text(
                        egui::pos2(label_x, label_y),
                        egui::Align2::LEFT_TOP,
                        &loop_val_str,
                        egui::FontId::monospace(10.0),
                        Color32::WHITE,
                    );
                    loop_painter.text(
                        egui::pos2(label_x + 60.0, label_y), // 60px offset for value
                        egui::Align2::LEFT_TOP,
                        "Loop",
                        egui::FontId::proportional(10.0),
                        Color32::WHITE,
                    );
                    // Draw loop speed as a bar (simple visualization)
                    let loop_val = self.loop_speed as f32;
                    let loop_max = DMX_TICK_TIME.as_micros() as f32;
                    let loop_bar_width = (loop_val / loop_max).min(1.0) * (graph_width - 8.0);
                    let loop_bar_rect = egui::Rect::from_min_max(
                        loop_resp.rect.left_top() + egui::vec2(4.0, 16.0),
                        loop_resp.rect.left_top()
                            + egui::vec2(4.0 + loop_bar_width, graph_height - 8.0),
                    );
                    let color = match self.loop_speed {
                        v if v as f32 > loop_max => Color32::RED,
                        _ => Color32::from_rgb(0, 200, 255),
                    };
                    loop_painter.rect_filled(loop_bar_rect, 2.0, color);

                    // Tick Speed Graph
                    let (tick_resp, tick_painter) = ui.allocate_painter(
                        egui::vec2(graph_width, graph_height),
                        egui::Sense::hover(),
                    );
                    let tick_val_str = format!("{:.0} μs", self.tick_speed);
                    let tick_label_y = tick_resp.rect.top() + 2.0;
                    let tick_label_x = tick_resp.rect.left() + 4.0;
                    tick_painter.text(
                        egui::pos2(tick_label_x, tick_label_y),
                        egui::Align2::LEFT_TOP,
                        &tick_val_str,
                        egui::FontId::monospace(10.0),
                        Color32::WHITE,
                    );
                    tick_painter.text(
                        egui::pos2(tick_label_x + 60.0, tick_label_y),
                        egui::Align2::LEFT_TOP,
                        "Tick",
                        egui::FontId::proportional(10.0),
                        Color32::WHITE,
                    );
                    let tick_val = self.tick_speed as f32;
                    let tick_max = 1000.0; // 1 MS
                    let tick_bar_width = (tick_val / tick_max).min(1.0) * (graph_width - 8.0);
                    let tick_bar_rect = egui::Rect::from_min_max(
                        tick_resp.rect.left_top() + egui::vec2(4.0, 16.0),
                        tick_resp.rect.left_top()
                            + egui::vec2(4.0 + tick_bar_width, graph_height - 8.0),
                    );
                    tick_painter.rect_filled(tick_bar_rect, 2.0, Color32::from_rgb(255, 200, 0));
                });

                ui.add_space(8.0);

                // --- Heartbeat Indicator ---
                ui.horizontal(|ui| {
                    // Heartbeat: small circle, green if recent, gray if not
                    let heartbeat_recent = self.last_heartbeat_frame + 30 > self.frame_count;
                    let color = if heartbeat_recent {
                        Color32::from_rgb(0, 255, 0)
                    } else {
                        Color32::from_gray(80)
                    };
                    ui.painter().circle_filled(
                        ui.cursor().left_top() + egui::vec2(10.0, 10.0),
                        8.0,
                        color,
                    );
                    ui.label("Heartbeat");
                });

                // --- Reload button ---
                {
                    let signal = *self.data.state.mainloop_state.read().unwrap();

                    ui.label(format!("LOOP {:?}", signal));

                    let bg_color = match signal {
                        AudioThreadControlSignal::CONTINUE => egui::Color32::from_gray(40),
                        AudioThreadControlSignal::ABORT
                        | AudioThreadControlSignal::ABORTED
                        | AudioThreadControlSignal::CRASHED => egui::Color32::DARK_RED,
                        AudioThreadControlSignal::RELOAD => egui::Color32::from_rgb(60, 120, 200),
                    };

                    let rect = ui.allocate_exact_size(egui::vec2(90.0, 60.0), egui::Sense::click());
                    let painter = ui.painter();
                    painter.rect_filled(rect.0, 0.0, bg_color);

                    painter.text(
                        rect.0.center(),
                        egui::Align2::CENTER_CENTER,
                        "RELOAD",
                        egui::FontId::proportional(16.0),
                        egui::Color32::WHITE,
                    );

                    if rect.1.clicked() && signal != AudioThreadControlSignal::RELOAD {
                        self.data
                            .from_frontend_sender
                            .send(FromFrontend::Reload)
                            .unwrap();

                        self.show_popup(PopupSpec::with_duration(
                            Duration::from_secs(2),
                            "Reload in progress...".to_string(),
                        ));
                    }
                }
            });
        });

        if self.confirm_shutdown_open {
            components::dialog(
                ctx,
                "Confirm Shutdown",
                egui::vec2(200.0, 100.0),
                false,
                |ui| {
                    let blink = ((self.animation_time * 4.0) as i32) % 2 == 0;

                    ui.heading(
                        RichText::new("Confirm Shutdown")
                            .color(if blink {
                                Color32::RED
                            } else {
                                ui.visuals().text_color()
                            })
                            .strong(),
                    );

                    ui.add_space(12.0);

                    ui.horizontal(|ui| {
                        if components::button(ui, false, "Confirm", ButtonSize::Large) {
                            let status = Command::new("/usr/bin/shutdown.sh")
                                .status()
                                .expect("Failed to execute shutdown command");

                            if status.success() {
                                println!("Shutdown command executed successfully.");
                            } else {
                                eprintln!("Shutdown command failed!");
                            }

                            self.confirm_shutdown_open = false;
                        }

                        if components::button(ui, true, "Cancel", ButtonSize::Large) {
                            self.confirm_shutdown_open = false;
                        }
                    });
                },
            );
        }

        ui.separator();

        egui::SidePanel::right("right_panel")
            .resizable(true)
            .default_width(250.0)
            .width_range(200.0..=400.0)
            .show(ctx, |ui| {
                // --- Plugin Overview ---
                ui.label("Plugins");

                ui.add_space(4.0);

                {
                    let plugins = self.data.state.plugins.read().unwrap();
                    let current_visibility =
                        self.data.state.plugin_ui_visibility.read().unwrap().clone();

                    for (i, (id, plugin)) in plugins.iter().enumerate() {
                        let box_size = egui::vec2(ui.available_width(), 42.0);
                        ui.allocate_ui_with_layout(
                            box_size,
                            egui::Layout::top_down(egui::Align::Center),
                            |ui| {
                                let (rect, _response) =
                                    ui.allocate_exact_size(box_size, egui::Sense::empty());
                                let painter = ui.painter();

                                // State color and blinking logic
                                let mut show_border = true;
                                let border_color = match (plugin.has_errored(), plugin.is_enabled())
                                {
                                    // Alive and healthy.
                                    (false, true) => egui::Color32::from_rgb(0, 200, 0),
                                    // Dead, crashed.
                                    (true, true) => {
                                        let blink = ((self.animation_time * 8.0) as i32) % 2 == 0;
                                        show_border = blink;
                                        egui::Color32::from_rgb(200, 0, 0)
                                    }
                                    // Disabled.
                                    (_, false) => {
                                        let blink = ((self.animation_time * 2.0) as i32) % 2 == 0;
                                        show_border = blink;
                                        egui::Color32::from_rgb(200, 200, 0)
                                    }
                                };

                                // Draw the main box
                                painter.rect_filled(rect, 0.0, egui::Color32::from_gray(30));

                                // Draw the left border if needed
                                let border_width = 6.0;
                                if show_border {
                                    let border_rect = egui::Rect::from_min_max(
                                        rect.left_top(),
                                        rect.left_bottom() + egui::vec2(border_width, 0.0),
                                    );
                                    painter.rect_filled(border_rect, 0.0, border_color);
                                }

                                // Plugin name
                                let path_str = plugin.path.to_string().to_string();
                                let basename =
                                    Path::new(&path_str).file_name().unwrap().to_string_lossy();
                                let name = format!("P:{basename} ({})", i + 1);

                                let text_padding = 5.0;

                                painter.text(
                                    rect.left_center()
                                        + egui::vec2(border_width + text_padding, 0.0),
                                    egui::Align2::LEFT_CENTER,
                                    name,
                                    egui::FontId::monospace(12.0),
                                    if plugin.has_errored() {
                                        Color32::WHITE
                                    } else {
                                        egui::Color32::from_gray(90)
                                    },
                                );
                            },
                        );
                        ui.horizontal(|ui| {
                            let is_open = *current_visibility.get(id).unwrap_or(&false);
                            let label = if is_open { "Hide UI" } else { "Show UI" };
                            if ui.small_button(label).clicked() {
                                let mut map = self.data.state.plugin_ui_visibility.write().unwrap();
                                let entry = map.entry(*id).or_insert(false);
                                *entry = !*entry;
                            }
                        });
                        ui.add_space(8.0);
                    }
                    ui.separator();

                    mem::drop(plugins)
                }
            });
    }
}
