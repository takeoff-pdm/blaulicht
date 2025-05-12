use map_range::MapRange;

use crate::{
    blaulicht::{self, bl_controls_log, bl_controls_set, bl_udp, TickInput},
    elapsed, printc, println, smidi,
    user::{
        logo, midi::{
            DDJ_FX_ON, DDJ_MASTER_CUE, DDJ_RIGHT_SAMPLER_0_0, DDJ_RIGHT_SAMPLER_0_1, DDJ_RIGHT_SAMPLER_1_0, DDJ_RIGHT_SAMPLER_1_1, DDJ_RIGHT_SAMPLER_2_0, DDJ_RIGHT_SAMPLER_2_1, DDJ_RIGHT_SAMPLER_3_0, DDJ_RIGHT_SAMPLER_3_1, VIDEO_BRI, VIDEO_FILE_1, VIDEO_FILE_2, VIDEO_FILE_3, VIDEO_FILE_4, VIDEO_FILE_5, VIDEO_FILE_6, VIDEO_FILE_7, VIDEO_FILE_8, VIDEO_FRY, VIDEO_ROTATE, VIDEO_SPEED, VIDEO_SPEED_SYNC
        }, strobe
    },
};

use super::state::{self, State, VideoFile};

// From raw MIDI.
pub fn set_speed_multiplier(state: &mut State, value: u8) {
    let (parsed_value, value_str) = strobe::parse_speed_multiplier(value);
    let parsed_value = 1.0 / parsed_value;
    printc!(VIDEO_SPEED.0, VIDEO_SPEED.1, "VID SPEED {value_str}",);
    //     (state.animation.video.speed = (state.animation.video.speed + 1) % 16);

    if parsed_value != state.animation.video.speed {
        send_speed_multiplier(parsed_value * state.animation.video.file.speed());
        state.animation.video.speed = parsed_value;
    }
}

pub fn set_fry(state: &mut State, value: u8) {
    let val = (value as u16).map_range(0..127, 1..10);
    printc!(VIDEO_FRY.0, VIDEO_FRY.1, "VID FRY <br> {}%", val * 100);
    bl_udp("127.0.0.1:9000", &[110, val as u8]);
    state.animation.video.fry = val as u8;
}

pub fn set_brightness(state: &mut State, value: u8) {
    let val = (value as u16).map_range(0..127, 1..100);
    state.animation.video.brightness = val as u8;

    if state.animation.video.brightness_strobe_synced {
        return;
    }

    set_brightness_internal(state, val as u8);
}
pub fn set_brightness_internal(state: &mut State, val: u8) {
    printc!(VIDEO_BRI.0, VIDEO_BRI.1, "VID BRI <br> {}%", val);
    bl_udp("127.0.0.1:9000", &[130, val]);
}

pub fn set_rotate(value: u8) {
    let val = (value as u16).map_range(0..127, 0..360);
    printc!(VIDEO_ROTATE.0, VIDEO_ROTATE.1, "VID ROT <br> {val}deg",);
    let val_bytes = val.to_le_bytes();
    let mut buf = Vec::with_capacity(1 + val_bytes.len());
    buf.push(120);
    buf.extend_from_slice(&val_bytes);
    bl_udp("127.0.0.1:9000", &buf);
}

pub fn set_video_bpm_speed_synced(state: &mut State, value: bool) {
    state.animation.video.speed_bpm_sync = value;
    smidi!(DDJ_FX_ON, (value as u8) * 127);
    bl_controls_set(VIDEO_SPEED_SYNC.0, VIDEO_SPEED_SYNC.1, value);

    if !value {
        printc!(VIDEO_SPEED_SYNC.0, VIDEO_SPEED_SYNC.1, "SYNC SPEED OFF");
    }
}

pub fn set_video_brightness_strobe_synced(state: &mut State, value: bool) {
    state.animation.video.brightness_strobe_synced = value;
    smidi!(DDJ_MASTER_CUE, (value as u8) * 127);
    bl_controls_set(VIDEO_BRI.0, VIDEO_BRI.1, value);

    if value {
        printc!(VIDEO_BRI.0, VIDEO_BRI.1, "VID BRI <br> SYNC");
    } else {
        set_brightness_internal(state, state.animation.video.brightness);
    }
}

fn send_speed_multiplier(value: f32) {
    let fac_bytes = value.to_le_bytes();
    let mut body = Vec::with_capacity(1 + fac_bytes.len());
    body.push(200);
    body.extend_from_slice(&fac_bytes);

    blaulicht::bl_udp("127.0.0.1:9000", &body);
    println!("[UDP] Sent speed event.");
}

pub fn set_video_file(state: &mut State, value: VideoFile) {
    let (
        x0_y0_value,
        x1_y0_value,
        x2_y0_value,
        x3_y0_value,
        x0_y1_value,
        x1_y1_value,
        x2_y1_value,
        x3_y1_value,
    ) = match &value {
        VideoFile::Cheese => (127, 0, 0, 0, 0, 0, 0, 0),
        VideoFile::Grr => (0, 127, 0, 0, 0, 0, 0, 0),
        VideoFile::Swim => (0, 0, 127, 0, 0, 0, 0, 0),
        VideoFile::Cyonic => (0, 0, 0, 127, 0, 0, 0, 0),
        VideoFile::Jacky => (0, 0, 0, 0, 127, 0, 0, 0),
        VideoFile::Loveletter => (0, 0, 0, 0, 0, 127, 0, 0),
        VideoFile::Molly => (0, 0, 0, 0, 0, 0, 127, 0),
        VideoFile::Platzhalter => (0, 0, 0, 0, 0, 0, 0, 127),
    };
    smidi!(DDJ_RIGHT_SAMPLER_0_0, x0_y0_value);
    smidi!(DDJ_RIGHT_SAMPLER_1_0, x1_y0_value);
    smidi!(DDJ_RIGHT_SAMPLER_2_0, x2_y0_value);
    smidi!(DDJ_RIGHT_SAMPLER_3_0, x3_y0_value);

    smidi!(DDJ_RIGHT_SAMPLER_0_1, x0_y1_value);
    smidi!(DDJ_RIGHT_SAMPLER_1_1, x1_y1_value);
    smidi!(DDJ_RIGHT_SAMPLER_2_1, x2_y1_value);
    smidi!(DDJ_RIGHT_SAMPLER_3_1, x3_y1_value);

    bl_controls_set(VIDEO_FILE_1.0, VIDEO_FILE_1.1, x0_y0_value == 127);
    bl_controls_set(VIDEO_FILE_2.0, VIDEO_FILE_2.1, x1_y0_value == 127);
    bl_controls_set(VIDEO_FILE_3.0, VIDEO_FILE_3.1, x2_y0_value == 127);
    bl_controls_set(VIDEO_FILE_4.0, VIDEO_FILE_4.1, x3_y0_value == 127);
    bl_controls_set(VIDEO_FILE_5.0, VIDEO_FILE_5.1, x0_y1_value == 127);
    bl_controls_set(VIDEO_FILE_6.0, VIDEO_FILE_6.1, x1_y1_value == 127);
    bl_controls_set(VIDEO_FILE_7.0, VIDEO_FILE_7.1, x2_y1_value == 127);
    bl_controls_set(VIDEO_FILE_8.0, VIDEO_FILE_8.1, x3_y1_value == 127);

    bl_controls_log(
        VIDEO_FILE_1.0,
        VIDEO_FILE_1.1,
        &format!("{}", VideoFile::Cheese.path()),
    );

    bl_controls_log(
        VIDEO_FILE_2.0,
        VIDEO_FILE_2.1,
        &format!("{}", VideoFile::Grr.path()),
    );

    bl_controls_log(
        VIDEO_FILE_3.0,
        VIDEO_FILE_3.1,
        &format!("{}", VideoFile::Swim.path()),
    );

    // DJS


    bl_controls_log(
        VIDEO_FILE_4.0,
        VIDEO_FILE_4.1,
        &format!("{}", VideoFile::Cyonic.path()),
    );

    bl_controls_log(
        VIDEO_FILE_5.0,
        VIDEO_FILE_5.1,
        &format!("{}", VideoFile::Jacky.path()),
    );

    bl_controls_log(
        VIDEO_FILE_6.0,
        VIDEO_FILE_6.1,
        &format!("{}", VideoFile::Loveletter.path()),
    );

    bl_controls_log(
        VIDEO_FILE_7.0,
        VIDEO_FILE_7.1,
        &format!("{}", VideoFile::Molly.path()),
    );

    bl_controls_log(
        VIDEO_FILE_8.0,
        VIDEO_FILE_8.1,
        &format!("{}", VideoFile::Platzhalter.path()),
    );

    state.animation.video.file = value;

    let body_str = value.path().as_bytes();
    let mut body = Vec::with_capacity(body_str.len() + 1);
    body.push(100);
    body.extend_from_slice(body_str);
    // TODO: make IP configurable.
    blaulicht::bl_udp("127.0.0.1:9000", &body);
    println!("[UDP] Play event.");

    send_speed_multiplier(state.animation.video.speed * value.speed());
}

pub fn send_sync() {
    blaulicht::bl_udp("127.0.0.1:9000", &[150, 0]);
    println!("[UDP] Sync event.");
}

pub fn tick(state: &mut State, input: TickInput, force: bool) {
    // Return if not beat syncing.
    if !state.animation.video.speed_bpm_sync {
        return;
    }

    // Sync
    let original_bpm = state.animation.video.file.base_bpm();
    let target_bpm = input.bpm;

    if target_bpm == 0 {
        printc!(VIDEO_SPEED_SYNC.0, VIDEO_SPEED_SYNC.1, "NO BPM",);
        return;
    }

    let speed_factor = target_bpm as f32 / original_bpm as f32;
    let modified_speed_factor =
        speed_factor * state.animation.video.speed * state.animation.video.file.speed();

    const MILLIS_BETWEEN_SYNC: u32 = 1000;

    if modified_speed_factor != state.animation.video.speed_bpm_sync_last_factor
        && (elapsed!(input, state.animation.video.speed_sync_last_update) > MILLIS_BETWEEN_SYNC
            || force)
    {
        state.animation.video.speed_bpm_sync_last_factor;
        state.animation.video.speed_sync_last_update = input.time;

        printc!(
            VIDEO_SPEED_SYNC.0,
            VIDEO_SPEED_SYNC.1,
            "SYNC SPEED <br> {} bpm -> {modified_speed_factor}",
            input.bpm
        );
        send_speed_multiplier(modified_speed_factor);

        logo::sync_bpm(input.bpm);
    }
}
