use std::collections::BTreeMap;

use bincode::{Decode, Encode};
use map_range::MapRange;
use serde::{Deserialize, Serialize};

use crate::{
    FixtureProperty, HSVColor,
    fixture::{
        state::{FixtureOrientation, FixtureState},
        value::FixtureValue,
    },
};

/// Maximum recursion depth when resolving chains of pointer palettes.
pub const MAX_PALETTE_CHAIN_DEPTH: u8 = 4;

#[derive(Debug, Clone, Serialize, Deserialize, Encode, Decode)]
pub struct Palette {
    pub name: String,
    pub kind: PaletteKind,
}

#[derive(Debug, Clone, Serialize, Deserialize, Encode, Decode)]
pub enum PaletteKind {
    Color(HSVColor),
    Position(FixtureOrientation),
    Beam {
        focus: u8,
        strobe_speed: u8,
    },
    Single(FixtureProperty, u16),
    /// Reads the resolved value of `target` for each property the target covers
    /// (inherited recursively) and yields those as its own value.
    ///
    /// `property` selects which property the `ops` transform. When `None`, the
    /// ops apply to every property. When `Some(p)`, the ops apply only to `p`
    /// and every other property passes through untouched — so e.g. a color
    /// target keeps its hue/saturation while only the chosen channel is scaled.
    Pointer {
        target: u8,
        property: Option<FixtureProperty>,
        ops: Vec<PaletteOp>,
    },
}

/// A single transformation step applied to a value flowing through a pointer
/// palette. Internally values are carried as `f32` and rounded/clamped to `u16`
/// at the end of the chain.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, Encode, Decode, PartialEq)]
pub enum PaletteOp {
    Add(f32),
    Mul(f32),
    Div(f32),
    Clamp { min: u16, max: u16 },
    Min(u16),
    Max(u16),
}

impl PaletteOp {
    pub fn apply(self, value: f32) -> f32 {
        match self {
            Self::Add(n) => value + n,
            Self::Mul(n) => value * n,
            Self::Div(n) => {
                if n.abs() < f32::EPSILON {
                    value
                } else {
                    value / n
                }
            }
            Self::Clamp { min, max } => value.clamp(min as f32, max as f32),
            Self::Min(n) => value.min(n as f32),
            Self::Max(n) => value.max(n as f32),
        }
    }

    pub fn label(&self) -> &'static str {
        match self {
            Self::Add(_) => "Add",
            Self::Mul(_) => "Mul",
            Self::Div(_) => "Div",
            Self::Clamp { .. } => "Clamp",
            Self::Min(_) => "Min",
            Self::Max(_) => "Max",
        }
    }
}

impl PaletteKind {
    /// The set of fixture properties this palette provides values for.
    ///
    /// For pointer palettes this delegates to the target (recursively, up to
    /// `MAX_PALETTE_CHAIN_DEPTH` hops). A broken or over-deep chain reports
    /// no properties.
    pub fn properties(&self, palettes: &BTreeMap<u8, Palette>) -> Vec<FixtureProperty> {
        match self {
            PaletteKind::Color(_) => vec![
                FixtureProperty::ColorHue,
                FixtureProperty::ColorSaturation,
                FixtureProperty::ColorValue,
            ],
            PaletteKind::Position(_) => vec![FixtureProperty::Pan, FixtureProperty::Tilt],
            PaletteKind::Beam { .. } => vec![FixtureProperty::Focus, FixtureProperty::Strobe],
            PaletteKind::Single(prop, _) => vec![*prop],
            // A pointer always covers every property its target covers; the
            // `property` override only narrows *which* property the ops touch.
            PaletteKind::Pointer { target, .. } => {
                properties_via_chain(palettes, *target, MAX_PALETTE_CHAIN_DEPTH)
            }
        }
    }

    pub fn apply_to(&self, state: &mut FixtureState, palettes: &BTreeMap<u8, Palette>) {
        match self {
            PaletteKind::Color(color) => {
                state.color_h = FixtureValue::Literal(color.h.clamp(0.0, 360.0) as u16);
                state.color_s =
                    FixtureValue::Literal(color.s.map_range(0.0..1.0, 0.0..255.0) as u16);
                state.color_v =
                    FixtureValue::Literal(color.v.map_range(0.0..1.0, 0.0..255.0) as u16);
            }
            PaletteKind::Position(orientation) => {
                state.pan = FixtureValue::literal_u8(orientation.pan);
                state.tilt = FixtureValue::literal_u8(orientation.tilt);
            }
            PaletteKind::Beam {
                focus,
                strobe_speed,
            } => {
                state.focus = FixtureValue::literal_u8(*focus);
                state.strobe_speed = FixtureValue::literal_u8(*strobe_speed);
            }
            PaletteKind::Single(prop, value) => {
                state.apply_value(*value, *prop);
            }
            PaletteKind::Pointer {
                target,
                property,
                ops,
            } => {
                let props = properties_via_chain(palettes, *target, MAX_PALETTE_CHAIN_DEPTH);
                for prop in props {
                    let inner = resolve_via_chain(
                        palettes,
                        *target,
                        prop,
                        MAX_PALETTE_CHAIN_DEPTH.saturating_sub(1),
                    );
                    // Ops apply only to the selected property (or to all when
                    // none is selected). Other properties pass through, so e.g.
                    // a color target keeps its hue/saturation while only the
                    // chosen channel is transformed.
                    let v = apply_pointer_ops(inner, ops, *property, prop);
                    state.apply_value(v, prop);
                }
            }
        }
    }
}

/// Resolve the value of `palette_id` for `property`, following pointer chains
/// up to `depth` hops. Returns 0 for missing palettes, exceeded depth, or
/// properties not covered by the leaf palette.
pub fn resolve_via_chain(
    palettes: &BTreeMap<u8, Palette>,
    palette_id: u8,
    property: FixtureProperty,
    depth: u8,
) -> u16 {
    if depth == 0 {
        return 0;
    }
    let Some(palette) = palettes.get(&palette_id) else {
        return 0;
    };
    match &palette.kind {
        PaletteKind::Pointer {
            target,
            property: op_property,
            ops,
        } => {
            // Always pass the requested property through from the target; the
            // ops only transform the selected property (or all when none).
            let inner = resolve_via_chain(palettes, *target, property, depth - 1);
            apply_pointer_ops(inner, ops, *op_property, property)
        }
        kind => extract_leaf_property(kind, property),
    }
}

/// Applies a pointer's `ops` to `inner` if `property` is the pointer's selected
/// op-target (`op_property`), or if no target is selected (`None` = all). Other
/// properties are returned unchanged so the rest of the target's value (e.g. a
/// color's hue/saturation) survives.
fn apply_pointer_ops(
    inner: u16,
    ops: &[PaletteOp],
    op_property: Option<FixtureProperty>,
    property: FixtureProperty,
) -> u16 {
    if op_property.is_some_and(|p| p != property) {
        return inner;
    }
    ops.iter()
        .fold(inner as f32, |acc, op| op.apply(acc))
        .round()
        .clamp(0.0, u16::MAX as f32) as u16
}

fn properties_via_chain(
    palettes: &BTreeMap<u8, Palette>,
    palette_id: u8,
    depth: u8,
) -> Vec<FixtureProperty> {
    if depth == 0 {
        return vec![];
    }
    let Some(palette) = palettes.get(&palette_id) else {
        return vec![];
    };
    match &palette.kind {
        PaletteKind::Pointer { target, .. } => properties_via_chain(palettes, *target, depth - 1),
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

fn extract_leaf_property(kind: &PaletteKind, property: FixtureProperty) -> u16 {
    match (kind, property) {
        (PaletteKind::Color(c), FixtureProperty::ColorHue) => c.h as u16,
        (PaletteKind::Color(c), FixtureProperty::ColorSaturation) => {
            c.s.map_range(0.0..1.0, 0.0..255.0) as u16
        }
        (PaletteKind::Color(c), FixtureProperty::ColorValue) => {
            c.v.map_range(0.0..1.0, 0.0..255.0) as u16
        }
        (PaletteKind::Position(p), FixtureProperty::Pan) => p.pan as u16,
        (PaletteKind::Position(p), FixtureProperty::Tilt) => p.tilt as u16,
        (PaletteKind::Beam { focus, .. }, FixtureProperty::Focus) => *focus as u16,
        (PaletteKind::Beam { strobe_speed, .. }, FixtureProperty::Strobe) => *strobe_speed as u16,
        (PaletteKind::Single(prop, value), p) if *prop == p => *value,
        _ => 0,
    }
}

/// Returns true if writing `new_kind` under `new_id` would create a cycle in
/// the pointer graph. Walks forward from the new target following pointer
/// targets; a cycle exists iff we ever land back on `new_id`.
pub fn would_create_cycle(
    palettes: &BTreeMap<u8, Palette>,
    new_id: u8,
    new_kind: &PaletteKind,
) -> bool {
    let PaletteKind::Pointer { target, .. } = new_kind else {
        return false;
    };
    let mut current = *target;
    let mut visited = std::collections::HashSet::new();
    loop {
        if current == new_id {
            return true;
        }
        if !visited.insert(current) {
            return false;
        }
        let Some(p) = palettes.get(&current) else {
            return false;
        };
        match &p.kind {
            PaletteKind::Pointer { target: next, .. } => current = *next,
            _ => return false,
        }
    }
}
