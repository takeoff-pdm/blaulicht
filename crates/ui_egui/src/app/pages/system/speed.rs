use crate::app::BlaulichtApp;
use egui::Ui;

impl BlaulichtApp {
    pub fn render_system_speed_graph(&mut self, ui: &mut Ui) {
        let graph_width = 220.0;
        let graph_height = 90.0;
        ui.horizontal(|ui| {
            // Loop speed over time.
            let (loop_resp, loop_painter) =
                ui.allocate_painter(egui::vec2(graph_width, graph_height), egui::Sense::hover());
            self.loop_speed_graph.draw(loop_painter, loop_resp.rect);

            // Plugin speed over time.
            let (plugin_resp, plugin_painter) =
                ui.allocate_painter(egui::vec2(graph_width, graph_height), egui::Sense::hover());
            self.plugin_speed_graph
                .draw(plugin_painter, plugin_resp.rect);
        });
    }
}
