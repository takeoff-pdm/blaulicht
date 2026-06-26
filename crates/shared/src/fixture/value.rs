use std::collections::BTreeMap;

use bincode::{Decode, Encode};
use serde::{Deserialize, Serialize};

use crate::{
    FixtureProperty,
    palette::{MAX_PALETTE_CHAIN_DEPTH, Palette, resolve_via_chain},
};

/// A single fixture-property value slot.
///
/// Either holds a concrete `u16` (set by the user, by an animation, etc.)
/// or points to a palette entry that resolves to a value at output time.
#[derive(Serialize, Deserialize, Debug, Clone, Copy, Encode, Decode, PartialEq, Eq)]
pub enum FixtureValue {
    Literal(u16),
    PalettePointer { palette_id: u8 },
}

impl Default for FixtureValue {
    fn default() -> Self {
        Self::Literal(0)
    }
}

impl FixtureValue {
    pub const fn literal_u8(v: u8) -> Self {
        Self::Literal(v as u16)
    }

    pub const fn literal(v: u16) -> Self {
        Self::Literal(v)
    }

    pub fn palette_id(&self) -> Option<u8> {
        match self {
            Self::PalettePointer { palette_id } => Some(*palette_id),
            _ => None,
        }
    }

    pub fn is_frozen(&self) -> bool {
        matches!(self, Self::PalettePointer { .. })
    }

    /// Resolves to a concrete `u16` in the unit of `property`.
    ///
    /// Follows pointer-palette chains up to `MAX_PALETTE_CHAIN_DEPTH` hops;
    /// missing palettes, exhausted depth, or unsupported (kind, property)
    /// pairs fall back to `0`.
    pub fn resolve(&self, palettes: &BTreeMap<u8, Palette>, property: FixtureProperty) -> u16 {
        match self {
            Self::Literal(v) => *v,
            Self::PalettePointer { palette_id } => {
                resolve_via_chain(palettes, *palette_id, property, MAX_PALETTE_CHAIN_DEPTH)
            }
        }
    }
}
