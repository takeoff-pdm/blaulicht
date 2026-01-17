use blaulicht_plugin_framework::{
    self as bpf, midi, println, send_event, ui, MidiConnection, MidiEvent,
};
use blaulicht_shared::{
    hsv_to_rgb,
    misc_event::videowall::{
        REQUEST_STATUS_REFRESH, SET_BRIGHTNESS, SET_FRY, SET_ROTATION, SET_SPEED, SET_VIDEO_INDEX,
    },
    AppPage, ControlEvent, ControlEventMessage, MainUiEvent, PluginUiEvent, TickInput,
};
use map_range::MapRange;
use std::{fmt::Display, mem::MaybeUninit};

const STROBE_SCENE: u8 = 2;
const APC_PAGE_PADS: [u8; 8] = [63, 55, 47, 39, 31, 23, 15, 7];

//
// MIDI start.
//

#[derive(Clone, Copy)]
enum MidiDevice {
    MidiMix,
    APCMini,
}

impl Display for MidiDevice {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "{}",
            match self {
                MidiDevice::MidiMix => "MIDI Mix",
                MidiDevice::APCMini => "APC mini mk2",
            }
        )
    }
}

impl MidiDevice {
    fn index(&self) -> usize {
        match self {
            MidiDevice::MidiMix => 0,
            MidiDevice::APCMini => 1,
        }
    }
}

//
// MIDI end.
//

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
            let midi_handle = MidiConnection::open(&name).unwrap();
            println!(
                "Got MIDI handle to device {dev} | HANDLE ID: {}",
                midi_handle.get_meta().device_id
            );

            midi_handles.push((dev, midi_handle));
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
        {
            ui::begin();
            let cid = 0;
            ui::checkbox("Fans", cid, self.fans);
            for ev in &input.events.events {
                match ev.body() {
                    ControlEvent::PluginUi(PluginUiEvent::Checkbox { id, checked }, pid) => {
                        if pid != input.id {
                            continue;
                        }
                        if cid == id {
                            self.set_fans(checked);
                        }
                    }
                    ControlEvent::MiscEvent { .. } => {}
                    _ => {}
                }
            }
        }

        let state = bpf::get_dmx();
        if state.scenes.len() > 0 && self.intensity_mapping.is_empty() {
            println!("RUNNING INIT for intensity");
            self.intensity_mapping = vec![];

            let mut index = 0;

            for _ in 0..state.scenes.len() {
                for (scene_id, scene) in state.scenes.iter() {
                    let char_to_test = index.to_string().chars().nth(0).unwrap();
                    let name_char = scene.name.chars().nth(0).unwrap();
                    // println!("TESTING INDEX {index} and scene {scene_id} | CHAR: {char_to_test} vs {name_char}");
                    if scene.name.len() > 0 && name_char == char_to_test {
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

        let dmx = bpf::get_dmx();

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
                (176, 19, v) => {
                    bpf::send_event(ControlEvent::SetAlpha(
                        (v as u16).map_range(0..127, 0..255) as u8
                    ));
                }
                (176, 23, v) => {
                    bpf::send_event(ControlEvent::SetStrobeSpeed(
                        (v as u16).map_range(0..127, 0..255) as u8,
                    ));
                }
                (176, 27, v) => {
                    bpf::send_event(ControlEvent::SetFocus(
                        (v as u16).map_range(0..127, 0..255) as u8
                    ));
                }
                (176, 31, v) => {
                    bpf::send_event(ControlEvent::SetTilt(
                        (v as u16).map_range(0..127, 0..255) as u8
                    ));
                }
                (176, 49, v) => {
                    bpf::send_event(ControlEvent::SetPan(
                        (v as u16).map_range(0..127, 0..255) as u8
                    ));
                }
                (144, 1, 127) => {
                    self.enabled = !self.enabled;
                    conn.send(144, 1, self.counter as u8 % 127);
                    self.counter += 1.0;
                }
                (176, 16, value) => {
                    let val = (value as u16).map_range(0..127, 0..360);
                    bpf::send_event(ControlEvent::SetColorHue(val));
                }
                (176, 17, value) => {
                    let val = (value as u16).map_range(0..127, 0..255);
                    bpf::send_event(ControlEvent::SetColorSaturation(val as u8));
                }
                (176, 18, value) => {
                    let val = (value as u16).map_range(0..127, 0..255);
                    bpf::send_event(ControlEvent::SetColorValue(val as u8));
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
        const SCENES: [u8; 8] = [56, 48, 40, 32, 24, 16, 8, 0];

        const SCENES_INT: [u8; 5] = [60, 52, 44, 36, 28];
        const VIDEO_PADS: [u8; 8] = [62, 54, 46, 38, 30, 22, 14, 6];

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
            for s in SCENES {
                conn.send(0x96, s, 0);
            }

            if (scene as usize) < SCENES.len() {
                conn.send(0x96, SCENES[scene as usize], 10);

                for s in SCENES_INT {
                    conn.send(0x96, s, 0);
                }

                if let Some(rev_mapped) = self.intensity_mapping.iter().position(|e| *e == scene) {
                    conn.send(0x96, SCENES_INT[rev_mapped], 20);
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
                    bpf::send_event(ControlEvent::MiscEvent {
                        descriptor: SET_SPEED,
                        value: val,
                    });
                }
                (176, 53, val) => {
                    bpf::send_event(ControlEvent::MiscEvent {
                        descriptor: SET_FRY,
                        value: val,
                    });
                }
                (176, 54, val) => {
                    bpf::send_event(ControlEvent::MiscEvent {
                        descriptor: SET_ROTATION,
                        value: val,
                    });
                }
                (176, 55, val) => {
                    println!("val");
                    bpf::send_event(ControlEvent::MiscEvent {
                        descriptor: SET_BRIGHTNESS,
                        value: val,
                    });
                }
                (144, scene, 127) if SCENES_INT.contains(&scene) => {
                    let normal_index = SCENES_INT.iter().position(|v| *v == scene).unwrap();
                    println!("INTENSITY: normal_index={normal_index}, scene={scene}");
                    let mapped_index = self.intensity_mapping[normal_index];

                    let dmx = bpf::get_dmx();

                    if !dmx.scenes.contains_key(&mapped_index) {
                        println!("E: no such scene");
                        return;
                    }

                    bpf::send_event(ControlEvent::SetSceneFocus(mapped_index));
                }
                (144, scene, 127) if SCENES.contains(&scene) => {
                    let index = SCENES.iter().position(|v| *v == scene).unwrap() as u8;

                    let dmx = bpf::get_dmx();

                    if !dmx.scenes.contains_key(&index) {
                        println!("E: no such scene");
                        return;
                    }

                    bpf::send_event(ControlEvent::SetSceneFocus(index));
                }
                (144, page, 127) => {
                    if let Some(app_page) = Self::app_page_from_pad(page) {
                        self.page_update_from_ui = false;
                        self.pending_app_page_sync = false;
                        self.current_app_page = Some(app_page.clone());
                        self.sync_app_page(&conn);
                        bpf::send_event(ControlEvent::MainUi(MainUiEvent::NavigatePage(app_page)));
                    }
                }
                (128, pad, _) if VIDEO_PADS.contains(&pad) => {
                    if let Some(idx) = VIDEO_PADS.iter().position(|v| *v == pad) {
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

    fn sync_app_page(&mut self, conn: &MidiConnection) {
        for pad in APC_PAGE_PADS {
            conn.send(0x96, pad, 0);
        }

        if let Some(ref page) = self.current_app_page {
            if let Some(pad) = Self::pad_for_app_page(page) {
                conn.send(0x96, pad, 10);
            }

            if !self.page_update_from_ui {
                bpf::send_event(ControlEvent::MainUi(MainUiEvent::NavigatePage(page.clone())));
            }
        }

        self.last_app_page = self.current_app_page.clone();
        self.page_update_from_ui = false;
        self.pending_app_page_sync = false;
    }

    fn app_page_from_pad(pad: u8) -> Option<AppPage> {
        match pad {
            63 => Some(AppPage::Logs),
            55 => Some(AppPage::System),
            47 => Some(AppPage::Audio),
            39 => Some(AppPage::FixturesSetup),
            31 => Some(AppPage::View),
            23 => Some(AppPage::ViewPerformance),
            15 => Some(AppPage::FixturesPerformance),
            7 => Some(AppPage::Animations),
            _ => None,
        }
    }

    fn pad_for_app_page(page: &AppPage) -> Option<u8> {
        let pad = match page {
            AppPage::Logs => 63,
            AppPage::System => 55,
            AppPage::Audio => 47,
            AppPage::FixturesSetup => 39,
            AppPage::View => 31,
            AppPage::ViewPerformance => 23,
            AppPage::FixturesPerformance => 15,
            AppPage::Animations => 7,
        };

        Some(pad)
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
