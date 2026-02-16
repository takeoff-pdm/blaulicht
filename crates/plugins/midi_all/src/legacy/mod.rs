mod apc_midi;
mod page_nav;

use blaulicht_plugin_framework::{self as bpf, println, ui, MidiConnection, MidiEvent};
use blaulicht_shared::{
    misc_event::videowall::{
        REQUEST_STATUS_REFRESH, SET_BRIGHTNESS, SET_FRY, SET_ROTATION, SET_SPEED, SET_VIDEO_INDEX,
    },
    AnimationSpeedModifier, AppPage, ControlEvent, MainUiEvent, PluginUiEvent, TickInput,
};
use map_range::MapRange;

use crate::legacy::apc_midi::MidiDevice;

const APC_SCENES: [u8; 8] = [56, 48, 40, 32, 24, 16, 8, 0];
const APC_SCENES_INT: [u8; 5] = [60, 52, 44, 36, 28];
const APC_VIDEO_PADS: [u8; 8] = [62, 54, 46, 38, 30, 22, 14, 6];

const FAN_SWITCH_ID: u8 = 0;
const VIRTUAL_APC_CANVAS_ID: u8 = 40;

const VIRTUAL_APC_WIDTH: i32 = 620;
const VIRTUAL_APC_HEIGHT: i32 = 520;
const VIRTUAL_APC_PAD_SIZE: i32 = 32;
const VIRTUAL_APC_PAD_GAP: i32 = 6;
const VIRTUAL_APC_GRID_X: i32 = 30;
const VIRTUAL_APC_GRID_Y: i32 = 70;
const VIRTUAL_APC_RIGHT_GAP: i32 = 16;
const VIRTUAL_APC_TOP_Y: i32 = 30;
const VIRTUAL_APC_TOP_HEIGHT: i32 = 18;
const VIRTUAL_APC_FADER_WIDTH: i32 = 14;
const VIRTUAL_APC_FADER_TOP_MARGIN: i32 = 24;
const VIRTUAL_APC_FADER_BOTTOM_MARGIN: i32 = 24;

const VIRTUAL_MIDIMIX_CANVAS_ID: u8 = 41;
const VIRTUAL_MIDIMIX_WIDTH: i32 = 620;
const VIRTUAL_MIDIMIX_HEIGHT: i32 = 520;
const VIRTUAL_MIDIMIX_KNOB_ROWS: usize = 3;
const VIRTUAL_MIDIMIX_KNOB_COLS: usize = 8;
const VIRTUAL_MIDIMIX_FADER_COUNT: usize = 9;
const VIRTUAL_MIDIMIX_KNOB_RADIUS: i32 = 16;
const VIRTUAL_MIDIMIX_KNOB_GAP_X: i32 = 64;
const VIRTUAL_MIDIMIX_KNOB_GAP_Y: i32 = 58;
const VIRTUAL_MIDIMIX_KNOB_START_X: i32 = 42;
const VIRTUAL_MIDIMIX_KNOB_START_Y: i32 = 40;
const VIRTUAL_MIDIMIX_FADER_TRACK_H: i32 = 150;
const VIRTUAL_MIDIMIX_FADER_TRACK_W: i32 = 14;
const VIRTUAL_MIDIMIX_FADER_GAP: i32 = 62;
const VIRTUAL_MIDIMIX_FADER_START_X: i32 = 48;
const VIRTUAL_MIDIMIX_FADER_START_Y: i32 = 240;
const VIRTUAL_DEVICE_TABS_ID: u8 = 50;

const VIRTUAL_KORG_CANVAS_ID: u8 = 42;
const VIRTUAL_KORG_WIDTH: i32 = 620;
const VIRTUAL_KORG_HEIGHT: i32 = 350;
const VIRTUAL_KORG_COLS: usize = 8;
const VIRTUAL_KORG_BUTTON_ROWS: usize = 4;
const VIRTUAL_KORG_COL_START_X: i32 = 230;
const VIRTUAL_KORG_COL_GAP: i32 = 42;
const VIRTUAL_KORG_BUTTON_START_Y: i32 = 60;
const VIRTUAL_KORG_BUTTON_W: i32 = 28;
const VIRTUAL_KORG_BUTTON_H: i32 = 14;
const VIRTUAL_KORG_BUTTON_GAP_Y: i32 = 7;
const VIRTUAL_KORG_KNOB_RADIUS: i32 = 13;
const VIRTUAL_KORG_KNOB_Y: i32 = 150;
const VIRTUAL_KORG_FADER_START_Y: i32 = 190;
const VIRTUAL_KORG_FADER_H: i32 = 140;
const VIRTUAL_KORG_FADER_W: i32 = 16;
#[derive(Default)]
pub struct LegacyState {
    counter: f32,
    midi_handles: Vec<(MidiDevice, MidiConnection)>,
    // ids_to_midi_types: HashMap<MidiDevice, usize>,
    enabled: bool,
    last_update: u32,

    // Selection state.
    groups: Vec<bool>,
    brightness_mod: u8,

    hsv: (u16, f32, f32),

    last_scene: u8,
    current_app_page: Option<AppPage>,
    last_app_page: Option<AppPage>,
    page_update_from_ui: bool,
    pending_app_page_sync: bool,
    is_apc_init: bool,

    fans: bool,
    // strobe_enabled: bool,
    // drums_enabled: bool,
    // drums_enabled_bef: bool,
    intensity_mapping: Vec<u8>,

    virtual_apc_faders: [u8; 9],
    virtual_apc_top_buttons: [bool; 8],
    virtual_apc_last_pad: Option<(u8, u32)>,
    last_ui_clock: u32,
    active_apc_fader: Option<usize>,
    last_apc_drag: u32,

    midimix_knobs: [u8; 24],
    midimix_faders: [u8; 9],
    virtual_device_tab: u8,
    active_midimix: Option<MidimixDragTarget>,
    last_midimix_drag: u32,

    korg_knobs: [u8; 8],
    korg_faders: [u8; 8],
    korg_buttons: [bool; 32],
    korg_wheel: u8,
    active_korg: Option<KorgDragTarget>,
    last_korg_drag: u32,
}

// static mut STATE: MaybeUninit<LegacyState> = MaybeUninit::uninit();

impl LegacyState {
    pub fn init(&mut self) {
        println!("[LEGACY] Initializing...");

        // bpf::send_event(ControlEvent::MiscEvent {
        //     descriptor: SET_VIDEO_INDEX,
        //     value: 0,
        // });
        // bpf::send_event(ControlEvent::MiscEvent {
        //     descriptor: SET_ROTATION,
        //     value: 0,
        // });

        //
        // return;

        let mut devices = vec![MidiDevice::MidiMix, MidiDevice::APCMini];
        devices.sort_by_key(|a| a.index());

        let mut midi_handles = Vec::with_capacity(devices.len());
        // let mut ids_to_midi_types = HashMap::new();

        for dev in devices {
            let name = dev.to_string();
            match MidiConnection::open(&name) {
                Ok(midi_handle) => {
                    println!(
                        "Got MIDI handle to device {dev} | HANDLE ID: {}",
                        midi_handle.get_meta().device_id
                    );
                    midi_handles.push((dev, midi_handle));
                }
                Err(err) => {
                    println!("[LEGACY] MIDI device not found ({dev}): {:?}", err);
                    if !crate::ALLOW_MISSING_MIDI {
                        panic!("[LEGACY] MIDI device missing and ALLOW_MISSING_MIDI=false");
                    }
                }
            }
        }

        self.hsv = (127, 127.0, 127.0);
        self.counter = 0.0;
        self.midi_handles = midi_handles;
        // self.ids_to_midi_type = ids_to;
        self.enabled = false;
        self.last_update = 0;
        self.groups = vec![false; 8];
        self.brightness_mod = 0;
        self.last_scene = 0;
        self.current_app_page = Some(AppPage::Logs);
        self.last_app_page = None;
        self.page_update_from_ui = false;
        self.pending_app_page_sync = true;
        self.is_apc_init = true;
        self.virtual_apc_faders = [0; 9];
        self.virtual_apc_top_buttons = [false; 8];
        self.virtual_apc_last_pad = None;
        self.last_ui_clock = 0;
        self.active_apc_fader = None;
        self.last_apc_drag = 0;
        self.midimix_knobs = [64; 24];
        self.midimix_faders = [64; 9];
        self.virtual_device_tab = 1;
        self.active_midimix = None;
        self.last_midimix_drag = 0;
        self.korg_knobs = [64; 8];
        self.korg_faders = [64; 8];
        self.korg_buttons = [false; 32];
        self.korg_wheel = 64;
        self.active_korg = None;
        self.last_korg_drag = 0;
        // self.drums_enabled = true;
        // self.drums_enabled_bef = true;

        // Initialize fans in the end.
        // bpf::system("sudo fans on");
        self.set_fans(true);
    }

    pub fn set_fans(&mut self, v: bool) {
        let _ = bpf::system(&format!("sudo fans {}", if v { "on" } else { "off" }));
        self.fans = v;
    }

    pub fn run(&mut self, input: TickInput) {
        self.handle_plugin_ui_events(&input);
        self.last_ui_clock = input.clock;
        if self.active_midimix.is_some()
            && input.clock.saturating_sub(self.last_midimix_drag) > 150
        {
            self.active_midimix = None;
        }
        if self.active_apc_fader.is_some()
            && input.clock.saturating_sub(self.last_apc_drag) > 150
        {
            self.active_apc_fader = None;
        }
        if self.active_korg.is_some() && input.clock.saturating_sub(self.last_korg_drag) > 150 {
            self.active_korg = None;
        }

        self.render_ui(input.clock);

        let state = bpf::get_dmx();
        if !state.scenes.is_empty() && self.intensity_mapping.is_empty() {
            println!("RUNNING INIT for intensity");
            self.intensity_mapping = vec![];

            let mut index = 0;

            for _ in 0..state.scenes.len() {
                for (scene_id, scene) in state.scenes.iter() {
                    let char_to_test = index.to_string().chars().nth(0).unwrap();
                    let name_char = scene.name.chars().nth(0).unwrap();
                    // println!("TESTING INDEX {index} and scene {scene_id} | CHAR: {char_to_test} vs {name_char}");
                    if !scene.name.is_empty() && name_char == char_to_test {
                        self.intensity_mapping.push(*scene_id);
                        println!("added intensity {index} --> Scene {scene_id}");
                        index += 1;
                        continue;
                    }
                }
            }

            if self.intensity_mapping.is_empty() {
                self.intensity_mapping.push(0)
            }

            println!("INTENSITY: {:?}", self.intensity_mapping);
        }

        // let state = unsafe {
        //     #[allow(static_mut_refs)]
        //     STATE.assume_init_mut()
        // };

        // if input.audio_data.bass_avg < 70 && !self.arm_drop {
        //     println!("ARMED DROP");
        //     self.arm_drop = true;
        //     self.arm_drop_timer = input.clock;
        // }

        // if self.arm_drop {
        // if input.clock - state.arm_drop_timer > 1000 {
        //     match dmx.current_scene_focus == 3 {
        //         true => {
        //             bl_send(ControlEvent::SetSceneFocus(5)); // HYPE UP
        //         }
        //         false => {
        //             bl_send(ControlEvent::SetSceneFocus(3)); // HYPE UP
        //         }
        //     };
        //
        //     state.arm_drop_timer = input.clock;
        // }
        // }

        // Drop detection and so on.
        // match input.audio_data.
        //     if self.arm_drop
        //     // && input.audio_data.bpm != 0
        //     && input.audio_data.bass_avg_short > 200
        // && dmx.current_scene_focus != STROBE_SCENE
        // && !self.strobe_auto_enable
        //     {
        //         self.strobe_enable_time = input.clock;
        //         self.strobe_auto_enable = true;
        //         bpf::send_event(ControlEvent::SetSceneFocus(STROBE_SCENE));
        //         println!("enable strobe");
        //     } else {
        //         // println!(
        //         //     "{} {}",
        //         //     input.audio_data.bpm, input.audio_data.bass_avg_short
        //         // );
        //     }

        //     if input.clock - self.strobe_enable_time > 3500 && self.strobe_auto_enable {
        //         bpf::send_event(ControlEvent::SetSceneFocus(0));
        //         println!("disable strobe");
        //         self.strobe_auto_enable = false;
        //         self.arm_drop = false;
        //     }

        // if dmx.current_scene_focus != STROBE_SCENE {
        //     state.strobe_auto_enable = false;
        // }

        // println!("{:?}", input.audio_data);

        // if self.disable_fog && input.clock - self.enable_fog_time > 2000 {
        //     bpf::send_event(ControlEvent::Transaction(vec![
        //         ControlEvent::PushSelection,
        //         ControlEvent::SelectGroup(2),
        //         ControlEvent::SetAlpha(0),
        //         ControlEvent::PopSelection,
        //     ]));
        //     self.disable_fog = false;
        // }

        // if self.activate_wall == 1 {
        //     set_brightness_internal(0);
        //     self.activate_wall = 0;
        // }

        // if self.activate_wall == 2 {
        //     set_brightness_internal(100);
        //     self.activate_wall = 1;
        // }

        // if input.clock > 60000 {
        //     let dmx = get_dmx();
        //
        //     println!("{dmx:?}");
        //     panic!("");
        // }

        // return;

        for ev in &input.events.events {
            match ev.body() {
                ControlEvent::MainUi(MainUiEvent::NavigatePage(page)) => {
                    self.current_app_page = Some(page.clone());
                    self.page_update_from_ui = true;
                    self.pending_app_page_sync = true;
                }
                _ => {}
            }

            println!("---> EVENT: {ev:?}");
        }

        let handles = self.midi_handles.clone();
        for (dev, handle) in handles {
            let res = handle.poll();

            match dev {
                MidiDevice::MidiMix => self.midimix(handle, res),
                MidiDevice::APCMini => self.apc(handle, res, input.clone()),
            }
        }

        //     match ev {
        //         ControlEvent::SelectGroup(42) => {
        //             state.midi_handle.send(151, 4, 127);
        //             state.enabled = true;
        //         }
        //         ControlEvent::DeSelectGroup(42) => {
        //             state.midi_handle.send(151, 4, 0);
        //             state.enabled = false;
        //         }
        //         _ => {}
        //     }
        // }

        // let res = state.midi_handle.poll();
        // let speed = 0.1;

        // if !res.is_empty() {
        //     // println!("res {res:?}");
        //     for sig in res {
        //         if sig.status == 151 && sig.kind == 4 && sig.value == 127 {
        //             // state.midi_handle.send(151, 4, state.enabled as u8 * 127);
        //             state.enabled = !state.enabled;
        //             bl_send(if state.enabled {
        //                 ControlEvent::SelectGroup(42)
        //             } else {
        //                 ControlEvent::DeSelectGroup(42)
        //             });
        //         }

        //         if sig.status == 176 && sig.kind == 34 {
        //             if sig.value == 65 {
        //                 state.counter += speed;
        //             } else if sig.value == 63 {
        //                 state.counter -= speed;
        //             }

        //             println!("counter: {}", state.counter as u32);
        //         }
        //     }
        // }

        // state.midi_handle.send(176, 90, state.counter as u8 % 127);
    }

    fn midimix(&mut self, conn: MidiConnection, ev: Vec<MidiEvent>) {
        for e in ev {
            match (e.status, e.kind, e.value) {
                (176, kind, value) => {
                    self.handle_midimix_cc(kind, value);
                }
                (144, 1, 127) => {
                    self.enabled = !self.enabled;
                    conn.send(144, 1, self.counter as u8 % 127);
                    self.counter += 1.0;
                }
                (128, 1, 127) => {
                    // conn.send(144, 1, 0);
                }
                _ => {
                    println!("{}: {:?}", conn.get_meta().device_id, e);
                }
            }
        }
    }

    fn apc(&mut self, conn: MidiConnection, ev: Vec<MidiEvent>, input: TickInput) {
        if self.is_apc_init {
            for i in 0..64 {
                conn.send(0x96, i as u8, 0);
            }

            self.is_apc_init = false;
        }

        // if self.drums_enabled != self.drums_enabled_bef {
        //     self.sync_drums_enabled(conn);
        // }

        // if ev.is_empty() {
        //     return;
        // }

        // if input.clock - state.last_update > 0 {
        //     // for i in 0..64 {
        //     conn.send(0x96, 56 as u8, (state.counter as usize % 127) as u8);
        //     // }
        //     state.counter += 1.0;
        //     state.last_update = input.clock;
        // }

        // if input.clock - state.last_update > 0 {
        //     // for i in 0..64 {
        //     for s in SCENES {
        //         conn.send(0x96, s as u8, 0);
        //     }
        //     conn.send(0x96, 56 as u8, 100);
        //     // }
        //     state.counter += 1.0;
        //     state.last_update = input.clock;
        // }

        let scene = bpf::get_dmx().current_scene_focus;
        if scene != self.last_scene {
            // for i in 0..64 {
            for s in APC_SCENES {
                conn.send(0x96, s, 0);
            }

            if (scene as usize) < APC_SCENES.len() {
                conn.send(0x96, APC_SCENES[scene as usize], 10);

                for s in APC_SCENES_INT {
                    conn.send(0x96, s, 0);
                }

                if let Some(rev_mapped) = self.intensity_mapping.iter().position(|e| *e == scene) {
                    conn.send(0x96, APC_SCENES_INT[rev_mapped], 20);
                }

                // }
                // state.counter += 1.0;
                // state.last_update = input.clock;
                println!("sync scene");

                self.last_scene = scene;
            }
        }

        if self.current_app_page != self.last_app_page || self.pending_app_page_sync {
            self.sync_app_page(&conn);
        }

        for e in ev {
            match (e.status, e.kind, e.value) {
                (176, 52, val) => {
                    self.set_virtual_apc_fader_value(0, val);
                    bpf::send_event(ControlEvent::MiscEvent {
                        descriptor: SET_SPEED,
                        value: val,
                    });
                }
                (176, 53, val) => {
                    self.set_virtual_apc_fader_value(1, val);
                    bpf::send_event(ControlEvent::MiscEvent {
                        descriptor: SET_FRY,
                        value: val,
                    });
                }
                (176, 54, val) => {
                    self.set_virtual_apc_fader_value(2, val);
                    bpf::send_event(ControlEvent::MiscEvent {
                        descriptor: SET_ROTATION,
                        value: val,
                    });
                }
                (176, 55, val) => {
                    self.set_virtual_apc_fader_value(3, val);
                    println!("val");
                    bpf::send_event(ControlEvent::MiscEvent {
                        descriptor: SET_BRIGHTNESS,
                        value: val,
                    });
                }
                (144, scene, 127) if APC_SCENES_INT.contains(&scene) => {
                    self.virtual_apc_last_pad = Some((scene, input.clock));
                    let normal_index = APC_SCENES_INT
                        .iter()
                        .position(|v| *v == scene)
                        .unwrap();
                    println!("INTENSITY: normal_index={normal_index}, scene={scene}");
                    let mapped_index = self.intensity_mapping[normal_index];

                    let dmx = bpf::get_dmx();

                    if !dmx.scenes.contains_key(&mapped_index) {
                        println!("E: no such scene");
                        return;
                    }

                    bpf::send_event(ControlEvent::SetSceneFocus(mapped_index));
                }
                (144, scene, 127) if APC_SCENES.contains(&scene) => {
                    self.virtual_apc_last_pad = Some((scene, input.clock));
                    let index = APC_SCENES.iter().position(|v| *v == scene).unwrap() as u8;

                    let dmx = bpf::get_dmx();

                    if !dmx.scenes.contains_key(&index) {
                        println!("E: no such scene");
                        return;
                    }

                    bpf::send_event(ControlEvent::SetSceneFocus(index));
                }
                (144, page, 127) => {
                    if page % 8 == 7 {
                        self.virtual_apc_last_pad = Some((page, input.clock));
                        if let Some(app_page) = Self::app_page_from_pad(page) {
                            self.page_update_from_ui = false;
                            self.pending_app_page_sync = false;
                            self.current_app_page = Some(app_page.clone());
                            self.sync_app_page(&conn);
                            bpf::send_event(ControlEvent::MainUi(MainUiEvent::NavigatePage(app_page)));
                        }
                    }
                }
                (128, pad, _) if APC_VIDEO_PADS.contains(&pad) => {
                    self.virtual_apc_last_pad = Some((pad, input.clock));
                    if let Some(idx) = APC_VIDEO_PADS.iter().position(|v| *v == pad) {
                        bpf::send_event(ControlEvent::MiscEvent {
                            descriptor: SET_VIDEO_INDEX,
                            value: idx as u8,
                        });
                    }
                }
                (176, 56, val) => {
                    // Allow requesting status refresh from a spare knob
                    if val == 127 {
                        bpf::send_event(ControlEvent::MiscEvent {
                            descriptor: REQUEST_STATUS_REFRESH,
                            value: 1,
                        });
                    }
                }
                _ => {
                    println!("{}: {:?}", conn.get_meta().device_id, e);
                }
            }
        }
    }

    fn handle_plugin_ui_events(&mut self, input: &TickInput) {
        for ev in &input.events.events {
            let ControlEvent::PluginUi(ui_ev, pid) = ev.body() else {
                continue;
            };
            if pid != input.id {
                continue;
            }

            match ui_ev {
                PluginUiEvent::Switch { id, value } if id == FAN_SWITCH_ID => {
                    self.set_fans(value);
                }
                PluginUiEvent::CanvasClick { id, x, y } if id == VIRTUAL_APC_CANVAS_ID => {
                    self.handle_virtual_apc_canvas_click(x, y, input);
                }
                PluginUiEvent::CanvasDrag { id, x, y, .. } if id == VIRTUAL_APC_CANVAS_ID => {
                    self.handle_virtual_apc_canvas_drag(x, y, input.clock);
                }
                PluginUiEvent::CanvasClick { id, x, y } if id == VIRTUAL_MIDIMIX_CANVAS_ID => {
                    self.handle_virtual_midimix_canvas_click(x, y, input.clock);
                }
                PluginUiEvent::CanvasDrag { id, x, y, .. } if id == VIRTUAL_MIDIMIX_CANVAS_ID => {
                    self.handle_virtual_midimix_canvas_drag(x, y, input.clock);
                }
                PluginUiEvent::CanvasClick { id, x, y } if id == VIRTUAL_KORG_CANVAS_ID => {
                    self.handle_virtual_korg_canvas_click(x, y, input.clock);
                }
                PluginUiEvent::CanvasDrag { id, x, y, .. } if id == VIRTUAL_KORG_CANVAS_ID => {
                    self.handle_virtual_korg_canvas_drag(x, y, input.clock);
                }
                PluginUiEvent::TabChanged { tabs_id, tab_id }
                    if tabs_id == VIRTUAL_DEVICE_TABS_ID =>
                {
                    self.virtual_device_tab = tab_id;
                }
                _ => {}
            }
        }
    }

    fn handle_virtual_apc_canvas_click(&mut self, x: i32, y: i32, input: &TickInput) {
        let layout = VirtualApcLayout::new();

        if let Some(idx) = layout.top_button_at(x, y) {
            self.virtual_apc_top_buttons[idx] = !self.virtual_apc_top_buttons[idx];
            self.active_apc_fader = None;
            return;
        }

        if let Some((row, col)) = layout.grid_pad_at(x, y) {
            let note = layout.grid_note(row, col);
            self.virtual_apc_last_pad = Some((note, input.clock));
            self.handle_virtual_apc_note(note, input);
            self.active_apc_fader = None;
            return;
        }

        if let Some(row) = layout.right_pad_at(x, y) {
            let note = 63u8.saturating_sub((row as u8) * 8);
            self.virtual_apc_last_pad = Some((note, input.clock));
            self.handle_virtual_apc_note(note, input);
            self.active_apc_fader = None;
            return;
        }

        if let Some(fader) = layout.fader_at(x, y) {
            let value = layout.fader_value_from_y(y);
            self.active_apc_fader = Some(fader);
            self.last_apc_drag = input.clock;
            self.apply_virtual_apc_fader(fader, value);
            return;
        }
        self.active_apc_fader = None;
    }

    fn handle_virtual_apc_canvas_drag(&mut self, x: i32, y: i32, now: u32) {
        let layout = VirtualApcLayout::new();
        if let Some(active) = self.active_apc_fader {
            let value = layout.fader_value_from_y(y);
            self.last_apc_drag = now;
            self.apply_virtual_apc_fader(active, value);
            return;
        }
        if let Some(fader) = layout.fader_at(x, y) {
            let value = layout.fader_value_from_y(y);
            self.active_apc_fader = Some(fader);
            self.last_apc_drag = now;
            self.apply_virtual_apc_fader(fader, value);
        }
    }

    fn handle_virtual_midimix_canvas_click(&mut self, x: i32, y: i32, now: u32) {
        let layout = VirtualMidimixLayout::new();
        if let Some((idx, cx, cy)) = layout.knob_hit(x, y) {
            let value = midimix_angle_to_value((y - cy) as f32, (x - cx) as f32);
            self.active_midimix = Some(MidimixDragTarget::Knob(idx));
            self.last_midimix_drag = now;
            self.apply_virtual_midimix_knob(idx, value);
            return;
        }
        if let Some(fader) = layout.fader_at(x, y) {
            let value = layout.fader_value_from_y(y);
            self.active_midimix = Some(MidimixDragTarget::Fader(fader));
            self.last_midimix_drag = now;
            self.apply_virtual_midimix_fader(fader, value);
            return;
        }
        self.active_midimix = None;
    }

    fn handle_virtual_midimix_canvas_drag(&mut self, x: i32, y: i32, now: u32) {
        let layout = VirtualMidimixLayout::new();
        if let Some(active) = self.active_midimix {
            match active {
                MidimixDragTarget::Knob(idx) => {
                    let (cx, cy) = layout.knob_center(idx / VIRTUAL_MIDIMIX_KNOB_COLS, idx % VIRTUAL_MIDIMIX_KNOB_COLS);
                    let value = midimix_angle_to_value((y - cy) as f32, (x - cx) as f32);
                    self.last_midimix_drag = now;
                    self.apply_virtual_midimix_knob(idx, value);
                    return;
                }
                MidimixDragTarget::Fader(idx) => {
                    let value = layout.fader_value_from_y(y);
                    self.last_midimix_drag = now;
                    self.apply_virtual_midimix_fader(idx, value);
                    return;
                }
            }
        }
        if let Some((idx, cx, cy)) = layout.knob_hit(x, y) {
            let value = midimix_angle_to_value((y - cy) as f32, (x - cx) as f32);
            self.active_midimix = Some(MidimixDragTarget::Knob(idx));
            self.last_midimix_drag = now;
            self.apply_virtual_midimix_knob(idx, value);
            return;
        }
        if let Some(fader) = layout.fader_at(x, y) {
            let value = layout.fader_value_from_y(y);
            self.active_midimix = Some(MidimixDragTarget::Fader(fader));
            self.last_midimix_drag = now;
            self.apply_virtual_midimix_fader(fader, value);
        }
    }

    fn handle_virtual_korg_canvas_click(&mut self, x: i32, y: i32, now: u32) {
        let layout = VirtualKorgLayout::new();

        if let Some((row, col)) = layout.button_at(x, y) {
            let idx = row * VIRTUAL_KORG_COLS + col;
            if idx < self.korg_buttons.len() {
                self.korg_buttons[idx] = !self.korg_buttons[idx];
            }
            self.active_korg = None;
            return;
        }

        if let Some((cx, cy)) = layout.wheel_hit(x, y) {
            let value = midimix_angle_to_value((y - cy) as f32, (x - cx) as f32);
            self.active_korg = Some(KorgDragTarget::Wheel);
            self.last_korg_drag = now;
            self.korg_wheel = value;
            return;
        }

        if let Some((idx, cx, cy)) = layout.knob_hit(x, y) {
            let value = midimix_angle_to_value((y - cy) as f32, (x - cx) as f32);
            self.active_korg = Some(KorgDragTarget::Knob(idx));
            self.last_korg_drag = now;
            self.apply_virtual_korg_knob(idx, value);
            return;
        }

        if let Some(fader) = layout.fader_at(x, y) {
            let value = layout.fader_value_from_y(y);
            self.active_korg = Some(KorgDragTarget::Fader(fader));
            self.last_korg_drag = now;
            self.apply_virtual_korg_fader(fader, value);
            return;
        }

        self.active_korg = None;
    }

    fn handle_virtual_korg_canvas_drag(&mut self, x: i32, y: i32, now: u32) {
        let layout = VirtualKorgLayout::new();
        if let Some(active) = self.active_korg {
            match active {
                KorgDragTarget::Knob(idx) => {
                    let (cx, cy) = layout.knob_center(idx);
                    let value = midimix_angle_to_value((y - cy) as f32, (x - cx) as f32);
                    self.last_korg_drag = now;
                    self.apply_virtual_korg_knob(idx, value);
                    return;
                }
                KorgDragTarget::Fader(idx) => {
                    let value = layout.fader_value_from_y(y);
                    self.last_korg_drag = now;
                    self.apply_virtual_korg_fader(idx, value);
                    return;
                }
                KorgDragTarget::Wheel => {
                    let value = midimix_angle_to_value(
                        (y - layout.wheel_cy) as f32,
                        (x - layout.wheel_cx) as f32,
                    );
                    self.last_korg_drag = now;
                    self.korg_wheel = value;
                    return;
                }
            }
        }

        if let Some((cx, cy)) = layout.wheel_hit(x, y) {
            let value = midimix_angle_to_value((y - cy) as f32, (x - cx) as f32);
            self.active_korg = Some(KorgDragTarget::Wheel);
            self.last_korg_drag = now;
            self.korg_wheel = value;
            return;
        }

        if let Some((idx, cx, cy)) = layout.knob_hit(x, y) {
            let value = midimix_angle_to_value((y - cy) as f32, (x - cx) as f32);
            self.active_korg = Some(KorgDragTarget::Knob(idx));
            self.last_korg_drag = now;
            self.apply_virtual_korg_knob(idx, value);
            return;
        }

        if let Some(fader) = layout.fader_at(x, y) {
            let value = layout.fader_value_from_y(y);
            self.active_korg = Some(KorgDragTarget::Fader(fader));
            self.last_korg_drag = now;
            self.apply_virtual_korg_fader(fader, value);
        }
    }

    fn handle_virtual_apc_note(&mut self, note: u8, _input: &TickInput) {
        if APC_SCENES_INT.contains(&note) {
            let normal_index = APC_SCENES_INT.iter().position(|v| *v == note).unwrap();
            if normal_index >= self.intensity_mapping.len() {
                return;
            }
            let mapped_index = self.intensity_mapping[normal_index];

            let dmx = bpf::get_dmx();
            if !dmx.scenes.contains_key(&mapped_index) {
                println!("E: no such scene");
                return;
            }

            bpf::send_event(ControlEvent::SetSceneFocus(mapped_index));
            return;
        }

        if APC_SCENES.contains(&note) {
            let index = APC_SCENES.iter().position(|v| *v == note).unwrap() as u8;
            let dmx = bpf::get_dmx();
            if !dmx.scenes.contains_key(&index) {
                println!("E: no such scene");
                return;
            }
            bpf::send_event(ControlEvent::SetSceneFocus(index));
            return;
        }

        if note % 8 == 7 {
            if let Some(app_page) = Self::app_page_from_pad(note) {
                self.page_update_from_ui = false;
                self.pending_app_page_sync = false;
                self.current_app_page = Some(app_page.clone());
                bpf::send_event(ControlEvent::MainUi(MainUiEvent::NavigatePage(app_page)));
                return;
            }
        }

        if APC_VIDEO_PADS.contains(&note) {
            if let Some(idx) = APC_VIDEO_PADS.iter().position(|v| *v == note) {
                bpf::send_event(ControlEvent::MiscEvent {
                    descriptor: SET_VIDEO_INDEX,
                    value: idx as u8,
                });
            }
        }
    }

    fn set_virtual_apc_fader_value(&mut self, index: usize, value: u8) {
        if index < self.virtual_apc_faders.len() {
            self.virtual_apc_faders[index] = value;
        }
    }

    fn apply_virtual_apc_fader(&mut self, index: usize, value: u8) {
        if index >= self.virtual_apc_faders.len() {
            return;
        }
        if self.virtual_apc_faders[index] == value {
            return;
        }
        self.virtual_apc_faders[index] = value;

        match index {
            0 => bpf::send_event(ControlEvent::MiscEvent {
                descriptor: SET_SPEED,
                value,
            }),
            1 => bpf::send_event(ControlEvent::MiscEvent {
                descriptor: SET_FRY,
                value,
            }),
            2 => bpf::send_event(ControlEvent::MiscEvent {
                descriptor: SET_ROTATION,
                value,
            }),
            3 => bpf::send_event(ControlEvent::MiscEvent {
                descriptor: SET_BRIGHTNESS,
                value,
            }),
            _ => {}
        }
    }

    fn handle_midimix_cc(&mut self, kind: u8, value: u8) {
        if (16..=39).contains(&kind) {
            let index = (kind - 16) as usize;
            if index < self.midimix_knobs.len() {
                self.midimix_knobs[index] = value;
            }
        } else if (48..=55).contains(&kind) {
            let index = (kind - 48) as usize;
            if index < self.midimix_faders.len() {
                self.midimix_faders[index] = value;
            }
        } else if kind == 62 {
            self.midimix_faders[8] = value;
        }

        match kind {
            19 => {
                bpf::send_event(ControlEvent::SetAlpha(
                    (value as u16).map_range(0..127, 0..255) as u8,
                ));
            }
            23 => {
                bpf::send_event(ControlEvent::SetStrobeSpeed(
                    (value as u16).map_range(0..127, 0..255) as u8,
                ));
            }
            27 => {
                bpf::send_event(ControlEvent::SetFocus(
                    (value as u16).map_range(0..127, 0..255) as u8,
                ));
            }
            31 => {
                bpf::send_event(ControlEvent::SetTilt(
                    (value as u16).map_range(0..127, 0..255) as u8,
                ));
            }
            49 => {
                bpf::send_event(ControlEvent::SetPan(
                    (value as u16).map_range(0..127, 0..255) as u8,
                ));
            }
            16 => {
                let val = (value as u16).map_range(0..127, 0..360);
                bpf::send_event(ControlEvent::SetColorHue(val));
            }
            17 => {
                let val = (value as u16).map_range(0..127, 0..255);
                bpf::send_event(ControlEvent::SetColorSaturation(val as u8));
            }
            18 => {
                let val = (value as u16).map_range(0..127, 0..255);
                bpf::send_event(ControlEvent::SetColorValue(val as u8));
            }
            _ => {}
        }
    }

    fn apply_virtual_midimix_knob(&mut self, index: usize, value: u8) {
        if index >= self.midimix_knobs.len() {
            return;
        }
        if self.midimix_knobs[index] == value {
            return;
        }
        self.midimix_knobs[index] = value;
        let kind = 16u8.saturating_add(index as u8);
        self.handle_midimix_cc(kind, value);
    }

    fn apply_virtual_midimix_fader(&mut self, index: usize, value: u8) {
        if index >= self.midimix_faders.len() {
            return;
        }
        if self.midimix_faders[index] == value {
            return;
        }
        self.midimix_faders[index] = value;
        let kind = if index == 8 {
            62
        } else {
            48u8.saturating_add(index as u8)
        };
        self.handle_midimix_cc(kind, value);
    }

    fn korg_scene_ids() -> Vec<u8> {
        let dmx = bpf::get_dmx();
        let mut scenes = vec![dmx.current_scene_focus];
        scenes.extend_from_slice(&dmx.current_overlay_scenes);
        scenes
    }

    fn apply_virtual_korg_fader(&mut self, index: usize, value: u8) {
        if index >= self.korg_faders.len() {
            return;
        }
        if self.korg_faders[index] == value {
            return;
        }
        self.korg_faders[index] = value;
        let scenes = Self::korg_scene_ids();
        let Some(scene_id) = scenes.get(index) else {
            return;
        };
        let alpha = (value as u16).map_range(0..127, 0..100) as u8;
        bpf::send_event(ControlEvent::SetSceneMasterAlpha(*scene_id, alpha));
    }

    fn apply_virtual_korg_knob(&mut self, index: usize, value: u8) {
        if index >= self.korg_knobs.len() {
            return;
        }
        if self.korg_knobs[index] == value {
            return;
        }
        self.korg_knobs[index] = value;
        let scenes = Self::korg_scene_ids();
        let Some(scene_id) = scenes.get(index) else {
            return;
        };
        let max_index = AnimationSpeedModifier::ALL.len() as u16 - 1;
        let speed_index = (value as u16).map_range(0..127, 0..max_index) as usize;
        let speed = AnimationSpeedModifier::from_index(speed_index);
        bpf::send_event(ControlEvent::SetSceneMasterSpeed(*scene_id, speed));
    }

    fn render_virtual_apc(&self, now: u32) {
        let layout = VirtualApcLayout::new();
        let dmx = bpf::get_dmx();
        let active_scene_pad = APC_SCENES
            .get(dmx.current_scene_focus as usize)
            .copied();
        let active_intensity_pad = self
            .intensity_mapping
            .iter()
            .position(|e| *e == dmx.current_scene_focus)
            .and_then(|idx| APC_SCENES_INT.get(idx).copied());
        let active_page_pad = self
            .current_app_page
            .as_ref()
            .and_then(|page| Self::pad_for_app_page(page));

        ui::label("APC mini (virtual)");
        ui::painter_begin(VIRTUAL_APC_CANVAS_ID, VIRTUAL_APC_WIDTH, VIRTUAL_APC_HEIGHT);

        ui::painter_rect(0, 0, VIRTUAL_APC_WIDTH, VIRTUAL_APC_HEIGHT, 25, 25, 25, 255);
        ui::painter_rect(0, 0, 10, VIRTUAL_APC_HEIGHT, 160, 0, 0, 255);
        ui::painter_rect(
            VIRTUAL_APC_WIDTH - 10,
            0,
            10,
            VIRTUAL_APC_HEIGHT,
            160,
            0,
            0,
            255,
        );

        for row in 0..8 {
            for col in 0..8 {
                let (x, y, w, h) = layout.grid_pad_rect(row, col);
                let note = layout.grid_note(row, col);
                let mut color = (70, 70, 70, 255);

                if let Some((last, at)) = self.virtual_apc_last_pad {
                    if last == note && now.saturating_sub(at) < 200 {
                        color = (200, 190, 90, 255);
                    }
                }

                if Some(note) == active_page_pad {
                    color = (200, 110, 40, 255);
                } else if Some(note) == active_intensity_pad {
                    color = (90, 140, 200, 255);
                } else if Some(note) == active_scene_pad {
                    color = (80, 170, 90, 255);
                }

                ui::painter_rect(x, y, w, h, color.0, color.1, color.2, color.3);
                ui::painter_rect_stroke(x, y, w, h, 35, 35, 35, 255, 1);
            }
        }

        for row in 0..8 {
            let (x, y, w, h) = layout.right_pad_rect(row);
            let note = 63u8.saturating_sub((row as u8) * 8);
            let mut color = (60, 60, 60, 255);

            if let Some((last, at)) = self.virtual_apc_last_pad {
                if last == note && now.saturating_sub(at) < 200 {
                    color = (200, 190, 90, 255);
                }
            }
            if Some(note) == active_page_pad {
                color = (200, 110, 40, 255);
            }

            ui::painter_rect(x, y, w, h, color.0, color.1, color.2, color.3);
            ui::painter_rect_stroke(x, y, w, h, 30, 30, 30, 255, 1);
        }

        for i in 0..8 {
            let (x, y, w, h) = layout.top_button_rect(i);
            let active = self.virtual_apc_top_buttons[i];
            let color = if active {
                (160, 160, 160, 255)
            } else {
                (55, 55, 55, 255)
            };
            ui::painter_rect(x, y, w, h, color.0, color.1, color.2, color.3);
            ui::painter_rect_stroke(x, y, w, h, 25, 25, 25, 255, 1);
        }

        for i in 0..9 {
            let (x, y, w, h) = layout.fader_track_rect(i);
            ui::painter_rect(x, y, w, h, 35, 35, 35, 255);

            let value = self.virtual_apc_faders[i];
            let handle_y = layout.fader_handle_y(value);
            ui::painter_rect(
                x - 2,
                handle_y,
                w + 4,
                8,
                190,
                190,
                190,
                255,
            );

            let label = match i {
                0 => "SPD",
                1 => "FRY",
                2 => "ROT",
                3 => "BRI",
                4 => "F5",
                5 => "F6",
                6 => "F7",
                7 => "F8",
                _ => "MST",
            };
            ui::painter_text(x - 2, y + h + 6, 10, 200, 200, 200, 255, label);
        }

        ui::painter_end();
        ui::separator();
    }

    fn render_virtual_midimix(&self) {
        let layout = VirtualMidimixLayout::new();

        ui::label("MIDI Mix (virtual)");
        ui::painter_begin(VIRTUAL_MIDIMIX_CANVAS_ID, VIRTUAL_MIDIMIX_WIDTH, VIRTUAL_MIDIMIX_HEIGHT);

        ui::painter_rect(0, 0, VIRTUAL_MIDIMIX_WIDTH, VIRTUAL_MIDIMIX_HEIGHT, 20, 20, 20, 255);
        ui::painter_rect(0, 0, 10, VIRTUAL_MIDIMIX_HEIGHT, 160, 0, 0, 255);
        ui::painter_rect(
            VIRTUAL_MIDIMIX_WIDTH - 10,
            0,
            10,
            VIRTUAL_MIDIMIX_HEIGHT,
            160,
            0,
            0,
            255,
        );

        for row in 0..VIRTUAL_MIDIMIX_KNOB_ROWS {
            for col in 0..VIRTUAL_MIDIMIX_KNOB_COLS {
                let idx = row * VIRTUAL_MIDIMIX_KNOB_COLS + col;
                let (cx, cy) = layout.knob_center(row, col);
                ui::painter_circle(
                    cx,
                    cy,
                    layout.knob_radius,
                    40,
                    40,
                    40,
                    255,
                );
                ui::painter_circle_stroke(
                    cx,
                    cy,
                    layout.knob_radius,
                    90,
                    90,
                    90,
                    255,
                    2,
                );

                let val = self.midimix_knobs.get(idx).copied().unwrap_or(64);
                let angle = midimix_knob_value_to_angle(val);
                let line_len = (layout.knob_radius - 4) as f32;
                let lx = cx as f32 + angle.cos() * line_len;
                let ly = cy as f32 + angle.sin() * line_len;
                ui::painter_line(
                    cx,
                    cy,
                    lx.round() as i32,
                    ly.round() as i32,
                    210,
                    210,
                    210,
                    255,
                    2,
                );
            }
        }

        for i in 0..VIRTUAL_MIDIMIX_FADER_COUNT {
            let (x, y, w, h) = layout.fader_track_rect(i);
            ui::painter_rect(x, y, w, h, 35, 35, 35, 255);
            ui::painter_rect_stroke(x, y, w, h, 55, 55, 55, 255, 1);

            let value = self.midimix_faders.get(i).copied().unwrap_or(64);
            let range = (h - 10).max(1);
            let handle_y = y + (range - (value as i32 * range / 127));
            ui::painter_rect(
                x - 4,
                handle_y,
                w + 8,
                8,
                180,
                180,
                180,
                255,
            );
        }

        ui::painter_end();
        ui::separator();
    }

    fn render_virtual_korg(&self) {
        let layout = VirtualKorgLayout::new();

        ui::label("Korg nanoKONTROL (virtual)");
        ui::painter_begin(VIRTUAL_KORG_CANVAS_ID, VIRTUAL_KORG_WIDTH, VIRTUAL_KORG_HEIGHT);

        ui::painter_rect(0, 0, VIRTUAL_KORG_WIDTH, VIRTUAL_KORG_HEIGHT, 18, 18, 18, 255);
        ui::painter_rect(0, 0, 6, VIRTUAL_KORG_HEIGHT, 160, 0, 0, 255);
        ui::painter_rect(VIRTUAL_KORG_WIDTH - 6, 0, 6, VIRTUAL_KORG_HEIGHT, 160, 0, 0, 255);
        ui::painter_rect(0, 0, VIRTUAL_KORG_WIDTH, 6, 160, 0, 0, 120);

        // Left-side button clusters (scene + transport)
        let left_x = 24;
        let left_y = 46;
        let btn_w = 26;
        let btn_h = 16;
        let btn_gap = 6;

        // Scene row (2 buttons)
        for col in 0..2 {
            let x = left_x + col * (btn_w + btn_gap);
            let y = left_y;
            ui::painter_rect(x, y, btn_w, btn_h, 40, 40, 40, 255);
            ui::painter_rect_stroke(x, y, btn_w, btn_h, 70, 70, 70, 255, 1);
        }

        // Marker row (3 buttons)
        let marker_y = left_y + btn_h + btn_gap;
        for col in 0..3 {
            let x = left_x + col * (btn_w + btn_gap);
            ui::painter_rect(x, marker_y, btn_w, btn_h, 45, 45, 45, 255);
            ui::painter_rect_stroke(x, marker_y, btn_w, btn_h, 80, 80, 80, 255, 1);
        }

        // Transport block (2 rows x 4)
        let t_y = marker_y + btn_h + btn_gap + 6;
        for row in 0..2 {
            for col in 0..4 {
                let x = left_x + col * (btn_w + btn_gap);
                let y = t_y + row * (btn_h + btn_gap);
                ui::painter_rect(x, y, btn_w, btn_h, 45, 45, 45, 255);
                ui::painter_rect_stroke(x, y, btn_w, btn_h, 80, 80, 80, 255, 1);
            }
        }

        // Jog wheel
        let wheel_cx = layout.wheel_cx;
        let wheel_cy = layout.wheel_cy;
        let wheel_r = layout.wheel_r - 8;
        ui::painter_circle(wheel_cx, wheel_cy, wheel_r, 28, 28, 28, 255);
        ui::painter_circle_stroke(wheel_cx, wheel_cy, wheel_r, 70, 70, 70, 255, 2);
        ui::painter_circle(wheel_cx, wheel_cy, 18, 18, 18, 18, 255);
        let jog_val = self.korg_wheel;
        let jog_angle = midimix_knob_value_to_angle(jog_val);
        let jog_r = 26.0;
        let jog_x = wheel_cx as f32 + jog_angle.cos() * jog_r;
        let jog_y = wheel_cy as f32 + jog_angle.sin() * jog_r;
        ui::painter_circle(jog_x.round() as i32, jog_y.round() as i32, 12, 24, 24, 24, 255);

        for row in 0..VIRTUAL_KORG_BUTTON_ROWS {
            for col in 0..VIRTUAL_KORG_COLS {
                let (x, y, w, h) = layout.button_rect(row, col);
                let idx = row * VIRTUAL_KORG_COLS + col;
                let active = self.korg_buttons.get(idx).copied().unwrap_or(false);
                let color = if active { (200, 200, 200, 255) } else { (45, 45, 45, 255) };
                ui::painter_rect(x, y, w, h, color.0, color.1, color.2, color.3);
                ui::painter_rect_stroke(x, y, w, h, 70, 70, 70, 255, 1);
            }
        }

        for col in 0..VIRTUAL_KORG_COLS {
            let idx = col;
            let (cx, cy) = layout.knob_center(idx);
            ui::painter_circle(cx, cy, VIRTUAL_KORG_KNOB_RADIUS, 40, 40, 40, 255);
            ui::painter_circle_stroke(cx, cy, VIRTUAL_KORG_KNOB_RADIUS, 90, 90, 90, 255, 2);

            let val = self.korg_knobs.get(idx).copied().unwrap_or(64);
            let angle = midimix_knob_value_to_angle(val);
            let line_len = (VIRTUAL_KORG_KNOB_RADIUS - 3) as f32;
            let lx = cx as f32 + angle.cos() * line_len;
            let ly = cy as f32 + angle.sin() * line_len;
            ui::painter_line(cx, cy, lx.round() as i32, ly.round() as i32, 210, 210, 210, 255, 2);
        }

        for col in 0..VIRTUAL_KORG_COLS {
            let (x, y, w, h) = layout.fader_track_rect(col);
            ui::painter_rect(x, y, w, h, 35, 35, 35, 255);
            ui::painter_rect_stroke(x, y, w, h, 55, 55, 55, 255, 1);

            let value = self.korg_faders.get(col).copied().unwrap_or(64);
            let range = (h - 10).max(1);
            let handle_y = y + (range - (value as i32 * range / 127));
            // Track markers
            let markers = 8;
            let step = (h - 2) / markers;
            for i in 0..=markers {
                let my = y + i * step;
                ui::painter_line(x - 6, my, x - 2, my, 120, 120, 120, 180, 1);
            }

            ui::painter_rect(x - 6, handle_y, w + 12, 10, 200, 200, 200, 255);
        }

        ui::painter_end();
        ui::separator();
    }

    pub fn render_ui(&self, now: u32) {
        ui::begin();
        ui::switch("Fans", FAN_SWITCH_ID, self.fans);
        ui::separator();
        ui::begin_tabs(VIRTUAL_DEVICE_TABS_ID);
        ui::begin_tab(VIRTUAL_DEVICE_TABS_ID, 1, "APC mini");
        if self.virtual_device_tab == 1 {
            self.render_virtual_apc(now);
        }
        ui::end_tab();
        ui::begin_tab(VIRTUAL_DEVICE_TABS_ID, 2, "MIDI Mix");
        if self.virtual_device_tab == 2 {
            self.render_virtual_midimix();
        }
        ui::end_tab();
        ui::begin_tab(VIRTUAL_DEVICE_TABS_ID, 3, "Korg nanoKONTROL");
        if self.virtual_device_tab == 3 {
            self.render_virtual_korg();
        }
        ui::end_tab();
        ui::end_tabs();
    }

    pub fn ui_clock(&self) -> u32 {
        self.last_ui_clock
    }

    // fn sync_drums_enabled(&mut self, conn: MidiConnection) {
    //     match self.drums_enabled {
    //         true => {
    //             conn.send(0x96, 63, 20);
    //         }
    //         false => {
    //             conn.send(0x96, 63, 0);
    //         }
    //     }
    //
    //     send_event(ControlEvent::MiscEvent {
    //         descriptor: 42,
    //         value: self.drums_enabled as u8,
    //     });
    //
    //     self.drums_enabled_bef = self.drums_enabled;
    //     println!("DRUMS SYNC");
    // }

    //
    // fn enable_strobe() {
    //     bl_send(ControlEvent::SetSceneFocus(STROBE_SCENE));
    // }
}

fn midimix_knob_value_to_angle(value: u8) -> f32 {
    let t = value as f32 / 127.0;
    (-2.6) + t * 5.2
}

fn midimix_angle_to_value(dy: f32, dx: f32) -> u8 {
    let angle = dy.atan2(dx).clamp(-2.6, 2.6);
    let t = (angle + 2.6) / 5.2;
    (t * 127.0).round().clamp(0.0, 127.0) as u8
}

#[derive(Clone, Copy)]
enum MidimixDragTarget {
    Knob(usize),
    Fader(usize),
}

#[derive(Clone, Copy)]
enum KorgDragTarget {
    Knob(usize),
    Fader(usize),
    Wheel,
}

#[derive(Clone, Copy)]
struct VirtualApcLayout {
    grid_x: i32,
    grid_y: i32,
    pad_size: i32,
    pad_gap: i32,
    grid_w: i32,
    grid_h: i32,
    right_x: i32,
    top_y: i32,
    top_h: i32,
    fader_top: i32,
    fader_bottom: i32,
    fader_w: i32,
    fader_xs: [i32; 9],
}

impl VirtualApcLayout {
    fn new() -> Self {
        let pad_size = VIRTUAL_APC_PAD_SIZE;
        let pad_gap = VIRTUAL_APC_PAD_GAP;
        let cell = pad_size + pad_gap;
        let grid_w = pad_size * 8 + pad_gap * 7;
        let grid_h = grid_w;
        let right_x = VIRTUAL_APC_GRID_X + grid_w + VIRTUAL_APC_RIGHT_GAP;
        let fader_top = VIRTUAL_APC_GRID_Y + grid_h + VIRTUAL_APC_FADER_TOP_MARGIN;
        let fader_bottom = VIRTUAL_APC_HEIGHT - VIRTUAL_APC_FADER_BOTTOM_MARGIN;

        let mut fader_xs = [0; 9];
        for col in 0..8 {
            fader_xs[col] = VIRTUAL_APC_GRID_X + col as i32 * cell + (pad_size - VIRTUAL_APC_FADER_WIDTH) / 2;
        }
        fader_xs[8] = right_x + (pad_size - VIRTUAL_APC_FADER_WIDTH) / 2;

        Self {
            grid_x: VIRTUAL_APC_GRID_X,
            grid_y: VIRTUAL_APC_GRID_Y,
            pad_size,
            pad_gap,
            grid_w,
            grid_h,
            right_x,
            top_y: VIRTUAL_APC_TOP_Y,
            top_h: VIRTUAL_APC_TOP_HEIGHT,
            fader_top,
            fader_bottom,
            fader_w: VIRTUAL_APC_FADER_WIDTH,
            fader_xs,
        }
    }

    fn cell(&self) -> i32 {
        self.pad_size + self.pad_gap
    }

    fn grid_note(&self, row: usize, col: usize) -> u8 {
        ((7 - row) as u8) * 8 + col as u8
    }

    fn grid_pad_rect(&self, row: usize, col: usize) -> (i32, i32, i32, i32) {
        let x = self.grid_x + col as i32 * self.cell();
        let y = self.grid_y + row as i32 * self.cell();
        (x, y, self.pad_size, self.pad_size)
    }

    fn right_pad_rect(&self, row: usize) -> (i32, i32, i32, i32) {
        let y = self.grid_y + row as i32 * self.cell();
        (self.right_x, y, self.pad_size, self.pad_size)
    }

    fn top_button_rect(&self, idx: usize) -> (i32, i32, i32, i32) {
        let x = self.grid_x + idx as i32 * self.cell();
        (x, self.top_y, self.pad_size, self.top_h)
    }

    fn fader_track_rect(&self, idx: usize) -> (i32, i32, i32, i32) {
        let x = self.fader_xs[idx];
        let h = self.fader_bottom - self.fader_top;
        (x, self.fader_top, self.fader_w, h)
    }

    fn fader_handle_y(&self, value: u8) -> i32 {
        let range = (self.fader_bottom - self.fader_top - 8).max(1);
        self.fader_bottom - 8 - (value as i32 * range / 127)
    }

    fn fader_value_from_y(&self, y: i32) -> u8 {
        let clamped = y.clamp(self.fader_top, self.fader_bottom);
        let range = (self.fader_bottom - self.fader_top).max(1);
        let ratio = (self.fader_bottom - clamped) as f32 / range as f32;
        (ratio * 127.0).round().clamp(0.0, 127.0) as u8
    }

    fn grid_pad_at(&self, x: i32, y: i32) -> Option<(usize, usize)> {
        if x < self.grid_x || y < self.grid_y {
            return None;
        }
        if x >= self.grid_x + self.grid_w || y >= self.grid_y + self.grid_h {
            return None;
        }

        let rel_x = x - self.grid_x;
        let rel_y = y - self.grid_y;
        let cell = self.cell();
        let col = (rel_x / cell) as usize;
        let row = (rel_y / cell) as usize;
        if col >= 8 || row >= 8 {
            return None;
        }
        if rel_x % cell >= self.pad_size || rel_y % cell >= self.pad_size {
            return None;
        }
        Some((row, col))
    }

    fn right_pad_at(&self, x: i32, y: i32) -> Option<usize> {
        if x < self.right_x || x >= self.right_x + self.pad_size {
            return None;
        }
        if y < self.grid_y || y >= self.grid_y + self.grid_h {
            return None;
        }
        let rel_y = y - self.grid_y;
        let cell = self.cell();
        let row = (rel_y / cell) as usize;
        if row >= 8 {
            return None;
        }
        if rel_y % cell >= self.pad_size {
            return None;
        }
        Some(row)
    }

    fn top_button_at(&self, x: i32, y: i32) -> Option<usize> {
        if y < self.top_y || y >= self.top_y + self.top_h {
            return None;
        }
        if x < self.grid_x || x >= self.grid_x + self.grid_w {
            return None;
        }
        let rel_x = x - self.grid_x;
        let cell = self.cell();
        let idx = (rel_x / cell) as usize;
        if idx >= 8 {
            return None;
        }
        if rel_x % cell >= self.pad_size {
            return None;
        }
        Some(idx)
    }

    fn fader_at(&self, x: i32, y: i32) -> Option<usize> {
        if y < self.fader_top || y > self.fader_bottom {
            return None;
        }
        for (idx, fx) in self.fader_xs.iter().enumerate() {
            if x >= *fx && x <= *fx + self.fader_w {
                return Some(idx);
            }
        }
        None
    }
}

#[derive(Clone, Copy)]
struct VirtualMidimixLayout {
    knob_radius: i32,
    knob_gap_x: i32,
    knob_gap_y: i32,
    knob_start_x: i32,
    knob_start_y: i32,
    fader_track_h: i32,
    fader_track_w: i32,
    fader_gap: i32,
    fader_start_x: i32,
    fader_start_y: i32,
}

impl VirtualMidimixLayout {
    fn new() -> Self {
        Self {
            knob_radius: VIRTUAL_MIDIMIX_KNOB_RADIUS,
            knob_gap_x: VIRTUAL_MIDIMIX_KNOB_GAP_X,
            knob_gap_y: VIRTUAL_MIDIMIX_KNOB_GAP_Y,
            knob_start_x: VIRTUAL_MIDIMIX_KNOB_START_X,
            knob_start_y: VIRTUAL_MIDIMIX_KNOB_START_Y,
            fader_track_h: VIRTUAL_MIDIMIX_FADER_TRACK_H,
            fader_track_w: VIRTUAL_MIDIMIX_FADER_TRACK_W,
            fader_gap: VIRTUAL_MIDIMIX_FADER_GAP,
            fader_start_x: VIRTUAL_MIDIMIX_FADER_START_X,
            fader_start_y: VIRTUAL_MIDIMIX_FADER_START_Y,
        }
    }

    fn knob_center(&self, row: usize, col: usize) -> (i32, i32) {
        let cx = self.knob_start_x + col as i32 * self.knob_gap_x;
        let cy = self.knob_start_y + row as i32 * self.knob_gap_y;
        (cx, cy)
    }

    fn knob_hit(&self, x: i32, y: i32) -> Option<(usize, i32, i32)> {
        for row in 0..VIRTUAL_MIDIMIX_KNOB_ROWS {
            for col in 0..VIRTUAL_MIDIMIX_KNOB_COLS {
                let (cx, cy) = self.knob_center(row, col);
                let dx = x - cx;
                let dy = y - cy;
                if dx * dx + dy * dy <= self.knob_radius * self.knob_radius {
                    let idx = row * VIRTUAL_MIDIMIX_KNOB_COLS + col;
                    return Some((idx, cx, cy));
                }
            }
        }
        None
    }

    fn fader_track_rect(&self, idx: usize) -> (i32, i32, i32, i32) {
        let x = self.fader_start_x + idx as i32 * self.fader_gap;
        (x, self.fader_start_y, self.fader_track_w, self.fader_track_h)
    }

    fn fader_at(&self, x: i32, y: i32) -> Option<usize> {
        for idx in 0..VIRTUAL_MIDIMIX_FADER_COUNT {
            let (fx, fy, fw, fh) = self.fader_track_rect(idx);
            if x >= fx - 6 && x <= fx + fw + 6 && y >= fy && y <= fy + fh {
                return Some(idx);
            }
        }
        None
    }

    fn fader_value_from_y(&self, y: i32) -> u8 {
        let top = self.fader_start_y;
        let bottom = self.fader_start_y + self.fader_track_h;
        let clamped = y.clamp(top, bottom);
        let range = (bottom - top).max(1);
        let ratio = (bottom - clamped) as f32 / range as f32;
        (ratio * 127.0).round().clamp(0.0, 127.0) as u8
    }
}

#[derive(Clone, Copy)]
struct VirtualKorgLayout {
    col_start_x: i32,
    col_gap: i32,
    button_start_y: i32,
    button_w: i32,
    button_h: i32,
    button_gap_y: i32,
    knob_y: i32,
    fader_start_y: i32,
    fader_h: i32,
    fader_w: i32,
    wheel_cx: i32,
    wheel_cy: i32,
    wheel_r: i32,
}

impl VirtualKorgLayout {
    fn new() -> Self {
        Self {
            col_start_x: VIRTUAL_KORG_COL_START_X,
            col_gap: VIRTUAL_KORG_COL_GAP,
            button_start_y: VIRTUAL_KORG_BUTTON_START_Y,
            button_w: VIRTUAL_KORG_BUTTON_W,
            button_h: VIRTUAL_KORG_BUTTON_H,
            button_gap_y: VIRTUAL_KORG_BUTTON_GAP_Y,
            knob_y: VIRTUAL_KORG_KNOB_Y,
            fader_start_y: VIRTUAL_KORG_FADER_START_Y,
            fader_h: VIRTUAL_KORG_FADER_H,
            fader_w: VIRTUAL_KORG_FADER_W,
            wheel_cx: 86,
            wheel_cy: 250,
            wheel_r: 52,
        }
    }

    fn col_x(&self, col: usize) -> i32 {
        self.col_start_x + col as i32 * self.col_gap
    }

    fn button_rect(&self, row: usize, col: usize) -> (i32, i32, i32, i32) {
        let x = self.col_x(col) - self.button_w / 2;
        let y = self.button_start_y + row as i32 * (self.button_h + self.button_gap_y);
        (x, y, self.button_w, self.button_h)
    }

    fn button_at(&self, x: i32, y: i32) -> Option<(usize, usize)> {
        for row in 0..VIRTUAL_KORG_BUTTON_ROWS {
            for col in 0..VIRTUAL_KORG_COLS {
                let (bx, by, bw, bh) = self.button_rect(row, col);
                if x >= bx && x <= bx + bw && y >= by && y <= by + bh {
                    return Some((row, col));
                }
            }
        }
        None
    }

    fn knob_center(&self, index: usize) -> (i32, i32) {
        let col = index.min(VIRTUAL_KORG_COLS - 1);
        (self.col_x(col), self.knob_y)
    }

    fn knob_hit(&self, x: i32, y: i32) -> Option<(usize, i32, i32)> {
        for col in 0..VIRTUAL_KORG_COLS {
            let (cx, cy) = self.knob_center(col);
            let dx = x - cx;
            let dy = y - cy;
            if dx * dx + dy * dy <= VIRTUAL_KORG_KNOB_RADIUS * VIRTUAL_KORG_KNOB_RADIUS {
                return Some((col, cx, cy));
            }
        }
        None
    }

    fn fader_track_rect(&self, idx: usize) -> (i32, i32, i32, i32) {
        let x = self.col_x(idx) - self.fader_w / 2;
        (x, self.fader_start_y, self.fader_w, self.fader_h)
    }

    fn fader_at(&self, x: i32, y: i32) -> Option<usize> {
        for idx in 0..VIRTUAL_KORG_COLS {
            let (fx, fy, fw, fh) = self.fader_track_rect(idx);
            if x >= fx - 6 && x <= fx + fw + 6 && y >= fy && y <= fy + fh {
                return Some(idx);
            }
        }
        None
    }

    fn fader_value_from_y(&self, y: i32) -> u8 {
        let top = self.fader_start_y;
        let bottom = self.fader_start_y + self.fader_h;
        let clamped = y.clamp(top, bottom);
        let range = (bottom - top).max(1);
        let ratio = (bottom - clamped) as f32 / range as f32;
        (ratio * 127.0).round().clamp(0.0, 127.0) as u8
    }

    fn wheel_hit(&self, x: i32, y: i32) -> Option<(i32, i32)> {
        let dx = x - self.wheel_cx;
        let dy = y - self.wheel_cy;
        if dx * dx + dy * dy <= self.wheel_r * self.wheel_r {
            Some((self.wheel_cx, self.wheel_cy))
        } else {
            None
        }
    }
}
