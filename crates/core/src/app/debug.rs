use crate::app::BlaulichtApp;
use egui::{Context, FontId, RichText};

impl BlaulichtApp {
    pub fn debug_dialog(&mut self, ctx: &Context) {
        if !self.system_ui_state.debug_open {
            return;
        }
        egui::Window::new("Debug")
            .title_bar(false)
            .fixed_size(egui::vec2(200.0, 150.0))
            .show(ctx, |ui| {
                ui.set_width(200.0);
                ui.set_height(150.0);
                let dt = ctx.input(|i| i.stable_dt);
                let fps = if dt > 0.0 { 1.0 / dt } else { 0.0 };

                if self.fps_samples.len() + 2 == self.fps_samples.capacity() {
                    self.fps_samples.pop_front();
                }

                self.fps_samples.push_back(fps);

                let fps_avg = self.fps_samples.iter().sum::<f32>() / self.fps_samples.len() as f32;

                ui.label(
                    RichText::new(format!("FPS: {:.0}", fps_avg)).font(FontId::monospace(24.0)),
                );

                ui.separator();
                let perf_fontsize = 12.0;

                ui.label(
                    RichText::new(format!("DMX ENGINE: {:?}", self.tick_speeds.dmx.dmx_engine))
                        .font(FontId::monospace(perf_fontsize)),
                );

                ui.label(
                    RichText::new(format!("DMX WRITE: {:?}", self.tick_speeds.dmx.dmx_write))
                        .font(FontId::monospace(perf_fontsize)),
                );

                ui.separator();

                ui.label(
                    RichText::new(format!("PLUGINS: {:?}", self.tick_speeds.plugins))
                        .font(FontId::monospace(perf_fontsize)),
                );

                ui.label(
                    RichText::new(format!("LOOP TOT: {:?}", self.tick_speeds.loop_total))
                        .font(FontId::monospace(perf_fontsize)),
                );

                ui.label(
                    RichText::new(format!("AUDIO: {:?}", self.tick_speeds.audio_processing))
                        .font(FontId::monospace(perf_fontsize)),
                );
            });
    }
}
