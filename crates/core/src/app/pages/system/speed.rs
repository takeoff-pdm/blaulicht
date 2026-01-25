use crate::{app::BlaulichtApp, mainloop::DMX_TICK_TIME};
use egui::{Color32, Ui};

impl BlaulichtApp {
    pub fn render_system_speed_graph(&mut self, ui: &mut Ui) {
        ui.horizontal(|ui| {
            let graph_width = 100.0;
            let graph_height = 36.0;
            // Loop Speed Graph
            let (loop_resp, loop_painter) =
                ui.allocate_painter(egui::vec2(graph_width, graph_height), egui::Sense::hover());
            // Draw value and label above the bar, left-aligned
            let loop_val_str = match self.loop_speed {
                v if v > 1000 => format!("{:.0} ms", self.loop_speed / 1000),
                _ => format!("{:.0} us", self.loop_speed),
            };
            let label_y = loop_resp.rect.top() + 2.0;
            let label_x = loop_resp.rect.left() + 4.0;
            loop_painter.text(
                egui::pos2(label_x, label_y),
                egui::Align2::LEFT_TOP,
                &loop_val_str,
                egui::FontId::monospace(10.0),
                Color32::WHITE,
            );
            loop_painter.text(
                egui::pos2(label_x + 60.0, label_y), // 60px offset for value
                egui::Align2::LEFT_TOP,
                "Loop",
                egui::FontId::proportional(10.0),
                Color32::WHITE,
            );
            // Draw loop speed as a bar (simple visualization)
            let loop_val = self.loop_speed as f32;
            let loop_max = DMX_TICK_TIME.as_micros() as f32;
            let loop_bar_width = (loop_val / loop_max).min(1.0) * (graph_width - 8.0);
            let loop_bar_rect = egui::Rect::from_min_max(
                loop_resp.rect.left_top() + egui::vec2(4.0, 16.0),
                loop_resp.rect.left_top() + egui::vec2(4.0 + loop_bar_width, graph_height - 8.0),
            );
            let color = match self.loop_speed {
                v if v as f32 > loop_max => Color32::RED,
                _ => Color32::from_rgb(0, 200, 255),
            };
            loop_painter.rect_filled(loop_bar_rect, 2.0, color);

            // Tick Speed Graph
            let (tick_resp, tick_painter) =
                ui.allocate_painter(egui::vec2(graph_width, graph_height), egui::Sense::hover());
            let tick_val_str = format!("{:.0} μs", self.tick_speed);
            let tick_label_y = tick_resp.rect.top() + 2.0;
            let tick_label_x = tick_resp.rect.left() + 4.0;
            tick_painter.text(
                egui::pos2(tick_label_x, tick_label_y),
                egui::Align2::LEFT_TOP,
                &tick_val_str,
                egui::FontId::monospace(10.0),
                Color32::WHITE,
            );
            tick_painter.text(
                egui::pos2(tick_label_x + 60.0, tick_label_y),
                egui::Align2::LEFT_TOP,
                "Tick",
                egui::FontId::proportional(10.0),
                Color32::WHITE,
            );
            let tick_val = self.tick_speed as f32;
            let tick_max = 1000.0; // 1 MS
            let tick_bar_width = (tick_val / tick_max).min(1.0) * (graph_width - 8.0);
            let tick_bar_rect = egui::Rect::from_min_max(
                tick_resp.rect.left_top() + egui::vec2(4.0, 16.0),
                tick_resp.rect.left_top() + egui::vec2(4.0 + tick_bar_width, graph_height - 8.0),
            );
            tick_painter.rect_filled(tick_bar_rect, 2.0, Color32::from_rgb(255, 200, 0));
        });
    }
}
