//
// Log Window Component
//

use blaulicht_shared::LogLevel;
use chrono::{DateTime, Local};
use egui::{Context, RichText};
use std::collections::{BTreeSet, VecDeque};
use strum::IntoEnumIterator;

use crate::app::components::{self, dropdown, ButtonSize, Dialog};
use crate::app::ui_tokens::{studio, Density, Weight};

mod desktop;

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
const LOG_SOURCE_COL_WIDTH: f32 = 84.0;
const LOG_COL_GAP: f32 = 8.0;

/// Index of `level` in `LogLevel` order.
fn level_index(level: &LogLevel) -> usize {
    match level {
        LogLevel::Debug => 0,
        LogLevel::Info => 1,
        LogLevel::Warn => 2,
        LogLevel::Err => 3,
    }
}

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
    let time = entry.timestamp;

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

        // Keep wrapped text clear of the (floating) vertical scroll bar.
        let scroll = &ui.spacing().scroll;
        let bar_reserve = scroll.bar_inner_margin + scroll.bar_width + scroll.bar_outer_margin;
        let message_width = (ui.available_width() - bar_reserve).max(0.0);
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

/// The level dropdown's label: "All levels", the shown levels, or "No
/// levels".
fn level_filter_label(shown: &[bool; 4]) -> String {
    const NAMES: [&str; 4] = ["Debug", "Info", "Warn", "Error"];
    let on: Vec<&str> = NAMES
        .iter()
        .zip(shown)
        .filter(|(_, on)| **on)
        .map(|(n, _)| *n)
        .collect();
    match on.len() {
        4 => "All levels".to_string(),
        0 => "No levels".to_string(),
        _ => on.join(", "),
    }
}

/// One line as copied by Copy: time, level, source and message.
fn log_line_text(entry: &LogEntry) -> String {
    format!(
        "{} {} {} {}",
        entry.timestamp.format("%H:%M:%S%.3f"),
        log_level_label(&entry.level),
        entry.source,
        entry.message.trim_end()
    )
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
    /// Desktop level filter: which levels are shown, in `LogLevel` order.
    shown_levels: [bool; 4],
    /// Desktop: the selected row's [`LogEntry::seq`] (`tr.sl`).
    selected_row: Option<u64>,
    /// Sequence number for the next entry.
    next_seq: u64,
    // log_height: f32,
}

#[derive(Clone, Debug)]
pub struct LogEntry {
    timestamp: DateTime<Local>,
    level: LogLevel,
    pub message: String,
    pub additional: Option<String>,
    source: String,
    /// Unique per window, so a selected row stays selected as lines scroll.
    seq: u64,
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
            shown_levels: [true; 4],
            selected_row: None,
            next_seq: 0,
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
            timestamp: Local::now(),
            level,
            message,
            additional,
            source,
            seq: self.next_seq,
        };
        self.next_seq += 1;

        self.logs.push_back(entry);

        // Keep only the latest logs
        if self.logs.len() > self.max_logs {
            self.logs.pop_front();
        }
    }

    /// Whether `entry` passes the desktop filters (levels, source, text).
    fn desktop_shows(&self, entry: &LogEntry) -> bool {
        self.shown_levels[level_index(&entry.level)]
            && log_entry_matches_source(entry, self.selected_source.as_deref())
            && log_entry_matches_filter(entry, &self.filter_text)
    }

    /// The desktop Logs page (`design/studio/pages.html#logs`): a 30px bar
    /// with the title, search, level and source dropdowns, Follow, Copy and
    /// Clear; the log table; a footer with counts and the follow state.
    fn draw_desktop(&mut self, outer: &mut egui::Ui) {
        let t = studio(outer.ctx());
        // Edge to edge: out of the central panel's 8px margin, flush with
        // the sidebar, the window top and the status bar.
        let full = outer.max_rect().expand(8.0).intersect(outer.clip_rect());
        outer.allocate_rect(outer.max_rect(), egui::Sense::hover());
        outer.painter().rect_filled(full, 0.0, t.pane);
        let mut page = outer.new_child(
            egui::UiBuilder::new()
                .max_rect(full)
                .layout(egui::Layout::top_down(egui::Align::Min)),
        );
        page.set_clip_rect(full);
        let ui = &mut page;
        ui.spacing_mut().item_spacing = egui::Vec2::ZERO;
        self.draw_desktop_bar(ui);

        let sources = self.known_sources();
        let tag_width = desktop::source_tag_width(ui, &sources);
        let source_col = (tag_width + 2.0 * desktop::CELL_PAD).max(desktop::SOURCE_COL);
        desktop::header(ui, source_col);

        let foot_h = desktop::FOOT_HEIGHT;
        let body_h = (ui.available_height() - foot_h).max(0.0);
        let mut shown = 0usize;
        let mut warnings = 0usize;
        let mut errors = 0usize;
        let query = self.filter_text.clone();
        let scroll_output = ui
            .allocate_ui(egui::vec2(ui.available_width(), body_h), |ui| {
                // Rows stop at the header's bottom and the footer's top: the
                // ScrollArea widens its clip by `clip_rect_margin`, so pin
                // the clip to the body itself.
                let body = ui.max_rect().intersect(ui.clip_rect());
                ui.set_clip_rect(body);
                egui::ScrollArea::vertical()
                    .id_salt("log_desktop_scroll")
                    .auto_shrink([false, false])
                    .stick_to_bottom(self.auto_scroll)
                    .show(ui, |ui| {
                        ui.spacing_mut().item_spacing = egui::Vec2::ZERO;
                        let mut clicked = None;
                        for entry in self.logs.iter().filter(|e| self.desktop_shows(e)) {
                            let key = desktop::row_key(entry);
                            let selected = self.selected_row == Some(key);
                            if desktop::row(
                                ui,
                                entry,
                                shown % 2 == 1,
                                selected,
                                source_col,
                                tag_width,
                                &query,
                            )
                            .clicked()
                            {
                                clicked = Some(key);
                            }
                            shown += 1;
                            match entry.level {
                                LogLevel::Warn => warnings += 1,
                                LogLevel::Err => errors += 1,
                                _ => {}
                            }
                        }
                        if let Some(key) = clicked {
                            // Clicking the selected row again deselects it.
                            self.selected_row = (self.selected_row != Some(key)).then_some(key);
                        }
                        if shown == 0 {
                            desktop::empty(ui);
                        }
                    })
            })
            .inner;

        const AUTO_SCROLL_EPSILON: f32 = 2.0;
        let max_offset_y =
            (scroll_output.content_size.y - scroll_output.inner_rect.height()).max(0.0);
        let at_bottom = scroll_output.state.offset.y >= max_offset_y - AUTO_SCROLL_EPSILON;
        // Scrolling up pauses Follow; scrolling back to the end resumes it.
        let scrolled = ui.rect_contains_pointer(scroll_output.inner_rect)
            && ui.input(|i| i.smooth_scroll_delta.y != 0.0);
        if scrolled && self.auto_scroll != at_bottom {
            self.auto_scroll = at_bottom;
        }

        desktop::footer(
            ui,
            shown,
            self.logs.len(),
            warnings,
            errors,
            self.auto_scroll,
        );
    }

    fn draw_desktop_bar(&mut self, ui: &mut egui::Ui) {
        let t = studio(ui.ctx());
        let (rect, _) = ui.allocate_exact_size(
            egui::vec2(ui.available_width(), desktop::BAR_HEIGHT),
            egui::Sense::hover(),
        );
        ui.painter().rect_filled(rect, 0.0, t.head);
        let inner = rect.shrink2(egui::vec2(6.0, 0.0));
        let mut bar = ui.new_child(
            egui::UiBuilder::new()
                .max_rect(inner)
                .layout(egui::Layout::left_to_right(egui::Align::Center)),
        );
        let bar = &mut bar;
        bar.spacing_mut().item_spacing.x = 6.0;

        bar.add(egui::Label::new(
            RichText::new("Logs")
                .font(Density::of(bar.ctx()).font(Weight::Medium, 13.0))
                .color(t.strong),
        ));
        bar.add_space(6.0);
        desktop::search(bar, &mut self.filter_text);

        let mut counts = [0usize; 4];
        for entry in &self.logs {
            counts[level_index(&entry.level)] += 1;
        }
        let levels = [
            ("Debug", egui_phosphor::regular::BUG, t.scale, false),
            ("Info", egui_phosphor::regular::INFO, t.text, false),
            ("Warn", egui_phosphor::fill::WARNING, t.warn, true),
            ("Error", egui_phosphor::fill::X_CIRCLE, t.red_text, true),
        ];
        let list: Vec<dropdown::Item> = levels
            .iter()
            .zip(counts)
            .map(|((label, icon, color, fill), n)| {
                let item = dropdown::Item::new(*label).meta(n.to_string());
                if *fill {
                    item.icon_fill(icon, *color)
                } else {
                    item.icon(icon, *color)
                }
            })
            .collect();
        let text = level_filter_label(&self.shown_levels);
        // `min-width: 128px`, growing with a longer label.
        let fit = desktop::level_button_width(bar, &text);
        dropdown::multi_select(
            bar,
            "log_levels",
            Some(egui_phosphor::regular::FUNNEL),
            &text,
            &list,
            &mut self.shown_levels,
            Some(fit),
        );

        let sources = self.known_sources();
        let mut list = vec![dropdown::Item::new("All sources").none()];
        list.extend(sources.iter().map(dropdown::Item::new));
        let current = match &self.selected_source {
            None => Some(0),
            Some(s) => sources.iter().position(|x| x == s).map(|i| i + 1),
        };
        let text = self.selected_source.as_deref().unwrap_or("All sources");
        if let Some(i) = dropdown::select(bar, "log_source", &list, current, text, Some(110.0)) {
            self.selected_source = i.checked_sub(1).map(|i| sources[i].clone());
        }

        // Actions flush right: Follow, Copy, Clear.
        let actions_w = desktop::actions_width(bar);
        let gap = (bar.available_width() - actions_w).max(0.0);
        bar.add_space(gap);
        if desktop::icon_button(
            bar,
            egui_phosphor::regular::ARROW_LINE_DOWN,
            Some("Follow"),
            self.auto_scroll,
            "Follow new lines",
        )
        .clicked()
        {
            self.auto_scroll = !self.auto_scroll;
        }
        if desktop::icon_button(
            bar,
            egui_phosphor::regular::COPY,
            None,
            false,
            "Copy visible lines",
        )
        .clicked()
        {
            let text = self
                .logs
                .iter()
                .filter(|e| self.desktop_shows(e))
                .map(log_line_text)
                .collect::<Vec<_>>()
                .join("\n");
            bar.ctx().copy_text(text);
        }
        if desktop::icon_button(bar, egui_phosphor::regular::TRASH, None, false, "Clear").clicked()
        {
            self.clear_logs();
        }
    }

    pub fn clear_logs(&mut self) {
        self.logs.clear();
    }

    pub fn draw(&mut self, ctx: &Context, ui: &mut egui::Ui) {
        if dropdown::is_desktop(ui) {
            self.draw_desktop(ui);
            return;
        }

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

            {
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
        filter_button_label, level_filter_label, log_entry_matches_filter,
        log_entry_matches_source, LogWindow,
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

    #[test]
    fn level_dropdown_label_and_hiding_debug() {
        assert_eq!(level_filter_label(&[true; 4]), "All levels");
        assert_eq!(level_filter_label(&[false; 4]), "No levels");
        assert_eq!(
            level_filter_label(&[false, true, true, true]),
            "Info, Warn, Error"
        );

        let mut window = LogWindow::new(10);
        window.add_log(LogLevel::Debug, "d".to_string(), None, "a".to_string());
        window.add_log(LogLevel::Info, "i".to_string(), None, "a".to_string());
        window.shown_levels[0] = false;
        let shown: Vec<_> = window
            .logs
            .iter()
            .filter(|e| window.desktop_shows(e))
            .map(|e| e.message.as_str())
            .collect();
        assert_eq!(shown, vec!["i"]);
    }
}
