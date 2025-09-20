//
// Log Window Component
//

use std::collections::VecDeque;

use blaulicht_shared::LogLevel;
use chrono::{DateTime, Local};
use strum::IntoEnumIterator;

fn log_level_color(from: &LogLevel) -> egui::Color32 {
    match from {
        LogLevel::Debug => egui::Color32::from_gray(150),
        LogLevel::Info => egui::Color32::from_rgb(100, 150, 255),
        LogLevel::Warn => egui::Color32::from_rgb(255, 200, 100),
        LogLevel::Err => egui::Color32::from_rgb(255, 100, 100),
    }
}

/// A log window component that displays scrolling log messages
pub struct LogWindow {
    logs: VecDeque<LogEntry>,
    max_logs: usize,
    auto_scroll: bool,
    filter_text: String,
    selected_log_level: Option<LogLevel>,
    // log_height: f32,
}

#[derive(Clone, Debug)]
pub struct LogEntry {
    timestamp: std::time::SystemTime,
    level: LogLevel,
    message: String,
    source: String,
}

impl LogWindow {
    pub fn new(max_logs: usize) -> Self {
        Self {
            logs: VecDeque::new(),
            max_logs,
            auto_scroll: true,
            filter_text: String::new(),
            selected_log_level: None,
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
        // Controls
        ui.horizontal(|ui| {
            ui.label("Logs");

            ui.separator();

            ui.label("Filter:");
            ui.text_edit_singleline(&mut self.filter_text);

            ui.separator();

            ui.label("Level:");
            egui::ComboBox::from_id_salt("log_level")
                .selected_text(format!("{:?}", self.selected_log_level))
                .show_ui(ui, |ui| {
                    for level in LogLevel::iter() {
                        ui.selectable_value(
                            &mut self.selected_log_level,
                            Some(level.clone()),
                            format!("{level:?}"),
                        );
                    }
                    ui.selectable_value(&mut self.selected_log_level, None, "All");
                });

            ui.separator();

            ui.checkbox(&mut self.auto_scroll, "Auto-scroll");

            if ui.button("Clear").clicked() {
                self.clear_logs();
            }
        });

        // Log display area - resizable content area
        // ui.horizontal(|ui| {
        //     ui.label("Log Height:");
        //     ui.add(egui::Slider::new(&mut self.log_height, 100.0..=600.0).text("height"));
        // });

        egui::ScrollArea::vertical()
            .max_height(ui.available_height())
            .auto_shrink([false, false])
            .show(ui, |ui| {
                ui.style_mut().override_text_style = Some(egui::TextStyle::Monospace);

                let mut should_scroll_to_bottom = false;

                for entry in &self.logs {
                    // Apply filters
                    match &self.selected_log_level {
                        Some(level) if entry.level != *level => continue,
                        Some(_) | None => {}
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

                    let datetime: DateTime<Local> = entry.timestamp.into();
                    let time_formatted = datetime.format("%H:%M:%S").to_string();

                    // Create log line
                    let log_text = format!(
                        "[{}] {:?} | {} | {}",
                        time_formatted, entry.level, entry.source, entry.message
                    );

                    // Display with appropriate color
                    ui.colored_label(log_level_color(&entry.level), log_text);

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
