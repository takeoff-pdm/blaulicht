// Wasm imports
#[link(wasm_import_module = "blaulicht")]
extern "C" {
    fn log(plugin_id: u8, ptr: *const u8, len: usize, log_level: i32);
    fn sys(plugin_id: u8, ptr: *const u8, len: usize);
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

    // Egui UI host imports
    fn ui_begin(plugin_id: u8);
    fn ui_label(plugin_id: u8, ptr: *const u8, len: usize);
    fn ui_separator(plugin_id: u8);
    fn ui_button(plugin_id: u8, ptr: *const u8, len: usize, id: u8);
    fn ui_checkbox(plugin_id: u8, ptr: *const u8, len: usize, id: u8, checked: i32);
    fn ui_slider(
        plugin_id: u8,
        ptr: *const u8,
        len: usize,
        id: u8,
        min: i32,
        max: i32,
        value: i32,
    );
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

pub fn bl_log(msg: &str, level: LogLevel) {
    unsafe { log(PLUGIN_ID, msg.as_ptr(), msg.len(), level.into()) }
}

pub fn system(cmd: &str) {
    unsafe { sys(PLUGIN_ID, cmd.as_ptr(), cmd.len()) }
}

pub fn send_event(event: ControlEvent) {
    let serialized = event.serialize();
    unsafe { bl_send_event(serialized.as_ptr(), serialized.len()) };
}

pub fn send_udp(addr: &str, body: &[u8]) {
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

// ---- UI (egui) helpers ----

pub mod ui {
    use super::{ui_begin as host_ui_begin, ui_button as host_ui_button, ui_label as host_ui_label, ui_separator as host_ui_separator, PLUGIN_ID};

    pub fn begin() {
        unsafe { host_ui_begin(unsafe { PLUGIN_ID }) };
    }

    pub fn label(text: &str) {
        unsafe { host_ui_label(unsafe { PLUGIN_ID }, text.as_ptr(), text.len()) };
    }

    pub fn separator() {
        unsafe { host_ui_separator(unsafe { PLUGIN_ID }) };
    }

    /// Enqueue a button with a stable `id` byte. When clicked on host, a ControlEvent::MiscEvent will be sent back with descriptor=id, value=1.
    pub fn button(label: &str, id: u8) {
        unsafe { host_ui_button(unsafe { PLUGIN_ID }, label.as_ptr(), label.len(), id) };
    }

    pub fn checkbox(label: &str, id: u8, checked: bool) {
        unsafe {
            host_ui_checkbox(
                unsafe { PLUGIN_ID },
                label.as_ptr(),
                label.len(),
                id,
                if checked { 1 } else { 0 },
            )
        };
    }

    pub fn slider(label: &str, id: u8, min: u8, max: u8, value: u8) {
        unsafe {
            host_ui_slider(
                unsafe { PLUGIN_ID },
                label.as_ptr(),
                label.len(),
                id,
                min as i32,
                max as i32,
                value as i32,
            )
        };
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
                use blaulicht_shared::LogLevel;
                $crate::blaulicht::bl_log!("", LogLevel::Info);
            };
            ($($arg:tt)*) => {{
                use blaulicht_shared::LogLevel;
                $crate::blaulicht::bl_log(&format!($($arg)*), LogLevel::Info);
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

use blaulicht_shared::{ControlEvent, LogLevel};
pub use elapsed;

#[macro_export]
macro_rules! nelapsed {
    ($input: expr, $time: expr) => {
        $input.time as i32 - $time as i32
    };
}

pub use nelapsed;
