use blaulicht_shared::{LogLevel, TickInput};
use std::mem::MaybeUninit;
pub mod blaulicht;
mod error;
pub mod midi;
pub mod serial;
mod state;
pub use blaulicht::*;
pub use midi::*;
pub use state::*;

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

static mut PLUGIN: MaybeUninit<Box<dyn Plugin>> = MaybeUninit::uninit();

pub fn hook_plugin(plugin: Box<dyn Plugin>) {
    unsafe {
        PLUGIN.write(plugin);
    }
}

//
// END PLUGIN
//

extern "C" {
    fn main();
}

#[no_mangle]
/// # Safety
/// Only called with the correct inputs from outside.
pub unsafe extern "C" fn internal_tick(tick_input_array: *mut u8, tick_input_length: usize) {
    let tick_array = unsafe { blaulicht::_get_array(tick_input_array, tick_input_length) };

    // Run user code
    let tick_input = TickInput::deserialize(tick_array);

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
            unsafe { main() };

            let plugin = unsafe {
                #[allow(static_mut_refs)]
                PLUGIN.assume_init_mut()
            };

            plugin.initialize(tick_input)
        }
        false => {
            let plugin = unsafe {
                #[allow(static_mut_refs)]
                PLUGIN.assume_init_mut()
            };
            plugin.run(tick_input);
        }
    };
}

#[no_mangle]
pub extern "C" fn internal_render_ui() {
    let plugin = unsafe {
        #[allow(static_mut_refs)]
        PLUGIN.assume_init_mut()
    };

    plugin.ui();
}
