use crate::app::BlaulichtApp;
use egui::{Color32, FontId, Painter, Rect, Sense, Ui, Vec2};

impl BlaulichtApp {
    pub fn draw_progress_bar(ui: &mut Ui, progress: f32, height: f32, text: &str) {
        let (rect, _response) =
            ui.allocate_exact_size(Vec2::new(ui.available_width(), height), Sense::hover());

        let painter: &Painter = ui.painter();

        painter.rect_filled(rect, 0.0, Color32::from_rgb(30, 30, 30));

        let fill_rect = Rect::from_min_max(
            rect.min,
            egui::pos2(rect.min.x + rect.width() * progress, rect.max.y),
        );
        painter.rect_filled(fill_rect, 0.0, Color32::from_rgb(60, 120, 200));

        painter.text(
            rect.center(),
            egui::Align2::CENTER_CENTER,
            text,
            FontId::proportional(12.0),
            Color32::from_gray(200),
        );
    }
}
