use blaulicht_shared::{ControlEventCollection, TickInput};

mod blaulicht;
mod error;
mod midi;
mod user;

#[no_mangle]
pub extern "C" fn internal_tick(
    // Tick input.
    tick_input_array: *mut u8,
    tick_input_length: usize,
) {
    let tick_array = unsafe { blaulicht::_get_array(tick_input_array, tick_input_length) };

    // Run user code
    let tick_input = TickInput::deserialize(tick_array);
    // let midi_inputs = midi::decode_midi(midi_array);

    match tick_input.initial {
        true => {
            std::panic::set_hook(Box::new(|info| {
                blaulicht::bl_log(&format!("***PANIC***: {}", info.to_string()));
            }));

            user::initialize(tick_input)
        }
        false => {
            user::run(tick_input);
        }
    };
}
