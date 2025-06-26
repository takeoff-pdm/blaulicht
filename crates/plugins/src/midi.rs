use std::u8;

use crate::{
    blaulicht::{self},
    error::{MidiError, Result},
};

struct MidiSource {
    buffer: [u32; 256],
    current_length: u32,
}

static mut GLOBAL_MIDI_SOURCE: MidiSource = MidiSource {
    buffer: [0; 256],
    current_length: 0,
};

// Function called by the engine to get the location of the MIDI buffer.
// The engine will write MIDI events into this buffer.
#[no_mangle]
pub extern "C" fn __internal_get_global_midi_buffer_start_addr() -> *mut u32 {
    unsafe { &raw mut GLOBAL_MIDI_SOURCE.buffer as *mut [u32; 256] as *mut u32 }
}

// Same as the above, just for the length of the buffer.
#[no_mangle]
pub extern "C" fn __internal_get_global_midi_buffer_length_start_addr() -> *mut u32 {
    unsafe { &raw mut GLOBAL_MIDI_SOURCE.current_length as *mut usize as *mut u32 }
}

// --------------------------------------------------------

#[derive(Debug, Clone, Copy)]
pub struct MidiConnection {
    device_id: u8,
}

const MIDI_DEVICE_NOT_FOUND: u8 = u8::MAX;

impl MidiConnection {
    pub fn open(device_name: &str) -> Result<Self> {
        // Request connection to MIDI device.
        let device_id_or_error_code = blaulicht::bl_open_midi_device_safe(device_name);

        if device_id_or_error_code == MIDI_DEVICE_NOT_FOUND {
            return Err(MidiError::DeviceNotFound(device_name.to_string()).into());
        }

        Ok(Self {
            device_id: device_id_or_error_code,
        })
    }

    pub fn send(&self, status: u8, kind: u8, value: u8) {
        blaulicht::bl_transmit_midi_safe(self.device_id, status, kind, value);
    }

    pub fn poll(&self) -> Vec<MidiEvent> {
        // Get matching MIDI events from the global buffer.
        unsafe {
            decode_midi_selective(
                &GLOBAL_MIDI_SOURCE.buffer,
                GLOBAL_MIDI_SOURCE.current_length as usize,
                self.device_id,
            )
        }
    }
}

// --------------------------------------------------------

impl MidiEvent {
    pub fn tup(&self) -> (u8, u8) {
        (self.status, self.kind)
    }
}

#[derive(Debug, Clone, Copy)]
pub struct MidiEvent {
    pub device: u8,
    pub status: u8,
    pub kind: u8,
    pub value: u8,
}

#[macro_export]
macro_rules! cmidi {
    ($status: expr, $kind: expr) => {
        MidiEvent {
            status: $status,
            kind: $kind,
            value: 0,
        }
    };
    ($status: expr, $kind: expr, $value: expr) => {
        MidiEvent {
            status: $status,
            kind: $kind,
            value: $value,
        }
    };
}

pub use cmidi;

// TODO: maybe also put this into shared.
fn decode_midi_selective(
    midi: &[u32],
    current_midi_buffer_len: usize,
    device_id: u8,
) -> Vec<MidiEvent> {
    let res: Vec<MidiEvent> = midi
        .iter()
        .take(current_midi_buffer_len)
        .filter_map(|word| {
            let device = ((word & 0xFF000000) >> 24) as u8;

            if device != device_id {
                return None;
            }

            let data1 = (word & 0x000000FF) as u8;
            let data0 = ((word & 0x0000FF00) >> 8) as u8;
            let status = ((word & 0x00FF0000) >> 16) as u8;

            Some(MidiEvent {
                device: device.into(),
                status,
                kind: data0,
                value: data1,
            })
        })
        .collect();

    res
}
