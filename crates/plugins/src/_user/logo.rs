//
// Takeoff logo.
//

// use crate::{
//     blaulicht::{self, TickInput},
//     elapsed, println,
// };

// use super::state::{State};

// const TAKEOFF_LOGO_HOST: &'static str = "192.168.0.100:5005";

// pub fn takeoff_logo_sync() {
//     blaulicht::bl_udp(TAKEOFF_LOGO_HOST, b"N");
// }

// pub fn sync_bpm(bpm: u8) {
//     let body = format!("B{bpm}");
//     blaulicht::bl_udp(TAKEOFF_LOGO_HOST, body.as_bytes());
//     println!("Transmit BPM to LOGO.");
// }

// pub fn set_mode(state: &mut State, mode: LogoMode, force: bool) {
//     if state.logo_mode == mode && !force {
//         return;
//     }

//     println!("LOGO set mode: {mode:?}");
//     blaulicht::bl_udp(TAKEOFF_LOGO_HOST, mode.bytes());
//     state.logo_mode = mode;
// }

// pub fn tick(state: &mut State, input: TickInput) {
//     // let el = elapsed!(input, state.animation.strobe.strobe_activate_time.unwrap_or(0));

//     // if el > 3000 && el < 10000 {
//     //     set_mode(state, LogoMode::Breakdown, false);
//     // } else if el > 10000 {
//     //     set_mode(state, LogoMode::Normal, false);
//     // }
// }
