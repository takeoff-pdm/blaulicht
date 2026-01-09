use egui::{vec2, Color32, CornerRadius, Frame, Margin, Stroke, Ui, Vec2};

use crate::app::popup::render_popup_backdrop;

pub struct Dialog {
    label: String,
    popup_size: Vec2,
    fixed_pos: Option<Vec2>,
    moveable: bool,
    with_backdrop: bool,
}

impl Dialog {
    pub fn new(label: String, popup_size: Vec2) -> Self {
        Self {
            label,
            popup_size,
            fixed_pos: None,
            moveable: false,
            with_backdrop: false,
        }
    }

    pub fn moveable(self) -> Self {
        Self {
            moveable: true,
            ..self
        }
    }

    pub fn with_backdrop(self) -> Self {
        Self {
            with_backdrop: true,
            ..self
        }
    }

    pub fn fixed_pos(self, pos: Vec2) -> Self {
        Self {
            fixed_pos: Some(pos),
            ..self
        }
    }

    pub fn show(&self, ctx: &egui::Context, add_contents: impl FnOnce(&mut Ui)) {
        if self.with_backdrop {
            render_popup_backdrop(ctx);
        }

        match self.fixed_pos {
            Some(pos) => self.show_fixed_pos(ctx, pos, add_contents),
            None => self.show_dynamic_pos(ctx, add_contents),
        }
    }

    fn show_dynamic_pos(&self, ctx: &egui::Context, add_contents: impl FnOnce(&mut Ui)) {
        let screen_rect = ctx.screen_rect();

        // println!(
        //     "center: {} {}",
        //     screen_rect.center().x,
        //     screen_rect.center().y
        // );
        let center_pos = egui::Pos2::new(
            screen_rect.center().x - self.popup_size.x / 2.0,
            screen_rect.center().y - self.popup_size.y / 2.0,
        );

        // Clamp to screen boundaries
        // center_pos.x = center_pos
        //     .x
        //     .clamp(screen_rect.left(), screen_rect.right() - popup_size.x);
        // center_pos.y = center_pos
        //     .y
        //     .clamp(screen_rect.top(), screen_rect.bottom() - popup_size.y);

        let window_proto = egui::Window::new(&self.label)
            // .min_size(popup_size)
            .fixed_size(vec2(self.popup_size.x, 0.0))
            .collapsible(false)
            .resizable(false)
            .title_bar(false);

        let window_proto = match self.moveable {
            true => window_proto.default_pos(center_pos),
            false => window_proto.fixed_pos(center_pos),
        };

        window_proto
            .frame(Frame {
                corner_radius: CornerRadius::same(1),
                fill: Color32::from_gray(40),
                stroke: Stroke::new(1.0, Color32::from_gray(60)),
                inner_margin: Margin::symmetric(6, 12),
                ..Frame::default()
            })
            .show(ctx, |ui| {
                // ui.allocate_ui_with_layout(
                //     egui::vec2(ui.available_width(), ui.available_height()), // fixed width, max height
                //     egui::Layout::top_down(egui::Align::Min),
                //     |ui| {
                ui.vertical_centered(|ui| {
                    add_contents(ui);
                });
            });
    }

    fn show_fixed_pos(
        &self,
        ctx: &egui::Context,
        fixed_pos: Vec2,
        add_contents: impl FnOnce(&mut Ui),
    ) {
        let window_proto = egui::Window::new(&self.label)
            // .min_size(popup_size)
            .min_size(self.popup_size)
            .fixed_size(self.popup_size)
            .collapsible(false)
            .resizable(false)
            .title_bar(false);

        let window_proto = match self.moveable {
            true => window_proto.default_pos(fixed_pos.to_pos2()),
            false => window_proto.fixed_pos(fixed_pos.to_pos2()),
        };

        window_proto
            .frame(Frame {
                corner_radius: CornerRadius::same(1),
                fill: Color32::from_gray(40),
                stroke: Stroke::new(1.0, Color32::from_gray(60)),
                inner_margin: Margin::symmetric(6, 12),
                ..Frame::default()
            })
            .show(ctx, |ui| {
                let frame = Frame::NONE;

                frame.show(ui, |ui| {
                    ui.set_min_size(self.popup_size);
                    ui.vertical_centered(|ui| {
                        add_contents(ui);
                    });
                });
            });
    }
}

// impl Default for Dialog {
//     fn default() -> Self {
//         Self::new()
//     }
// }
