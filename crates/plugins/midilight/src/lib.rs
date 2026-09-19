mod clock;
mod editor;
mod model;

use blaulicht_plugin_framework as bpf;
use blaulicht_shared::{
    AnimationPropertyWrite, AnimationTickInput, AnimationTickOutput, ControlEvent, FixtureProperty,
    TickInput,
};
use bpf::BlaulichtAnimationPlugin;
#[cfg(not(test))]
use bpf::AnimationPluginDescriptor;
use clock::BeatClock;
use editor::Editor;
use model::Pattern;

#[derive(Default)]
struct Midilight {
    pattern: Pattern,
    clock: BeatClock,
    editor: Editor,
}

impl Midilight {
    fn save(&self) {
        if let Ok(raw) = serde_json::to_string(&self.pattern) {
            bpf::save_plugin_state(bpf::PluginStateLocation::Showfile, &raw);
        }
    }
}

impl BlaulichtAnimationPlugin for Midilight {
    fn initialize(&mut self, input: &AnimationTickInput, _common: &TickInput) {
        self.pattern = Pattern::new_for(&input.fixtures);
        if let Some(raw) = bpf::load_plugin_state(bpf::PluginStateLocation::Showfile) {
            let loaded = serde_json::from_str::<Pattern>(&raw).ok().and_then(|mut pattern| {
                pattern.validate_loaded().then_some(pattern)
            });
            if let Some(pattern) = loaded {
                self.pattern = pattern;
            } else {
                bpf::bl_log("Midilight: invalid saved pattern; starting with an empty roll", blaulicht_shared::LogLevel::Warn);
            }
        }
        if self.pattern.update_selection(&input.fixtures) {
            self.editor.mapping_error = true;
            self.save();
        }
    }

    fn run(&mut self, input: &AnimationTickInput, common: &TickInput) -> AnimationTickOutput {
        let mut changed = self.pattern.update_selection(&input.fixtures);
        if changed {
            self.editor.mapping_error = true;
            self.editor.cancel_gesture();
            self.clock.reset();
        }
        let previously_valid = self.pattern.valid_mapping();
        for message in &common.events.events {
            if let ControlEvent::AnimationPluginUi(event, _, _) = message.body() {
                changed |= self.editor.event(&mut self.pattern, event);
            }
        }
        if changed {
            self.save();
        }
        let valid = self.pattern.valid_mapping();
        if previously_valid != valid {
            self.clock.reset();
        }
        if self.clock.update(common.clock, &common.audio_data) && valid {
            bpf::bl_log("Midilight: source has no bar position; estimating bar alignment from the first beat", blaulicht_shared::LogLevel::Warn);
        }
        if !valid {
            self.clock.reset();
        }
        if input.paused {
            return AnimationTickOutput::default();
        }
        let values = if valid && self.clock.running {
            self.pattern.output(&input.fixtures, self.clock.step, self.clock.loops)
        } else {
            vec![0; input.fixtures.len()]
        };
        AnimationTickOutput {
            writes: values.into_iter().enumerate().map(|(i, value)| AnimationPropertyWrite {
                fixture_index: i as u32, property: FixtureProperty::Alpha, value,
            }).collect(),
        }
    }

    fn ui(&mut self, input: &AnimationTickInput, common: &TickInput) {
        self.editor.render(&self.pattern, &self.clock, input.paused, common.audio_data.bpm);
    }
}

#[cfg(not(test))]
#[no_mangle]
extern "C" fn main() {
    bpf::hook_animation_plugin(
        AnimationPluginDescriptor::new("org.blaulicht.midilight", "Midilight"),
        || Box::new(Midilight::default()),
    );
}
