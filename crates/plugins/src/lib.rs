use blaulicht_shared::TickInput;

mod blaulicht;
mod user;
mod error;
mod midi;

#[no_mangle]
pub extern "C" fn internal_tick(
    // Tick input.
    tick_input_array: *const u32,
    tick_input_length: usize,
    // Data array.
    data_array: *mut u8,
) {
    let tick_array = unsafe { blaulicht::_get_array_u32(tick_input_array, tick_input_length) };
    // let dmx_array = unsafe { blaulicht::_get_array(dmx_array, dmx_array_length) };
    // let midi_array = unsafe { blaulicht::_get_array_u32(midi_array, midi_len) };

    // Run user code
    let tick_input = TickInput::deserialize(tick_array);
    // let midi_inputs = midi::decode_midi(midi_array);

    match tick_input.initial {
        true => {
            std::panic::set_hook(Box::new(|info| {
                blaulicht::bl_log(&format!("***PANIC***: {}", info.to_string()));
            }));

            user::initialize(tick_input, data_array)
        }
        false => user::run(tick_input, data_array),
    };
}
