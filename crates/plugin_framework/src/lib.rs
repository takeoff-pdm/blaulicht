use blaulicht_shared::{AnimationTickInput, AnimationTickOutput, PluginTickInput, TickInput};
use std::{cell::RefCell, collections::HashMap};
pub mod artnet;
pub mod blaulicht;
mod error;
pub mod midi;
pub mod serial;
mod state;
pub mod udp;
pub use artnet::*;
pub use blaulicht::*;
pub use blaulicht_shared::{
    ArtNetReceiverInfo, ExternalScreenInfo, PluginStateLocation, UdpReceived,
};
pub use midi::*;
pub use state::*;
pub use udp::*;

#[no_mangle]
pub extern "C" fn __blaulicht_plugin_abi_version() -> u32 {
    blaulicht_shared::PLUGIN_ABI_VERSION
}

#[cfg(test)]
mod abi_tests {
    #[test]
    fn exported_abi_version_matches_shared_protocol() {
        assert_eq!(
            super::__blaulicht_plugin_abi_version(),
            blaulicht_shared::PLUGIN_ABI_VERSION
        );
    }
}

pub struct BufferSource<T, const N: usize> {
    buffer: [T; N],
    current_length: u32,
}

//
// PLUGIN
//

pub trait Plugin {
    fn initialize(&mut self, input: TickInput);
    fn run(&mut self, input: TickInput);
    fn ui(&mut self) {}
}

/// One isolated animation application. The framework creates one object per
/// host instance ID, so implementations never need their own instance map.
pub trait BlaulichtAnimationPlugin {
    fn initialize(&mut self, _input: &AnimationTickInput, _common: &TickInput) {}
    fn run(&mut self, input: &AnimationTickInput, common: &TickInput) -> AnimationTickOutput;
    fn ui(&mut self, _input: &AnimationTickInput, _common: &TickInput) {}
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AnimationPluginDescriptor {
    pub stable_key: String,
    pub display_name: String,
}

impl AnimationPluginDescriptor {
    pub fn new(stable_key: impl Into<String>, display_name: impl Into<String>) -> Self {
        Self {
            stable_key: stable_key.into(),
            display_name: display_name.into(),
        }
    }
}

type AnimationFactory = dyn Fn() -> Box<dyn BlaulichtAnimationPlugin>;

fn framework_panic(message: &str) {
    #[cfg(not(test))]
    blaulicht::report_panic(message);
    #[cfg(test)]
    let _ = message;
}

struct AnimationPluginRegistration {
    factory: Box<AnimationFactory>,
    instances: HashMap<u64, Box<dyn BlaulichtAnimationPlugin>>,
}

thread_local! {
    static PLUGIN: RefCell<Option<Box<dyn Plugin>>> = RefCell::new(None);
    static ANIMATION_PLUGIN: RefCell<Option<AnimationPluginRegistration>> = RefCell::new(None);
}

pub fn hook_plugin(plugin: Box<dyn Plugin>) {
    if ANIMATION_PLUGIN.with(|slot| slot.borrow().is_some()) {
        framework_panic("A module cannot register both normal and animation plugin kinds");
        return;
    }
    PLUGIN.with(|slot| *slot.borrow_mut() = Some(plugin));
    register_plugin_kind(0, "", "");
}

pub fn hook_animation_plugin<F>(descriptor: AnimationPluginDescriptor, factory: F)
where
    F: Fn() -> Box<dyn BlaulichtAnimationPlugin> + 'static,
{
    if PLUGIN.with(|slot| slot.borrow().is_some()) {
        framework_panic("A module cannot register both normal and animation plugin kinds");
        return;
    }
    register_plugin_kind(1, &descriptor.stable_key, &descriptor.display_name);
    ANIMATION_PLUGIN.with(|slot| {
        *slot.borrow_mut() = Some(AnimationPluginRegistration {
            factory: Box::new(factory),
            instances: HashMap::new(),
        });
    });
}

#[cfg(not(test))]
fn register_plugin_kind(kind: i32, key: &str, name: &str) {
    blaulicht::register_plugin_kind(kind, key, name);
}

#[cfg(test)]
fn register_plugin_kind(_kind: i32, _key: &str, _name: &str) {}

const ANIMATION_OUTPUT_BUFFER_LEN: usize = 1024 * 1024;
static mut ANIMATION_OUTPUT_BUFFER: [u8; ANIMATION_OUTPUT_BUFFER_LEN] =
    [0; ANIMATION_OUTPUT_BUFFER_LEN];
static mut ANIMATION_OUTPUT_LENGTH: u32 = 0;

fn store_animation_output(output: AnimationTickOutput) {
    let encoded = output.serialize();
    if encoded.len() > ANIMATION_OUTPUT_BUFFER_LEN {
        framework_panic("Animation output exceeds the framework buffer");
        unsafe { ANIMATION_OUTPUT_LENGTH = 0 };
        return;
    }
    unsafe {
        ANIMATION_OUTPUT_BUFFER[..encoded.len()].copy_from_slice(&encoded);
        ANIMATION_OUTPUT_LENGTH = encoded.len() as u32;
    }
}

#[no_mangle]
pub extern "C" fn __internal_get_animation_output_start_addr() -> *mut u8 {
    &raw mut ANIMATION_OUTPUT_BUFFER as *mut u8
}

#[no_mangle]
pub extern "C" fn __internal_get_animation_output_length_start_addr() -> *mut u8 {
    &raw mut ANIMATION_OUTPUT_LENGTH as *mut u32 as *mut u8
}

#[no_mangle]
pub extern "C" fn internal_drop_animation_instance(instance_id: u64) {
    ANIMATION_PLUGIN.with(|slot| {
        if let Some(registration) = slot.borrow_mut().as_mut() {
            registration.instances.remove(&instance_id);
        }
    });
}

//
// END PLUGIN
//

#[cfg(not(test))]
extern "C" {
    fn main();
}

#[no_mangle]
/// # Safety
/// Only called with the correct inputs from outside.
pub unsafe extern "C" fn internal_tick(tick_input_array: *mut u8, tick_input_length: usize) {
    let tick_array = unsafe { blaulicht::_get_array(tick_input_array, tick_input_length) };

    // Run user code
    let envelope = PluginTickInput::deserialize(tick_array);
    let tick_input = envelope.common;

    match tick_input.initial {
        true => {
            // Set plugin ID.
            unsafe { blaulicht::PLUGIN_ID = tick_input.id };

            // Set panic-hook.
            std::panic::set_hook(Box::new(|info| {
                // blaulicht::bl_log(&msg, LogLevel::Err);
                let msg = info.to_string();
                blaulicht::report_panic(&msg);
            }));

            // Call user-exposed init code.
            #[cfg(not(test))]
            unsafe {
                main();
            }

            if let Some(animation_input) = envelope.animation {
                dispatch_animation(animation_input, tick_input);
            } else {
                PLUGIN.with(|slot| {
                    if let Some(plugin) = slot.borrow_mut().as_mut() {
                        plugin.initialize(tick_input);
                    }
                });
            }
        }
        false => {
            if let Some(animation_input) = envelope.animation {
                dispatch_animation(animation_input, tick_input);
            } else {
                PLUGIN.with(|slot| {
                    if let Some(plugin) = slot.borrow_mut().as_mut() {
                        plugin.run(tick_input);
                    } else if ANIMATION_PLUGIN.with(|animation| animation.borrow().is_none()) {
                        blaulicht::report_panic("Plugin tick received before initialization");
                    }
                });
            }
        }
    };
}

fn dispatch_animation(input: AnimationTickInput, common: TickInput) {
    ANIMATION_PLUGIN.with(|slot| {
        let mut registration_slot = slot.borrow_mut();
        let Some(registration) = registration_slot.as_mut() else {
            framework_panic("Animation tick sent to a normal plugin");
            return;
        };
        let mut created = false;
        let instance = registration
            .instances
            .entry(input.instance_id)
            .or_insert_with(|| {
                created = true;
                (registration.factory)()
            });
        if created {
            instance.initialize(&input, &common);
        }
        let output = instance.run(&input, &common);
        instance.ui(&input, &common);
        store_animation_output(output);
    });
}

#[no_mangle]
pub extern "C" fn internal_render_ui() {
    PLUGIN.with(|slot| {
        if let Some(plugin) = slot.borrow_mut().as_mut() {
            plugin.ui();
        }
    });
}

#[cfg(test)]
mod animation_tests {
    use super::*;
    use blaulicht_shared::{AnimationPropertyWrite, AnimationSpeedModifier, FixtureProperty};

    #[derive(Default)]
    struct Counter(u16);

    impl BlaulichtAnimationPlugin for Counter {
        fn run(&mut self, _input: &AnimationTickInput, _common: &TickInput) -> AnimationTickOutput {
            self.0 += 1;
            AnimationTickOutput {
                writes: vec![AnimationPropertyWrite {
                    fixture_index: 0,
                    property: FixtureProperty::Alpha,
                    value: self.0,
                }],
            }
        }
    }

    fn input(instance_id: u64) -> AnimationTickInput {
        AnimationTickInput {
            instance_id,
            scene_id: Some(0),
            animation_id: 0,
            fixtures: vec![(0, 0)],
            paused: false,
            editor: false,
            delta_ms: 12,
            speed_factor: AnimationSpeedModifier::_1,
            scene_speed_factor: AnimationSpeedModifier::_1,
        }
    }

    fn output() -> AnimationTickOutput {
        let length = unsafe { ANIMATION_OUTPUT_LENGTH as usize };
        AnimationTickOutput::deserialize(unsafe { &ANIMATION_OUTPUT_BUFFER[..length] })
    }

    #[test]
    fn factory_state_is_isolated_per_instance() {
        ANIMATION_PLUGIN.with(|slot| {
            *slot.borrow_mut() = Some(AnimationPluginRegistration {
                factory: Box::new(|| Box::new(Counter::default())),
                instances: HashMap::new(),
            });
        });
        dispatch_animation(input(1), TickInput::default());
        dispatch_animation(input(1), TickInput::default());
        assert_eq!(output().writes[0].value, 2);
        dispatch_animation(input(2), TickInput::default());
        assert_eq!(output().writes[0].value, 1);
        internal_drop_animation_instance(1);
        dispatch_animation(input(1), TickInput::default());
        assert_eq!(output().writes[0].value, 1);
        ANIMATION_PLUGIN.with(|slot| *slot.borrow_mut() = None);
    }
}
