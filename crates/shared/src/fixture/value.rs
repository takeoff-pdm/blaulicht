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
    /// Points at a palette entry that resolves to a value at output time.
    ///
    /// `property` optionally overrides which fixture property is read out of
    /// the (possibly multi-property) palette. When `None`, the value resolves
    /// using the property of the slot/context it lives in — this is what plain
    /// fixture-slot bindings use, where the slot already fixes the property.
    /// An explicit `Some(p)` is used where the context property is ambiguous
    /// or irrelevant (e.g. an animation amplitude bound to the `value` channel
    /// of a color palette).
    PalettePointer {
        palette_id: u8,
        property: Option<FixtureProperty>,
    },
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
            Self::PalettePointer { palette_id, .. } => Some(*palette_id),
            _ => None,
        }
    }

    /// The explicit per-binding property override, if any. `None` for literals
    /// and for palette pointers that inherit their slot/context property.
    pub fn palette_property(&self) -> Option<FixtureProperty> {
        match self {
            Self::PalettePointer { property, .. } => *property,
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
            Self::PalettePointer {
                palette_id,
                property: override_property,
            } => {
                let prop = override_property.unwrap_or(property);
                resolve_via_chain(palettes, *palette_id, prop, MAX_PALETTE_CHAIN_DEPTH)
            }
        }
    }
}
