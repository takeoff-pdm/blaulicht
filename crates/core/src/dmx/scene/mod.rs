use std::collections::{BTreeMap, HashMap, HashSet};

use blaulicht_shared::{fixture::state::FixtureGroup, ControlEvent, FixtureProperty};
use serde::{Deserialize, Serialize};

use crate::dmx::{ActiveAnimation, FixtureSelection, FixtureState};
