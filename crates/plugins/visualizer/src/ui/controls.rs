use blaulicht_plugin_framework as bpf;
use crate::resources::VisualizationConfig;

pub fn render_controls_ui(config: &mut VisualizationConfig) {
    bpf::ui::begin_horizontal();
    
    bpf::ui::label("View Settings");
    bpf::ui::checkbox("Show Labels", 20, config.show_labels);
    bpf::ui::checkbox("Show Grid", 21, config.show_grid);
    bpf::ui::checkbox("Show Light Beams", 22, config.show_light_beams);
    
    bpf::ui::end_horizontal();

    bpf::ui::begin_horizontal();
    bpf::ui::label("Zoom");
    bpf::ui::slider("zoom", 23, 10, 200, (config.scale * 100.0) as u8);
    bpf::ui::end_horizontal();

    bpf::ui::separator();
}

pub fn handle_ui_events(
    config: &mut VisualizationConfig,
    events: &[blaulicht_shared::ControlEventMessage],
) {
    use blaulicht_shared::{ControlEvent, PluginUiEvent};

    for event in events {
        if let ControlEvent::PluginUi(ui_event) = event.body() {
            match ui_event {
                PluginUiEvent::Checkbox { id: 20, checked } => {
                    config.show_labels = checked;
                }
                PluginUiEvent::Checkbox { id: 21, checked } => {
                    config.show_grid = checked;
                }
                PluginUiEvent::Checkbox { id: 22, checked } => {
                    config.show_light_beams = checked;
                }
                PluginUiEvent::Slider { id: 23, value } => {
                    config.scale = value as f32 / 100.0;
                }
                _ => {}
            }
        }
    }
}
