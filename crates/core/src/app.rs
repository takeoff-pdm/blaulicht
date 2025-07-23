use blaulicht_shared::{
    CollectedAudioSnapshot, ControlEvent, ControlEventMessage, EventOriginator,
};
use cpal::traits::DeviceTrait;
use cpal::Device;
use crossbeam_channel::{Receiver, Select, TryRecvError};
use egui::Color32;
use serde::Deserialize;
use std::collections::VecDeque;
use std::mem;
use std::sync::{Arc, Mutex};
use std::time::Instant;

use crate::msg::FromFrontend;
use crate::{
    audio::capture::SignalCollector,
    msg::{SystemMessage, UnifiedMessage},
    routes::AppStateWrapper,
};
use crate::{system_message, utils};

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
const TIME_WINDOW_MS: u64 = 3000;
const GRAPH_UPDATE_INTERVAL_MS: u64 = 5;

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
        let mut points: Vec<egui::Pos2> = actual_data
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

        // Downsample points if there are more points than pixels available
        let max_points = (graph_rect.width() as usize).max(1);
        let mut downsampled_points = Vec::new();
        if points.len() > max_points {
            let bin_size = points.len() as f32 / max_points as f32;
            for i in 0..max_points {
                let start = (i as f32 * bin_size).floor() as usize;
                let end = ((i as f32 + 1.0) * bin_size).ceil() as usize;
                let end = end.min(points.len());
                if start < end {
                    // Average the points in this bin
                    let (sum_x, sum_y, count) =
                        points[start..end].iter().fold((0.0, 0.0, 0), |acc, p| {
                            (acc.0 + p.x, acc.1 + p.y, acc.2 + 1)
                        });
                    downsampled_points.push(egui::pos2(sum_x / count as f32, sum_y / count as f32));
                }
            }
            points = downsampled_points;
        }

        // Draw smooth curves between points using cubic interpolation
        if points.len() > 1 {
            // Remove shadow rendering
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
        let bg_rect =
            egui::Rect::from_min_max(egui::pos2(bg_left, bg_top), egui::pos2(bg_right, bg_bottom));

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
    Fixtures,
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
    last_heartbeat_frame: u64,
    selected_fixture_group: Option<u8>,

    // Audio devices.
    available_audio_devices: Vec<String>,
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
            log_window: LogWindow::new(100),
            current_page: AppPage::Main,
            last_heartbeat_frame: 0,
            selected_fixture_group: None,
            available_audio_devices: vec![],
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

            match self.data.signal_receiver.try_recv() {
                Ok(signal) => {
                    self.collector.signal(signal);
                    // new_signal = true;
                }
                Err(TryRecvError::Empty) => {
                    empty += 1;
                }
                Err(TryRecvError::Disconnected) => unreachable!("CANNOT REACH"),
            }

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
                    SystemMessage::Log(log_msg) => {
                        self.log_window
                            .add_log(LogLevel::Info, log_msg, "System".to_string());
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
                        // Check if number of devices changed.
                        //
                        if self.available_audio_devices.len() != items.len() {
                            self.log_window.add_log(
                                LogLevel::Debug,
                                format!("Available audio devices updated: {} devices", items.len()),
                                "Audio".to_string(),
                            );
                            let items_str = items.into_iter().map(|(_, dev)| dev.name().unwrap()).collect();
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

                if ui
                    .selectable_label(self.current_page == AppPage::Main, "Main")
                    .clicked()
                {
                    self.current_page = AppPage::Main;
                }
                if ui
                    .selectable_label(self.current_page == AppPage::Fixtures, "Fixtures")
                    .clicked()
                {
                    self.current_page = AppPage::Fixtures;
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

                // --- Existing right panel content ---
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
                                ui.heading("Audio");
                                let mut selected_device = self.data.state.audio.read().unwrap().device_name.clone();
                                ui.label(format!("Input device: {}", selected_device.clone().unwrap_or_else(||"N/A".to_string())));

                                let before = selected_device.clone();

                                egui::ComboBox::from_label("Audio Device")
                                    .selected_text(format!("{:?}", selected_device))
                                    .show_ui(ui, |ui| {
                                        for dev in &self.available_audio_devices {
                                            ui.selectable_value(&mut selected_device, Some(dev.to_owned()), dev);
                                        }
                                        ui.selectable_value(&mut selected_device, None, "None");
                                    }
                                );

                                if selected_device != before {
                                    // Handle selection change
                                    let new_dev = selected_device.map(|d|utils::device_from_name(d).unwrap());
                                    self.data.from_frontend_sender.send(FromFrontend::SelectInputDevice(new_dev)).unwrap();
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

                                ui.add_space(padding);
                                let (response, painter) = ui.allocate_painter(
                                    egui::vec2(graph_width, graph_height),
                                    egui::Sense::hover(),
                                );
                                self.volume_graph.draw(painter, response.rect);

                                ui.add_space(padding);
                                let (response_beat_volume, painter_beat_volume) = ui
                                    .allocate_painter(
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
                                let (response_bass_avg_short, painter_bass_avg_short) = ui
                                    .allocate_painter(
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
                                self.time_between_beats_graph.draw(
                                    painter_time_between_beats,
                                    response_time_between_beats.rect,
                                );

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
                AppPage::Fixtures => {
                    ui.heading("Fixtures");
                    ui.separator();
                    let dmx_engine = {
                        self.data.state.dmx_engine.read().unwrap().clone()
                     };

                    let groups = dmx_engine.groups();
                    // let group_count = groups.len();
                    // let mut selected_group = self.selected_fixture_group;
                    // Layout: left (groups), right (fixtures if one group selected)

                    // let mut selected_groups = vec![];

                    ui.horizontal(|ui| {
                        // Left: groups list
                        let selection = dmx_engine.selection();
                        ui.vertical(|ui| {
                            ui.label("Groups:");
                            ui.add_space(8.0);
                            for (group_id, group) in groups.iter() {
                                let is_selected = selection.group_ids.contains(group_id);

                                // if is_selected {
                                //     selected_groups.push(group_id);
                                // }

                                let rect = ui.allocate_exact_size(
                                    egui::vec2(180.0, 48.0),
                                    egui::Sense::click(),
                                );
                                let painter = ui.painter();
                                let bg_color = if is_selected {
                                    egui::Color32::from_rgb(60, 120, 200)
                                } else {
                                    egui::Color32::from_gray(40)
                                };
                                painter.rect_filled(rect.0, 6.0, bg_color);
                                let fixture_count = group.fixtures.len();
                                let name = format!("Group {}", group_id);
                                painter.text(
                                    rect.0.left_top() + egui::vec2(12.0, 8.0),
                                    egui::Align2::LEFT_TOP,
                                    &name,
                                    egui::FontId::proportional(16.0),
                                    egui::Color32::WHITE,
                                );
                                painter.text(
                                    rect.0.left_bottom() - egui::vec2(-12.0, 8.0),
                                    egui::Align2::LEFT_BOTTOM,
                                    format!("{} fixtures", fixture_count),
                                    egui::FontId::proportional(12.0),
                                    egui::Color32::GRAY,
                                );
                                if rect.1.clicked() {
                                    // Toggle group selection
                                    let msg = if is_selected {
                                        ControlEvent::DeSelectGroup(*group_id)
                                    } else {
                                        ControlEvent::SelectGroup(*group_id)
                                    };

                                    self.data
                                        .event_bus_connection
                                        .send(ControlEventMessage::new(EventOriginator::Web, msg));
                                }
                                ui.add_space(8.0);
                            }
                            // self.selected_fixture_group = selected_group;
                        });
                        // Right: fixtures in selected group

                        #[derive(PartialEq, Eq, Clone, Copy)]
                        enum Selection {
                            Off,
                            Limited,
                            Cascading,
                        }

                        impl Selection {
                            fn color(&self) -> Color32 {
                                match self {
                                    Selection::Off => Color32::from_gray(60),
                                    Selection::Limited => Color32::from_rgb(120, 180, 80),
                                    Selection::Cascading => Color32::from_rgb(80, 180, 120),
                                }
                            }
                        }

                        if !selection.group_ids.is_empty() {
                            // Get selection info
                            let dmx_engine = self.data.state.dmx_engine.read().unwrap();
                            let selection = dmx_engine.selection();

                            let highlight_all = selection.fixtures_in_group.is_empty();
                            let highlight_fixtures = &selection.fixtures_in_group;

                            let mut total_fixtures = vec![];
                            for g_id in selection.group_ids.iter() {
                                let group = groups.get(g_id).unwrap();
                                if selection.fixtures_in_group.is_empty() {
                                    total_fixtures.extend( group.fixtures.keys().map(|f_id| (*g_id, *f_id, Selection::Cascading)));
                                } else {
                                    total_fixtures.extend( group.fixtures.keys().map(|f_id|{
                                        let is_limited = selection.fixtures_in_group.contains(f_id);
                                        let selection = match is_limited {
                                            true => Selection::Limited,
                                            false => Selection::Off,
                                        };

                                    (*g_id, *f_id, selection)
                                    }
                                ));
                                }
                            }

                            // Layout: left (fixtures), right (controls)
                            ui.horizontal(|ui| {
                                // Fixtures list
                                ui.vertical(|ui| {
                                    ui.label(format!("Fixtures in Group"));
                                    ui.add_space(8.0);

                                    for (group_id, fix_id, fixture_selection) in &total_fixtures {
                                        // let is_selected =
                                        //      highlight_fixtures.contains(fix_id);
                                        let fixture = groups.get(&group_id).unwrap().fixtures.get(&fix_id).unwrap();

                                        let fix_rect = ui.allocate_exact_size(
                                            egui::vec2(180.0, 36.0),
                                            egui::Sense::click(),
                                        );
                                        let painter = ui.painter();

                                        let bg = fixture_selection.color();

                                        painter.rect_filled(fix_rect.0, 4.0, bg);
                                        painter.text(
                                            fix_rect.0.left_center() + egui::vec2(12.0, 0.0),
                                            egui::Align2::LEFT_CENTER,
                                            &fixture.name,
                                            egui::FontId::proportional(14.0),
                                            egui::Color32::WHITE,
                                        );

                                        // Click to toggle fixture selection if exactly one group is selected
                                        if fix_rect.1.clicked() && selection.group_ids.len() == 1 && total_fixtures.len() != 1 {
                                            let is_fix_selected =
                                                highlight_fixtures.contains(fix_id);

                                    let msg = if is_fix_selected {
                                        ControlEvent::UnLimitSelectionToFixtureInCurrentGroup(*fix_id)
                                    } else {
                                        ControlEvent::LimitSelectionToFixtureInCurrentGroup(*fix_id)
                                    };

                                    self.data
                                        .event_bus_connection
                                        .send(ControlEventMessage::new(EventOriginator::Web, msg));
                                }}
                                });
                                // Controls panel (right)
                                // Only show if one fixture group is selected
                                let selected_fixture = if selection.fixtures_in_group.len() == 1 {
                                    total_fixtures.iter().filter(|(_, _, select)| *select == Selection::Limited ).next().cloned()
                                } else if  total_fixtures.len() == 1 {
                                    total_fixtures.first().cloned()
                                } else {
                                    None
                                };

                                if let Some((g_id, f_id, _)) = selected_fixture {
                                    let fixture = groups.get(&g_id).unwrap().fixtures.get(&f_id).unwrap();

                                    ui.vertical(|ui| {
                                        ui.label("Fixture Controls");
                                        ui.add_space(8.0);
                                        // Brightness slider
                                        let mut brightness = fixture.state.brightness as u32;
                                        if ui
                                            .add(
                                                egui::Slider::new(&mut brightness, 0..=255)
                                                    .text("Brightness"),
                                            )
                                            .changed()
                                        {
                                            // println!("SetBrightness({})", brightness);
                                            self.data
                                                .event_bus_connection
                                                .send(ControlEventMessage::new(EventOriginator::Web, ControlEvent::SetBrightness(brightness as u8)));
                                        }

                                        // Alpha slider
                                        // let mut alpha = fixture.state.alpha as u32;
                                        // if ui
                                        //     .add(
                                        //         egui::Slider::new(&mut alpha, 0..=255)
                                        //             .text("Alpha"),
                                        //     )
                                        //     .changed()
                                        // {
                                        //     // println!("SetAlpha({})", alpha);
                                        //     self.data
                                        //         .event_bus_connection
                                        //         .send(ControlEventMessage::new(EventOriginator::Web, ControlEvent::));
                                        // }

                                        // Strobe speed slider
                                        // let mut strobe = fixture.state.strobe_speed as u32;
                                        // if ui
                                        //     .add(
                                        //         egui::Slider::new(&mut strobe, 0..=255)
                                        //             .text("Strobe Speed"),
                                        //     )
                                        //     .changed()
                                        // {
                                        //     println!("SetStrobeSpeed({})", strobe);
                                        //     self.data
                                        //         .event_bus_connection
                                        //         .send(ControlEventMessage::new(EventOriginator::Web, ControlEvent::Strobe ));
                                        // }

                                        // Color picker
                                        let mut color = [
                                            fixture.state.color.r as f32 / 255.0,
                                            fixture.state.color.g as f32 / 255.0,
                                            fixture.state.color.b as f32 / 255.0,
                                        ];
                                        if ui.color_edit_button_rgb(&mut color).changed() {
                                            let r = (color[0] * 255.0) as u8;
                                            let g = (color[1] * 255.0) as u8;
                                            let b = (color[2] * 255.0) as u8;
                                            println!("SetColor({}, {}, {})", r, g, b);

                                            self.data
                                                .event_bus_connection
                                                .send(ControlEventMessage::new(EventOriginator::Web, ControlEvent::SetColor((r, g, b))));
                                        }
                                    });
                                }
                            });
                        }
                    });
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
