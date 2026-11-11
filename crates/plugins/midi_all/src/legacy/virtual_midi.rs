//! A MIDI device handle that keeps working without hardware.
//!
//! `VirtualMidi` wraps an optional real `MidiConnection` and additionally keeps
//! a shadow copy of the device state (LED colours, fader positions) plus a
//! queue of synthetic input events. The on-screen digital twin renders the
//! shadow and injects events into the queue, so the rest of the plugin can
//! treat a clicked pad exactly like a hardware press.

use std::cell::{Cell, RefCell};
use std::collections::VecDeque;

use blaulicht_plugin_framework::{println, MidiConnection, MidiEvent};

use crate::hardware::HardwareMode;

/// Device id reported when no hardware is attached. Matches
/// `MidiConnection::dummy()` so it can never collide with a real handle.
pub const VIRTUAL_DEVICE_ID: u8 = u8::MAX;

/// Number of recent input events kept for the on-screen monitor.
const RECENT_CAPACITY: usize = 16;

pub struct VirtualMidi {
    name: String,
    hardware: Cell<Option<MidiConnection>>,
    mode: Cell<HardwareMode>,
    /// Last velocity sent per note (note-on family status bytes).
    leds: RefCell<[u8; 128]>,
    /// Status byte of that last send: the channel selects the APC's LED
    /// behaviour, so replaying an LED needs it.
    led_status: RefCell<[u8; 128]>,
    /// Last value seen per CC number, from hardware or from the twin.
    controls: RefCell<[u8; 128]>,
    /// Synthetic input injected by the twin UI, drained on the next `poll`.
    pending: RefCell<Vec<MidiEvent>>,
    /// Clock of the last (hardware or synthetic) press per note.
    last_press: RefCell<[u32; 128]>,
    /// Most recent input events (newest first), formatted for display.
    recent: RefCell<VecDeque<String>>,
}

impl VirtualMidi {
    /// In `Real` mode opens the hardware device if present and otherwise
    /// continues virtual-only. `Mock` never touches the device.
    pub fn open(name: &str, mode: HardwareMode) -> Self {
        let midi = Self::disconnected(name);
        midi.mode.set(mode);
        midi.hardware.set(match mode {
            HardwareMode::Real => midi.open_hardware(),
            HardwareMode::Mock => {
                println!("{name}: mock hardware, virtual only");
                None
            }
        });
        midi
    }

    /// A placeholder that never talks to hardware (for `Default` impls).
    pub fn disconnected(name: &str) -> Self {
        Self {
            name: name.to_string(),
            hardware: Cell::new(None),
            mode: Cell::new(HardwareMode::Real),
            leds: RefCell::new([0; 128]),
            led_status: RefCell::new([0; 128]),
            controls: RefCell::new([0; 128]),
            pending: RefCell::new(Vec::new()),
            last_press: RefCell::new([0; 128]),
            recent: RefCell::new(VecDeque::new()),
        }
    }

    fn open_hardware(&self) -> Option<MidiConnection> {
        let name = &self.name;
        match MidiConnection::open(name) {
            Ok(conn) => {
                println!("{name}: connected, handle id {}", conn.get_meta().device_id);
                Some(conn)
            }
            Err(err) => {
                println!("{name}: not connected, virtual only ({err})");
                None
            }
        }
    }

    /// Switches at runtime. Leaving `Real` darkens the device's note LEDs, so
    /// a frozen panel can't pass for a live one; entering it replays the
    /// shadow, so the device shows what the twin showed.
    pub fn set_mode(&self, mode: HardwareMode) {
        if self.mode.replace(mode) == mode {
            return;
        }

        let leds = *self.leds.borrow();
        let statuses = *self.led_status.borrow();
        let lit = (0u8..128).filter(|&n| leds[n as usize] != 0);

        match mode {
            HardwareMode::Mock => {
                if let Some(hw) = self.hardware.take() {
                    for note in lit {
                        hw.send(statuses[note as usize], note, 0);
                    }
                }
                println!("{}: mock hardware, virtual only", self.name);
            }
            HardwareMode::Real => {
                let hardware = self.open_hardware();
                self.hardware.set(hardware);
                if let Some(hw) = hardware {
                    for note in lit {
                        hw.send(statuses[note as usize], note, leds[note as usize]);
                    }
                }
            }
        }
    }

    pub fn is_mock(&self) -> bool {
        self.mode.get() == HardwareMode::Mock
    }

    pub fn name(&self) -> &str {
        &self.name
    }

    pub fn is_connected(&self) -> bool {
        self.hardware.get().is_some()
    }

    pub fn device_id(&self) -> u8 {
        self.hardware
            .get()
            .map(|h| h.get_meta().device_id)
            .unwrap_or(VIRTUAL_DEVICE_ID)
    }

    /// Same shape as `MidiConnection::send`. Updates the LED shadow and
    /// forwards to the hardware when present.
    pub fn send(&self, status: u8, kind: u8, value: u8) {
        if (kind as usize) < 128 {
            match status & 0xF0 {
                0x90 => {
                    self.leds.borrow_mut()[kind as usize] = value;
                    self.led_status.borrow_mut()[kind as usize] = status;
                }
                0x80 => self.leds.borrow_mut()[kind as usize] = 0,
                0xB0 => self.controls.borrow_mut()[kind as usize] = value,
                _ => {}
            }
        }

        if let Some(hw) = self.hardware.get() {
            hw.send(status, kind, value);
        }
    }

    /// Hardware events (if any) followed by the synthetic events injected
    /// since the last poll.
    pub fn poll(&self, clock: u32) -> Vec<MidiEvent> {
        let mut events = self
            .hardware
            .get()
            .map(|h| h.poll())
            .unwrap_or_default();
        events.append(&mut self.pending.borrow_mut());

        for e in &events {
            self.remember(e, clock);
            if (e.kind as usize) >= 128 {
                continue;
            }
            match e.status & 0xF0 {
                0xB0 => self.controls.borrow_mut()[e.kind as usize] = e.value,
                0x90 if e.value > 0 => self.last_press.borrow_mut()[e.kind as usize] = clock,
                _ => {}
            }
        }

        events
    }

    fn remember(&self, e: &MidiEvent, clock: u32) {
        let kind = match e.status & 0xF0 {
            0x90 => "note on ",
            0x80 => "note off",
            0xB0 => "cc      ",
            _ => "status  ",
        };
        let mut recent = self.recent.borrow_mut();
        recent.push_front(format!(
            "{:>7} {kind} {:>3} = {:>3}",
            clock, e.kind, e.value
        ));
        recent.truncate(RECENT_CAPACITY);
    }

    /// Recent input events, newest first.
    pub fn recent_events(&self) -> Vec<String> {
        self.recent.borrow().iter().cloned().collect()
    }

    fn inject(&self, status: u8, kind: u8, value: u8) {
        // Always tagged with the virtual id (even when hardware is attached)
        // so consumers can tell a twin click from a physical press.
        self.pending.borrow_mut().push(MidiEvent {
            device: VIRTUAL_DEVICE_ID,
            status,
            kind,
            value,
        });
    }

    /// Simulates a full press-and-release of a note (channel 1).
    pub fn inject_press(&self, note: u8) {
        self.inject(0x90, note, 127);
        self.inject(0x80, note, 0);
    }

    /// Simulates a control-change (channel 1).
    pub fn inject_cc(&self, cc: u8, value: u8) {
        self.inject(0xB0, cc, value.min(127));
    }

    pub fn led(&self, note: u8) -> u8 {
        self.leds.borrow().get(note as usize).copied().unwrap_or(0)
    }

    pub fn control(&self, cc: u8) -> u8 {
        self.controls.borrow().get(cc as usize).copied().unwrap_or(0)
    }

    /// Whether `note` was pressed within the last `window_ms` milliseconds.
    pub fn pressed_within(&self, note: u8, clock: u32, window_ms: u32) -> bool {
        let last = self.last_press.borrow()[note as usize & 127];
        last != 0 && clock.wrapping_sub(last) <= window_ms
    }
}
