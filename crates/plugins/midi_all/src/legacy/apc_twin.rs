//! On-screen digital twin of the Akai APC mini mk2.
//!
//! Draws the pad grid, side buttons and faders from the LED / control shadow
//! kept by [`VirtualMidi`] and turns canvas clicks into synthetic MIDI input,
//! so mappings can be developed and exercised without the hardware.

use std::rc::Rc;

use blaulicht_plugin_framework::ui;
use blaulicht_shared::{ControlEvent, ControlEventMessage, PluginUiEvent};

use crate::legacy::apc_midi::MidiDevice;
use crate::legacy::mapping::{is_reserved_pad, LED_WHITE, SHIFT_NOTE, SINGLE_LED_ON};
use crate::legacy::virtual_midi::VirtualMidi;
use crate::legacy::LegacyState;

pub const APC_CANVAS_ID: u8 = 40;

// Geometry mirrors the physical APC mini mk2: 8x8 wide pads, a column of
// round scene-launch buttons on the right, a row of round track buttons
// below the grid (with SHIFT on the right) and nine faders along the bottom.
const CANVAS_W: i32 = 740;
const CANVAS_H: i32 = 440;

/// Left edge of the MIDI-in monitor column.
const MONITOR_X: i32 = 566;

const RAIL_W: i32 = 8;

const PAD_W: i32 = 46;
const PAD_H: i32 = 22;
const PAD_GAP: i32 = 7;
const COL_STEP: i32 = PAD_W + PAD_GAP;
const ROW_STEP: i32 = PAD_H + PAD_GAP;
const GRID_X: i32 = 24;
const GRID_Y: i32 = 42;
const GRID_W: i32 = 8 * PAD_W + 7 * PAD_GAP;
const GRID_H: i32 = 8 * PAD_H + 7 * PAD_GAP;

const BUTTON_R: i32 = 7;
const BUTTON_HIT_R: i32 = 13;
/// Centre x of the scene-launch / shift / master column.
const SIDE_CX: i32 = GRID_X + GRID_W + 34;
/// Centre y of the track-button row.
const TRACK_CY: i32 = GRID_Y + GRID_H + 20;

const FADER_Y: i32 = TRACK_CY + 30;
const FADER_H: i32 = 104;
const FADER_TRACK_W: i32 = 8;
const FADER_THUMB_W: i32 = 26;
const FADER_THUMB_H: i32 = 12;
const FADER_HIT_W: i32 = 40;

const SCENE_LABELS: [&str; 8] = [
    "CLIP STOP",
    "SOLO",
    "MUTE",
    "REC ARM",
    "SELECT",
    "DRUM",
    "NOTE",
    "STOP ALL",
];
const TRACK_LABELS: [&str; 8] = ["VOLUME", "PAN", "SEND", "DEVICE", "UP", "DOWN", "LEFT", "RIGHT"];

/// APC mini mk2 note numbers.
const SCENE_LAUNCH_TOP: u8 = 112; // 112..=119, top to bottom
const TRACK_BUTTON_FIRST: u8 = 100; // 100..=107, left to right
const FADER_FIRST_CC: u8 = 48; // 48..=55 channel faders, 56 master

const PRESS_FLASH_MS: u32 = 150;

enum Hit {
    Note(u8),
    Fader { cc: u8, value: u8 },
}

impl LegacyState {
    pub fn apc_handle(&self) -> Option<Rc<VirtualMidi>> {
        self.midi_handles
            .iter()
            .find(|(dev, _)| matches!(dev, MidiDevice::APCMini))
            .map(|(_, h)| h.clone())
    }

    /// Consumes canvas input for the twin and redraws it.
    pub fn render_apc_twin(&mut self, events: &[ControlEventMessage], plugin_id: u8, clock: u32) {
        let Some(apc) = self.apc_handle() else {
            return;
        };

        for ev in events {
            let ControlEvent::PluginUi(ui_event, pid) = ev.body() else {
                continue;
            };
            if pid != plugin_id {
                continue;
            }
            match ui_event {
                PluginUiEvent::CanvasClick { id, x, y } if id == APC_CANVAS_ID => {
                    match hit_test(x, y) {
                        Some(Hit::Note(note)) => apc.inject_press(note),
                        Some(Hit::Fader { cc, value }) => apc.inject_cc(cc, value),
                        None => {}
                    }
                }
                PluginUiEvent::CanvasDrag { id, x, y, .. } if id == APC_CANVAS_ID => {
                    if let Some(Hit::Fader { cc, value }) = hit_test(x, y) {
                        apc.inject_cc(cc, value);
                    }
                }
                _ => {}
            }
        }

        let status = self.twin_status();
        let learning = self.is_learning();
        draw_twin(&apc, clock, status, learning);
    }
}

fn col_cx(col: u8) -> i32 {
    GRID_X + col as i32 * COL_STEP + PAD_W / 2
}

fn row_cy(row_from_top: u8) -> i32 {
    GRID_Y + row_from_top as i32 * ROW_STEP + PAD_H / 2
}

fn pad_rect(row: u8, col: u8) -> (i32, i32, i32, i32) {
    // Row 0 is the bottom row on the hardware (note = row * 8 + col).
    let x = GRID_X + col as i32 * COL_STEP;
    let y = GRID_Y + (7 - row as i32) * ROW_STEP;
    (x, y, PAD_W, PAD_H)
}

fn scene_button_center(index: u8) -> (i32, i32) {
    (SIDE_CX, row_cy(index))
}

fn track_button_center(index: u8) -> (i32, i32) {
    (col_cx(index), TRACK_CY)
}

fn shift_center() -> (i32, i32) {
    (SIDE_CX, TRACK_CY)
}

/// Fader `index` 0..=7 sit under the pad columns, 8 (master) under SHIFT.
fn fader_cx(index: u8) -> i32 {
    if index == 8 {
        SIDE_CX
    } else {
        col_cx(index)
    }
}

fn contains((rx, ry, rw, rh): (i32, i32, i32, i32), x: i32, y: i32) -> bool {
    x >= rx && x < rx + rw && y >= ry && y < ry + rh
}

fn near((cx, cy): (i32, i32), x: i32, y: i32, r: i32) -> bool {
    (x - cx).abs() <= r && (y - cy).abs() <= r
}

fn hit_test(x: i32, y: i32) -> Option<Hit> {
    for row in 0..8u8 {
        for col in 0..8u8 {
            if contains(pad_rect(row, col), x, y) {
                return Some(Hit::Note(row * 8 + col));
            }
        }
    }
    for i in 0..8u8 {
        if near(scene_button_center(i), x, y, BUTTON_HIT_R) {
            return Some(Hit::Note(SCENE_LAUNCH_TOP + i));
        }
        if near(track_button_center(i), x, y, BUTTON_HIT_R) {
            return Some(Hit::Note(TRACK_BUTTON_FIRST + i));
        }
    }
    if near(shift_center(), x, y, BUTTON_HIT_R) {
        return Some(Hit::Note(SHIFT_NOTE));
    }
    for i in 0..9u8 {
        let rect = (fader_cx(i) - FADER_HIT_W / 2, FADER_Y, FADER_HIT_W, FADER_H);
        if contains(rect, x, y) {
            let rel = (y - FADER_Y).clamp(0, FADER_H - 1);
            let value = 127 - (rel * 127 / (FADER_H - 1).max(1));
            return Some(Hit::Fader {
                cc: FADER_FIRST_CC + i,
                value: value.clamp(0, 127) as u8,
            });
        }
    }
    None
}

/// Approximate APC mini mk2 velocity -> colour mapping. The hardware palette
/// has 128 entries; the ones the plugin actually uses are listed explicitly and
/// the rest fall back to a hue derived from the index.
fn led_color(velocity: u8) -> (u8, u8, u8) {
    match velocity {
        0 => (38, 40, 46),
        1 => (90, 90, 96),
        2 => (160, 160, 168),
        3 => (235, 235, 240),
        4 | 5 => (230, 40, 40),
        6 | 7 => (120, 20, 20),
        8 | 9 => (240, 130, 30),
        10 | 11 => (140, 70, 15),
        12 | 13 => (240, 220, 40),
        14 | 15 => (130, 120, 20),
        16..=19 => (120, 230, 40),
        20 | 21 => (40, 220, 60),
        22 | 23 => (20, 120, 30),
        24..=35 => (40, 220, 160),
        36..=39 => (40, 200, 240),
        40..=43 => (60, 130, 240),
        44..=47 => (50, 70, 240),
        48..=51 => (120, 60, 240),
        52..=55 => (220, 60, 240),
        56..=59 => (240, 60, 160),
        _ => {
            // Spread the remaining indices around the hue circle.
            let hue = (velocity as f32 - 60.0) / 68.0 * 360.0;
            hsv_to_rgb(hue, 0.85, 0.9)
        }
    }
}

fn hsv_to_rgb(h: f32, s: f32, v: f32) -> (u8, u8, u8) {
    let c = v * s;
    let hp = (h.rem_euclid(360.0)) / 60.0;
    let x = c * (1.0 - (hp % 2.0 - 1.0).abs());
    let (r, g, b) = match hp as i32 {
        0 => (c, x, 0.0),
        1 => (x, c, 0.0),
        2 => (0.0, c, x),
        3 => (0.0, x, c),
        4 => (x, 0.0, c),
        _ => (c, 0.0, x),
    };
    let m = v - c;
    (
        ((r + m) * 255.0) as u8,
        ((g + m) * 255.0) as u8,
        ((b + m) * 255.0) as u8,
    )
}

fn pressed_stroke(apc: &VirtualMidi, note: u8, clock: u32) -> (u8, u8, u8, i32) {
    if apc.pressed_within(note, clock, PRESS_FLASH_MS) {
        (255, 255, 255, 3)
    } else {
        (70, 74, 84, 1)
    }
}

fn draw_pad(apc: &VirtualMidi, note: u8, clock: u32, learning: bool) {
    let (x, y, w, h) = pad_rect(note / 8, note % 8);
    let led = apc.led(note);
    let (r, g, b) = if led == 0 && is_reserved_pad(note) {
        // Fixed pads (scene / page columns) get a faint
        // red tint so users learn they cannot be remapped.
        (52, 30, 34)
    } else {
        led_color(led)
    };
    ui::painter_rect(x, y, w, h, r, g, b, 255);
    let (sr, sg, sb, t) = if learning && led == LED_WHITE {
        (255, 240, 200, 2)
    } else {
        pressed_stroke(apc, note, clock)
    };
    ui::painter_rect_stroke(x, y, w, h, sr, sg, sb, 255, t);

    // Dark text on bright pads, light text on dark ones.
    let luma = (r as u32 * 299 + g as u32 * 587 + b as u32 * 114) / 1000;
    let (tr, tg, tb) = if luma > 140 { (20, 20, 24) } else { (120, 126, 140) };
    ui::painter_text(x + 3, y + 2, 8, tr, tg, tb, 255, &format!("{note}"));
}

/// Round single-colour button (scene launch, track buttons, shift). Like the
/// hardware, track buttons light red and scene-launch buttons green for any
/// non-zero velocity; `SINGLE_LED_ON` (a mapped but idle button) is drawn as a
/// dim tint of that colour.
fn draw_round_button(apc: &VirtualMidi, note: u8, (cx, cy): (i32, i32), clock: u32) {
    let v = apc.led(note);
    let dim = v == SINGLE_LED_ON;
    let (r, g, b) = match note {
        _ if v == 0 => (48, 50, 56),
        TRACK_BUTTON_FIRST..=107 if dim => (96, 28, 28),
        TRACK_BUTTON_FIRST..=107 => (230, 40, 40),
        SCENE_LAUNCH_TOP..=119 if dim => (24, 84, 34),
        SCENE_LAUNCH_TOP..=119 => (40, 220, 60),
        _ => led_color(v),
    };
    ui::painter_circle(cx, cy, BUTTON_R, r, g, b, 255);
    let (sr, sg, sb, t) = pressed_stroke(apc, note, clock);
    ui::painter_circle_stroke(cx, cy, BUTTON_R, sr, sg, sb, 255, t);
}

fn draw_fader(apc: &VirtualMidi, index: u8) {
    let cx = fader_cx(index);
    let cc = FADER_FIRST_CC + index;
    let value = apc.control(cc) as i32;

    // Slot.
    ui::painter_rect(cx - 14, FADER_Y - 4, 28, FADER_H + 8, 22, 24, 29, 255);
    ui::painter_rect_stroke(cx - 14, FADER_Y - 4, 28, FADER_H + 8, 50, 54, 62, 255, 1);
    ui::painter_rect(cx - FADER_TRACK_W / 2, FADER_Y, FADER_TRACK_W, FADER_H, 8, 9, 12, 255);

    // Cap.
    let travel = FADER_H - FADER_THUMB_H;
    let thumb_y = FADER_Y + ((127 - value) * travel / 127);
    ui::painter_rect(cx - FADER_THUMB_W / 2, thumb_y, FADER_THUMB_W, FADER_THUMB_H, 205, 208, 215, 255);
    ui::painter_rect_stroke(cx - FADER_THUMB_W / 2, thumb_y, FADER_THUMB_W, FADER_THUMB_H, 120, 126, 140, 255, 1);
    ui::painter_line(cx - 9, thumb_y + FADER_THUMB_H / 2, cx + 9, thumb_y + FADER_THUMB_H / 2, 90, 94, 104, 255, 1);

    let label = if index == 8 { "M".to_string() } else { format!("{}", index + 1) };
    ui::painter_text(cx - 4, FADER_Y + FADER_H + 6, 9, 150, 156, 170, 255, &label);
    ui::painter_text(cx + 6, FADER_Y + FADER_H + 6, 9, 110, 116, 130, 255, &format!("{value}"));
}

/// Recent input events, so a mapping author can see what a pad/fader sends.
fn draw_monitor(apc: &VirtualMidi) {
    let x = MONITOR_X;
    let w = CANVAS_W - RAIL_W - 22 - x;
    ui::painter_rect(x, GRID_Y - 4, w, CANVAS_H - GRID_Y - 8, 12, 13, 17, 255);
    ui::painter_rect_stroke(x, GRID_Y - 4, w, CANVAS_H - GRID_Y - 8, 50, 54, 62, 255, 1);
    ui::painter_text(x + 6, GRID_Y - 14, 7, 130, 136, 150, 255, "MIDI IN  (clock  kind  note = value)");

    let mut y = GRID_Y + 4;
    for (i, line) in apc.recent_events().iter().enumerate() {
        let shade = if i == 0 { 230 } else { 150u8.saturating_sub(i as u8 * 6) };
        ui::painter_text(x + 6, y, 8, shade, shade, shade.saturating_add(10), 255, line);
        y += 14;
        if y > CANVAS_H - 24 {
            break;
        }
    }
    if apc.recent_events().is_empty() {
        ui::painter_text(x + 6, GRID_Y + 4, 8, 110, 116, 130, 255, "no input yet");
    }
}

fn draw_twin(
    apc: &VirtualMidi,
    clock: u32,
    status: Option<(String, (u8, u8, u8))>,
    learning: bool,
) {
    ui::painter_begin(APC_CANVAS_ID, CANVAS_W, CANVAS_H);

    // Body with the red side rails of the real unit.
    ui::painter_rect(0, 0, CANVAS_W, CANVAS_H, 18, 19, 23, 255);
    ui::painter_rect(0, 0, RAIL_W, CANVAS_H, 196, 34, 44, 255);
    ui::painter_rect(CANVAS_W - RAIL_W, 0, RAIL_W, CANVAS_H, 196, 34, 44, 255);
    ui::painter_rect_stroke(0, 0, CANVAS_W, CANVAS_H, 60, 64, 74, 255, 2);

    ui::painter_text(GRID_X, 8, 15, 235, 235, 240, 255, "AKAI");
    ui::painter_text(GRID_X + 50, 13, 7, 150, 156, 170, 255, "PROFESSIONAL");
    ui::painter_text(GRID_X + GRID_W - 66, 8, 14, 235, 235, 240, 255, "APC mini");

    let (status, (sr, sg, sb)) = status.unwrap_or_else(|| {
        if apc.is_connected() {
            (
                format!("{} — connected, twin mirrors LEDs", apc.name()),
                (120, 220, 140),
            )
        } else {
            (
                format!("{} — NOT connected, virtual twin: click to play", apc.name()),
                (240, 170, 60),
            )
        }
    });
    ui::painter_text(GRID_X, 28, 8, sr, sg, sb, 255, &status);

    for note in 0..64u8 {
        draw_pad(apc, note, clock, learning);
    }

    // Scene launch column with the printed secondary functions.
    ui::painter_text(SIDE_CX - 12, GRID_Y - 12, 6, 130, 136, 150, 255, "SCENE LAUNCH");
    for i in 0..8u8 {
        let c = scene_button_center(i);
        draw_round_button(apc, SCENE_LAUNCH_TOP + i, c, clock);
        ui::painter_text(c.0 + 12, c.1 - 4, 6, 130, 136, 150, 255, SCENE_LABELS[i as usize]);
    }

    // Track buttons row.
    for i in 0..8u8 {
        let c = track_button_center(i);
        draw_round_button(apc, TRACK_BUTTON_FIRST + i, c, clock);
        ui::painter_text(c.0 - 16, c.1 + 10, 6, 130, 136, 150, 255, TRACK_LABELS[i as usize]);
    }
    let sc = shift_center();
    draw_round_button(apc, SHIFT_NOTE, sc, clock);
    ui::painter_text(sc.0 - 10, sc.1 + 10, 6, 130, 136, 150, 255, "SHIFT");

    for i in 0..9u8 {
        draw_fader(apc, i);
    }

    draw_monitor(apc);

    ui::painter_end();
}
