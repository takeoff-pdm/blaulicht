//
// Log Window Component
//

use blaulicht_shared::LogLevel;
use chrono::{DateTime, Local};
use egui::{Color32, Context, FontId, RichText, TextEdit};
use std::collections::VecDeque;
use strum::IntoEnumIterator;

use crate::app::components::{self, ButtonSize};

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
    select_dialog_open: bool,
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
            select_dialog_open: false,
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

    pub fn draw(&mut self, ctx: &Context, ui: &mut egui::Ui) {
        // Controls
        ui.horizontal(|ui| {
            const BUTTON_SIZE: ButtonSize = ButtonSize::Medium;
            const FONT_SIZE: f32 = ButtonSize::Medium.dim().1;

            ui.set_min_height(BUTTON_SIZE.dim().0.y);

            ui.label("Logs");

            ui.separator();

            ui.label("Filter:");

            ui.add(
                TextEdit::singleline(&mut self.filter_text).font(FontId::proportional(FONT_SIZE)),
            );

            ui.separator();

            if components::button(ui, self.auto_scroll, "Auto-Scroll", BUTTON_SIZE) {
                self.auto_scroll = !self.auto_scroll;
            }

            if components::button(ui, false, "Clear", BUTTON_SIZE) {
                self.clear_logs();
            }

            ui.separator();

            if components::button(
                ui,
                self.selected_log_level.is_some(),
                "Filter Level",
                BUTTON_SIZE,
            ) {
                self.select_dialog_open = true;
            }

            ui.separator();

            ui.label(RichText::new("Level Filter:").size(FONT_SIZE));
            ui.label(
                RichText::new(match self.selected_log_level.is_some() {
                    true => "Active",
                    false => "N/A",
                })
                .size(FONT_SIZE)
                .color(Color32::LIGHT_RED),
            );

            if self.select_dialog_open {
                const ALL_LEVELS: &str = "All";
                let mut options = LogLevel::iter()
                    .map(|l| l.to_string())
                    .collect::<Vec<String>>();
                options.push(ALL_LEVELS.to_string());

                let (new_log_level, changed) = components::selection_dialog(
                    ctx,
                    options,
                    self.selected_log_level
                        .as_ref()
                        .map(|l| l.to_string())
                        .unwrap_or_else(|| ALL_LEVELS.to_string()),
                    &mut self.select_dialog_open,
                );

                if changed {
                    self.selected_log_level = match new_log_level.as_str() {
                        ALL_LEVELS => None,
                        other => Some(LogLevel::from(other)),
                    }
                }
            }
        });

        ui.separator();

        // Log display area - resizable content area
        // ui.horizontal(|ui| {
        //     ui.label("Log Height:");
        //     ui.add(egui::Slider::new(&mut self.log_height, 100.0..=600.0).text("height"));
        // });

        egui::ScrollArea::vertical()
            .max_height(ui.available_height())
            .max_width(ctx.screen_rect().width() - 100.0)
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
                    ui.colored_label(
                        log_level_color(&entry.level),
                        RichText::new(log_text).size(12.0),
                    );

                    should_scroll_to_bottom = true;

                    ui.separator();
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
