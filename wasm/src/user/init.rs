// use crate::{
//     blaulicht::{self, TickInput},
//     printc, println,
//     user::{
//         config::{Config, Light, StrobeGroup},
//         dim, logo,
//         midi::{
//             self, MOOD_ALTERNATING, MOOD_FORCE_TOGGLE, MOOD_ON_BEAT, MOOD_PALETTE_ALL,
//             MOOD_PALETTE_CYAN_MAGENTA, MOOD_PALETTE_ORANGE_BLUE, MOOD_PALETTE_WHITE, MOOD_SNAKE,
//             MOOD_SYNCED, STROBE_ALTERNATING, STROBE_AUTOMATION_TOGGLE, STROBE_ON_BEAT,
//             STROBE_SYNCED, STROBE_TOGGLE,
//         },
//         mood::{self},
//         strobe::{self},
//         video,
//     },
// };

use crate::{
    blaulicht::TickInput,
    println,
    user::config::{Config, Light, MovingHead, StrobeGroup},
};

use super::{
    config::FixtureGroup,
    state::{strobe::StrobeState, State},
};

fn remove_old_state(state: &mut State, dmx: &mut [u8]) {
    state.reset();

    // Turn off all channels.
    for i in 0..513 {
        dmx[i] = 0;
    }
}

pub fn initialize(state: &mut State, input: TickInput, dmx: &mut [u8]) {
    // state.last_beat_time = input.time;
    // state.init_time = input.time;

    //
    // Initialize fixtures.
    //

    // 0-initialize.
    *state = State::default();

    state.config = Config {
        strobe_groups: vec![StrobeGroup::new(
            true,
            FixtureGroup::new(
                "Primary Strobe".into(),
                vec![(MovingHead::MartinMacAura.into(), 40).into()],
            ),
        )],
        mood_groups: vec![FixtureGroup::new(
            "Primary Mood".into(),
            vec![(Light::LEDPartyTCLSpot.into(), 20).into()],
        )],
        dimmer_groups: vec![FixtureGroup::new(
            "Primary Dimmer".into(),
            vec![(Light::Generic4ChanWithAlpha.into(), 30).into()],
        )],
    };

    // Initialize the control surface.
    // blaulicht::bl_controls_config(8, 8);
    // printc!(STROBE_TOGGLE.0, STROBE_TOGGLE.1, "STROBE ENABLE");

    // printc!(
    //     STROBE_AUTOMATION_TOGGLE.0,
    //     STROBE_AUTOMATION_TOGGLE.1,
    //     "STROBE AUTO"
    // );
    // strobe::set_strobe_automation_on(state, state.animation.strobe.controls.strobe_auto_enable);

    // printc!(STROBE_ON_BEAT.0, STROBE_ON_BEAT.1, "STROBE ON BEAT");
    // strobe::set_on_beat(state, state.animation.strobe.controls.on_beat);

    // printc!(STROBE_SYNCED.0, STROBE_SYNCED.1, "STROBE SYNC");
    // printc!(
    //     STROBE_ALTERNATING.0,
    //     STROBE_ALTERNATING.1,
    //     "STROBE ALTERNATING"
    // );

    // strobe::set_tilt_anim(
    //     state,
    //     state.animation.strobe.controls.tilt_animation_enabled,
    // );
    // strobe::set_tilt_speed(state, 127 / 2);

    // strobe::set_drop_duration(
    //     state,
    //     state.animation.strobe.controls.strobe_drop_duration_secs,
    // );
    // strobe::set_brightness(state, state.animation.strobe.controls.brightness);
    // strobe::set_animation(state, dmx, StrobeAnimation::Synced);
    // strobe::set_speed_multiplier(state, 127 / 2);

    // strobe::set_on_multiplier(state, 127 / 2);
    // strobe::set_off_multiplier(state, 127 / 2);

    // strobe::toggle_moving_head_group(state, true, dmx);
    // strobe::toggle_stage_group(state, true, dmx);
    // strobe::toggle_panel_group(state, true, dmx);

    // mood::set_brightness(state, state.animation.mood.controls.brightness);

    // printc!(MOOD_FORCE_TOGGLE.0, MOOD_FORCE_TOGGLE.1, "MOOD + STROBE",);
    // mood::set_force_on(state, state.animation.mood.controls.force);

    // printc!(MOOD_ON_BEAT.0, MOOD_ON_BEAT.1, "MOOD ON BEAT");
    // mood::set_on_beat(state, state.animation.mood.controls.on_beat);

    // printc!(MOOD_SYNCED.0, MOOD_SYNCED.1, "MOOD SYNC");
    // printc!(MOOD_ALTERNATING.0, MOOD_ALTERNATING.1, "MOOD ALTERNATING");
    // printc!(MOOD_SNAKE.0, MOOD_SNAKE.1, "MOOD SNAKE");
    // mood::set_animation(state, state.animation.mood.controls.animation.clone());

    // printc!(MOOD_PALETTE_ALL.0, MOOD_PALETTE_ALL.1, "CLR ALL");
    // printc!(
    //     MOOD_PALETTE_CYAN_MAGENTA.0,
    //     MOOD_PALETTE_CYAN_MAGENTA.1,
    //     "CLR C/M"
    // );
    // printc!(
    //     MOOD_PALETTE_ORANGE_BLUE.0,
    //     MOOD_PALETTE_ORANGE_BLUE.1,
    //     "CLR O/B"
    // );
    // printc!(MOOD_PALETTE_WHITE.0, MOOD_PALETTE_WHITE.1, "CLR WHITE");
    // mood::set_color_palette(
    //     state,
    //     state.animation.mood.controls.color_palette.clone(),
    //     input,
    // );
    // mood::set_speed_multiplier(state, 127 / 2);
    // mood::set_speed_multiplier_beat(state, 127 / 2);
    // mood::set_on_beat_anim(state, state.animation.mood.controls.animation_on_beat);

    // // AAAA

    // midi::set_left_fader_target_alt(state, false);
    // midi::set_right_fader_target_alt(state, false);

    // //
    // // Video
    // //

    // //
    // // Send initialization UDP.
    // //

    // // let body_str = b"Hello World!";
    // // let mut body = Vec::with_capacity(body_str.len() + 1);
    // // body.push(42);
    // // body.extend_from_slice(body_str);
    // // bl_udp("127.0.0.1:9000", &body);
    // // println!("Sent init UDP.");

    // // bl_udp("127.0.0.1:9000", &[200, 1, 1]);
    // // println!("Sent speed UDP.");

    // video::set_video_file(state, state.animation.video.file);
    // video::set_speed_multiplier(state, 127 / 2);
    // video::set_video_bpm_speed_synced(state, state.animation.video.speed_bpm_sync);
    // video::set_video_brightness_strobe_synced(
    //     state,
    //     state.animation.video.brightness_strobe_synced,
    // );
    // video::set_fry(state, state.animation.video.fry);
    // video::set_rotate(0);
    // video::set_brightness_internal(state, state.animation.video.brightness);

    // dim::set_brightness_stage(state, state.animation.dim.controls.brightness_stage);
    // dim::set_brightness_other(state, state.animation.dim.controls.brightness_other);

    // logo::set_mode(state, LogoMode::Normal, true);

    // midi::toggle_fogger(state, false);
    // midi::set_fogger_intensity(state, 0);

    // midi::toggle_crossfader_input(state, false);

    println!("[SETUP] Running...");
}
