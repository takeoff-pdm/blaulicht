mod blaulicht;
mod user;

fn decode_midi(midi: &[u32]) -> Vec<blaulicht::MidiEvent> {
    // if midi.len() > 0 {
    //     println!("MIDI len: {}", midi.len());
    // }

    let res: Vec<blaulicht::MidiEvent> = midi
        .iter()
        .map(|word| {
            let data1 = (word & 0x000000FF) as u8;
            let data0 = ((word & 0x0000FF00) >> 8) as u8;
            let status = ((word & 0x00FF0000) >> 16) as u8;

            blaulicht::MidiEvent {
                status,
                kind: data0,
                value: data1,
            }
        })
        .collect();

    res
}

fn tickinput_from_array(arr: &[u32]) -> blaulicht::TickInput {
    const ARRAY_LEN: usize = 9;
    if arr.len() != ARRAY_LEN {
        panic!(
            "tick array len in 'tickinput_from_array' is not expected length: {}",
            arr.len()
        );
    }

    blaulicht::TickInput {
        time: arr[0],
        volume: arr[1] as u8,
        beat_volume: arr[2] as u8,
        bass: arr[3] as u8,
        bass_avg_short: arr[4] as u8,
        bass_avg: arr[5] as u8,
        bpm: arr[6] as u8,
        time_between_beats_millis: arr[7] as u16,
        initial: arr[8] != 0,
    }
}

#[no_mangle]
pub extern "C" fn internal_tick(
    // Tick input.
    tick_input_array: *const u32,
    tick_input_length: usize,
    // DMX array.
    dmx_array: *mut u8,
    dmx_array_length: usize,
    // Data array.
    data_array: *mut u8,
    _data_length: usize,
    // Midi array.
    midi_array: *const u32,
    midi_len: usize,
) {
    let tick_array = unsafe { blaulicht::_get_array_u32(tick_input_array, tick_input_length) };
    let dmx_array = unsafe { blaulicht::_get_array(dmx_array, dmx_array_length) };
    let midi_array = unsafe { blaulicht::_get_array_u32(midi_array, midi_len) };

    // Run user code
    let tick_input = tickinput_from_array(tick_array);
    let midi_inputs = decode_midi(midi_array);

    match tick_input.initial {
        true => {
            std::panic::set_hook(Box::new(|info| {
                blaulicht::bl_log(&format!("***PANIC***: {}", info.to_string()));
            }));

            user::initialize(tick_input, dmx_array, data_array)
        }
        false => user::run(tick_input, dmx_array, data_array, &midi_inputs),
    };
}
