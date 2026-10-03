//
// Log Window Component
//

use blaulicht_shared::LogLevel;
use chrono::{DateTime, Local};
use egui::{Context, RichText};
use std::collections::{BTreeSet, VecDeque};
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

const LOG_FONT_SIZE: f32 = 12.0;
const LOG_TIME_COL_WIDTH: f32 = 60.0;
const LOG_LEVEL_COL_WIDTH: f32 = 48.0;
const LOG_SOURCE_COL_WIDTH: f32 = 118.0;
const LOG_COL_GAP: f32 = 8.0;

fn log_level_label(level: &LogLevel) -> &'static str {
    match level {
        LogLevel::Debug => "DEBUG",
        LogLevel::Info => "INFO",
        LogLevel::Warn => "WARN",
        LogLevel::Err => "ERROR",
    }
}

/// Message text color: problems keep their level color, routine lines use the
/// normal text color so the level badge carries the signal.
fn log_message_color(ui: &egui::Ui, level: &LogLevel) -> egui::Color32 {
    match level {
        LogLevel::Debug => ui.visuals().weak_text_color(),
        LogLevel::Info => ui.visuals().text_color(),
        LogLevel::Warn | LogLevel::Err => log_level_color(level),
    }
}

/// Runs `add_contents` in a column of exactly `width`.
fn log_column<R>(
    ui: &mut egui::Ui,
    width: f32,
    add_contents: impl FnOnce(&mut egui::Ui) -> R,
) -> R {
    ui.allocate_ui_with_layout(
        egui::vec2(width, 0.0),
        egui::Layout::top_down(egui::Align::Min),
        |ui| {
            ui.set_width(width);
            add_contents(ui)
        },
    )
    .inner
}

fn draw_log_header(ui: &mut egui::Ui) {
    let header = |text: &str| RichText::new(text).size(11.0).strong().weak();
    ui.horizontal_top(|ui| {
        ui.spacing_mut().item_spacing.x = LOG_COL_GAP;
        log_column(ui, LOG_TIME_COL_WIDTH, |ui| ui.label(header("TIME")));
        log_column(ui, LOG_LEVEL_COL_WIDTH, |ui| ui.label(header("LEVEL")));
        log_column(ui, LOG_SOURCE_COL_WIDTH, |ui| ui.label(header("SOURCE")));
        ui.label(header("MESSAGE"));
    });
}

fn draw_log_row(ui: &mut egui::Ui, entry: &LogEntry, striped: bool) {
    // Reserve the background slot first so the stripe is painted below the row.
    let background = ui.painter().add(egui::Shape::Noop);
    let level_color = log_level_color(&entry.level);
    let message_color = log_message_color(ui, &entry.level);
    let time: DateTime<Local> = entry.timestamp.into();

    let row = ui.horizontal_top(|ui| {
        ui.spacing_mut().item_spacing.x = LOG_COL_GAP;

        log_column(ui, LOG_TIME_COL_WIDTH, |ui| {
            ui.label(
                RichText::new(time.format("%H:%M:%S").to_string())
                    .monospace()
                    .size(LOG_FONT_SIZE)
                    .color(ui.visuals().weak_text_color()),
            );
        });

        log_column(ui, LOG_LEVEL_COL_WIDTH, |ui| {
            egui::Frame::new()
                .fill(level_color.gamma_multiply(0.18))
                .corner_radius(3.0)
                .inner_margin(egui::Margin::symmetric(4, 0))
                .show(ui, |ui| {
                    ui.label(
                        RichText::new(log_level_label(&entry.level))
                            .monospace()
                            .size(11.0)
                            .strong()
                            .color(level_color),
                    );
                });
        });

        log_column(ui, LOG_SOURCE_COL_WIDTH, |ui| {
            ui.add(
                egui::Label::new(
                    RichText::new(&entry.source)
                        .monospace()
                        .size(LOG_FONT_SIZE)
                        .color(ui.visuals().strong_text_color()),
                )
                .truncate(),
            )
            .on_hover_text(&entry.source);
        });

        let message_width = ui.available_width();
        log_column(ui, message_width, |ui| {
            ui.add(
                egui::Label::new(
                    RichText::new(entry.message.trim_end())
                        .monospace()
                        .size(LOG_FONT_SIZE)
                        .color(message_color),
                )
                .wrap(),
            );

            if let Some(additional) = entry.additional.as_deref() {
                egui::CollapsingHeader::new(RichText::new("Full trace").size(11.0).monospace())
                    .id_salt((entry.timestamp, &entry.source, &entry.message))
                    .default_open(false)
                    .show(ui, |ui| {
                        ui.label(
                            RichText::new(additional)
                                .size(11.0)
                                .monospace()
                                .color(message_color),
                        );
                    });
            }
        });
    });

    if striped {
        ui.painter().set(
            background,
            egui::Shape::rect_filled(
                row.response.rect.expand2(egui::vec2(2.0, 1.0)),
                2.0,
                ui.visuals().faint_bg_color,
            ),
        );
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
    selected_source: Option<String>,
    source_dialog_open: bool,
    // log_height: f32,
}

#[derive(Clone, Debug)]
pub struct LogEntry {
    timestamp: std::time::SystemTime,
    level: LogLevel,
    pub message: String,
    pub additional: Option<String>,
    source: String,
}

fn log_entry_matches_source(entry: &LogEntry, selected_source: Option<&str>) -> bool {
    match selected_source {
        Some(source) => entry.source == source,
        None => true,
    }
}

fn log_entry_matches_filter(entry: &LogEntry, filter_text: &str) -> bool {
    if filter_text.is_empty() {
        return true;
    }

    let filter_text = filter_text.to_lowercase();
    entry.message.to_lowercase().contains(&filter_text)
        || entry.source.to_lowercase().contains(&filter_text)
        || entry
            .additional
            .as_deref()
            .is_some_and(|additional| additional.to_lowercase().contains(&filter_text))
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
            selected_source: None,
            source_dialog_open: false,
        }
    }

    /// Sources of the buffered logs, sorted, for the source filter.
    fn known_sources(&self) -> Vec<String> {
        self.logs
            .iter()
            .map(|entry| entry.source.clone())
            .collect::<BTreeSet<_>>()
            .into_iter()
            .collect()
    }

    pub fn add_log(
        &mut self,
        level: LogLevel,
        message: String,
        additional: Option<String>,
        source: String,
    ) {
        let entry = LogEntry {
            timestamp: std::time::SystemTime::now(),
            level,
            message,
            additional,
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

            if components::button(
                ui,
                self.selected_source.is_some(),
                "Filter Source",
                BUTTON_SIZE,
            ) {
                self.source_dialog_open = true;
            }

            if self.source_dialog_open {
                const ALL_SOURCES: &str = "All";
                let mut options = vec![ALL_SOURCES.to_string()];
                options.extend(self.known_sources());

                let (new_source, changed) = components::selection_dialog(
                    ctx,
                    options,
                    self.selected_source
                        .clone()
                        .unwrap_or_else(|| ALL_SOURCES.to_string()),
                    &mut self.source_dialog_open,
                    "Select Log Source".to_string(),
                );

                if changed {
                    self.selected_source = match new_source.as_str() {
                        ALL_SOURCES => None,
                        other => Some(other.to_string()),
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

        draw_log_header(ui);

        let scroll_output = egui::ScrollArea::vertical()
            .max_height(ui.available_height())
            .max_width(ctx.screen_rect().width() - 100.0)
            .auto_shrink([false, false])
            .stick_to_bottom(self.auto_scroll)
            .show(ui, |ui| {
                let mut row_index = 0usize;
                for entry in &self.logs {
                    // Apply filters
                    match &self.selected_log_level {
                        Some(level) if entry.level != *level => continue,
                        Some(_) | None => {}
                    }

                    if !log_entry_matches_source(entry, self.selected_source.as_deref())
                        || !log_entry_matches_filter(entry, &self.filter_text)
                    {
                        continue;
                    }

                    draw_log_row(ui, entry, row_index % 2 == 1);
                    row_index += 1;
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
            if let Some(source) = &self.selected_source {
                ui.separator();
                ui.label(format!("Source: {source}"));
            }
            if !self.filter_text.is_empty() {
                ui.separator();
                ui.label(format!("Filter: '{}'", self.filter_text));
            }
        });
    }
}

#[cfg(test)]
mod tests {
    use super::{
        filter_button_label, log_entry_matches_filter, log_entry_matches_source, LogWindow,
    };
    use blaulicht_shared::LogLevel;

    #[test]
    fn filter_button_exposes_clear_icon_only_for_an_active_filter() {
        assert_eq!(filter_button_label(""), "Filter");
        assert_eq!(
            filter_button_label("artnet"),
            format!("Filter {}", egui_phosphor::regular::TRASH)
        );
    }

    #[test]
    fn log_details_are_stored_and_included_in_filtering() {
        let mut window = LogWindow::new(10);
        window.add_log(
            LogLevel::Err,
            "Plugin crashed".to_string(),
            Some("hidden_function_name".to_string()),
            "WASM".to_string(),
        );

        let entry = window.logs.back().unwrap();
        assert_eq!(entry.additional.as_deref(), Some("hidden_function_name"));
        assert!(log_entry_matches_filter(entry, "hidden_function"));
        assert!(!log_entry_matches_filter(entry, "unrelated"));
    }

    #[test]
    fn source_filter_keeps_only_the_selected_source() {
        let mut window = LogWindow::new(10);
        window.add_log(LogLevel::Info, "a".to_string(), None, "DMX".to_string());
        window.add_log(
            LogLevel::Info,
            "b".to_string(),
            None,
            "inspector".to_string(),
        );
        window.add_log(LogLevel::Info, "c".to_string(), None, "DMX".to_string());

        assert_eq!(window.known_sources(), vec!["DMX", "inspector"]);

        let dmx: Vec<_> = window
            .logs
            .iter()
            .filter(|entry| log_entry_matches_source(entry, Some("DMX")))
            .map(|entry| entry.message.as_str())
            .collect();
        assert_eq!(dmx, vec!["a", "c"]);
        assert!(window
            .logs
            .iter()
            .all(|entry| log_entry_matches_source(entry, None)));
    }
}
