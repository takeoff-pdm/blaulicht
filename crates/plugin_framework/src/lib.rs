use blaulicht_shared::{LogLevel, TickInput};
use std::cell::RefCell;
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

thread_local! {
    static PLUGIN: RefCell<Option<Box<dyn Plugin>>> = RefCell::new(None);
}

pub fn hook_plugin(plugin: Box<dyn Plugin>) {
    PLUGIN.with(|slot| *slot.borrow_mut() = Some(plugin));
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

            PLUGIN.with(|slot| {
                let mut plugin_slot = slot.borrow_mut();
                let Some(plugin) = plugin_slot.as_mut() else {
                    blaulicht::report_panic(
                        "Plugin did not register itself during initialization",
                    );
                    return;
                };
                plugin.initialize(tick_input);
            });
        }
        false => {
            PLUGIN.with(|slot| {
                if let Some(plugin) = slot.borrow_mut().as_mut() {
                    plugin.run(tick_input);
                } else {
                    blaulicht::report_panic("Plugin tick received before initialization");
                }
            });
        }
    };
}

#[no_mangle]
pub extern "C" fn internal_render_ui() {
    PLUGIN.with(|slot| {
        if let Some(plugin) = slot.borrow_mut().as_mut() {
            plugin.ui();
        }
    });
}
