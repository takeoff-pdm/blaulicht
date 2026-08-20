use blaulicht_plugin_framework as bpf;
use blaulicht_shared::{
    AnimationPropertyWrite, AnimationTickInput, AnimationTickOutput, FixtureProperty, TickInput,
};
use bpf::{AnimationPluginDescriptor, BlaulichtAnimationPlugin};

#[derive(Default)]
struct SampleAnimation {
    value: u16,
}

impl BlaulichtAnimationPlugin for SampleAnimation {
    fn run(&mut self, input: &AnimationTickInput, common: &TickInput) -> AnimationTickOutput {
        for message in &common.events.events {
            if let blaulicht_shared::ControlEvent::AnimationPluginUi(
                blaulicht_shared::PluginUiEvent::Slider { id: 1, value },
                _,
                _,
            ) = message.body()
            {
                self.value = u16::from(value);
                bpf::save_plugin_state(bpf::PluginStateLocation::Showfile, &self.value.to_string());
            }
        }

        if input.paused {
            return AnimationTickOutput::default();
        }
        AnimationTickOutput {
            writes: input
                .fixtures
                .iter()
                .enumerate()
                .map(|(fixture_index, _)| AnimationPropertyWrite {
                    fixture_index: fixture_index as u32,
                    property: FixtureProperty::Alpha,
                    value: self.value,
                })
                .collect(),
        }
    }

    fn initialize(&mut self, _input: &AnimationTickInput, _common: &TickInput) {
        self.value = bpf::load_plugin_state(bpf::PluginStateLocation::Showfile)
            .and_then(|value| value.parse().ok())
            .unwrap_or(255);
    }

    fn ui(&mut self, _input: &AnimationTickInput, _common: &TickInput) {
        bpf::ui::begin();
        bpf::ui::label("Per-instance brightness");
        bpf::ui::slider("Value", 1, 0, 255, self.value.min(255) as u8);
    }
}

#[no_mangle]
extern "C" fn main() {
    bpf::hook_animation_plugin(
        AnimationPluginDescriptor::new("org.blaulicht.sample-animation", "Sample animation"),
        || Box::new(SampleAnimation::default()),
    );
}
