// pub trait UIComponent {
//     pub fn text(&mut self, text: &str);
//     pub fn new(text: &str, )
// }

use crate::{blaulicht::bl_controls_set, printc};

type ButtonCallback = fn(value: bool);

pub struct Button {
    x: usize,
    y: usize,
    callback: ButtonCallback,
    text: String,
    state: bool,
}

impl Button {
    pub fn new(x: usize, y: usize, text: String, callback: ButtonCallback) -> Self {
        let btn = Self {
            x,
            y,
            callback,
            text,
            state: false,
        };

        btn.render();

        btn
    }

    pub fn set_value(&mut self, value: bool) {
        self.state = value;
        self.render();
    }

    pub fn set_text(&mut self, value: String) {
        self.text = value;
        self.render();
    }

    fn render(&self) {
        printc!(self.x as u8, self.y as u8, "{}", self.text);
        bl_controls_set(self.x as u8, self.y as u8, self.state);
    }
}
