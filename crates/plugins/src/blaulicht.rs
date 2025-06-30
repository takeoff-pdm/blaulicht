// Wasm imports
#[link(wasm_import_module = "blaulicht")]
extern "C" {
    fn log(plugin_id: u8, ptr: *const u8, len: usize);
    fn udp(
        target_addr_ptr: *const u8,
        target_addr_len: usize,
        body_ptr: *const u8,
        body_len: usize,
    );

    fn bl_open_midi_device(device_name_ptr: *const u8, device_name_len: usize) -> u8;
    fn bl_transmit_midi(device_id: u8, status: u8, data0: u8, data1: u8);
    fn bl_report_panic();

    fn bl_send_event(serialized_buf: *const u8, buf_len: usize);

    fn controls_log(x: u8, y: u8, ptr: *const u8, len: usize);
    fn controls_set(x: u8, y: u8, value: bool);
    fn controls_config(x: u8, y: u8);
}

pub fn bl_open_midi_device_safe(device_name: &str) -> u8 {
    unsafe { bl_open_midi_device(device_name.as_ptr(), device_name.len()) }
}

pub fn report_panic() {
    unsafe {
        bl_report_panic();
    }
}

pub fn bl_transmit_midi_safe(device: u8, status: u8, data0: u8, data1: u8) {
    unsafe { bl_transmit_midi(device, status, data0, data1) }
}

/// Log a string to the BL output

pub static mut PLUGIN_ID: u8 = 0;

pub fn bl_log(msg: &str) {
    unsafe { log(PLUGIN_ID, msg.as_ptr(), msg.len()) }
}

pub fn bl_send(event: ControlEvent) {
    let serialized = event.serialize();
    unsafe { bl_send_event(serialized.as_ptr(), serialized.len()) };
}

pub fn bl_udp(addr: &str, body: &[u8]) {
    unsafe { udp(addr.as_ptr(), addr.len(), body.as_ptr(), body.len()) }
}

pub fn bl_controls_log(x: u8, y: u8, msg: &str) {
    unsafe { controls_log(x, y, msg.as_ptr(), msg.len()) }
}

pub fn bl_controls_set(x: u8, y: u8, value: bool) {
    unsafe { controls_set(x, y, value) }
}

pub fn bl_controls_config(x: u8, y: u8) {
    println!("[MATRIX] Configuring controls to ({}, {})...", x, y);
    unsafe {
        controls_config(x, y);
    }
}

// const PAGE_SIZE: usize = 65536;

#[doc(hidden)]
pub unsafe fn _get_array(array_pointer: *mut u8, array_length: usize) -> &'static mut [u8] {
    // Safety: This is unsafe because we're dealing with raw pointers.
    let slice = unsafe {
        assert!(!array_pointer.is_null(), "Pointer is null");
        std::slice::from_raw_parts_mut(array_pointer, array_length)
    };

    slice
}

#[doc(hidden)]
pub unsafe fn _get_array_u32(array_pointer: *const u32, array_length: usize) -> &'static [u32] {
    // Safety: This is unsafe because we're dealing with raw pointers.
    let slice = unsafe {
        assert!(!array_pointer.is_null(), "Pointer is null");
        std::slice::from_raw_parts(array_pointer, array_length)
    };

    slice
}

pub mod prelude {
    #[macro_export]
    macro_rules! println {
            () => {
                $crate::blaulicht::bl_log!("");
            };
            ($($arg:tt)*) => {{
                $crate::blaulicht::bl_log(&format!($($arg)*));
            }};
        }
    pub use println;

    #[macro_export]
    macro_rules! printc {
            () => {
                $crate::blaulicht::bl_controls_log!("");
            };
            ($x: expr, $y:expr, $($arg:tt)*) => {{
                $crate::blaulicht::bl_controls_log($x, $y, &format!($($arg)*));
            }};
        }
    pub use printc;

    #[macro_export]
    macro_rules! print {
        ($($arg:tt)*) => {
            panic!("'print!' not supported in Blaulicht!");
        };
    }

    #[macro_export]
    macro_rules! dbg {
        ($($arg:tt)*) => {
            panic!("'dbg!' not supported in Blaulicht!");
        };
    }

    #[macro_export]
    macro_rules! smidi {
        ($tuple: expr, $value: expr) => {
            blaulicht::bl_midi_safe(0, $tuple.0, $tuple.1, $value);
            blaulicht::bl_midi_safe(1, $tuple.0, $tuple.1, $value);
        };
    }
}

// pub fn fmod(a: f64, b: f64) -> f64 {
//     a - b * (a / b).trunc()
// }

#[macro_export]
macro_rules! elapsed {
    ($input: expr, $time: expr) => {
        $input.time - $time
    };
}

use std::fmt::Display;

use blaulicht_shared::ControlEvent;
pub use elapsed;

#[macro_export]
macro_rules! nelapsed {
    ($input: expr, $time: expr) => {
        $input.time as i32 - $time as i32
    };
}

pub use nelapsed;
