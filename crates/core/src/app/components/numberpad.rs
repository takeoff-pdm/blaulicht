use egui::{vec2, Align, DragValue, Label, Layout, Response, RichText, Ui, Vec2};

use crate::app::components::{button, ButtonSize, Dialog};

const LONG_PRESS_THRESHOLD: f64 = 0.35;

#[derive(Debug, Default)]
pub struct NumberpadState {
    pub dialog_open: bool,
    press_started_at: Option<f64>,
    long_press_triggered: bool,
    input_buffer: String,
}

impl NumberpadState {
    fn reset_gesture(&mut self) {
        self.press_started_at = None;
        self.long_press_triggered = false;
    }

    fn close_dialog(&mut self) {
        self.dialog_open = false;
        self.input_buffer.clear();
        self.reset_gesture();
    }

    fn initialize_buffer<T: egui::emath::Numeric>(&mut self, value: &T) {
        self.input_buffer = format_numeric_value(value);
    }
}

pub struct Numberpad {
    dialog_title: String,
    dialog_size: Vec2,
    state: NumberpadState,
}

impl Numberpad {
    pub fn new() -> Self {
        Self {
            dialog_title: "Numberpad".to_owned(),
            dialog_size: vec2(480.0, 420.0),
            state: NumberpadState::default(),
        }
    }

    pub fn dialog_title(mut self, title: impl Into<String>) -> Self {
        self.dialog_title = title.into();
        self
    }

    pub fn dialog_size(mut self, size: Vec2) -> Self {
        self.dialog_size = size;
        self
    }

    pub fn ui<T>(&mut self, ui: &mut Ui, value: &mut T) -> Response
    where
        T: egui::emath::Numeric,
    {
        let state = &mut self.state;
        let button_size = ButtonSize::Medium.dim().0;
        let response = ui.add_sized(button_size, DragValue::new(value));
        let now = ui.input(|i| i.time);

        if response.is_pointer_button_down_on() {
            if let Some(start) = state.press_started_at {
                if !state.long_press_triggered && (now - start) >= LONG_PRESS_THRESHOLD {
                    state.long_press_triggered = true;
                    response.request_focus();
                    ui.ctx().request_repaint();
                }
            } else {
                state.press_started_at = Some(now);
                state.long_press_triggered = false;
            }
        }

        if response.clicked() {
            let duration = state
                .press_started_at
                .map(|start| now - start)
                .unwrap_or_default();

            if state.long_press_triggered || duration >= LONG_PRESS_THRESHOLD {
                response.request_focus();
            } else {
                state.initialize_buffer(value);
                state.dialog_open = true;
                state.reset_gesture();
                response.surrender_focus();
                ui.ctx().request_repaint();
            }
        }

        if !response.is_pointer_button_down_on() {
            state.reset_gesture();
        }

        if state.dialog_open {
            if state.input_buffer.is_empty() {
                state.initialize_buffer(value);
            }

            Dialog::new(self.dialog_title.clone(), self.dialog_size)
                .with_backdrop()
                .show(ui.ctx(), |dialog_ui| {
                    render_numberpad_contents(dialog_ui, value, state)
                });
        }

        response
    }

    pub fn close(&mut self) {
        self.state.close_dialog();
    }
}

fn render_numberpad_contents<T: egui::emath::Numeric>(
    ui: &mut Ui,
    value: &mut T,
    state: &mut NumberpadState,
) {
    const PAD_SIDE: f32 = 60.0;
    let pad_button = ButtonSize::Medium
        .with_width(PAD_SIDE)
        .with_height(PAD_SIDE);
    let pad_size = pad_button.dim().0;

    let display_height = pad_size.y;
    let display_font_size = display_height * 0.65;
    let display_text = state.input_buffer.clone();

    ui.allocate_ui_with_layout(
        vec2(ui.available_width(), display_height),
        Layout::right_to_left(Align::Center),
        move |display_ui| {
            display_ui.add_sized(
                vec2(display_ui.available_width(), display_height),
                Label::new(
                    RichText::new(display_text)
                        .monospace()
                        .size(display_font_size),
                ),
            );
        },
    );

    ui.add_space(8.0);

    let layout: [[&str; 7]; 4] = [
        ["", "", "7", "8", "9", "", ""],
        ["CLR", "", "4", "5", "6", "", "Cancel"],
        ["Backspace", "", "1", "2", "3", "", "Enter"],
        ["", "", "", "0", "", "", ""],
    ];

    egui::Grid::new("numberpad_grid")
        .spacing(vec2(6.0, 6.0))
        .show(ui, |grid| {
            for row in layout {
                for label in row {
                    if label.is_empty() {
                        grid.add_sized(pad_size, Label::new(""));
                        continue;
                    }

                    if button(grid, false, label, pad_button) {
                        handle_button_action(label, value, state);
                        grid.ctx().request_repaint();
                    }
                }
                grid.end_row();
            }
        });
}

fn handle_button_action<T: egui::emath::Numeric>(
    label: &str,
    value: &mut T,
    state: &mut NumberpadState,
) {
    match label {
        "Backspace" => {
            if state.input_buffer.len() <= 1 {
                state.input_buffer = "0".to_string();
            } else {
                state.input_buffer.pop();
                if state.input_buffer == "-" {
                    state.input_buffer = "0".to_string();
                }
            }
        }
        "CLR" => {
            state.input_buffer = "0".to_string();
        }
        "Enter" => {
            if let Some(parsed) = parse_numeric_input::<T>(&state.input_buffer) {
                *value = parsed;
                state.close_dialog();
            }
        }
        "Cancel" => {
            state.close_dialog();
        }
        digit if digit.len() == 1 && digit.chars().all(|c| c.is_ascii_digit()) => {
            append_digit(&mut state.input_buffer, digit.chars().next().unwrap());
        }
        _ => {}
    }
}

fn append_digit(buffer: &mut String, digit: char) {
    if buffer == "0" {
        buffer.clear();
    }
    buffer.push(digit);
}

fn format_numeric_value<T: egui::emath::Numeric>(value: &T) -> String {
    let value_f64 = value.to_f64();

    let mut formatted = if T::INTEGRAL || value_f64.fract().abs() < f64::EPSILON {
        format!("{:.0}", value_f64)
    } else {
        format!("{:.4}", value_f64)
    };

    if formatted.contains('.') {
        while formatted.ends_with('0') {
            formatted.pop();
        }
        if formatted.ends_with('.') {
            formatted.pop();
        }
    }

    if formatted.is_empty() {
        "0".to_string()
    } else {
        formatted
    }
}

fn parse_numeric_input<T: egui::emath::Numeric>(input: &str) -> Option<T> {
    let trimmed = input.trim();
    if trimmed.is_empty() {
        return Some(T::from_f64(0.0));
    }

    let mut value = trimmed.parse::<f64>().ok()?;

    if T::INTEGRAL {
        value = value.round();
    }

    let min = T::MIN.to_f64();
    if min.is_finite() {
        value = value.max(min);
    }

    let max = T::MAX.to_f64();
    if max.is_finite() {
        value = value.min(max);
    }

    if value.is_finite() {
        Some(T::from_f64(value))
    } else {
        None
    }
}
