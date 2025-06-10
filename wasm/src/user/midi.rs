// use crate::blaulicht::{self, TickInput};

// use map_range::MapRange;

// use crate::{
//     blaulicht::{bl_controls_set, MidiEvent},
//     printc, smidi,
//     // user::state::{LeftFaderTarget, RightFaderTarget},
// };

// use super::{
//     dim, mood,
//     // state::{MoodAnimation, MoodColorPalette, StrobeAnimation, VideoFile},
//     strobe, video, State,
// };

// //
// // Web matrix.
// //


// pub const STROBE_ON_BEAT: (u8, u8) = (0, 0);
// pub const STROBE_TOGGLE: (u8, u8) = (1, 0);
// pub const STROBE_AUTOMATION_TOGGLE: (u8, u8) = (2, 0);
// pub const STROBE_REMAINING_TIME: (u8, u8) = (3, 0);
// pub const STROBE_BRIGHTNESS: (u8, u8) = (4, 0);
// pub const STROBE_SPEED: (u8, u8) = (5, 0);
// pub const STROBE_ON_OFF_TIME: (u8, u8) = (6, 0);
// pub const MOVING_HEAD_TILT_ANIM_SPEED: (u8, u8) = (5, 1);

// pub const STROBE_MOVING_HEAD_GROUP: (u8, u8) = (2, 1);
// pub const STROBE_STAGE_GROUP: (u8, u8) = (3, 1);
// pub const STROBE_PANEL_GROUP: (u8, u8) = (4, 1);

// pub const STROBE_SYNCED: (u8, u8) = (0, 1);
// pub const STROBE_ALTERNATING: (u8, u8) = (1, 1);

// pub const MOOD_ON_BEAT: (u8, u8) = (0, 2);
// pub const MOOD_FORCE_TOGGLE: (u8, u8) = (1, 2);
// pub const MOOD_BEAT_ANIM: (u8, u8) = (3, 2);
// pub const MOOD_BRIGHTNESS: (u8, u8) = (4, 2);
// pub const MOOD_SPEED: (u8, u8) = (5, 2);
// pub const MOOD_SPEED_BEAT: (u8, u8) = (6, 2);

// pub const MOOD_SYNCED: (u8, u8) = (0, 3);
// pub const MOOD_ALTERNATING: (u8, u8) = (1, 3);
// pub const MOOD_SNAKE: (u8, u8) = (2, 3);

// pub const MOOD_PALETTE_ALL: (u8, u8) = (0, 4);
// pub const MOOD_PALETTE_CYAN_MAGENTA: (u8, u8) = (1, 4);
// pub const MOOD_PALETTE_ORANGE_BLUE: (u8, u8) = (2, 4);
// pub const MOOD_PALETTE_WHITE: (u8, u8) = (3, 4);

// pub const VIDEO_SPEED: (u8, u8) = (4, 5);
// pub const VIDEO_SPEED_SYNC: (u8, u8) = (3, 5);
// pub const VIDEO_FRY: (u8, u8) = (1, 5);
// pub const VIDEO_BRI: (u8, u8) = (0, 5);
// pub const VIDEO_ROTATE: (u8, u8) = (2, 5);

// pub const VIDEO_TOGGLE: (u8, u8) = (0, 6);
// pub const VIDEO_FILE_1: (u8, u8) = (1, 6);
// pub const VIDEO_FILE_2: (u8, u8) = (2, 6);
// pub const VIDEO_FILE_3: (u8, u8) = (3, 6);

// pub const VIDEO_FILE_4: (u8, u8) = (4, 6);
// pub const VIDEO_FILE_5: (u8, u8) = (5, 6);
// pub const VIDEO_FILE_6: (u8, u8) = (6, 6);

// pub const VIDEO_FILE_7: (u8, u8) = (0, 7);
// pub const VIDEO_FILE_8: (u8, u8) = (1, 7);
// pub const VIDEO_FILE_9: (u8, u8) = (2, 7);

// pub const LEFT_FADER_ALT: (u8, u8) = (5, 7);
// pub const RIGHT_FADER_ALT: (u8, u8) = (6, 7);
// pub const ENABLE_TILT_ANIM: (u8, u8) = (4, 7);

// pub const DIM_BRIGHTNESS_STAGE: (u8, u8) = (7, 2);
// pub const DIM_BRIGHTNESS_OTHER: (u8, u8) = (8, 2);

// pub const CROSSFADER_INPUT: (u8, u8) = (7, 4);
// pub const FOGGER: (u8, u8) = (6, 4);

// //
// // DDJ-400.
// //

// pub const DDJ_LEFT_BEAT_SYNC: (u8, u8) = (144, 88);
// pub const DDJ_LEFT_CUE: (u8, u8) = (144, 84);
// pub const DDJ_LEFT_RELOOP: (u8, u8) = (144, 77);

// pub const DDJ_RIGHT_BEAT_SYNC: (u8, u8) = (145, 88);
// pub const DDJ_RIGHT_CUE: (u8, u8) = (145, 84);
// pub const DDJ_RIGHT_RELOOP: (u8, u8) = (145, 77);

// pub const DDJ_LEFT_HOT_CUE_0_1: (u8, u8) = (151, 4);
// pub const DDJ_LEFT_HOT_CUE_1_1: (u8, u8) = (151, 5);

// pub const DDJ_LEFT_BEAT_JMP_0_0: (u8, u8) = (151, 32);
// pub const DDJ_LEFT_BEAT_JMP_1_0: (u8, u8) = (151, 33);

// pub const DDJ_LEFT_BEAT_JMP_0_1: (u8, u8) = (151, 36);
// pub const DDJ_LEFT_BEAT_JMP_1_1: (u8, u8) = (151, 37);
// pub const DDJ_LEFT_BEAT_JMP_2_1: (u8, u8) = (151, 38);

// pub const DDJ_RIGHT_HOT_CUE_0_1: (u8, u8) = (153, 4);
// pub const DDJ_RIGHT_HOT_CUE_1_1: (u8, u8) = (153, 5);
// pub const DDJ_RIGHT_HOT_CUE_2_1: (u8, u8) = (153, 6);

// pub const DDJ_RIGHT_BEAT_LOOP_0_0: (u8, u8) = (153, 96);
// pub const DDJ_RIGHT_BEAT_LOOP_1_0: (u8, u8) = (153, 97);
// pub const DDJ_RIGHT_BEAT_LOOP_2_0: (u8, u8) = (153, 98);
// pub const DDJ_RIGHT_BEAT_LOOP_3_0: (u8, u8) = (153, 99);

// pub const DDJ_RIGHT_BEAT_JMP_0_0: (u8, u8) = (153, 32);
// pub const DDJ_RIGHT_BEAT_JMP_1_0: (u8, u8) = (153, 33);
// pub const DDJ_RIGHT_BEAT_JMP_2_0: (u8, u8) = (153, 34);

// pub const DDJ_RIGHT_SAMPLER_0_0: (u8, u8) = (153, 48);
// pub const DDJ_RIGHT_SAMPLER_1_0: (u8, u8) = (153, 49);
// pub const DDJ_RIGHT_SAMPLER_2_0: (u8, u8) = (153, 50);
// pub const DDJ_RIGHT_SAMPLER_3_0: (u8, u8) = (153, 51);

// pub const DDJ_RIGHT_SAMPLER_0_1: (u8, u8) = (153, 52);
// pub const DDJ_RIGHT_SAMPLER_1_1: (u8, u8) = (153, 53);
// pub const DDJ_RIGHT_SAMPLER_2_1: (u8, u8) = (153, 54);
// pub const DDJ_RIGHT_SAMPLER_3_1: (u8, u8) = (153, 55);

// pub const DDJ_VOL_FADER_LEFT: (u8, u8) = (176, 19);
// pub const DDJ_VOL_FADER_RIGHT: (u8, u8) = (177, 19);
// pub const DDJ_CROSSFADER: (u8, u8) = (182, 63);

// // Filters on the left.
// pub const DDJ_ALL_FILTER_LEFT: (u8, u8) = (182, 23);
// pub const DDJ_LOW_FILTER_LEFT: (u8, u8) = (176, 15);
// pub const DDJ_MID_FILTER_LEFT: (u8, u8) = (176, 11);
// pub const DDJ_HIGH_FILTER_LEFT: (u8, u8) = (176, 7);

// // Filters on the right.
// pub const DDJ_ALL_FILTER_RIGHT: (u8, u8) = (182, 24);
// pub const DDJ_HIGH_FILTER_RIGHT: (u8, u8) = (177, 7);
// pub const DDJ_MID_FILTER_RIGHT: (u8, u8) = (177, 11);
// pub const DDJ_LOW_FILTER_RIGHT: (u8, u8) = (177, 15);
// pub const DDJ_TRIM_RIGHT: (u8, u8) = (177, 36);

// pub const DDJ_TEMPO_LEFT: (u8, u8) = (176, 0);
// pub const DDJ_TEMPO_RIGHT: (u8, u8) = (177, 0);

// // General filters.
// pub const DDJ_LEVEL_DEPTH_WHEEL: (u8, u8) = (180, 2);

// pub const DDJ_FX_ON: (u8, u8) = (148, 71);
// pub const DDJ_PHONES_LEVEL: (u8, u8) = (182, 13);
// pub const DDJ_PHONES_MIX: (u8, u8) = (182, 12);
// pub const DDJ_MASTER_LVL: (u8, u8) = (182, 8);
// pub const DDJ_MASTER_CUE: (u8, u8) = (150, 99);

// pub const DDJ_LEFT_IN_LOOP: (u8, u8) = (144, 16);
// pub const DDJ_LEFT_OUT_LOOP: (u8, u8) = (144, 17);

// pub const DDJ_LEFT_CUE_ROUND: (u8, u8) = (144, 12);
// pub const DDJ_RIGHT_CUE_ROUND: (u8, u8) = (145, 12);
// pub const DDJ_RIGHT_PLAY_ROUND: (u8, u8) = (145, 11);

// pub fn tick(state: &mut State, dmx: &mut [u8], midi_events: &[MidiEvent], input: TickInput) {
//     for event in midi_events {
//         match (event.tup(), event.value) {
//             // Web Matrix.
//             // (
//             //     STROBE_ON_BEAT
//             //     | MOOD_ON_BEAT
//             //     | STROBE_TOGGLE
//             //     | STROBE_AUTOMATION_TOGGLE
//             //     | MOOD_FORCE_TOGGLE,
//             //     _,
//             // ) => {
//             //     let value = (1 - event.value) != 0;
//             //     match event.tup() {
//             //         STROBE_TOGGLE => strobe::set_strobe_on(state, value),
//             //         STROBE_AUTOMATION_TOGGLE => strobe::set_strobe_automation_on(state, value),
//             //         MOOD_FORCE_TOGGLE => mood::set_force_on(state, value),
//             //         STROBE_ON_BEAT => strobe::set_on_beat(state, value),
//             //         other => unreachable!("Got other: {:?}", other),
//             //     }
//             // }
//             // DDJ-400.
//             (DDJ_CROSSFADER, vol) => {
//                 let val = (vol as u16).map_range(0..127, 0..255) as u8;
//                 crossfader_input(state, val);
//             }
//             (CROSSFADER_INPUT, _) | (DDJ_RIGHT_CUE_ROUND, 127) => {
//                 toggle_crossfader_input(state, !state.crossfader_input);
//             }
//             // Strobe basics.
//             (STROBE_ON_BEAT, _) | (DDJ_LEFT_BEAT_SYNC, 127) => {
//                 strobe::set_on_beat(state, !state.animation.strobe.controls.on_beat)
//             }
//             (STROBE_TOGGLE, _) | (DDJ_LEFT_CUE, 127) => {
//                 strobe::set_strobe_on(state, !state.animation.strobe.controls.strobe_enabled)
//             }
//             (STROBE_AUTOMATION_TOGGLE, _) | (DDJ_LEFT_RELOOP, 127) => {
//                 strobe::set_strobe_automation_on(
//                     state,
//                     !state.animation.strobe.controls.strobe_auto_enable,
//                 )
//             }
//             (DDJ_VOL_FADER_LEFT, vol) => {
//                 let val = (vol as u16).map_range(0..127, 0..255) as u8;
//                 left_fader_input(val, state);
//             }
//             (DDJ_ALL_FILTER_LEFT, vol) => {
//                 strobe::set_speed_multiplier(state, vol);
//             }
//             (DDJ_LOW_FILTER_LEFT, vol) => {
//                 strobe::set_on_multiplier(state, vol);
//             }
//             (DDJ_MID_FILTER_LEFT, vol) => {
//                 strobe::set_off_multiplier(state, vol);
//             }
//             (DDJ_HIGH_FILTER_LEFT, vol) => {
//                 strobe::set_tilt_speed(state, vol);
//             }
//             // Strobe animation.
//             (STROBE_SYNCED, 0) | (DDJ_LEFT_BEAT_JMP_0_0, 127) => {
//                 strobe::set_animation(state, dmx, StrobeAnimation::Synced);
//             }
//             (STROBE_ALTERNATING, 0) | (DDJ_LEFT_BEAT_JMP_1_0, 127) => {
//                 strobe::set_animation(state, dmx, StrobeAnimation::alternating());
//             }
//             (ENABLE_TILT_ANIM, _) | (DDJ_LEFT_CUE_ROUND, 127) => {
//                 strobe::set_tilt_anim(
//                     state,
//                     !state.animation.strobe.controls.tilt_animation_enabled,
//                 );
//             }
//             // Strobe toggles.
//             (STROBE_MOVING_HEAD_GROUP, _) | (DDJ_LEFT_BEAT_JMP_0_1, 127) => {
//                 strobe::toggle_moving_head_group(
//                     state,
//                     !state.animation.strobe.controls.moving_head_group_enabled,
//                     dmx,
//                 );
//             }
//             (STROBE_STAGE_GROUP, _) | (DDJ_LEFT_BEAT_JMP_1_1, 127) => {
//                 strobe::toggle_stage_group(
//                     state,
//                     !state.animation.strobe.controls.stage_light_group_enabled,
//                     dmx,
//                 );
//             }
//             (STROBE_PANEL_GROUP, _) | (DDJ_LEFT_BEAT_JMP_2_1, 127) => {
//                 strobe::toggle_panel_group(
//                     state,
//                     !state.animation.strobe.controls.panel_group_enabled,
//                     dmx,
//                 );
//             }
//             // Dimmers.
//             (DDJ_HIGH_FILTER_RIGHT, vol) => {
//                 dim::set_brightness_stage(state, vol);
//             }
//             (DDJ_MID_FILTER_RIGHT, vol) => {
//                 dim::set_brightness_other(state, vol);
//             }
//             // Mood basics.
//             (DDJ_VOL_FADER_RIGHT, vol) => {
//                 let val = (vol as u16).map_range(0..127, 0..255) as u8;
//                 right_fader_input(val, state);
//             }
//             (MOOD_ON_BEAT, _) | (DDJ_RIGHT_BEAT_SYNC, 127) => {
//                 mood::set_on_beat(state, !state.animation.mood.controls.on_beat);
//             }
//             (MOOD_BEAT_ANIM, _) | (DDJ_RIGHT_RELOOP, 127) => {
//                 mood::set_on_beat_anim(state, !state.animation.mood.controls.animation_on_beat);
//             }
//             (MOOD_FORCE_TOGGLE, _) | (DDJ_RIGHT_CUE, 127) => {
//                 mood::set_force_on(state, !state.animation.mood.controls.force);
//             }
//             // Mood animation.
//             (DDJ_ALL_FILTER_RIGHT, vol) => {
//                 mood::set_speed_multiplier(state, vol);
//             }
//             (DDJ_LOW_FILTER_RIGHT, vol) => {
//                 mood::set_speed_multiplier_beat(state, vol);
//             }
//             (MOOD_SYNCED, 0) | (DDJ_RIGHT_BEAT_JMP_0_0, 127) => {
//                 mood::set_animation(state, MoodAnimation::Synced);
//             }
//             (MOOD_ALTERNATING, 0) | (DDJ_RIGHT_BEAT_JMP_1_0, 127) => {
//                 mood::set_animation(state, MoodAnimation::alternating());
//             }
//             (MOOD_SNAKE, 0) | (DDJ_RIGHT_BEAT_JMP_2_0, 127) => {
//                 mood::set_animation(state, MoodAnimation::snake());
//             }
//             // Mood color.
//             (MOOD_PALETTE_ALL, 0) | (DDJ_RIGHT_BEAT_LOOP_0_0, 127) => {
//                 mood::set_color_palette(state, MoodColorPalette::All, input);
//             }
//             (MOOD_PALETTE_CYAN_MAGENTA, 0) | (DDJ_RIGHT_BEAT_LOOP_1_0, 127) => {
//                 mood::set_color_palette(state, MoodColorPalette::CyanMagenta, input);
//             }
//             (MOOD_PALETTE_ORANGE_BLUE, 0) | (DDJ_RIGHT_BEAT_LOOP_2_0, 127) => {
//                 mood::set_color_palette(state, MoodColorPalette::OrangeBlue, input);
//             }
//             (MOOD_PALETTE_WHITE, 0) | (DDJ_RIGHT_BEAT_LOOP_3_0, 127) => {
//                 mood::set_color_palette(state, MoodColorPalette::White, input);
//             }
//             // Video modes.
//             (DDJ_LEVEL_DEPTH_WHEEL, vol) => {
//                 video::set_speed_multiplier(state, vol);
//             }
//             (DDJ_PHONES_MIX, vol) => {
//                 video::set_fry(state, vol);
//             }
//             (DDJ_PHONES_LEVEL, vol) => {
//                 video::set_rotate(vol);
//             }
//             (DDJ_MASTER_LVL, vol) => {
//                 video::set_brightness(state, vol);
//             }
//             (DDJ_FX_ON, 127) => {
//                 video::set_video_bpm_speed_synced(state, !state.animation.video.speed_bpm_sync);
//             }
//             (DDJ_MASTER_CUE, 127) => {
//                 video::set_video_brightness_strobe_synced(
//                     state,
//                     !state.animation.video.brightness_strobe_synced,
//                 );
//             }
//             (VIDEO_FILE_1, 0) | (DDJ_RIGHT_SAMPLER_0_0, 127) => {
//                 video::set_video_file(state, VideoFile::Cheese);
//             }
//             (VIDEO_FILE_2, 0) | (DDJ_RIGHT_SAMPLER_1_0, 127) => {
//                 video::set_video_file(state, VideoFile::Grr);
//             }
//             (VIDEO_FILE_3, 0) | (DDJ_RIGHT_SAMPLER_2_0, 127) => {
//                 video::set_video_file(state, VideoFile::Swim);
//             }
//             (VIDEO_FILE_4, 0) | (DDJ_RIGHT_SAMPLER_3_0, 127) => {
//                 video::set_video_file(state, VideoFile::Cyonic);
//             }
//             (VIDEO_FILE_5, 0) | (DDJ_RIGHT_SAMPLER_0_1, 127) => {
//                 video::set_video_file(state, VideoFile::Jacky);
//             }
//             (VIDEO_FILE_6, 0) | (DDJ_RIGHT_SAMPLER_1_1, 127) => {
//                 video::set_video_file(state, VideoFile::Loveletter);
//             }
//             (VIDEO_FILE_7, 0) | (DDJ_RIGHT_SAMPLER_2_1, 127) => {
//                 video::set_video_file(state, VideoFile::Molly);
//             }
//             (VIDEO_FILE_8, 0) | (DDJ_RIGHT_SAMPLER_3_1, 127) => {
//                 video::set_video_file(state, VideoFile::Platzhalter);
//             }
//             // Fader controls
//             (LEFT_FADER_ALT, _) | (DDJ_LEFT_IN_LOOP, 127) => {
//                 set_left_fader_target_alt(state, !state.faders.left_target.is_alt());
//             }
//             (RIGHT_FADER_ALT, _) | (DDJ_LEFT_OUT_LOOP, 127) => {
//                 set_right_fader_target_alt(state, !state.faders.right_target.is_alt());
//             }
//             (DDJ_TEMPO_LEFT, vol) => {
//                 let val = (vol as u16).map_range(0..127, 0..255) as u8;
//                 set_pan(state, val);
//             }
//             (DDJ_TEMPO_RIGHT, vol) => {
//                 let val = (vol as u16).map_range(0..127, 0..255) as u8;
//                 set_tilt(state, val);
//             }
//             // Fogger
//             (FOGGER, _) | (DDJ_RIGHT_PLAY_ROUND, 127) => {
//                 toggle_fogger(state, !state.fogger);
//             }
//             (DDJ_TRIM_RIGHT, vol) => {
//                 let val = (vol as u16).map_range(0..127, 0..255) as u8;
//                 set_fogger_intensity(state, val);
//             }
//             _ => {
//                 if event.value != 0 {
//                     println!("Unknown MIDI event: {:?}", event);
//                 }
//                 continue;
//             }
//         }
//     }
// }

// pub fn set_left_fader_target_alt(state: &mut State, value: bool) {
//     let target = match value {
//         true => LeftFaderTarget::MovingHeadPan,
//         false => LeftFaderTarget::StrobeBrightness,
//     };

//     state.faders.left_target = target;

//     smidi!(DDJ_LEFT_IN_LOOP, (value as u8) * 127);
//     printc!(LEFT_FADER_ALT.0, LEFT_FADER_ALT.1, "LEFT FAD ALT");
//     bl_controls_set(LEFT_FADER_ALT.0, LEFT_FADER_ALT.1, value);
// }

// pub fn set_right_fader_target_alt(state: &mut State, value: bool) {
//     let target = match value {
//         true => RightFaderTarget::MovingHeadTilt,
//         false => RightFaderTarget::MoodBrightness,
//     };

//     state.faders.right_target = target;

//     smidi!(DDJ_LEFT_OUT_LOOP, (value as u8) * 127);
//     printc!(RIGHT_FADER_ALT.0, RIGHT_FADER_ALT.1, "RIGHT FAD ALT");
//     bl_controls_set(RIGHT_FADER_ALT.0, RIGHT_FADER_ALT.1, value);
// }

// fn left_fader_input(val: u8, state: &mut State) {
//     match state.faders.left_target {
//         LeftFaderTarget::StrobeBrightness => {
//             strobe::set_brightness(state, val);
//         }
//         LeftFaderTarget::MovingHeadPan => {
//             set_pan(state, val);
//         }
//     }
// }

// fn set_pan(state: &mut State, val: u8) {
//     state.animation.strobe.controls.pan = val;
// }

// fn right_fader_input(val: u8, state: &mut State) {
//     match state.faders.right_target {
//         RightFaderTarget::MoodBrightness => {
//             mood::set_brightness(state, val);
//         }
//         RightFaderTarget::MovingHeadTilt => set_tilt(state, val),
//     }
// }

// fn set_tilt(state: &mut State, val: u8) {
//     state.animation.strobe.controls.tilt = val;
// }

// fn crossfader_input(state: &mut State, val: u8) {
//     if !state.crossfader_input {
//         return;
//     }
//     state.animation.strobe.controls.brightness = val;
//     state.animation.mood.controls.brightness = 255 - val;
// }

// pub fn toggle_crossfader_input(state: &mut State, val: bool) {
//     state.crossfader_input = val;
//     smidi!(DDJ_RIGHT_CUE_ROUND, val as u8 * 127);
//     printc!(CROSSFADER_INPUT.0, CROSSFADER_INPUT.1, "CRS FADER");
//     bl_controls_set(CROSSFADER_INPUT.0, CROSSFADER_INPUT.1, val);
// }

// pub fn toggle_fogger(state: &mut State, val: bool) {
//     state.fogger = val;
//     smidi!(DDJ_RIGHT_PLAY_ROUND, val as u8 * 127);
//     printc!(FOGGER.0, FOGGER.1, "FOGGER {}", state.fogger_int);
//     bl_controls_set(FOGGER.0, FOGGER.1, val);
// }

// pub fn set_fogger_intensity(state: &mut State, val: u8) {
//     state.fogger_int = val;
//     printc!(FOGGER.0, FOGGER.1, "FOGGER {}", state.fogger_int);
// }
