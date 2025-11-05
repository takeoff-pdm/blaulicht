use crate::error::IoError;
use crate::BufferSource;
use crate::{blaulicht, error::Result};
use bincode::{config, Decode, Encode};
use blaulicht_shared::{SerialCollector, SerialReceived};

const SERIAL_BUFFER_LEN: usize = 1000 * 1024; // Very big.
type SerialBufferT = u8;

static mut GLOBAL_SERIAL_SOURCE: BufferSource<SerialBufferT, SERIAL_BUFFER_LEN> = BufferSource {
    buffer: [0; SERIAL_BUFFER_LEN],
    current_length: 0,
};

// Function called by the engine to get the location of the serial buffer.
// The engine will write its serial data into this buffer.
#[no_mangle]
pub extern "C" fn __internal_get_global_serial_buffer_start_addr() -> *mut SerialBufferT {
    unsafe { &raw mut GLOBAL_SERIAL_SOURCE.buffer as *mut SerialBufferT }
}

// Same as the above, just for the length of the buffer.
#[no_mangle]
pub extern "C" fn __internal_get_global_serial_buffer_length_start_addr() -> *mut SerialBufferT {
    unsafe { &raw mut GLOBAL_SERIAL_SOURCE.current_length as *mut usize as *mut SerialBufferT }
}

//
// Begin serial impl.
//

/// Decodes current buffer state.
fn get_serial() -> Vec<SerialReceived> {
    // Sanity check for memory usage.
    let curr_len = unsafe { GLOBAL_SERIAL_SOURCE.current_length };
    if curr_len as usize >= SERIAL_BUFFER_LEN {
        panic!(
            "OOM: The serial buffer exceeded the predefined size: {curr_len} vs. {SERIAL_BUFFER_LEN}",
        )
    }

    let buf = unsafe { GLOBAL_SERIAL_SOURCE.buffer };
    let collector = SerialCollector::deserialize(&buf);
    collector.events()
}

// --------------------------------------------------------

#[derive(Debug, Clone, Copy)]
pub struct SerialConnection {
    device_id: u8,
}

pub struct SerialConnectionMeta {
    pub device_id: u8,
}

const SERIAL_DEVICE_NOT_FOUND: u8 = u8::MAX;

impl SerialConnection {
    pub unsafe fn dummy() -> Self {
        Self { device_id: 255 }
    }

    pub fn open(port_path: &str, baud_rate: u32) -> Result<Self> {
        // Request connection to a serial device.
        let device_id_or_error_code = blaulicht::bl_open_serial_device_safe(port_path, baud_rate);

        if device_id_or_error_code == SERIAL_DEVICE_NOT_FOUND {
            return Err(IoError::SerialDeviceNotFound(port_path.to_string()).into());
        }

        Ok(Self {
            device_id: device_id_or_error_code,
        })
    }

    pub fn get_meta(&self) -> SerialConnectionMeta {
        SerialConnectionMeta {
            device_id: self.device_id,
        }
    }

    pub fn send(&self, status: u8, kind: u8, value: u8) {
        blaulicht::bl_transmit_midi_safe(self.device_id, status, kind, value);
    }

    pub fn poll(&self) -> Vec<SerialReceived> {
        let all_events = get_serial();
        all_events
            .into_iter()
            .filter(|e| e.device == self.device_id)
            .collect()
    }
}

// --------------------------------------------------------
