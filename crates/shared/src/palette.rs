use bincode::{Decode, Encode};
use serde::{Deserialize, Serialize};

use crate::{FixtureProperty, HSVColor, fixture::state::{FixtureOrientation, FixtureState}};

#[derive(Debug, Clone, Serialize, Deserialize, Encode, Decode)]
pub struct Palette {
    pub name: String,
    pub kind: PaletteKind,
}

#[derive(Debug, Clone, Serialize, Deserialize, Encode, Decode)]
pub enum PaletteKind {
    Color(HSVColor),
    Position(FixtureOrientation),
    Beam { focus: u8, strobe_speed: u8 },
    Single(FixtureProperty, u16),
}

impl PaletteKind {
    pub fn properties(&self) -> Vec<FixtureProperty> {
        match self {
            PaletteKind::Color(_) => vec![
                FixtureProperty::ColorHue,
                FixtureProperty::ColorSaturation,
                FixtureProperty::ColorValue,
            ],
            PaletteKind::Position(_) => vec![FixtureProperty::Pan, FixtureProperty::Tilt],
            PaletteKind::Beam { .. } => vec![FixtureProperty::Focus, FixtureProperty::Strobe],
            PaletteKind::Single(prop, _) => vec![*prop],
        }
    }

    pub fn apply_to(&self, state: &mut FixtureState) {
        match self {
            PaletteKind::Color(color) => {
                state.color = color.clone();
            }
            PaletteKind::Position(orientation) => {
                state.orientation = orientation.clone();
            }
            PaletteKind::Beam { focus, strobe_speed } => {
                state.focus = *focus;
                state.strobe_speed = *strobe_speed;
            }
            PaletteKind::Single(prop, value) => {
                state.apply_value(*value, *prop);
            }
        }
    }
}
