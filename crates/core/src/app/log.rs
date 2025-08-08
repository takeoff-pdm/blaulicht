//
// Log Window Component
//

use std::collections::VecDeque;

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
