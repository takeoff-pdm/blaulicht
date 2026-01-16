use egui::{Align, FontId, Response, TextBuffer, TextEdit, Ui};

use crate::app::components::ButtonSize;

const FONT_SIZE_OFFSET: f32 = 7.0;

pub struct TextInput {
    width: f32,
    button_size: ButtonSize,
    hint_text: Option<String>,
    monospace: bool,
    font_size_offset: f32,
}

impl TextInput {
    pub fn new(width: f32) -> Self {
        Self {
            width,
            button_size: ButtonSize::Medium,
            hint_text: None,
            monospace: true,
            font_size_offset: FONT_SIZE_OFFSET,
        }
    }

    pub fn with_width(mut self, width: f32) -> Self {
        self.width = width;
        self
    }

    pub fn with_button_size(mut self, size: ButtonSize) -> Self {
        self.button_size = size;
        self
    }

    pub fn with_hint_text(mut self, hint: impl Into<String>) -> Self {
        self.hint_text = Some(hint.into());
        self
    }

    pub fn with_monospace_font(mut self, monospace: bool) -> Self {
        self.monospace = monospace;
        self
    }

    pub fn with_font_size_offset(mut self, offset: f32) -> Self {
        self.font_size_offset = offset;
        self
    }

    pub fn ui<T: TextBuffer>(&self, ui: &mut Ui, value: &mut T) -> Response {
        let (button_size, base_font_size) = self.button_size.dim();
        let font_size = base_font_size + self.font_size_offset;

        let mut text_edit = TextEdit::singleline(value).vertical_align(Align::Center);

        text_edit = if self.monospace {
            text_edit.font(FontId::monospace(font_size))
        } else {
            text_edit.font(FontId::proportional(font_size))
        };

        if let Some(hint) = &self.hint_text {
            text_edit = text_edit.hint_text(hint.clone());
        }

        ui.add_sized([self.width, button_size.y], text_edit)
    }
}
