use crate::{
    app::{
        components::{self, ButtonSize, Dialog},
        pages::health::{
            render_dmx_or_artnet_health_box, DMX_ICON,
            DMX_OR_ARTNET_HEALTH_LABEL_COLOR,
        },
        BlaulichtApp,
    },
    state::DmxHealthState,
};
use egui::{
    Color32, Context, FontFamily, Label, RichText,
};

impl BlaulichtApp {
    pub fn render_dmx_dialog(&mut self, ctx: &Context, universe_number: usize) {
        if !self.system_ui_state.dmx_dialogs_open[universe_number] {
            return;
        }

        let health_data = self.data.state.health_data.read().unwrap();
        let health_state = &health_data.dmx_universes_healthy[universe_number];

        Dialog::new("DMX".to_string(), egui::vec2(500.0, 200.0))
            .with_backdrop()
            .show(ctx, |ui| {
                let label = format!("(DMX {})", universe_number);
                ui.heading(RichText::new(&label).strong());

                ui.add_space(12.0);

                ui.horizontal_top(|ui| {
                    render_dmx_or_artnet_health_box(
                        ui,
                        Label::new(
                            RichText::new(&health_state.port)
                                .color(DMX_OR_ARTNET_HEALTH_LABEL_COLOR)
                                .size(12.0),
                        ),
                        DMX_ICON,
                        28.0,
                        health_state.is_healthy(),
                        false,
                        egui::vec2(100.0, 65.0),
                    );

                    let label = match &health_state.state {
                        DmxHealthState::Error(err) => Label::new(
                            RichText::new(err)
                                .color(Color32::RED)
                                .family(FontFamily::Monospace),
                        )
                        .wrap(),
                        DmxHealthState::Healthy => Label::new("- No Error -"),
                    };

                    ui.add_sized([350.0, 20.0], label);
                });

                if components::button(ui, true, "Close", ButtonSize::Medium) {
                    self.system_ui_state.dmx_dialogs_open[universe_number] = false;
                }
            });
    }
}
