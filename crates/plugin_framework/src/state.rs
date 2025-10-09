use crate::BufferSource;
use blaulicht_shared::EngineState;

const STATE_BUFFER_LEN: usize = 1024 * 100; // Big Ass!
type StateBufferT = u8;

static mut GLOBAL_STATE_SOURCE: BufferSource<StateBufferT, STATE_BUFFER_LEN> = BufferSource {
    buffer: [0; STATE_BUFFER_LEN],
    current_length: 0,
};

// Function called by the engine to get the location of the state buffer.
// The engine will write its state into this buffer.
#[no_mangle]
pub extern "C" fn __internal_get_global_state_buffer_start_addr() -> *mut StateBufferT {
    unsafe { &raw mut GLOBAL_STATE_SOURCE.buffer as *mut StateBufferT }
}

// Same as the above, just for the length of the buffer.
#[no_mangle]
pub extern "C" fn __internal_get_global_state_buffer_length_start_addr() -> *mut StateBufferT {
    unsafe { &raw mut GLOBAL_STATE_SOURCE.current_length as *mut usize as *mut StateBufferT }
}

pub fn get_dmx() -> EngineState {
    // Sanity check for memory usage.
    let curr_len = unsafe { GLOBAL_STATE_SOURCE.current_length };
    if curr_len as usize >= STATE_BUFFER_LEN {
        panic!(
            "OOM: The state buffer exceeded the predefined size: {curr_len} vs. {STATE_BUFFER_LEN}",
        )
    }

    let buf = unsafe { GLOBAL_STATE_SOURCE.buffer };
    EngineState::deserialize(&buf)
}
