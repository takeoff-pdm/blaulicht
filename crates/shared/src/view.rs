use std::collections::{BTreeMap, HashMap, HashSet, VecDeque};

use bincode::{Decode, Encode};
use serde::{Deserialize, Serialize};

use crate::{
    ActiveAnimation, ControlEvent, FixtureProperty,
    fixture::state::{FixtureGroup, FixtureState},
};

#[derive(Clone, PartialEq, Eq, Hash, Debug, Serialize, Deserialize, Encode, Decode)]
pub struct View {
    pub name: String,
    pub base_scene: u8,
    pub overlays: Vec<u8>,
}
