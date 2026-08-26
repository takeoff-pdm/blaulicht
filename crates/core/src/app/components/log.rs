//
// Log Window Component
//

use blaulicht_shared::LogLevel;
use chrono::{DateTime, Local};
use egui::{Context, RichText};
use std::collections::VecDeque;
use strum::IntoEnumIterator;

use crate::app::components::{self, ButtonSize, Dialog};

fn filter_button_label(filter_text: &str) -> String {
    if filter_text.is_empty() {
        "Filter".to_string()
    } else {
        format!("Filter {}", egui_phosphor::regular::TRASH)
    }
}

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
    pub logs: VecDeque<LogEntry>,
    max_logs: usize,
    auto_scroll: bool,
    filter_text: String,
    filter_dialog_open: bool,
    selected_log_level: Option<LogLevel>,
    select_dialog_open: bool,
    // log_height: f32,
}

#[derive(Clone, Debug)]
pub struct LogEntry {
    timestamp: std::time::SystemTime,
    level: LogLevel,
    pub message: String,
    source: String,
}

impl LogWindow {
    pub fn new(max_logs: usize) -> Self {
        Self {
            logs: VecDeque::new(),
            max_logs,
            auto_scroll: true,
            filter_text: String::new(),
            filter_dialog_open: false,
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
        ui.horizontal_wrapped(|ui| {
            const BUTTON_SIZE: ButtonSize = ButtonSize::Medium;
            ui.set_min_height(BUTTON_SIZE.dim().0.y);

            ui.label("Logs");

            ui.separator();

            let filter_label = filter_button_label(&self.filter_text);
            if components::button(ui, !self.filter_text.is_empty(), &filter_label, BUTTON_SIZE) {
                if self.filter_text.is_empty() {
                    self.filter_dialog_open = true;
                } else {
                    self.filter_text.clear();
                    self.filter_dialog_open = false;
                }
            }

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
                    "Select Log Level".to_string(),
                );

                if changed {
                    self.selected_log_level = match new_log_level.as_str() {
                        ALL_LEVELS => None,
                        other => Some(LogLevel::from(other)),
                    }
                }
            }
        });

        if self.filter_dialog_open {
            let dialog_width = (ctx.content_rect().width() - 32.0).clamp(240.0, 400.0);
            let response = Dialog::new("Filter Logs".to_string(), egui::vec2(dialog_width, 150.0))
                .with_backdrop()
                .dismiss_on_backdrop()
                .show(ctx, |ui| {
                    ui.heading(RichText::new("Filter logs").strong());
                    ui.add_space(12.0);
                    components::TextInput::new(dialog_width - 50.0)
                        .with_hint_text("Message or source")
                        .with_monospace_font(false)
                        .ui(ui, &mut self.filter_text);
                    ui.add_space(12.0);
                    ui.horizontal(|ui| {
                        if components::button(ui, true, "Done", ButtonSize::Medium) {
                            self.filter_dialog_open = false;
                        }
                        if !self.filter_text.is_empty()
                            && components::button(
                                ui,
                                false,
                                egui_phosphor::regular::TRASH,
                                ButtonSize::Medium,
                            )
                        {
                            self.filter_text.clear();
                            self.filter_dialog_open = false;
                        }
                    });
                });

            if response.cancel_requested {
                self.filter_dialog_open = false;
            }
        }

        ui.separator();

        // Log display area - resizable content area
        // ui.horizontal(|ui| {
        //     ui.label("Log Height:");
        //     ui.add(egui::Slider::new(&mut self.log_height, 100.0..=600.0).text("height"));
        // });

        let scroll_output = egui::ScrollArea::vertical()
            .max_height(ui.available_height())
            .max_width(ctx.screen_rect().width() - 100.0)
            .auto_shrink([false, false])
            .stick_to_bottom(self.auto_scroll)
            .show(ui, |ui| {
                ui.style_mut().override_text_style = Some(egui::TextStyle::Monospace);

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
                    let prefix = format!(
                        "[{}] {:?} | {} |",
                        time_formatted, entry.level, entry.source
                    );
                    let indent = " ".repeat(prefix.chars().count() + 1);
                    let color = log_level_color(&entry.level);

                    let mut lines = entry
                        .message
                        .split('\n')
                        .map(|line| line.trim_end_matches('\r'));

                    if let Some(first_line) = lines.next() {
                        let first = if first_line.is_empty() {
                            format!("{prefix} ")
                        } else {
                            format!("{prefix} {first_line}")
                        };

                        ui.colored_label(color, RichText::new(first).size(12.0).monospace());

                        for line in lines {
                            let continuation = if line.is_empty() {
                                format!("{indent} ")
                            } else {
                                format!("{indent}{line}")
                            };

                            ui.colored_label(
                                color,
                                RichText::new(continuation).size(12.0).monospace(),
                            );
                        }
                    } else {
                        ui.colored_label(
                            color,
                            RichText::new(format!("{prefix} ")).size(12.0).monospace(),
                        );
                    }

                    ui.separator();
                }
            });

        const AUTO_SCROLL_EPSILON: f32 = 2.0;
        let max_offset_y =
            (scroll_output.content_size.y - scroll_output.inner_rect.height()).max(0.0);
        let at_bottom = scroll_output.state.offset.y >= max_offset_y - AUTO_SCROLL_EPSILON;
        if self.auto_scroll != at_bottom {
            self.auto_scroll = at_bottom;
        }

        // Status bar
        ui.separator();

        ui.horizontal_wrapped(|ui| {
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

#[cfg(test)]
mod tests {
    use super::filter_button_label;

    #[test]
    fn filter_button_exposes_clear_icon_only_for_an_active_filter() {
        assert_eq!(filter_button_label(""), "Filter");
        assert_eq!(
            filter_button_label("artnet"),
            format!("Filter {}", egui_phosphor::regular::TRASH)
        );
    }
}
