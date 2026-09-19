//! On-screen digital twin of the KORG nanoKONTROL Studio.
//!
//! Draws the left cluster (scene, marker/track, transport, jog wheel) and the
//! eight channel strips (Mute/Solo/Rec/Select, knob, fader) from the
//! [`VirtualMidi`] shadow, and turns canvas clicks/drags into synthetic MIDI
//! so Korg mappings can be developed without the hardware.
//!
//! MIDI numbers live in one table (`KorgControl`). Entries marked VERIFIED
//! are what the existing plugin code already relies on; the rest follow the
//! nanoKONTROL Studio factory map as far as known and can be corrected after
//! pressing the physical button once and reading the twin's "last" line.

use blaulicht_plugin_framework::ui;
use blaulicht_shared::{ControlEvent, ControlEventMessage, PluginUiEvent};

use crate::korg::{KorgSubSystem, FADER_BYTES, JOG_WHEEL_CC, SELECT_BUTTON_STARTER};
use crate::legacy::virtual_midi::VirtualMidi;

pub const KORG_CANVAS_ID: u8 = 60;

const CANVAS_W: i32 = 760;
const CANVAS_H: i32 = 400;

const STRIP_X: i32 = 214;
const STRIP_STEP: i32 = 66;
const STRIP_W: i32 = 60;
const BUTTON_W: i32 = 44;
const BUTTON_H: i32 = 16;
const STRIP_BUTTON_YS: [i32; 4] = [40, 62, 84, 106]; // Mute, Solo, Rec, Select
const KNOB_CY: i32 = 160;
const KNOB_R: i32 = 16;
const FADER_Y: i32 = 200;
const FADER_H: i32 = 178;
const FADER_TRACK_W: i32 = 8;
const FADER_THUMB_W: i32 = 24;
const FADER_THUMB_H: i32 = 14;

const JOG_CX: i32 = 100;
const JOG_CY: i32 = 295;
const JOG_R: i32 = 72;

const PRESS_FLASH_MS: u32 = 150;

/// How a control talks MIDI.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Wire {
    /// Note on 127 / note off (status 0x90 / 0x80).
    Note(u8),
    /// Momentary CC: 127 on press, 0 on release.
    CcButton(u8),
}
// Knobs/faders are absolute CCs and the jog wheel a relative CC (1 = one step
// clockwise, 127 = one step counter-clockwise); they are handled by `Hit`
// variants directly rather than through `Wire`.

struct KorgControl {
    label: &'static str,
    wire: Wire,
    rect: (i32, i32, i32, i32),
}

// Left cluster geometry.
const LEFT_X: i32 = 14;
const COL2_X: i32 = 60;
const COL3_X: i32 = 106;
const COL4_X: i32 = 148;
const ROW_MARKER_Y: i32 = 96;
const ROW_TRACK_Y: i32 = 124;
const ROW_TRANSPORT_Y: i32 = 152;

/// Buttons of the left cluster. VERIFIED: CC 80/81 (mode toggles used by
/// korg.rs), note 82 ("Set", per the comment in korg.rs). The rest are
/// ASSUMED from the factory map.
const LEFT_BUTTONS: [KorgControl; 13] = [
    KorgControl { label: "Scene", wire: Wire::CcButton(84), rect: (LEFT_X, 60, BUTTON_W, 18) },
    KorgControl { label: "Cycle", wire: Wire::CcButton(80), rect: (LEFT_X, ROW_MARKER_Y, 40, BUTTON_H) },
    KorgControl { label: "Set", wire: Wire::Note(82), rect: (COL2_X, ROW_MARKER_Y, 40, BUTTON_H) },
    KorgControl { label: "<", wire: Wire::CcButton(81), rect: (COL3_X, ROW_MARKER_Y, 36, BUTTON_H) },
    KorgControl { label: ">", wire: Wire::CcButton(83), rect: (COL4_X, ROW_MARKER_Y, 36, BUTTON_H) },
    KorgControl { label: "<<", wire: Wire::CcButton(43), rect: (LEFT_X, ROW_TRACK_Y, 40, BUTTON_H) },
    KorgControl { label: ">>", wire: Wire::CcButton(44), rect: (COL2_X, ROW_TRACK_Y, 40, BUTTON_H) },
    KorgControl { label: "<", wire: Wire::CcButton(58), rect: (COL3_X, ROW_TRACK_Y, 36, BUTTON_H) },
    KorgControl { label: ">", wire: Wire::CcButton(59), rect: (COL4_X, ROW_TRACK_Y, 36, BUTTON_H) },
    KorgControl { label: "|<", wire: Wire::CcButton(40), rect: (LEFT_X, ROW_TRANSPORT_Y, 40, 20) },
    KorgControl { label: "STOP", wire: Wire::CcButton(42), rect: (COL2_X, ROW_TRANSPORT_Y, 40, 20) },
    KorgControl { label: "PLAY", wire: Wire::CcButton(41), rect: (COL3_X, ROW_TRANSPORT_Y, 36, 20) },
    KorgControl { label: "REC", wire: Wire::CcButton(45), rect: (COL4_X, ROW_TRANSPORT_Y, 36, 20) },
];

/// Per-strip button CCs (ASSUMED from the factory map) — Select is VERIFIED
/// as note 46 + strip.
const MUTE_CC_FIRST: u8 = 21;
const SOLO_CC_FIRST: u8 = 29;
const REC_CC_FIRST: u8 = 38;
/// Knob CCs are VERIFIED (13..=20), faders VERIFIED via `FADER_BYTES`.
const KNOB_CC_FIRST: u8 = 13;

enum Hit {
    Press(Wire),
    Absolute { cc: u8, value: u8 },
    Jog { delta: i32 },
    KnobDrag { cc: u8, dy: i32 },
}

fn strip_x(strip: u8) -> i32 {
    STRIP_X + strip as i32 * STRIP_STEP
}

fn strip_button_rect(strip: u8, row: usize) -> (i32, i32, i32, i32) {
    (strip_x(strip) + (STRIP_W - BUTTON_W) / 2, STRIP_BUTTON_YS[row], BUTTON_W, BUTTON_H)
}

fn strip_button_wire(strip: u8, row: usize) -> Wire {
    match row {
        0 => Wire::CcButton(MUTE_CC_FIRST + strip),
        1 => Wire::CcButton(SOLO_CC_FIRST + strip),
        2 => Wire::CcButton(REC_CC_FIRST + strip),
        _ => Wire::Note(SELECT_BUTTON_STARTER + strip),
    }
}

const STRIP_BUTTON_LABELS: [&str; 4] = ["Mute", "Solo", "Rec", "Select"];

fn knob_center(strip: u8) -> (i32, i32) {
    (strip_x(strip) + STRIP_W / 2, KNOB_CY)
}

fn fader_cx(strip: u8) -> i32 {
    strip_x(strip) + STRIP_W / 2
}

fn contains((rx, ry, rw, rh): (i32, i32, i32, i32), x: i32, y: i32) -> bool {
    x >= rx && x < rx + rw && y >= ry && y < ry + rh
}

fn dist2((cx, cy): (i32, i32), x: i32, y: i32) -> i32 {
    (x - cx) * (x - cx) + (y - cy) * (y - cy)
}

fn hit_test(x: i32, y: i32, drag: Option<(i32, i32)>) -> Option<Hit> {
    for b in &LEFT_BUTTONS {
        if contains(b.rect, x, y) {
            return drag.is_none().then_some(Hit::Press(b.wire));
        }
    }
    for strip in 0..8u8 {
        for row in 0..4 {
            if contains(strip_button_rect(strip, row), x, y) {
                return drag.is_none().then_some(Hit::Press(strip_button_wire(strip, row)));
            }
        }
        if dist2(knob_center(strip), x, y) <= (KNOB_R + 6) * (KNOB_R + 6) {
            let cc = KNOB_CC_FIRST + strip;
            return match drag {
                Some((_, dy)) => Some(Hit::KnobDrag { cc, dy }),
                // Click: upper half nudges up, lower half nudges down.
                None => Some(Hit::KnobDrag {
                    cc,
                    dy: if y < KNOB_CY { -8 } else { 8 },
                }),
            };
        }
        let cx = fader_cx(strip);
        if contains((cx - 20, FADER_Y, 40, FADER_H), x, y) {
            let rel = (y - FADER_Y).clamp(0, FADER_H - 1);
            let value = 127 - (rel * 127 / (FADER_H - 1).max(1));
            return Some(Hit::Absolute {
                cc: FADER_BYTES[strip as usize],
                value: value.clamp(0, 127) as u8,
            });
        }
    }
    if dist2((JOG_CX, JOG_CY), x, y) <= JOG_R * JOG_R {
        return Some(match drag {
            Some((dx, dy)) => Hit::Jog { delta: (dx - dy).signum() },
            // Click: right half = clockwise, left half = counter-clockwise.
            None => Hit::Jog {
                delta: if x >= JOG_CX { 1 } else { -1 },
            },
        });
    }
    None
}

impl KorgSubSystem {
    /// Consumes canvas input for the twin and redraws it.
    pub fn render_twin(&mut self, events: &[ControlEventMessage], plugin_id: u8, clock: u32) {
        for ev in events {
            let ControlEvent::PluginUi(ui_event, pid) = ev.body() else {
                continue;
            };
            if pid != plugin_id {
                continue;
            }
            let hit = match ui_event {
                PluginUiEvent::CanvasClick { id, x, y } if id == KORG_CANVAS_ID => {
                    hit_test(x, y, None)
                }
                PluginUiEvent::CanvasDrag { id, x, y, dx, dy } if id == KORG_CANVAS_ID => {
                    hit_test(x, y, Some((dx, dy)))
                }
                _ => None,
            };
            let Some(hit) = hit else { continue };
            let midi = &self.midi;
            match hit {
                Hit::Press(Wire::Note(n)) => midi.inject_press(n),
                Hit::Press(Wire::CcButton(cc)) => {
                    midi.inject_cc(cc, 127);
                    midi.inject_cc(cc, 0);
                }
                Hit::Absolute { cc, value } => midi.inject_cc(cc, value),
                Hit::KnobDrag { cc, dy } => {
                    let v = (midi.control(cc) as i32 - dy).clamp(0, 127) as u8;
                    midi.inject_cc(cc, v);
                }
                Hit::Jog { delta } if delta != 0 => {
                    midi.inject_cc(JOG_WHEEL_CC, if delta > 0 { 1 } else { 127 });
                }
                Hit::Jog { .. } => {}
            }
        }

        draw_twin(&self.midi, clock);
    }
}

// ---------------------------------------------------------------------------
// Drawing.
// ---------------------------------------------------------------------------

/// Lit state of a button as the device would show it (LED shadow or held CC).
fn button_lit(midi: &VirtualMidi, wire: Wire) -> bool {
    match wire {
        Wire::Note(n) => midi.led(n) > 0,
        Wire::CcButton(cc) => midi.control(cc) > 0,
    }
}

fn button_pressed(midi: &VirtualMidi, wire: Wire, clock: u32) -> bool {
    match wire {
        Wire::Note(n) => midi.pressed_within(n, clock, PRESS_FLASH_MS),
        Wire::CcButton(_) => false,
    }
}

fn draw_button(midi: &VirtualMidi, wire: Wire, rect: (i32, i32, i32, i32), label: &str, clock: u32) {
    let (x, y, w, h) = rect;
    let lit = button_lit(midi, wire);
    let (r, g, b) = if lit { (235, 235, 240) } else { (30, 31, 36) };
    ui::painter_rect(x, y, w, h, r, g, b, 255);
    let (sr, sg, sb, t) = if button_pressed(midi, wire, clock) {
        (255, 255, 255, 2)
    } else {
        (150, 154, 165, 1)
    };
    ui::painter_rect_stroke(x, y, w, h, sr, sg, sb, 255, t);
    let (tr, tg, tb) = if lit { (20, 20, 24) } else { (200, 204, 214) };
    let tx = x + (w - label.len() as i32 * 5) / 2;
    ui::painter_text(tx.max(x + 2), y + (h - 9) / 2, 8, tr, tg, tb, 255, label);
}

fn draw_knob(midi: &VirtualMidi, strip: u8) {
    let (cx, cy) = knob_center(strip);
    let value = midi.control(KNOB_CC_FIRST + strip) as f32;
    ui::painter_circle(cx, cy, KNOB_R, 26, 27, 32, 255);
    ui::painter_circle_stroke(cx, cy, KNOB_R, 120, 124, 136, 255, 1);
    // 270° sweep from 7 o'clock to 5 o'clock.
    let angle = (-135.0 + value / 127.0 * 270.0).to_radians();
    let (sx, sy) = (
        cx + (angle.sin() * (KNOB_R as f32 - 4.0)) as i32,
        cy - (angle.cos() * (KNOB_R as f32 - 4.0)) as i32,
    );
    ui::painter_line(cx, cy, sx, sy, 235, 235, 240, 255, 2);
    ui::painter_text(cx - 8, cy + KNOB_R + 3, 8, 120, 126, 140, 255, &format!("{}", value as i32));
}

fn draw_fader(midi: &VirtualMidi, strip: u8) {
    let cx = fader_cx(strip);
    let value = midi.control(FADER_BYTES[strip as usize]) as i32;
    ui::painter_rect(cx - 14, FADER_Y - 4, 28, FADER_H + 8, 22, 24, 29, 255);
    ui::painter_rect_stroke(cx - 14, FADER_Y - 4, 28, FADER_H + 8, 50, 54, 62, 255, 1);
    ui::painter_rect(cx - FADER_TRACK_W / 2, FADER_Y, FADER_TRACK_W, FADER_H, 8, 9, 12, 255);
    let travel = FADER_H - FADER_THUMB_H;
    let thumb_y = FADER_Y + ((127 - value) * travel / 127);
    ui::painter_rect(cx - FADER_THUMB_W / 2, thumb_y, FADER_THUMB_W, FADER_THUMB_H, 205, 208, 215, 255);
    ui::painter_rect_stroke(cx - FADER_THUMB_W / 2, thumb_y, FADER_THUMB_W, FADER_THUMB_H, 120, 126, 140, 255, 1);
    ui::painter_line(cx - 8, thumb_y + FADER_THUMB_H / 2, cx + 8, thumb_y + FADER_THUMB_H / 2, 90, 94, 104, 255, 1);
    ui::painter_text(cx - 10, FADER_Y + FADER_H + 6, 8, 120, 126, 140, 255, &format!("{value}"));
}

fn draw_twin(midi: &VirtualMidi, clock: u32) {
    ui::painter_begin(KORG_CANVAS_ID, CANVAS_W, CANVAS_H);
    ui::painter_rect(0, 0, CANVAS_W, CANVAS_H, 20, 21, 25, 255);
    ui::painter_rect_stroke(0, 0, CANVAS_W, CANVAS_H, 60, 64, 74, 255, 2);

    ui::painter_text(LEFT_X, 10, 13, 235, 235, 240, 255, "KORG");
    ui::painter_text(LEFT_X + 44, 12, 9, 200, 204, 214, 255, "nanoKONTROL Studio");
    ui::painter_text(LEFT_X + 44, 24, 6, 130, 136, 150, 255, "MOBILE MIDI CONTROLLER");

    // Status fits the left cluster (the strips start at STRIP_X).
    let (status, (sr, sg, sb)) = if midi.is_connected() {
        ("connected — twin mirrors LEDs", (120, 220, 140))
    } else {
        ("NOT connected — virtual twin, click to play", (240, 170, 60))
    };
    ui::painter_text(LEFT_X, 38, 7, sr, sg, sb, 255, status);
    if let Some(last) = midi.recent_events().first() {
        ui::painter_text(LEFT_X, 48, 7, 150, 156, 170, 255, &format!("last: {last}"));
    }

    // Scene LEDs (decorative: scene 1 active).
    for i in 0..5 {
        let (r, g, b) = if i == 0 { (235, 235, 240) } else { (60, 62, 70) };
        ui::painter_circle(LEFT_X + 70 + i * 14, 69, 2, r, g, b, 255);
    }
    ui::painter_text(LEFT_X + 95, ROW_MARKER_Y - 11, 6, 130, 136, 150, 255, "Marker");
    ui::painter_text(LEFT_X + 98, ROW_TRACK_Y - 11, 6, 130, 136, 150, 255, "Track");
    for b in &LEFT_BUTTONS {
        draw_button(midi, b.wire, b.rect, b.label, clock);
    }

    // Jog wheel.
    ui::painter_circle(JOG_CX, JOG_CY, JOG_R, 14, 15, 18, 255);
    ui::painter_circle_stroke(JOG_CX, JOG_CY, JOG_R, 90, 94, 104, 255, 2);
    ui::painter_circle(JOG_CX, JOG_CY, JOG_R - 30, 30, 31, 36, 255);
    ui::painter_circle_stroke(JOG_CX, JOG_CY, JOG_R - 30, 70, 74, 84, 255, 1);
    ui::painter_text(JOG_CX - 34, JOG_CY + JOG_R + 4, 7, 120, 126, 140, 255, "drag / click side = jog");

    for strip in 0..8u8 {
        let sx = strip_x(strip);
        ui::painter_line(sx, 34, sx, CANVAS_H - 10, 40, 42, 48, 255, 1);
        for row in 0..4 {
            draw_button(
                midi,
                strip_button_wire(strip, row),
                strip_button_rect(strip, row),
                STRIP_BUTTON_LABELS[row],
                clock,
            );
        }
        draw_knob(midi, strip);
        draw_fader(midi, strip);
    }

    ui::painter_end();
}
