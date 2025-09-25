use crate::app::components::ButtonSize;
use crate::app::{components, AnimationPageState, AppPage, BlaulichtApp, PopupSpec};
use crate::audio::defs::AudioThreadControlSignal;
use crate::dmx::animation::{MathematicalBaseFunction, PhaserDuration};
use crate::dmx::EngineState;
use crate::msg::FromFrontend;
use crate::{config, utils};
use crate::{msg::SystemMessage, state::AppStateWrapper};
use blaulicht_shared::LogLevel;
use cpal::traits::DeviceTrait;
use crossbeam_channel::TryRecvError;
use egui::{
    Button, Color32, Context, CornerRadius, FontId, Frame, Margin, Painter, ProgressBar, Rect,
    RichText, Rounding, Sense, Stroke, Style, TextStyle, Ui, Vec2,
};
use egui_file::FileDialog;
use std::ffi::OsStr;
use std::fmt::Display;
use std::fs::{self, File};
use std::io::Read;
use std::mem;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::str::FromStr;
use std::sync::Arc;
use std::time::{Duration, Instant};
use strum::IntoEnumIterator;

pub enum FileDialogOpenOrigin {
    Save,
    Load,
}

//
// Actual eframe shit.
//

// fn update_button_style(ctx: &egui::Context) {
//     let mut style = (*ctx.style()).clone();
//
//     // Base look
//     style.visuals.widgets.inactive.bg_fill = egui::Color32::from_rgb(220, 220, 220); // idle
//     style.visuals.widgets.hovered.bg_fill = egui::Color32::from_rgb(180, 200, 255); // hover
//     style.visuals.widgets.active.bg_fill = egui::Color32::from_rgb(0, 120, 255); // when pressed
//
//     ctx.set_style(style);
// }

const UI_RECV_KEY: &'static str = "eframe_ui";

impl Drop for BlaulichtApp {
    fn drop(&mut self) {
        // let mut consumers = self.data.to_frontend_consumers.lock().unwrap();
        // let removed = consumers.remove(UI_RECV_KEY);
        // debug_assert!(removed.is_some());
    }
}

impl BlaulichtApp {
    /// Called once before the first frame.
    pub fn new(cc: &eframe::CreationContext<'_>, state: AppStateWrapper) -> Self {
        // This is also where you can customize the look and feel of egui using
        // `cc.egui_ctx.set_visuals` and `cc.egui_ctx.set_fonts`.

        // Load previous app state (if any).
        // Note that you must enable the `persistence` feature for this to work.
        // let mut app: TemplateApp = if let Some(storage) = cc.storage {
        //     eframe::get_value(storage, eframe::APP_KEY).unwrap_or_default()
        // } else {
        //     Default::default()
        // };

        // let (sender, receiver) = crossbeam_channel::unbounded();
        {
            // let mut consumers = state.to_frontend_consumers.lock().unwrap();
            // consumers.insert(UI_RECV_KEY.to_string(), sender);
        }

        let mut app = Self::new_default(state);

        cc.egui_ctx.set_pixels_per_point(1.0);

        // Initialize animation time
        app.animation_time = 0.0;

        app
    }

    fn show_popup(&mut self, popup: PopupSpec) {
        self.popup = Some(popup);
        self.popup_open_time = Instant::now();
    }

    fn close_popup(&mut self) {
        self.popup = None;
    }

    // `ui` is your egui Ui, `progress` is a value between 0.0 and 1.0
    fn draw_progress_bar(ui: &mut Ui, progress: f32, height: f32, text: &str) {
        // Allocate space for the progress bar
        let (rect, _response) =
            ui.allocate_exact_size(Vec2::new(ui.available_width(), height), Sense::hover());

        let painter: &Painter = ui.painter();

        // Draw the background (non-rounded)
        painter.rect_filled(rect, 0.0, Color32::from_rgb(30, 30, 30));

        // Draw the filled portion
        let fill_rect = Rect::from_min_max(
            rect.min,
            egui::pos2(rect.min.x + rect.width() * progress, rect.max.y),
        );
        painter.rect_filled(fill_rect, 0.0, Color32::from_rgb(60, 120, 200));

        // Optional: draw percentage text
        painter.text(
            rect.center(),
            egui::Align2::CENTER_CENTER,
            text,
            FontId::proportional(12.0),
            Color32::WHITE,
        );
    }

    fn render_popup(&mut self, ctx: &egui::Context) {
        if let Some(ref popup) = self.popup {
            let popup = popup.clone();

            let screen_rect = ctx.screen_rect();
            let popup_size = egui::Vec2::new(200.0, 100.0); // desired popup size

            let center_pos = egui::Pos2::new(
                screen_rect.center().x - popup_size.x / 2.0,
                screen_rect.center().y - popup_size.y / 2.0,
            );

            let elapsed = self.popup_open_time.elapsed();
            if elapsed >= popup.lifetime_duration {
                self.close_popup();
            }

            egui::Window::new(&popup.label)
                .fixed_size(popup_size)
                .collapsible(false)
                .resizable(false)
                .title_bar(false)
                .fixed_pos(center_pos)
                .frame(Frame {
                    corner_radius: CornerRadius::same(1),
                    fill: Color32::from_gray(40),
                    stroke: Stroke::new(1.0, Color32::from_gray(60)),
                    inner_margin: Margin::symmetric(6, 12),
                    ..Frame::default()
                })
                .show(ctx, |ui| {
                    ui.vertical_centered(|ui| {
                        ui.label(RichText::new(&popup.label).size(24.0));

                        if let Some(ref btn) = popup.button {
                            ui.add_space(8.0);

                            if components::button(ui, false, &btn.label, ButtonSize::Large) {
                                self.close_popup();
                            }

                            ui.add_space(8.0);
                        }

                        let progress = 1.0
                            - elapsed.as_millis() as f32
                                / popup.lifetime_duration.as_millis() as f32;

                        let text = format!(
                            "{} seconds remaining",
                            popup.lifetime_duration.as_secs() - elapsed.as_secs()
                        );

                        Self::draw_progress_bar(ui, progress, 18.0, &text);
                    })
                });
        }
    }
}

impl eframe::App for BlaulichtApp {
    /// Called each time the UI needs repainting, which may be many times per second.
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        self.render_popup(ctx);
        // for i in 0..1000 {
        //     self.data
        //         .event_bus_connection
        //         .send(ControlEventMessage::new(
        //             EventOriginator::Web,
        //             ControlEvent::SetColor((0, 0, 0)),
        //         ));
        // }
        // println!("loop");
        // ctx.request_repaint_after(std::time::Duration::from_millis(16)); // ~60 FPS
        // return;

        //
        // At the beginning, process any incoming state changes.
        //

        // let mut new_signal = false;
        let mut empty = 0;
        loop {
            match self.data.event_bus_connection.try_recv() {
                Some(bus) => println!("bus: {:?}", bus),
                None => {
                    empty += 1;
                }
            }

            // match self.data.signal_receiver.try_recv() {
            //     Ok(signal) => {
            //         // self.collector.signal(signal);
            //         // new_signal = true;
            //     }
            //     Err(TryRecvError::Empty) => {
            //         empty += 1;
            //     }
            //     Err(TryRecvError::Disconnected) => unreachable!("CANNOT REACH"),
            // }

            match self.data.system_message_receiver.try_recv() {
                Ok(sys) => match sys {
                    SystemMessage::Heartbeat(_) => {
                        self.last_heartbeat_frame = self.frame_count;
                        // self.log_window.add_log(
                        //     LogLevel::Debug,
                        //     "Heartbeat received".to_string(),
                        //     "System".to_string(),
                        // );
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
                    SystemMessage::WasmControlsLog(wasm_controls_log) => {
                        self.log_window.add_log(
                            LogLevel::Debug,
                            format!("WASM Controls: {:?}", wasm_controls_log),
                            "WASM".to_string(),
                        );
                    }
                    SystemMessage::WasmControlsSet(wasm_controls_set) => {
                        self.log_window.add_log(
                            LogLevel::Info,
                            format!("WASM Controls Set: {:?}", wasm_controls_set),
                            "WASM".to_string(),
                        );
                    }
                    SystemMessage::WasmControlsConfig(wasm_controls_config) => {
                        self.log_window.add_log(
                            LogLevel::Info,
                            format!("WASM Controls Config: {:?}", wasm_controls_config),
                            "WASM".to_string(),
                        );
                    }
                    SystemMessage::LoopSpeed(duration) => {
                        self.loop_speed = duration.as_micros() as usize;
                        // self.log_window.add_log(
                        //     LogLevel::Debug,
                        //     format!("Loop speed: {} μs", duration.as_micros()),
                        //     "Performance".to_string(),
                        // );
                    }
                    SystemMessage::TickSpeed(duration) => {
                        self.tick_speed = duration.as_micros() as usize;
                        // self.log_window.add_log(
                        //     LogLevel::Debug,
                        //     format!("Tick speed: {} μs", duration.as_micros()),
                        //     "Performance".to_string(),
                        // );
                    }
                    SystemMessage::AudioSelected(device) => {
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
                    SystemMessage::DMX(dmx_msg) => {
                        // self.log_window.add_log(
                        //     LogLevel::Info,
                        //     format!("DMX message: {:?}", dmx_msg),
                        //     "DMX".to_string(),
                        // );
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

        // Update all graphs with current data
        {
            let audio_data = self.data.state.audio_snapshot.read().unwrap();
            self.volume_graph.update(audio_data.volume as i32);
            self.volume_graph.update(audio_data.volume as i32);
            self.beat_volume_graph.update(audio_data.beat_volume as i32);
            self.bass_graph.update(audio_data.bass as i32);
            self.bass_avg_graph.update(audio_data.bass_avg as i32);
            self.bass_avg_short_graph
                .update(audio_data.bass_avg_short as i32);
            self.bpm_graph.update(audio_data.bpm as i32);
            self.time_between_beats_graph
                .update(audio_data.time_between_beats_millis as i32);
        }

        // Put your widgets into a `SidePanel`, `TopBottomPanel`, `CentralPanel`, `Window` or `Area`.
        // For inspiration and more examples, go to https://emilk.github.io/egui

        // egui::TopBottomPanel::top("top_panel").show(ctx, |ui| {
        // The top panel is often a good place for a menu bar:

        //     egui::MenuBar::new().ui(ui, |ui| {
        //});
        // });

        // Left sidebar
        self.left_panel_ui(ctx);

        egui::CentralPanel::default().show(ctx, |ui| {
            // The central panel the region left after adding TopPanel's and SidePanel's
            // ui.heading("eframe template");
            //
            // Continuous rendering - always request repaints
            ctx.request_repaint_after(std::time::Duration::from_millis(16)); // ~60 FPS
                                                                             //
                                                                             // // Update animation time for continuous rendering
            self.frame_count += 1;
            self.animation_time += 0.016; // 16ms = 0.016 seconds
                                          //
                                          // ui.horizontal(|ui| {
                                          //     ui.label("Write something: ");
                                          //     ui.text_edit_singleline(&mut self.label);
                                          // });
                                          //
                                          // ui.add(egui::Slider::new(&mut self.value, 0.0..=10.0).text("value"));
                                          // if ui.button("Increment").clicked() {
                                          //     self.value += 1.0;
                                          // }
                                          //
                                          // ui.separator();

            // Page content based on selected tab
            match self.current_page {
                AppPage::Logs => {
                    self.logs_ui(ui, ctx);
                }
                AppPage::System => {
                    self.system_ui(ui, ctx);
                }
                AppPage::Audio => {
                    self.main_ui(ui, ctx);
                }
                AppPage::FixturesSetup => {
                    self.fixtures_ui_setup(ui, ctx);
                }
                AppPage::FixturesPerformance => {
                    self.fixtures_ui(ui, ctx);
                }
                AppPage::Animations => {
                    self.animations_ui(ui);
                }
            }

            // ui.separator();
            //
            // ui.add(egui::github_link_file!(
            //     "https://github.com/emilk/eframe_template/blob/main/",
            //     "Source code."
            // ));
            //
            // ui.with_layout(egui::Layout::bottom_up(egui::Align::LEFT), |ui| {
            //     powered_by_egui_and_eframe(ui);
            //     egui::warn_if_debug_build(ui);
            // });
        });
    }
}

impl BlaulichtApp {
    fn main_ui(&mut self, ui: &mut egui::Ui, ctx: &egui::Context) {
        // Main content area with graphs panel
        ui.horizontal(|ui| {
            // Left content area (3/4 width)
            let total_width = ui.available_width();
            let graph_panel_width = total_width / 3.0;
            // let main_panel_width = total_width - graph_panel_width - 16.0; // 16px for separator

            // ui.vertical(|ui| {
            //     ui.set_width(main_panel_width);
            //     ui.heading("Main Content");
            //     ui.label("This is the main content area taking up 3/4 of the width.");
            //     ui.add_space(20.0);
            //     ui.label("You can put your main application content here.");
            // });

            ui.allocate_ui_with_layout(
                egui::vec2(ui.available_width(), ui.available_height()),
                egui::Layout::top_down(egui::Align::LEFT),
                |ui| {
                    let mut selected_device =
                        self.data.state.audio.read().unwrap().device_name.clone();

                    let selected_device_label =
                        selected_device.clone().unwrap_or_else(|| "N/A".to_string());

                    let before = selected_device.clone();

                    ui.horizontal_centered(|ui| {
                        ui.set_min_height(ButtonSize::Medium.dim().0.y);

                        ui.label("Audio");

                        ui.separator();

                        if components::button(ui, false, "Change Device", ButtonSize::Medium) {
                            self.set_audio_device_popup_open = true;
                        }

                        ui.separator();

                        ui.label(RichText::new("Current Input:").size(ButtonSize::Medium.dim().1));
                        ui.label(
                            RichText::new(selected_device_label)
                                .size(ButtonSize::Medium.dim().1)
                                .color(Color32::LIGHT_RED),
                        );
                    });

                    if self.set_audio_device_popup_open {
                        const NONE_LABEL: &str = "None";

                        let mut options = self.available_audio_devices.clone();
                        debug_assert!(!options.contains(&NONE_LABEL.to_string()));
                        options.push(NONE_LABEL.to_string());

                        let (new_device, changed) = components::selection_dialog(
                            ctx,
                            options,
                            match selected_device.clone() {
                                Some(val) => val,
                                None => NONE_LABEL.to_string(),
                            },
                            &mut self.set_audio_device_popup_open,
                        );

                        if changed {
                            selected_device = match new_device.as_str() {
                                NONE_LABEL => None,
                                other => Some(other.to_string()),
                            };
                        }
                    }

                    if selected_device != before {
                        let new_dev = selected_device.map(|d| utils::device_from_name(d).unwrap());
                        self.data
                            .from_frontend_sender
                            .send(FromFrontend::SelectInputDevice(new_dev.clone()))
                            .unwrap();

                        let mut config_mut = self.data.config.lock().unwrap();

                        config_mut.default_audio_device = new_dev.map(|d| d.name().unwrap());

                        let path = PathBuf::from_str(&self.data.config_path).unwrap();
                        config::write_config(path, config_mut.clone()).unwrap();
                    }

                    // Set larger graph height
                    let graph_height = 140.0;
                    let graph_width = graph_panel_width - 5.0;
                    let padding = 10.0;

                    debug_assert!(graph_width > 0.0);

                    ui.separator();

                    ui.horizontal(|ui| {
                        ui.vertical(|ui| {
                            let (response, painter) = ui.allocate_painter(
                                egui::vec2(graph_width, graph_height),
                                egui::Sense::hover(),
                            );
                            self.volume_graph.draw(painter, response.rect);

                            ui.add_space(padding);
                            let (response_beat_volume, painter_beat_volume) = ui.allocate_painter(
                                egui::vec2(graph_width, graph_height),
                                egui::Sense::hover(),
                            );
                            self.beat_volume_graph
                                .draw(painter_beat_volume, response_beat_volume.rect);

                            ui.add_space(padding);
                            let (response_bass, painter_bass) = ui.allocate_painter(
                                egui::vec2(graph_width, graph_height),
                                egui::Sense::hover(),
                            );
                            self.bass_graph.draw(painter_bass, response_bass.rect);
                        });

                        ui.vertical(|ui| {
                            let (response_bass_avg, painter_bass_avg) = ui.allocate_painter(
                                egui::vec2(graph_width, graph_height),
                                egui::Sense::hover(),
                            );
                            self.bass_avg_graph
                                .draw(painter_bass_avg, response_bass_avg.rect);

                            ui.add_space(padding);
                            let (response_bass_avg_short, painter_bass_avg_short) = ui
                                .allocate_painter(
                                    egui::vec2(graph_width, graph_height),
                                    egui::Sense::hover(),
                                );
                            self.bass_avg_short_graph
                                .draw(painter_bass_avg_short, response_bass_avg_short.rect);
                        });

                        ui.vertical(|ui| {
                            let (response_bpm, painter_bpm) = ui.allocate_painter(
                                egui::vec2(graph_width, graph_height),
                                egui::Sense::hover(),
                            );
                            self.bpm_graph.draw(painter_bpm, response_bpm.rect);

                            ui.add_space(padding);
                            let (response_time_between_beats, painter_time_between_beats) = ui
                                .allocate_painter(
                                    egui::vec2(graph_width, graph_height),
                                    egui::Sense::hover(),
                                );
                            self.time_between_beats_graph
                                .draw(painter_time_between_beats, response_time_between_beats.rect);
                        })
                    });
                },
            );
        });
    }

    fn logs_ui(&mut self, ui: &mut egui::Ui, ctx: &Context) {
        self.log_window.draw(ctx, ui);

        // Terminal.
        // Right sidebar
        // Bottom log panel - fixed height
        // egui::TopBottomPanel::bottom("log_panel")
        //     .default_height(300.0)
        //     .show(ctx, |ui| {
        //     });
    }

    fn save_showfile(&mut self) {
        let mut config_mut = self.data.config.lock().unwrap();

        match config_mut.last_open_showfile.clone() {
            Some(ref path) => {
                let dmx = self.data.state.dmx_engine.read().unwrap();
                let serialized = postcard::to_allocvec(&dmx.clone()).unwrap();
                std::fs::write(&path, serialized).unwrap();

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

    fn system_ui(&mut self, ui: &mut egui::Ui, ctx: &Context) {
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

                            let mut f = File::open(file).expect("no file found");
                            let metadata = fs::metadata(file).expect("unable to read metadata");
                            let mut buffer = vec![0; metadata.len() as usize];
                            f.read(&mut buffer).expect("buffer overflow");

                            let decoded: EngineState = postcard::from_bytes(&buffer).unwrap();
                            let mut dmx = self.data.state.dmx_engine.write().unwrap();
                            // dmx.overwrite(decoded);
                            dmx.load_showfile(decoded);
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

        let button_size = ButtonSize::Medium.with_width(100.0);

        ui.vertical(|ui| {
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

                ui.label("Showfile:");
                ui.label(showfile);
            });

            if components::button(ui, false, "Load Showfile", button_size) {
                // Show only files with the extension "txt".
                let filter = Box::new({
                    let ext = Some(OsStr::new("txt"));
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
                // Show only files with the extension "txt".
                let filter = Box::new({
                    let ext = Some(OsStr::new("txt"));
                    move |path: &Path| -> bool { path.extension() == ext }
                });

                let config = self.data.config.lock().unwrap();

                let mut dialog = FileDialog::open_file(config.last_open_showfile.clone())
                    .show_files_filter(filter);

                dialog.open();
                self.open_file_dialog = Some(dialog);
                self.file_dialog_open_origin = FileDialogOpenOrigin::Save;
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
                false => ("Sace Showfile", false),
            };
            if components::button(ui, allowed, label, button_size) {
                self.save_showfile();
            }
        });

        if components::button(ui, false, "Quit", button_size) {
            ctx.send_viewport_cmd(egui::ViewportCommand::Close);
        }

        if components::button(ui, false, "Shutdown", button_size) {
            self.confirm_shutdown_open = true;
        }

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
                            let status = Command::new("sudo")
                                .arg("/sbin/shutdown")
                                .arg("-h")
                                .arg("now")
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

        ui.add_space(16.0);

        egui::widgets::global_theme_preference_buttons(ui);

        egui::SidePanel::right("right_panel")
            .resizable(true)
            .default_width(250.0)
            .width_range(200.0..=400.0)
            .show(ctx, |ui| {
                // --- Plugin Overview ---
                ui.heading("Plugins");
                ui.add_space(4.0);

                {
                    let plugins = self.data.state.plugins.read().unwrap();

                    for (i, (_id, plugin)) in plugins.iter().enumerate() {
                        let box_size = egui::vec2(ui.available_width(), 42.0);
                        ui.allocate_ui_with_layout(
                            box_size,
                            egui::Layout::top_down(egui::Align::Center),
                            |ui| {
                                let (rect, _response) =
                                    ui.allocate_exact_size(box_size, egui::Sense::hover());
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
                                    // Disabled
                                    (_, false) => {
                                        let blink = ((self.animation_time * 2.0) as i32) % 2 == 0;
                                        show_border = blink;
                                        egui::Color32::from_rgb(200, 200, 0)
                                    }
                                };

                                // Draw the main box
                                painter.rect_filled(rect, 0.0, egui::Color32::from_gray(30));

                                // Draw the left border if needed
                                if show_border {
                                    let border_width = 6.0;
                                    let border_rect = egui::Rect::from_min_max(
                                        rect.left_top(),
                                        rect.left_bottom() + egui::vec2(border_width, 0.0),
                                    );
                                    painter.rect_filled(border_rect, 0.0, border_color);
                                }

                                // Plugin name (use a placeholder if you can't access the path)
                                let name = format!("P:{} ({})", plugin.path, i + 1);
                                painter.text(
                                    rect.center(),
                                    egui::Align2::CENTER_CENTER,
                                    name,
                                    egui::FontId::monospace(10.0),
                                    if plugin.has_errored() {
                                        Color32::WHITE
                                    } else {
                                        egui::Color32::from_gray(90)
                                    },
                                );
                            },
                        );
                        ui.add_space(8.0);
                    }
                    ui.separator();

                    mem::drop(plugins)
                }

                // --- Loop Speed & Tick Speed Graphs ---
                ui.horizontal(|ui| {
                    let graph_width = (ui.available_width() - 16.0) / 2.0;
                    let graph_height = 36.0;
                    // Loop Speed Graph
                    let (loop_resp, loop_painter) = ui.allocate_painter(
                        egui::vec2(graph_width, graph_height),
                        egui::Sense::hover(),
                    );
                    // Draw value and label above the bar, left-aligned
                    let loop_val_str = format!("{:.0} μs", self.loop_speed);
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
                    let loop_max = 10000.0; // 10ms
                    let loop_bar_width = (loop_val / loop_max).min(1.0) * (graph_width - 8.0);
                    let loop_bar_rect = egui::Rect::from_min_max(
                        loop_resp.rect.left_top() + egui::vec2(4.0, 16.0),
                        loop_resp.rect.left_top()
                            + egui::vec2(4.0 + loop_bar_width, graph_height - 8.0),
                    );
                    loop_painter.rect_filled(loop_bar_rect, 2.0, Color32::from_rgb(0, 200, 255));

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
    }

    fn left_panel_ui(&mut self, ctx: &Context) {
        egui::SidePanel::left("left_panel")
            .resizable(false)
            .default_width(64.0)
            // .width_range(150.0..=300.0)
            .show(ctx, |ui| {
                // ui.label("Pages");
                // ui.add_space(8.0);

                let button_count = AppPage::iter().count();
                let spacing_top_bottom = 3.0;
                let spacing = 6.0;
                let button_height = ui.available_height() / button_count as f32;
                let button_size = ButtonSize::Large
                    .with_height(button_height - spacing - spacing_top_bottom)
                    .with_width(64.0);

                ui.add_space(spacing_top_bottom);

                for (idx, page) in AppPage::iter().enumerate() {
                    let is_selected = self.current_page == page;
                    let label = page.short().to_uppercase();

                    if components::button(ui, is_selected, &label, button_size) && !is_selected {
                        self.current_page = page;
                    }

                    if idx + 1 < button_count {
                        ui.add_space(spacing);
                    }
                }

                // // TODO: loop here.
                // if ui
                //     .selectable_label(self.current_page == AppPage::Main, "Main")
                //     .clicked()
                // {
                //     self.current_page = AppPage::Main;
                // }
                // if ui
                //     .selectable_label(self.current_page == AppPage::Fixtures, "Fixtures")
                //     .clicked()
                // {
                //     self.current_page = AppPage::Fixtures;
                // }
                // if ui
                //     .selectable_label(self.current_page == AppPage::Animations, "Animations")
                //     .clicked()
                // {
                //     self.current_page = AppPage::Animations;
                // }

                // ui.add_space(16.0);
                // ui.label("Tools");
                // ui.add_space(8.0);
                //
                // if ui.button("Calculator").clicked() {
                //     // Placeholder action
                // }
                // if ui.button("Notes").clicked() {
                //     // Placeholder action
                // }
                //
                // ui.add_space(16.0);
                // ui.label("Status");
                // ui.add_space(8.0);
                // ui.label("Online");
                // ui.label("Last updated: Now");
            });
    }
}

fn powered_by_egui_and_eframe(ui: &mut egui::Ui) {
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = 0.0;
        ui.label("Powered by ");
        ui.hyperlink_to("egui", "https://github.com/emilk/egui");
        ui.label(" and ");
        ui.hyperlink_to(
            "eframe",
            "https://github.com/emilk/egui/tree/master/crates/eframe",
        );
        ui.label(".");
    });
}
