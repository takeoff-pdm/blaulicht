use egui::{vec2, DragValue, Response, Ui, Vec2};

use crate::app::components::{button, ButtonSize, Dialog};

#[derive(Debug, Default)]
pub struct NumberpadState {
    pub dialog_open: bool,
    press_started_at: Option<f64>,
    long_press_triggered: bool,
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
            dialog_size: vec2(260.0, 280.0),
            state: NumberpadState {
                dialog_open: false,
                press_started_at: None,
                long_press_triggered: false,
            },
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
        const LONG_PRESS_THRESHOLD: f64 = 0.35;

        let size = ButtonSize::Medium.dim().0;

        let response = ui.add_sized(size, DragValue::new(value));

        let now = ui.input(|i| i.time);

        if response.is_pointer_button_down_on() {
            if let Some(start) = self.state.press_started_at {
                if !self.state.long_press_triggered && (now - start) >= LONG_PRESS_THRESHOLD {
                    self.state.long_press_triggered = true;
                    response.request_focus();
                    ui.ctx().request_repaint();
                }
            } else {
                self.state.press_started_at = Some(now);
                self.state.long_press_triggered = false;
            }
        }

        let press_started_at = self.state.press_started_at;
        let long_press = self.state.long_press_triggered;

        if response.clicked() {
            let duration = press_started_at
                .map(|start| now - start)
                .unwrap_or_default();

            if long_press || duration >= LONG_PRESS_THRESHOLD {
                response.request_focus();
            } else {
                self.state.dialog_open = true;
                response.surrender_focus();
            }
        }

        if !response.is_pointer_button_down_on() {
            self.state.press_started_at = None;
            self.state.long_press_triggered = false;
        }

        if self.state.dialog_open {
            let mut close_dialog = false;

            Dialog::new(self.dialog_title.clone(), self.dialog_size)
                .with_backdrop()
                .show(ui.ctx(), |ui| {
                    ui.label("Numberpad dialog placeholder");

                    if button(ui, false, "Close", ButtonSize::Medium) {
                        close_dialog = true;
                    }
                });

            if close_dialog {
                self.state.dialog_open = false;
            }
        }

        response
    }
}
