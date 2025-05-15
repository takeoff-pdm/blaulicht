use super::state::{self, State};

pub fn tick(state: &State, dmx: &mut [u8]) {
    dmx[23] = state.fogger as u8 * state.fogger_int;
}
