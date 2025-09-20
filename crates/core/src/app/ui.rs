use crate::app::graph::TimeSeriesGraph;
use crate::app::log::LogWindow;
use crate::app::{AnimationPageState, AppPage, BlaulichtApp, PopupSpec};
use crate::audio::defs::AudioThreadControlSignal;
use crate::dmx::animation::{MathematicalBaseFunction, PhaserDuration};
use crate::dmx::EngineState;
use crate::msg::FromFrontend;
use crate::{config, utils};
use crate::{msg::SystemMessage, routes::AppStateWrapper};
use blaulicht_shared::LogLevel;
use cpal::traits::DeviceTrait;
use crossbeam_channel::TryRecvError;
use egui::{
    Button, Color32, Context, CornerRadius, Frame, Margin, ProgressBar, RichText, Rounding, Stroke,
    Vec2,
};
use std::fs::{self, File};
use std::io::Read;
use std::mem;
use std::path::PathBuf;
use std::str::FromStr;
use std::time::Instant;
use strum::IntoEnumIterator;

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

impl BlaulichtApp {
    fn new_default(data: AppStateWrapper) -> Self {
        Self {
            // Example stuff:
            label: "Hello World!".to_owned(),
            value: 2.7,
            volume_graph: TimeSeriesGraph::new(
                "Volume".to_string(),
                0,
                255,
                egui::Color32::from_rgb(0, 200, 255),
            ),
            beat_volume_graph: TimeSeriesGraph::new(
                "Beat Volume".to_string(),
                0,
                255,
                egui::Color32::from_rgb(0, 200, 255),
            ),
            bass_graph: TimeSeriesGraph::new(
                "Bass".to_string(),
                0,
                255,
                egui::Color32::from_rgb(0, 200, 255),
            ),
            bass_avg_graph: TimeSeriesGraph::new(
                "Bass Avg".to_string(),
                0,
                255,
                egui::Color32::from_rgb(0, 200, 255),
            ),
            bass_avg_short_graph: TimeSeriesGraph::new(
                "Bass Avg Short".to_string(),
                0,
                255,
                egui::Color32::from_rgb(0, 200, 255),
            ),
            bpm_graph: TimeSeriesGraph::new(
                "BPM".to_string(),
                0,
                255,
                egui::Color32::from_rgb(0, 200, 255),
            ),
            time_between_beats_graph: TimeSeriesGraph::new(
                "Time Between Beats".to_string(),
                0,
                255,
                egui::Color32::from_rgb(0, 200, 255),
            ),
            frame_count: 0,
            animation_time: 0.0,
            data,
            // recv,
            // collector,
            loop_speed: 0,
            tick_speed: 0,
            // logs: vec![],
            log_window: LogWindow::new(100),
            current_page: AppPage::Audio,
            last_heartbeat_frame: 0,
            selected_fixture_group: None,
            available_audio_devices: vec![],
            animation_page: AnimationPageState {
                selected_animation: None,
                clamp_min: 0,
                clamp_max: 255,
                base_function: MathematicalBaseFunction::Sin,
                timing: PhaserDuration::Fixed(1000),
            },
            new_scene_name: "My Scene".to_string(),
            popup: None,
            popup_open_time: Instant::now(),
        }
    }
}

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
}

impl eframe::App for BlaulichtApp {
    /// Called each time the UI needs repainting, which may be many times per second.
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        if let Some(ref popup) = self.popup {
            let popup = popup.clone();
            // egui::Window::new(&popup)
            //     .collapsible(false)
            //     .resizable(false)
            //     .show(ctx, |ui| {
            //         ui.label(&popup);
            //         if ui.button("Close").clicked() {
            //             self.popup = None;
            //         }
            //     });

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

                        if let Some(ref button) = popup.button {
                            ui.add_space(8.0);

                            let rect = ui
                                .allocate_exact_size(egui::vec2(90.0, 60.0), egui::Sense::click());
                            let painter = ui.painter();
                            let bg_color = egui::Color32::from_rgb(60, 120, 200);
                            painter.rect_filled(rect.0, 0.0, bg_color);

                            painter.text(
                                rect.0.center(),
                                egui::Align2::CENTER_CENTER,
                                &button.label,
                                egui::FontId::proportional(16.0),
                                egui::Color32::WHITE,
                            );

                            if rect.1.clicked() {
                                self.close_popup();
                            }
                        }

                        let progress = 1.0
                            - elapsed.as_millis() as f32
                                / popup.lifetime_duration.as_millis() as f32;

                        ui.add(
                            ProgressBar::new(progress)
                                .text(format!(
                                    "{}S remaining",
                                    popup.lifetime_duration.as_secs() - elapsed.as_secs()
                                ))
                                .fill(Color32::from_rgb(60, 120, 200)),
                        );
                    })
                });
        }

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

        egui::TopBottomPanel::top("top_panel").show(ctx, |ui| {
            // The top panel is often a good place for a menu bar:

            egui::MenuBar::new().ui(ui, |ui| {
                ui.menu_button("File", |ui| {
                    if ui.button("Open showfile").clicked() {
                        if let Some(path) = rfd::FileDialog::new().save_file() {
                            let mut f = File::open(path.clone()).expect("no file found");
                            let metadata = fs::metadata(&path).expect("unable to read metadata");
                            let mut buffer = vec![0; metadata.len() as usize];
                            f.read(&mut buffer).expect("buffer overflow");

                            let decoded: EngineState = postcard::from_bytes(&buffer).unwrap();
                            let mut dmx = self.data.state.dmx_engine.write().unwrap();
                            // dmx.overwrite(decoded);
                            *dmx = decoded;

                            let mut config_mut = self.data.config.lock().unwrap();
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
                        }
                    }

                    if ui.button("Save to new showfile").clicked() {
                        if let Some(path) = rfd::FileDialog::new().save_file() {
                            let dmx = self.data.state.dmx_engine.read().unwrap();
                            let serialized = postcard::to_allocvec(&dmx.clone()).unwrap();
                            std::fs::write(&path, serialized).unwrap();

                            let mut config_mut = self.data.config.lock().unwrap();
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
                        }
                    }

                    let label = match &self
                        .data
                        .config
                        .lock()
                        .unwrap()
                        .last_open_showfile
                        .is_some()
                    {
                        true => "Save to current Showfile",
                        false => "Save to current Showfile (none open)",
                    };

                    if ui.button(label).clicked() {
                        let mut config_mut = self.data.config.lock().unwrap();

                        match config_mut.last_open_showfile.clone() {
                            Some(ref path) => {
                                let dmx = self.data.state.dmx_engine.read().unwrap();
                                let serialized = postcard::to_allocvec(&dmx.clone()).unwrap();
                                std::fs::write(&path, serialized).unwrap();

                                config_mut.last_open_showfile = Some(path.clone());

                                let config_path =
                                    PathBuf::from_str(&self.data.config_path).unwrap();
                                config::write_config(config_path, config_mut.clone()).unwrap();

                                self.data
                                    .system_message_sender
                                    .send(SystemMessage::Log(
                                        format!("Saved showfile to {path:?}"),
                                        LogLevel::Info,
                                    ))
                                    .unwrap();
                            }
                            None => {
                                self.data
                                    .system_message_sender
                                    .send(SystemMessage::Log(
                                        "No opened showfile, not saving".to_string(),
                                        LogLevel::Err,
                                    ))
                                    .unwrap();
                            }
                        }

                        ui.close();
                    }

                    if ui.button("Quit").clicked() {
                        ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                    }
                });
                ui.add_space(16.0);

                egui::widgets::global_theme_preference_buttons(ui);
            });
        });

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
                    self.main_ui(ui);
                }
                AppPage::Fixtures => {
                    self.fixtures_ui(ui);
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
    fn main_ui(&mut self, ui: &mut egui::Ui) {
        // Main content area with graphs panel
        ui.horizontal(|ui| {
            // Left content area (3/4 width)
            let total_width = ui.available_width();
            let graph_panel_width = total_width * 0.25;
            // let main_panel_width = total_width - graph_panel_width - 16.0; // 16px for separator

            // ui.vertical(|ui| {
            //     ui.set_width(main_panel_width);
            //     ui.heading("Main Content");
            //     ui.label("This is the main content area taking up 3/4 of the width.");
            //     ui.add_space(20.0);
            //     ui.label("You can put your main application content here.");
            // });

            // ui.separator();

            // Graphs panel (1/4 width) - fixed width
            ui.allocate_ui_with_layout(
                egui::vec2(graph_panel_width, ui.available_height()),
                egui::Layout::top_down(egui::Align::LEFT),
                |ui| {
                    ui.heading("Audio");
                    let mut selected_device =
                        self.data.state.audio.read().unwrap().device_name.clone();
                    ui.label(format!(
                        "Input device: {}",
                        selected_device.clone().unwrap_or_else(|| "N/A".to_string())
                    ));

                    let before = selected_device.clone();

                    egui::ComboBox::from_label("Audio Device")
                        .selected_text(format!("{:?}", selected_device))
                        .show_ui(ui, |ui| {
                            for dev in &self.available_audio_devices {
                                ui.selectable_value(
                                    &mut selected_device,
                                    Some(dev.to_owned()),
                                    dev,
                                );
                            }
                            ui.selectable_value(&mut selected_device, None, "None");
                        });

                    if selected_device != before {
                        // Handle selection change
                        let new_dev = selected_device.map(|d| utils::device_from_name(d).unwrap());
                        self.data
                            .from_frontend_sender
                            .send(FromFrontend::SelectInputDevice(new_dev.clone()))
                            .unwrap();

                        let mut config_mut = self.data.config.lock().unwrap();

                        config_mut.default_audio_device = match new_dev {
                            Some(d) => Some(d.name().unwrap()),
                            None => None,
                        };

                        let path = PathBuf::from_str(&self.data.config_path).unwrap();
                        config::write_config(path, config_mut.clone()).unwrap();
                    }

                    // Show animation info
                    ui.label(format!(
                        "Frame: {} | Animation Time: {:.2}s",
                        self.frame_count, self.animation_time
                    ));

                    // Set larger graph height
                    let graph_height = 140.0;
                    let graph_width = graph_panel_width - 16.0;
                    let padding = 10.0;

                    debug_assert!(graph_width > 0.0);

                    ui.add_space(padding);
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

                    ui.add_space(padding);
                    let (response_bass_avg, painter_bass_avg) = ui.allocate_painter(
                        egui::vec2(graph_width, graph_height),
                        egui::Sense::hover(),
                    );
                    self.bass_avg_graph
                        .draw(painter_bass_avg, response_bass_avg.rect);

                    ui.add_space(padding);
                    let (response_bass_avg_short, painter_bass_avg_short) = ui.allocate_painter(
                        egui::vec2(graph_width, graph_height),
                        egui::Sense::hover(),
                    );
                    self.bass_avg_short_graph
                        .draw(painter_bass_avg_short, response_bass_avg_short.rect);

                    ui.add_space(padding);
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

                    ui.add_space(padding);

                    // Graph controls
                    ui.horizontal(|ui| {
                        if ui.button("Add Point").clicked() {
                            let random_value = 0;
                            self.volume_graph.add_data_point(random_value);
                            self.log_window.add_log(
                                LogLevel::Debug,
                                format!("Added data point: {}", random_value),
                                "Graph".to_string(),
                            );
                        }

                        if ui.button("Clear").clicked() {
                            self.volume_graph.clear();
                            self.log_window.add_log(
                                LogLevel::Info,
                                "Volume graph cleared".to_string(),
                                "Graph".to_string(),
                            );
                        }

                        ui.label(format!("Points: {}", self.volume_graph.data_points_count()));
                    });
                },
            );
        });
    }

    fn logs_ui(&mut self, ui: &mut egui::Ui, ctx: &Context) {
        self.log_window.draw(ui);

        // Terminal.
        // Right sidebar
        // Bottom log panel - fixed height
        // egui::TopBottomPanel::bottom("log_panel")
        //     .default_height(300.0)
        //     .show(ctx, |ui| {
        //     });
    }

    fn system_ui(&mut self, ui: &mut egui::Ui, ctx: &Context) {
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
                                let is_alive = plugin.is_active();
                                let border_color = if is_alive {
                                    egui::Color32::from_rgb(0, 200, 0)
                                } else {
                                    egui::Color32::from_rgb(200, 0, 0)
                                };
                                let mut show_border = true;
                                if !is_alive {
                                    let blink = ((self.animation_time * 8.0) as i32) % 2 == 0;
                                    show_border = blink;
                                }

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
                                    egui::FontId::monospace(14.0),
                                    if plugin.is_active() {
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
                        // self.data
                        //     .from_frontend_sender
                        //     .send(FromFrontend::Reload)
                        //     .unwrap();

                        self.show_popup(PopupSpec::default("Reload in progress...".to_string()));
                    }
                }
            });
    }

    fn left_panel_ui(&mut self, ctx: &Context) {
        egui::SidePanel::left("left_panel")
            .resizable(false)
            .default_width(100.0)
            // .width_range(150.0..=300.0)
            .show(ctx, |ui| {
                // ui.label("Pages");
                // ui.add_space(8.0);

                for page in AppPage::iter() {
                    let is_selected = self.current_page == page;

                    let rect = ui.allocate_exact_size(egui::vec2(90.0, 60.0), egui::Sense::click());
                    let painter = ui.painter();
                    let bg_color = if is_selected {
                        egui::Color32::from_rgb(60, 120, 200)
                    } else {
                        egui::Color32::from_gray(40)
                    };
                    painter.rect_filled(rect.0, 0.0, bg_color);

                    // let fixture_count = group.fixtures.len();
                    let name = page.short().to_uppercase();

                    painter.text(
                        rect.0.center(),
                        egui::Align2::CENTER_CENTER,
                        &name,
                        egui::FontId::proportional(16.0),
                        egui::Color32::WHITE,
                    );

                    if rect.1.clicked() && !is_selected {
                        self.current_page = page;
                    }
                    ui.add_space(8.0);
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
