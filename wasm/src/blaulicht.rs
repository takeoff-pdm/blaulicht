// Wasm imports
#[link(wasm_import_module = "blaulicht")]
extern "C" {
    fn log(ptr: *const u8, len: usize);
    fn udp(
        target_addr_ptr: *const u8,
        target_addr_len: usize,
        body_ptr: *const u8,
        body_len: usize,
    );
    fn bl_midi(status: u8, data0: u8, data1: u8);
    fn controls_log(x: u8, y: u8, ptr: *const u8, len: usize);
    fn controls_set(x: u8, y: u8, value: bool);
    fn controls_config(x: u8, y: u8);
}

pub fn bl_midi_safe(status: u8, data0: u8, data1: u8) {
    unsafe { bl_midi(status, data0, data1) }
}

/// Log a string to the BL output
pub fn bl_log(msg: &str) {
    unsafe { log(msg.as_ptr(), msg.len()) }
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
    println!("Configuring controls to ({}, {})...", x, y);
    unsafe {
        controls_config(x, y);
    }
}

// const PAGE_SIZE: usize = 65536;

#[doc(hidden)]
pub unsafe fn _get_array(array_pointer: *mut u8, array_length: usize) -> &'static mut [u8] {
    // Safety: This is unsafe because we're dealing with raw pointers.
    let mut slice = unsafe {
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

#[derive(Debug, Clone, Copy)]
pub struct TickInput {
    pub time: u32,
    pub volume: u8,
    pub beat_volume: u8,
    pub bass: u8,
    pub bass_avg_short: u8,
    pub bass_avg: u8,
    pub bpm: u8,
    pub time_between_beats_millis: u16,
    pub initial: bool,
}

#[derive(Debug, Clone, Copy)]
pub struct MidiEvent {
    pub status: u8,
    pub kind: u8,
    pub value: u8,
}

#[macro_export]
macro_rules! midi {
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

pub use midi;

impl MidiEvent {
    pub fn tup(&self) -> (u8, u8) {
        (self.status, self.kind)
    }
}

pub mod prelude {
    // Reef
    pub use super::bl_log;

    // Dynamic borrow checking
    pub use std::cell::{Cell, RefCell};
    pub use std::rc::{Rc, Weak};

    // Growable Array collections (vector)
    pub use std::collections::VecDeque;

    // Hash collections (via BTreeMap)
    pub use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};
    // Other collections
    pub use std::collections::{BinaryHeap, LinkedList};

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
    pub use print;

    #[macro_export]
    macro_rules! dbg {
        ($($arg:tt)*) => {
            panic!("'dbg!' not supported in Blaulicht!");
        };
    }
    pub use dbg;

    #[macro_export]
    macro_rules! smidi {
        ($tuple: expr, $value: expr) => {
            blaulicht::bl_midi_safe($tuple.0, $tuple.1, $value)
        };
    }

    pub use smidi;
}

// pub fn fmod(a: f64, b: f64) -> f64 {
//     a - b * (a / b).trunc()
// }

/// Uses input ranging from 0 to 360.
pub fn hsv_to_rgb(h: u16) -> (u8, u8, u8) {
    // Normalize hue to [0, 360)
    // let mut h = h % 360;
    // if h < 0 {
    //     h += 360;
    // }

    let s = 1.0f32;
    let v = 1.0f32;
    let c = v * s;
    let h_prime = h as f32 / 60.0;
    let x = c * (1.0 - ((h_prime % 2.0) - 1.0).abs());
    let m = v - c;

    let (rf, gf, bf) = match h {
        0..=59 => (c, x, 0.0),
        60..=119 => (x, c, 0.0),
        120..=179 => (0.0, c, x),
        180..=239 => (0.0, x, c),
        240..=299 => (x, 0.0, c),
        _ => (c, 0.0, x),
    };

    let r = ((rf + m) * 255.0).round() as u8;
    let g = ((gf + m) * 255.0).round() as u8;
    let b = ((bf + m) * 255.0).round() as u8;

    (r, g, b)
}

#[macro_export]
macro_rules! colorize {
    ($rgb_triple:expr, $dmx:expr, $dmx_start:expr) => {
        $dmx[$dmx_start + 0] = $rgb_triple.0;
        $dmx[$dmx_start + 1] = $rgb_triple.1;
        $dmx[$dmx_start + 2] = $rgb_triple.2;
    };
}

pub use colorize;

#[macro_export]
macro_rules! elapsed {
    ($input: expr, $time: expr) => {
        $input.time - $time
    };
}

pub use elapsed;

#[macro_export]
macro_rules! nelapsed {
    ($input: expr, $time: expr) => {
        $input.time as i32 - $time as i32
    };
}

pub use nelapsed;
