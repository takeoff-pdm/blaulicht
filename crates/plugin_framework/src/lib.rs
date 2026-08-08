use blaulicht_shared::{LogLevel, TickInput};
pub mod artnet;
pub mod blaulicht;
mod error;
pub mod midi;
pub mod serial;
pub mod udp;
mod state;
pub use artnet::*;
pub use blaulicht::*;
pub use blaulicht_shared::{
    ArtNetReceiverInfo, ExternalScreenInfo, PluginStateLocation, UdpReceived,
};
pub use midi::*;
pub use udp::*;
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

static mut PLUGIN: Option<Box<dyn Plugin>> = None;

pub fn hook_plugin(plugin: Box<dyn Plugin>) {
    unsafe {
        PLUGIN = Some(plugin);
    }
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
            #[cfg(not(test))]
            unsafe {
                main();
            }

            let Some(plugin) = (unsafe { PLUGIN.as_mut() }) else {
                blaulicht::report_panic("Plugin did not register itself during initialization");
                return;
            };

            plugin.initialize(tick_input)
        }
        false => {
            if let Some(plugin) = (unsafe { PLUGIN.as_mut() }) {
                plugin.run(tick_input);
            } else {
                blaulicht::report_panic("Plugin tick received before initialization");
            }
        }
    };
}

#[no_mangle]
pub extern "C" fn internal_render_ui() {
    if let Some(plugin) = (unsafe { PLUGIN.as_mut() }) {
        plugin.ui();
    }
}
