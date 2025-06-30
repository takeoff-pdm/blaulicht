mod clock;
mod color;
mod fixture;

use crate::{dmx::fixture::{Fixture, FixtureGroup, FixtureType}, routes::AppState};
use serde::{Deserialize, Serialize};
use std::{borrow::Cow, collections::HashMap, sync::{Arc, RwLock}};

/// This module deals with applying events on fixtures to produce a continuous DMX output.

pub struct DmxEngine {
    // TODO: check if this is too slow.
    state_ref: Arc<AppState>,
    dmx: [u8; 513], // Starting at 1

    // TODO: add more, internal state.
}

impl DmxEngine {
    pub fn tick(&mut self) {
        // Only read if there were no events.
        let state = self.state_ref.dmx_engine.read();

        // TODO: flatten to DMX buffer. 
    }
}

//
// State.
//

#[derive(Debug, Serialize, Deserialize, Default)]
pub struct EngineState {
    // TODO: how to solve this?
    // fixtures: Vec<Fixture>,
    // Assigns an ID to a group.
    groups: HashMap<u8, FixtureGroup>,
}

// #[derive(Serialize, Deserialize, Debug)]
// pub struct FixtureGroupState {
//     pub fixtures: Vec<FixtureState>,
// }



// impl DmxEngine {
//     pub fn snapshot() -> EngineState {
//         todo!("Not implemented")
//     }
// }
