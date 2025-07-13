use blaulicht_shared::{CollectedAudioSnapshot, ControlEvent};
use cpal::Device;
use crossbeam_channel::{Receiver, TryRecvError};
use serde::Deserialize;
use std::collections::VecDeque;
use std::sync::{Arc, Mutex};

use crate::{
    audio::capture::SignalCollector,
    msg::{SystemMessage, UnifiedMessage},
    routes::AppStateWrapper,
};

//
// Log Window Component
//

/// A log window component that displays scrolling log messages
pub struct LogWindow {
    logs: VecDeque<LogEntry>,
    max_logs: usize,
    auto_scroll: bool,
    filter_text: String,
    selected_log_level: LogLevel,
    log_height: f32,
}

#[derive(Clone, Debug)]
pub struct LogEntry {
    timestamp: std::time::SystemTime,
    level: LogLevel,
    message: String,
    source: String,
}

#[derive(Clone, Debug, PartialEq)]
pub enum LogLevel {
    Debug,
    Info,
    Warning,
    Error,
    All,
}

impl LogLevel {
    fn color(&self) -> egui::Color32 {
        match self {
            LogLevel::Debug => egui::Color32::from_gray(150),
            LogLevel::Info => egui::Color32::from_rgb(100, 150, 255),
            LogLevel::Warning => egui::Color32::from_rgb(255, 200, 100),
            LogLevel::Error => egui::Color32::from_rgb(255, 100, 100),
            LogLevel::All => egui::Color32::WHITE,
        }
    }

    fn icon(&self) -> &'static str {
        match self {
            LogLevel::Debug => "🔍",
            LogLevel::Info => "ℹ️",
            LogLevel::Warning => "⚠️",
            LogLevel::Error => "❌",
            LogLevel::All => "📋",
        }
    }
}

impl LogWindow {
    pub fn new(max_logs: usize) -> Self {
        Self {
            logs: VecDeque::new(),
            max_logs,
            auto_scroll: true,
            filter_text: String::new(),
            selected_log_level: LogLevel::All,
            log_height: 200.0,
        }
    }

    pub fn add_log(&mut self, level: LogLevel, message: String, source: String) {
        let entry = LogEntry {
            timestamp: std::time::SystemTime::now(),
            level,
            message,
            source,
        };

        self.logs.push_back(entry);

        // Keep only the latest logs
        if self.logs.len() > self.max_logs {
            self.logs.pop_front();
        }
    }

    pub fn clear_logs(&mut self) {
        self.logs.clear();
    }

    pub fn draw(&mut self, ui: &mut egui::Ui) {
        ui.heading("Log Terminal");

        // Controls
        ui.horizontal(|ui| {
            ui.label("Filter:");
            ui.text_edit_singleline(&mut self.filter_text);

            ui.separator();

            ui.label("Level:");
            egui::ComboBox::from_id_salt("log_level")
                .selected_text(format!(
                    "{} {}",
                    self.selected_log_level.icon(),
                    format!("{:?}", self.selected_log_level)
                ))
                .show_ui(ui, |ui| {
                    for level in [
                        LogLevel::All,
                        LogLevel::Debug,
                        LogLevel::Info,
                        LogLevel::Warning,
                        LogLevel::Error,
                    ] {
                        ui.selectable_value(
                            &mut self.selected_log_level,
                            level.clone(),
                            format!("{} {:?}", level.icon(), level),
                        );
                    }
                });

            ui.separator();

            ui.checkbox(&mut self.auto_scroll, "Auto-scroll");

            if ui.button("Clear").clicked() {
                self.clear_logs();
            }
        });

        ui.separator();

        // Log display area - resizable content area
        ui.horizontal(|ui| {
            ui.label("Log Height:");
            ui.add(egui::Slider::new(&mut self.log_height, 100.0..=600.0).text("height"));
        });
        
        egui::ScrollArea::vertical()
            .max_height(self.log_height)
            .auto_shrink([false, false])
            .show(ui, |ui| {
                ui.style_mut().override_text_style = Some(egui::TextStyle::Monospace);

                let mut should_scroll_to_bottom = false;

                for entry in &self.logs {
                    // Apply filters
                    if self.selected_log_level != LogLevel::All
                        && entry.level != self.selected_log_level
                    {
                        continue;
                    }

                    if !self.filter_text.is_empty() {
                        if !entry
                            .message
                            .to_lowercase()
                            .contains(&self.filter_text.to_lowercase())
                            && !entry
                                .source
                                .to_lowercase()
                                .contains(&self.filter_text.to_lowercase())
                        {
                            continue;
                        }
                    }

                    // Format timestamp
                    let timestamp = entry
                        .timestamp
                        .duration_since(std::time::UNIX_EPOCH)
                        .unwrap_or_default()
                        .as_secs();

                    let time_str = format!(
                        "{:02}:{:02}:{:02}",
                        (timestamp / 3600) % 24,
                        (timestamp / 60) % 60,
                        timestamp % 60
                    );

                    // Create log line
                    let log_text = format!(
                        "[{}] {:?} {} | {} | {}",
                        time_str,
                        entry.level,
                        format!("{:?}", entry.level),
                        entry.source,
                        entry.message
                    );

                    // Display with appropriate color
                    ui.colored_label(entry.level.color(), log_text);

                    should_scroll_to_bottom = true;
                }

                // Auto-scroll to bottom
                if self.auto_scroll && should_scroll_to_bottom {
                    ui.scroll_to_cursor(Some(egui::Align::BOTTOM));
                }
            });

        // Status bar
        ui.separator();

        ui.horizontal(|ui| {
            ui.label(format!("Total logs: {}", self.logs.len()));
            ui.separator();
            ui.label(format!("Filtered level: {:?}", self.selected_log_level));
            if !self.filter_text.is_empty() {
                ui.separator();
                ui.label(format!("Filter: '{}'", self.filter_text));
            }
        });
    }
}

//
// Graph component
//

const CAP: usize = 1000;
const TIME_WINDOW_MS: u64 = 2000; // 2 seconds
const GRAPH_UPDATE_INTERVAL_MS: u64 = 20; // 20 ms update interval (50 Hz)

/// Modular time series graph component
pub struct TimeSeriesGraph {
    time_series_data: Vec<(u64, i32)>,
    start_time: u64,
    last_update: u64,
    title: String,
    min_value: i32,
    max_value: i32,
    line_color: egui::Color32,
    shadow_color: egui::Color32,
}

impl TimeSeriesGraph {
    pub fn new(title: String, min_value: i32, max_value: i32, line_color: egui::Color32) -> Self {
        let start_time = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_millis() as u64;

        Self {
            time_series_data: {
                let mut v = vec![];
                for i in 0..100 {
                    v.push((i * 20, 0))
                }
                v
            },
            start_time,
            last_update: 0,
            title,
            min_value,
            max_value,
            line_color,
            shadow_color: egui::Color32::from_gray(40),
        }
    }

    /// Add a new data point to the time series
    pub fn add_data_point(&mut self, value: i32) {
        let current_time = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_millis() as u64;
        let relative_time = current_time - self.start_time;
        self.time_series_data.push((relative_time, value));
        if self.time_series_data.len() > CAP {
            self.time_series_data.remove(0);
        }
    }

    /// Update the graph with new data if enough time has passed
    pub fn update(&mut self, current_value: i32) {
        let current_time = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_millis() as u64;
        let elapsed = current_time - self.last_update;
        if elapsed >= GRAPH_UPDATE_INTERVAL_MS {
            self.add_data_point(current_value);
            self.last_update = current_time;
        }
    }

    /// Create smooth curve points using cubic interpolation
    fn create_smooth_curve(&self, points: &[egui::Pos2], y_offset: f32) -> Vec<egui::Pos2> {
        if points.len() < 2 {
            return points.to_vec();
        }

        let mut smooth_points = Vec::new();
        let segments_per_point = 8; // Number of interpolated points between each original point

        for i in 0..points.len() - 1 {
            let p0 = if i > 0 { points[i - 1] } else { points[i] };
            let p1 = points[i];
            let p2 = points[i + 1];
            let p3 = if i + 2 < points.len() {
                points[i + 2]
            } else {
                points[i + 1]
            };

            for j in 0..=segments_per_point {
                let t = j as f32 / segments_per_point as f32;
                let x = self.cubic_interpolate(p0.x, p1.x, p2.x, p3.x, t);
                let y = self.cubic_interpolate(p0.y, p1.y, p2.y, p3.y, t) + y_offset;
                smooth_points.push(egui::pos2(x, y));
            }
        }

        smooth_points
    }

    /// Cubic interpolation between four points
    fn cubic_interpolate(&self, p0: f32, p1: f32, p2: f32, p3: f32, t: f32) -> f32 {
        let t2 = t * t;
        let t3 = t2 * t;

        // Catmull-Rom spline coefficients
        let a0 = -0.5 * p0 + 1.5 * p1 - 1.5 * p2 + 0.5 * p3;
        let a1 = p0 - 2.5 * p1 + 2.0 * p2 - 0.5 * p3;
        let a2 = -0.5 * p0 + 0.5 * p2;
        let a3 = p1;

        a0 * t3 + a1 * t2 + a2 * t + a3
    }

    /// Draw the time series graph using a painter
    pub fn draw(&self, painter: egui::Painter, rect: egui::Rect) {
        if self.time_series_data.is_empty() {
            painter.text(
                egui::pos2(rect.min.x + 10.0, rect.center().y),
                egui::Align2::CENTER_CENTER,
                "No data available",
                egui::FontId::proportional(16.0),
                egui::Color32::from_gray(150),
            );
            return;
        }
        // Use real time for smooth scrolling, but convert to relative time for filtering
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_millis() as u64;
        let relative_now = now - self.start_time;
        let min_relative_time = relative_now.saturating_sub(TIME_WINDOW_MS);
        let actual_data: Vec<_> = self
            .time_series_data
            .iter()
            .filter(|(t, _)| *t >= min_relative_time)
            .collect();
        if actual_data.len() < 2 {
            painter.text(
                egui::pos2(rect.min.x + 10.0, rect.center().y),
                egui::Align2::CENTER_CENTER,
                "No data available",
                egui::FontId::proportional(16.0),
                egui::Color32::from_gray(150),
            );
            return;
        }
        let graph_rect = rect.shrink(15.0);

        // Professional dark background
        let bg_color = egui::Color32::from_rgb(20, 20, 25);
        painter.rect_filled(graph_rect, 0.0, bg_color);



        // Smooth scrolling: X position based on timestamp
        let value_range = (self.max_value - self.min_value).max(1);
        let points: Vec<egui::Pos2> = actual_data
            .iter()
            .map(|(t, value)| {
                let x_frac = (*t as f32 - min_relative_time as f32) / (TIME_WINDOW_MS as f32);
                let x = graph_rect.min.x + x_frac.clamp(0.0, 1.0) * graph_rect.width();
                let y = (graph_rect.max.y
                    - ((*value - self.min_value) as f32 / value_range as f32)
                        * graph_rect.height())
                .round();
                egui::pos2(x, y)
            })
            .collect();

        // Draw smooth curves between points using cubic interpolation
        if points.len() > 1 {
            let shadow_offset = 2.0;

            // Create smooth curve points for shadow
            let shadow_points = self.create_smooth_curve(&points, shadow_offset);
            for i in 0..shadow_points.len() - 1 {
                painter.line_segment(
                    [shadow_points[i], shadow_points[i + 1]],
                    (2.0, self.shadow_color),
                );
            }

            // Create smooth curve points for main line
            let smooth_points = self.create_smooth_curve(&points, 0.0);
            for i in 0..smooth_points.len() - 1 {
                painter.line_segment(
                    [smooth_points[i], smooth_points[i + 1]],
                    (2.0, self.line_color),
                );
            }
        }
        // Title and current value
        let current_value = if let Some((_, value)) = actual_data.last() {
            *value
        } else {
            0
        };
        
        // Title with current value - styled with padding, smaller monospace font, and translucent background
        let title_text = format!("{}: {}", self.title, current_value);
        let title_font = egui::FontId::monospace(14.0);
        let padding = 8.0;
        let margin = 4.0;
        let estimated_width = title_text.len() as f32 * 8.0; // Rough estimate for monospace font
        let estimated_height = 20.0; // Fixed height for the background

        // Place background in the upper-left corner, fully inside the graph
        let bg_left = rect.min.x + margin;
        let bg_top = rect.min.y + margin;
        let bg_right = (bg_left + estimated_width + padding * 2.0).min(rect.max.x - margin);
        let bg_bottom = (bg_top + estimated_height + padding * 2.0).min(rect.max.y - margin);
        let bg_rect = egui::Rect::from_min_max(
            egui::pos2(bg_left, bg_top),
            egui::pos2(bg_right, bg_bottom),
        );

        // Draw translucent background
        let bg_color = egui::Color32::from_rgba_premultiplied(0, 0, 0, 180); // Semi-transparent black
        painter.rect_filled(bg_rect, 4.0, bg_color);

        // Draw title text, always inside the background
        let text_x = bg_left + padding;
        let text_y = bg_top + padding;
        painter.text(
            egui::pos2(text_x, text_y),
            egui::Align2::LEFT_TOP,
            &title_text,
            title_font,
            egui::Color32::from_rgb(220, 220, 220),
        );
        

    }

    /// Clear all data points
    pub fn clear(&mut self) {
        self.time_series_data.clear();
    }

    /// Get the number of data points
    pub fn data_points_count(&self) -> usize {
        self.time_series_data.len()
    }
}

//
// Actual eframe shit.
//

/// We derive Deserialize/Serialize so we can persist app state on shutdown.
#[derive(Clone, PartialEq)]
pub enum AppPage {
    Main,
    Settings,
}

pub struct TemplateApp {
    // Example stuff:
    label: String,

    value: f32,

    // Multiple graph instances for all audio values
    volume_graph: TimeSeriesGraph,
    beat_volume_graph: TimeSeriesGraph,
    bass_graph: TimeSeriesGraph,
    bass_avg_graph: TimeSeriesGraph,
    bass_avg_short_graph: TimeSeriesGraph,
    bpm_graph: TimeSeriesGraph,
    time_between_beats_graph: TimeSeriesGraph,

    // For continuous rendering
    frame_count: u64,

    animation_time: f32,

    data: AppStateWrapper,

    // recv: Receiver<UnifiedMessage>,
    collector: SignalCollector,

    loop_speed: usize,
    tick_speed: usize,

    // logs: Vec<String>,
    log_window: LogWindow,

    // Current page
    current_page: AppPage,
}

impl TemplateApp {
    fn new_default(data: AppStateWrapper) -> Self {
        let collector = SignalCollector::new();

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
            collector,
            loop_speed: 0,
            tick_speed: 0,
            // logs: vec![],
            log_window: LogWindow::new(1000),
            current_page: AppPage::Main,
        }
    }
}

const UI_RECV_KEY: &'static str = "eframe_ui";

impl Drop for TemplateApp {
    fn drop(&mut self) {
        // let mut consumers = self.data.to_frontend_consumers.lock().unwrap();
        // let removed = consumers.remove(UI_RECV_KEY);
        // debug_assert!(removed.is_some());
    }
}

impl TemplateApp {
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
}

impl eframe::App for TemplateApp {
    /// Called by the framework to save state before shutdown.
    // fn save(&mut self, storage: &mut dyn eframe::Storage) {
    //     eframe::set_value(storage, eframe::APP_KEY, self);
    // }

    /// Called each time the UI needs repainting, which may be many times per second.
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        //
        // At the beginning, process any incoming state changes.
        //

        // let mut new_signal = false;
        let mut empty = false;
        loop {
            match self.data.signal_receiver.try_recv() {
                Ok(signal) => {
                    self.collector.signal(signal);
                    // new_signal = true;
                }
                Err(TryRecvError::Empty) => {
                    empty = true;
                }
                Err(TryRecvError::Disconnected) => unreachable!("CANNOT REACH"),
            }

            match self.data.system_message_receiver.try_recv() {
                Ok(sys) => match sys {
                    SystemMessage::Heartbeat(_) => {
                        // self.log_window.add_log(
                        //     LogLevel::Debug,
                        //     "Heartbeat received".to_string(),
                        //     "System".to_string(),
                        // );
                    }
                    SystemMessage::Log(log_msg) => {
                        self.log_window.add_log(
                            LogLevel::Info,
                            log_msg,
                            "System".to_string(),
                        );
                    }
                    SystemMessage::WasmLog(wasm_log_body) => {
                        self.log_window.add_log(
                            LogLevel::Info,
                            format!("{:?}", wasm_log_body),
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
                        self.log_window.add_log(
                            LogLevel::Debug,
                            format!("Audio devices updated: {} devices", items.len()),
                            "Audio".to_string(),
                        );
                    }
                    SystemMessage::DMX(dmx_msg) => {
                        self.log_window.add_log(
                            LogLevel::Info,
                            format!("DMX message: {:?}", dmx_msg),
                            "DMX".to_string(),
                        );
                    }
                },
                Err(TryRecvError::Empty) if empty => {
                    break;
                }
                Err(TryRecvError::Empty) => {}
                Err(TryRecvError::Disconnected) => {
                    unreachable!("CANNOT REACH")
                }
            }
        }

        // Update all graphs with current data
        self.volume_graph
            .update(self.collector.take_snapshot().volume as i32);
        self.beat_volume_graph
            .update(self.collector.take_snapshot().beat_volume as i32);
        self.bass_graph
            .update(self.collector.take_snapshot().bass as i32);
        self.bass_avg_graph
            .update(self.collector.take_snapshot().bass_avg as i32);
        self.bass_avg_short_graph
            .update(self.collector.take_snapshot().bass_avg_short as i32);
        self.bpm_graph
            .update(self.collector.take_snapshot().bpm as i32);
        self.time_between_beats_graph
            .update(self.collector.take_snapshot().time_between_beats_millis as i32);

        // Put your widgets into a `SidePanel`, `TopBottomPanel`, `CentralPanel`, `Window` or `Area`.
        // For inspiration and more examples, go to https://emilk.github.io/egui

        egui::TopBottomPanel::top("top_panel").show(ctx, |ui| {
            // The top panel is often a good place for a menu bar:

            egui::MenuBar::new().ui(ui, |ui| {
                // NOTE: no File->Quit on web pages!
                let is_web = cfg!(target_arch = "wasm32");
                if !is_web {
                    ui.menu_button("File", |ui| {
                        if ui.button("Quit").clicked() {
                            ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                        }
                    });
                    ui.add_space(16.0);
                }

                egui::widgets::global_theme_preference_buttons(ui);
            });
        });

        // Left sidebar
        egui::SidePanel::left("left_panel")
            .resizable(true)
            .default_width(200.0)
            .width_range(150.0..=300.0)
            .show(ctx, |ui| {
                ui.heading("Navigation");
                ui.separator();

                // Tab switcher
                ui.label("Pages");
                ui.add_space(8.0);

                if ui.selectable_label(self.current_page == AppPage::Main, "Main").clicked() {
                    self.current_page = AppPage::Main;
                }
                if ui.selectable_label(self.current_page == AppPage::Settings, "Settings").clicked() {
                    self.current_page = AppPage::Settings;
                }

                ui.add_space(16.0);
                ui.label("Tools");
                ui.add_space(8.0);

                if ui.button("Calculator").clicked() {
                    // Placeholder action
                }
                if ui.button("Notes").clicked() {
                    // Placeholder action
                }

                ui.add_space(16.0);
                ui.label("Status");
                ui.add_space(8.0);
                ui.label("Online");
                ui.label("Last updated: Now");
            });

        // Right sidebar
        egui::SidePanel::right("right_panel")
            .resizable(true)
            .default_width(250.0)
            .width_range(200.0..=400.0)
            .show(ctx, |ui| {
                ui.heading("Right Sidebar");
                ui.separator();

                // Placeholder widgets for the right sidebar
                ui.label("Properties");
                ui.add_space(8.0);

                ui.label("Name:");
                ui.text_edit_singleline(&mut self.label);

                ui.add_space(8.0);
                ui.label("Value:");
                ui.add(egui::Slider::new(&mut self.value, 0.0..=10.0).text("value"));

                ui.add_space(16.0);
                ui.label("Actions");
                ui.add_space(8.0);

                if ui.button("Reset").clicked() {
                    self.value = 0.0;
                }
                if ui.button("Randomize").clicked() {
                    self.value = 0.0;
                }

                ui.add_space(16.0);
                ui.label("Info");
                ui.add_space(8.0);
                ui.label(format!("Current value: {:.2}", self.value));
                ui.label("Widget count: 3");
                ui.label("Theme: Default");
            });

        // Bottom log panel - fixed height
        egui::TopBottomPanel::bottom("log_panel")
            .default_height(300.0)
            .show(ctx, |ui| {
                self.log_window.draw(ui);
            });

        egui::CentralPanel::default().show(ctx, |ui| {
            // The central panel the region left after adding TopPanel's and SidePanel's
            ui.heading("eframe template");

            // Continuous rendering - always request repaints
            ctx.request_repaint_after(std::time::Duration::from_millis(16)); // ~60 FPS

            // Update animation time for continuous rendering
            self.frame_count += 1;
            self.animation_time += 0.016; // 16ms = 0.016 seconds

            ui.horizontal(|ui| {
                ui.label("Write something: ");
                ui.text_edit_singleline(&mut self.label);
            });

            ui.add(egui::Slider::new(&mut self.value, 0.0..=10.0).text("value"));
            if ui.button("Increment").clicked() {
                self.value += 1.0;
            }

            ui.separator();

            // Page content based on selected tab
            match self.current_page {
                AppPage::Main => {
                    // Main content area with graphs panel
                    ui.horizontal(|ui| {
                        // Left content area (3/4 width)
                        let total_width = ui.available_width();
                        let graph_panel_width = total_width * 0.25;
                        let main_panel_width = total_width - graph_panel_width - 16.0; // 16px for separator

                        ui.vertical(|ui| {
                            ui.set_width(main_panel_width);
                            ui.heading("Main Content");
                            ui.label("This is the main content area taking up 3/4 of the width.");
                            ui.add_space(20.0);
                            ui.label("You can put your main application content here.");
                        });

                        ui.separator();

                        // Graphs panel (1/4 width) - fixed width
                        ui.allocate_ui_with_layout(
                            egui::vec2(graph_panel_width, ui.available_height()),
                            egui::Layout::top_down(egui::Align::LEFT),
                            |ui| {
                                ui.heading("Audio Graphs");
                                ui.label("Real-time audio visualization");

                                // Show animation info
                                ui.label(format!(
                                    "Frame: {} | Animation Time: {:.2}s",
                                    self.frame_count, self.animation_time
                                ));

                                // Set larger graph height
                                let graph_height = 140.0;
                                let graph_width = graph_panel_width - 16.0;
                                let padding = 10.0;

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
                                self.beat_volume_graph.draw(painter_beat_volume, response_beat_volume.rect);

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
                                self.bass_avg_graph.draw(painter_bass_avg, response_bass_avg.rect);

                                ui.add_space(padding);
                                let (response_bass_avg_short, painter_bass_avg_short) = ui.allocate_painter(
                                    egui::vec2(graph_width, graph_height),
                                    egui::Sense::hover(),
                                );
                                self.bass_avg_short_graph.draw(painter_bass_avg_short, response_bass_avg_short.rect);

                                ui.add_space(padding);
                                let (response_bpm, painter_bpm) = ui.allocate_painter(
                                    egui::vec2(graph_width, graph_height),
                                    egui::Sense::hover(),
                                );
                                self.bpm_graph.draw(painter_bpm, response_bpm.rect);

                                ui.add_space(padding);
                                let (response_time_between_beats, painter_time_between_beats) = ui.allocate_painter(
                                    egui::vec2(graph_width, graph_height),
                                    egui::Sense::hover(),
                                );
                                self.time_between_beats_graph.draw(painter_time_between_beats, response_time_between_beats.rect);

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
                                            LogLevel::Warning,
                                            "Volume graph cleared".to_string(),
                                            "Graph".to_string(),
                                        );
                                    }

                                    ui.label(format!(
                                        "Points: {}",
                                        self.volume_graph.data_points_count()
                                    ));
                                });
                            },
                        );
                    });
                }
                AppPage::Settings => {
                    // Settings page content
                    ui.heading("Settings");
                    ui.separator();
                    
                    ui.label("Application Settings");
                    ui.add_space(20.0);
                    
                    ui.horizontal(|ui| {
                        ui.label("Sample Rate:");
                        ui.add(egui::Slider::new(&mut self.value, 0.0..=10.0).text("rate"));
                    });
                    
                    ui.horizontal(|ui| {
                        ui.label("Buffer Size:");
                        ui.add(egui::Slider::new(&mut self.value, 0.0..=10.0).text("size"));
                    });
                    
                    ui.add_space(20.0);
                    ui.label("Audio Configuration");
                    ui.add_space(10.0);
                    
                    if ui.button("Reset to Defaults").clicked() {
                        self.log_window.add_log(
                            LogLevel::Info,
                            "Settings reset to defaults".to_string(),
                            "Settings".to_string(),
                        );
                    }
                    
                    if ui.button("Save Settings").clicked() {
                        self.log_window.add_log(
                            LogLevel::Info,
                            "Settings saved".to_string(),
                            "Settings".to_string(),
                        );
                    }
                }
            }

            ui.separator();

            ui.add(egui::github_link_file!(
                "https://github.com/emilk/eframe_template/blob/main/",
                "Source code."
            ));

            ui.with_layout(egui::Layout::bottom_up(egui::Align::LEFT), |ui| {
                powered_by_egui_and_eframe(ui);
                egui::warn_if_debug_build(ui);
            });
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
