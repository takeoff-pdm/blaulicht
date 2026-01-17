use egui::{
    vec2, Align, Color32, DragValue, Event, Key, Label, Layout, Modifiers, Response, RichText,
    Sense, Ui, Vec2,
};
use std::sync::atomic::{AtomicU64, Ordering};

use crate::app::components::{button, ButtonSize, Dialog};

const LONG_PRESS_THRESHOLD: f64 = 0.35;
const CLAMP_FLASH_DURATION: f64 = 0.35;
static NEXT_DIALOG_ID: AtomicU64 = AtomicU64::new(1);

#[derive(Debug, Default)]
pub struct NumberpadState {
    pub dialog_open: bool,
    press_started_at: Option<f64>,
    long_press_triggered: bool,
    input_buffer: String,
    clamp_flash_until: Option<f64>,
    dialog_instance_id: Option<u64>,
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
        self.clamp_flash_until = None;
        self.dialog_instance_id = None;
    }

    fn trigger_clamp_flash(&mut self, now: f64) {
        self.clamp_flash_until = Some(now + CLAMP_FLASH_DURATION);
    }

    fn clamp_flash_active(&mut self, now: f64) -> bool {
        if let Some(until) = self.clamp_flash_until {
            if now < until {
                return true;
            }
            self.clamp_flash_until = None;
        }
        false
    }

    fn initialize_buffer<T: egui::emath::Numeric>(&mut self, value: &T, range: Option<(f64, f64)>) {
        self.input_buffer = format_numeric_value(value);
        enforce_input_constraints::<T>(&mut self.input_buffer, range);
    }
}

pub struct Numberpad {
    dialog_title: String,
    dialog_size: Vec2,
    field_dimensions: Vec2,
    state: NumberpadState,
    range: Option<(f64, f64)>,
    random_ids: bool,
}

impl Numberpad {
    pub fn new() -> Self {
        Self {
            dialog_title: "Numberpad".to_owned(),
            dialog_size: vec2(480.0, 420.0),
            field_dimensions: ButtonSize::Medium.dim().0,
            state: NumberpadState::default(),
            range: None,
            random_ids: false,
        }
    }

    pub fn dialog_title(mut self, title: impl Into<String>) -> Self {
        self.dialog_title = title.into();
        self
    }

    pub fn field_size(self, size: Vec2) -> Self {
        Self {
            field_dimensions: size,
            ..self
        }
    }

    pub fn field_width(self, size: f32) -> Self {
        Self {
            field_dimensions: Vec2 {
                x: size,
                y: self.field_dimensions.y,
            },
            ..self
        }
    }

    pub fn dialog_size(mut self, size: Vec2) -> Self {
        self.dialog_size = size;
        self
    }

    pub fn range(mut self, min: f64, max: f64) -> Self {
        let (min, max) = if max < min { (max, min) } else { (min, max) };
        self.range = Some((min, max));
        self
    }

    pub fn with_random_ids(mut self) -> Self {
        self.random_ids = true;
        self
    }

    pub fn ui<T>(&mut self, ui: &mut Ui, value: &mut T) -> Response
    where
        T: egui::emath::Numeric,
    {
        let state = &mut self.state;
        let button_size = self.field_dimensions;
        let mut drag_value = DragValue::new(value);
        if let Some((min, max)) = self.range {
            let min_t = T::from_f64(min);
            let max_t = T::from_f64(max);
            drag_value = drag_value.clamp_range(min_t..=max_t);
        }
        let response = ui.add_sized(button_size, drag_value);
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
                state.initialize_buffer(value, self.range);
                state.dialog_open = true;
                state.dialog_instance_id = None;
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
                state.initialize_buffer(value, self.range);
            }

            if self.random_ids && state.dialog_instance_id.is_none() {
                let new_id = NEXT_DIALOG_ID.fetch_add(1, Ordering::Relaxed);
                state.dialog_instance_id = Some(new_id);
            }

            let dialog_label = if let (true, Some(id)) = (self.random_ids, state.dialog_instance_id)
            {
                format!("{}::{id}", self.dialog_title)
            } else {
                self.dialog_title.clone()
            };

            Dialog::new(dialog_label, self.dialog_size)
                .with_backdrop()
                .show(ui.ctx(), |dialog_ui| {
                    render_numberpad_contents(dialog_ui, value, state, self.range)
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
    range: Option<(f64, f64)>,
) {
    const PAD_SIDE: f32 = 60.0;
    let pad_button = ButtonSize::Medium
        .with_width(PAD_SIDE)
        .with_height(PAD_SIDE);
    let pad_size = pad_button.dim().0;
    let now = ui.ctx().input(|i| i.time);

    let display_height = pad_size.y;
    let display_font_size = display_height * 0.65;

    let focus_id = ui.auto_id_with("numberpad_focus");
    let focus_rect = egui::Rect::from_min_size(ui.min_rect().min, Vec2::ZERO);
    let focus_response = ui.interact(focus_rect, focus_id, Sense::focusable_noninteractive());

    if !focus_response.has_focus() {
        focus_response.request_focus();
    }

    let mut clamp_applied = false;

    ui.input_mut(|input| {
        let mut appended_digit_from_text = false;
        let mut remaining_events = Vec::new();

        for event in std::mem::take(&mut input.events) {
            match event {
                Event::Text(text) => {
                    let mut non_digits = String::new();
                    let mut appended_digit = false;

                    for ch in text.chars() {
                        if ch.is_ascii_digit() {
                            append_digit(&mut state.input_buffer, ch);
                            appended_digit = true;
                        } else {
                            non_digits.push(ch);
                        }
                    }

                    if appended_digit {
                        appended_digit_from_text = true;
                    }

                    if !non_digits.is_empty() {
                        remaining_events.push(Event::Text(non_digits));
                    }
                }
                other => remaining_events.push(other),
            }
        }

        let digit_keys = [
            (Key::Num0, '0'),
            (Key::Num1, '1'),
            (Key::Num2, '2'),
            (Key::Num3, '3'),
            (Key::Num4, '4'),
            (Key::Num5, '5'),
            (Key::Num6, '6'),
            (Key::Num7, '7'),
            (Key::Num8, '8'),
            (Key::Num9, '9'),
        ];

        let mut fallback_digits = Vec::new();
        for &(key, digit) in &digit_keys {
            if input.consume_key(Modifiers::NONE, key) {
                fallback_digits.push(digit);
            }
        }

        if !fallback_digits.is_empty() && !appended_digit_from_text {
            for digit in fallback_digits {
                append_digit(&mut state.input_buffer, digit);
            }
        }

        if input.consume_key(Modifiers::NONE, Key::Backspace) {
            clamp_applied |= handle_button_action("Backspace", value, state, range);
        }

        if input.consume_key(Modifiers::NONE, Key::Enter) {
            clamp_applied |= handle_button_action("Enter", value, state, range);
        }

        if input.consume_key(Modifiers::NONE, Key::Escape) {
            clamp_applied |= handle_button_action("Cancel", value, state, range);
        }

        input.events = remaining_events;
    });

    if clamp_applied {
        state.trigger_clamp_flash(now);
        ui.ctx().request_repaint();
    }

    let display_text = state.input_buffer.clone();
    let clamp_flash_active = state.clamp_flash_active(now);

    if clamp_flash_active {
        ui.ctx().request_repaint();
    }

    ui.allocate_ui_with_layout(
        vec2(ui.available_width(), display_height),
        Layout::right_to_left(Align::Center),
        move |display_ui| {
            let mut display_rich_text = RichText::new(display_text)
                .monospace()
                .size(display_font_size);

            if clamp_flash_active {
                display_rich_text = display_rich_text.color(Color32::RED);
            }

            display_ui.add_sized(
                vec2(display_ui.available_width(), display_height),
                Label::new(display_rich_text),
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

                    let active = label == "Enter";

                    if button(grid, active, label, pad_button) {
                        let clamped = handle_button_action(label, value, state, range);
                        if clamped {
                            state.trigger_clamp_flash(now);
                        }
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
    range: Option<(f64, f64)>,
) -> bool {
    let mut clamped = false;

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
            if let Some((parsed, was_clamped)) =
                parse_numeric_input::<T>(&state.input_buffer, range)
            {
                *value = parsed;
                if was_clamped {
                    state.input_buffer = format_numeric_value(&parsed);
                    clamped |= was_clamped;
                } else {
                    state.close_dialog();
                }
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

    clamped
}

fn append_digit(buffer: &mut String, digit: char) {
    if buffer == "0" {
        buffer.clear();
    }
    buffer.push(digit);
}

fn enforce_input_constraints<T: egui::emath::Numeric>(
    buffer: &mut String,
    range: Option<(f64, f64)>,
) -> bool {
    match parse_numeric_input::<T>(buffer.as_str(), range) {
        Some((parsed, clamped)) => {
            let formatted = format_numeric_value(&parsed);
            if *buffer != formatted {
                *buffer = formatted;
            }
            clamped
        }
        None => {
            buffer.clear();
            buffer.push('0');
            true
        }
    }
}

fn clamp_value_to_bounds<T: egui::emath::Numeric>(
    mut value: f64,
    range: Option<(f64, f64)>,
) -> (f64, bool) {
    let mut clamped = false;

    if let Some((min, max)) = range {
        let ranged = value.clamp(min, max);
        if ranged != value {
            value = ranged;
            clamped = true;
        }
    }

    let min_bound = T::MIN.to_f64();
    if min_bound.is_finite() && value < min_bound {
        value = min_bound;
        clamped = true;
    }

    let max_bound = T::MAX.to_f64();
    if max_bound.is_finite() && value > max_bound {
        value = max_bound;
        clamped = true;
    }

    if T::INTEGRAL {
        let rounded = value.round();
        if (rounded - value).abs() > f64::EPSILON {
            clamped = true;
        }
        value = rounded;

        if let Some((min, max)) = range {
            let ranged = value.clamp(min, max);
            if ranged != value {
                value = ranged;
                clamped = true;
            }
        }

        if min_bound.is_finite() && value < min_bound {
            value = min_bound;
            clamped = true;
        }

        if max_bound.is_finite() && value > max_bound {
            value = max_bound;
            clamped = true;
        }
    }

    (value, clamped)
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

fn parse_numeric_input<T: egui::emath::Numeric>(
    input: &str,
    range: Option<(f64, f64)>,
) -> Option<(T, bool)> {
    let trimmed = input.trim();
    let mut clamped = false;

    let mut value = if trimmed.is_empty() {
        0.0
    } else {
        let parsed = trimmed.parse::<f64>().ok()?;
        if !parsed.is_finite() {
            return None;
        }
        parsed
    };

    let (bounded, was_clamped) = clamp_value_to_bounds::<T>(value, range);
    value = bounded;
    clamped |= was_clamped;

    if value.is_finite() {
        Some((T::from_f64(value), clamped))
    } else {
        None
    }
}
