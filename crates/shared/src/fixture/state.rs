use bincode::{Decode, Encode};
use map_range::MapRange;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

use crate::{
    ControlEvent, FixtureProperty, HSVColor, RGBColor,
    fixture::{FixtureType, value::FixtureValue},
    palette::Palette,
};

#[derive(Serialize, Deserialize, Debug, Clone, Encode, Decode)]
pub enum MergeStrategy {
    Highest,
    Latest,
    Interpolate,
}

/// Concrete pan/tilt as carried by palettes and the resolved view used by
/// fixture-write paths.
#[derive(Serialize, Deserialize, Debug, Default, Clone, Encode, Decode)]
pub struct FixtureOrientation {
    pub pan: u8,
    pub tilt: u8,
}

#[derive(Serialize, Deserialize, Debug, Clone, Default, Encode, Decode)]
pub struct FixtureGroup {
    pub name: String,
    pub fixtures: BTreeMap<u8, Fixture>,
}

#[derive(Serialize, Deserialize, Debug, Clone, Encode, Decode)]
pub struct Fixture {
    pub name: String,
    pub type_: FixtureType,
    pub pos: Position,
    #[serde(default)]
    pub rotation: Rotation,
    pub start_addr: usize,
    pub universe_no: usize,
}

impl Fixture {
    pub fn new(universe_no: usize, start_addr: usize, name: String, type_: FixtureType) -> Self {
        Self {
            name,
            type_,
            pos: Position::default(),
            rotation: Rotation::default(),
            start_addr,
            universe_no,
        }
    }

    pub fn write(&self, state: &ResolvedFixtureState, dmx: &mut [u8]) {
        self.type_.write(self, state, dmx)
    }

    pub fn state_from_dmx(&self, dmx: &[u8]) -> FixtureState {
        self.type_.state_from_dmx(self, dmx)
    }

    pub fn setup(&self, time: i32, state: &ResolvedFixtureState, dmx: &mut [u8]) {
        self.type_.setup(self, time, state, dmx);
    }
}

#[derive(Serialize, Deserialize, Debug, Clone, Default, Encode, Decode)]
pub struct Position {
    pub x: usize,
    pub y: usize,
    pub z: usize,
}

#[derive(Serialize, Deserialize, Debug, Clone, Default, Encode, Decode)]
pub struct Rotation {
    pub x: f32,
    pub y: f32,
    pub z: f32,
}

impl From<(usize, usize)> for Position {
    fn from(value: (usize, usize)) -> Self {
        Self {
            x: value.0,
            y: value.1,
            z: 0,
        }
    }
}

/// The editable, palette-aware fixture state.
///
/// Each property slot is a [`FixtureValue`] — either a literal value or a
/// pointer to a palette entry. Resolve into a [`ResolvedFixtureState`] before
/// writing DMX or rendering visuals.
#[derive(Serialize, Deserialize, Debug, Clone, Encode, Decode, Default)]
pub struct FixtureState {
    pub color_h: FixtureValue,
    pub color_s: FixtureValue,
    pub color_v: FixtureValue,
    pub alpha: FixtureValue,
    pub pan: FixtureValue,
    pub tilt: FixtureValue,
    pub strobe_speed: FixtureValue,
    pub focus: FixtureValue,
}

/// The concrete, post-resolution fixture state. This is what the fixture
/// type-specific write functions consume.
#[derive(Debug, Clone, Default)]
pub struct ResolvedFixtureState {
    pub color: HSVColor,
    pub alpha: u8,
    pub orientation: FixtureOrientation,
    pub strobe_speed: u8,
    pub focus: u8,
}

impl From<ResolvedFixtureState> for FixtureState {
    fn from(r: ResolvedFixtureState) -> Self {
        Self {
            color_h: FixtureValue::Literal(r.color.h.clamp(0.0, 360.0) as u16),
            color_s: FixtureValue::Literal(r.color.s.map_range(0.0..1.0, 0.0..255.0) as u16),
            color_v: FixtureValue::Literal(r.color.v.map_range(0.0..1.0, 0.0..255.0) as u16),
            alpha: FixtureValue::literal_u8(r.alpha),
            pan: FixtureValue::literal_u8(r.orientation.pan),
            tilt: FixtureValue::literal_u8(r.orientation.tilt),
            strobe_speed: FixtureValue::literal_u8(r.strobe_speed),
            focus: FixtureValue::literal_u8(r.focus),
        }
    }
}

impl FixtureState {
    pub fn slot(&self, property: FixtureProperty) -> FixtureValue {
        match property {
            FixtureProperty::Alpha => self.alpha,
            FixtureProperty::Strobe => self.strobe_speed,
            FixtureProperty::Focus => self.focus,
            FixtureProperty::ColorHue => self.color_h,
            FixtureProperty::ColorSaturation => self.color_s,
            FixtureProperty::ColorValue => self.color_v,
            FixtureProperty::Tilt => self.tilt,
            FixtureProperty::Pan => self.pan,
        }
    }

    pub fn slot_mut(&mut self, property: FixtureProperty) -> &mut FixtureValue {
        match property {
            FixtureProperty::Alpha => &mut self.alpha,
            FixtureProperty::Strobe => &mut self.strobe_speed,
            FixtureProperty::Focus => &mut self.focus,
            FixtureProperty::ColorHue => &mut self.color_h,
            FixtureProperty::ColorSaturation => &mut self.color_s,
            FixtureProperty::ColorValue => &mut self.color_v,
            FixtureProperty::Tilt => &mut self.tilt,
            FixtureProperty::Pan => &mut self.pan,
        }
    }

    /// Returns the *resolved* `u16` value for `property` in the same units
    /// the legacy `get_value` returned.
    pub fn resolved_value(
        &self,
        property: FixtureProperty,
        palettes: &BTreeMap<u8, Palette>,
    ) -> u16 {
        self.slot(property).resolve(palettes, property)
    }

    /// Resolve all slots through `palettes` into a concrete state usable by
    /// DMX writers and the visualizer.
    pub fn resolve(&self, palettes: &BTreeMap<u8, Palette>) -> ResolvedFixtureState {
        let hue = self.color_h.resolve(palettes, FixtureProperty::ColorHue) as f64;
        let sat = self.color_s.resolve(palettes, FixtureProperty::ColorSaturation) as f64;
        let val = self.color_v.resolve(palettes, FixtureProperty::ColorValue) as f64;

        ResolvedFixtureState {
            color: HSVColor {
                h: hue.clamp(0.0, 360.0),
                s: sat.map_range(0.0..255.0, 0.0..1.0),
                v: val.map_range(0.0..255.0, 0.0..1.0),
            },
            alpha: self.alpha.resolve(palettes, FixtureProperty::Alpha).min(255) as u8,
            orientation: FixtureOrientation {
                pan: self.pan.resolve(palettes, FixtureProperty::Pan).min(255) as u8,
                tilt: self.tilt.resolve(palettes, FixtureProperty::Tilt).min(255) as u8,
            },
            strobe_speed: self
                .strobe_speed
                .resolve(palettes, FixtureProperty::Strobe)
                .min(255) as u8,
            focus: self.focus.resolve(palettes, FixtureProperty::Focus).min(255) as u8,
        }
    }

    /// Merges a single property from another state using the given strategy.
    /// Resolution happens through `palettes` so a frozen slot still participates
    /// numerically. The result is written back as a literal.
    pub fn merge_from(
        &mut self,
        other: &FixtureState,
        property: FixtureProperty,
        strategy: MergeStrategy,
        palettes: &BTreeMap<u8, Palette>,
    ) {
        let self_value = self.resolved_value(property, palettes);
        let other_value = other.resolved_value(property, palettes);

        let new_value = match strategy {
            MergeStrategy::Highest => self_value.max(other_value),
            MergeStrategy::Latest => other_value,
            MergeStrategy::Interpolate => self_value.midpoint(other_value),
        };

        self.apply_value(new_value, property);
    }

    /// Writes a literal value into `property`. Palette-bound slots are
    /// frozen and left untouched; unbind first if you need to overwrite.
    pub fn apply_value(&mut self, value: u16, property: FixtureProperty) {
        if self.slot(property).is_frozen() {
            return;
        }
        *self.slot_mut(property) = FixtureValue::Literal(value);
    }

    /// Binds `property` to `palette_id`. The slider for that property becomes
    /// frozen until explicitly unbound or overwritten with a literal.
    pub fn bind_palette(&mut self, property: FixtureProperty, palette_id: u8) {
        *self.slot_mut(property) = FixtureValue::PalettePointer {
            palette_id,
            property: None,
        };
    }

    /// Replaces a palette pointer with the current resolved literal. No-op if
    /// the slot is already a literal.
    pub fn unbind_palette(
        &mut self,
        property: FixtureProperty,
        palettes: &BTreeMap<u8, Palette>,
    ) {
        let slot = self.slot_mut(property);
        if let FixtureValue::PalettePointer { .. } = *slot {
            let v = slot.resolve(palettes, property);
            *slot = FixtureValue::Literal(v);
        }
    }

    pub fn reset(&mut self) {
        *self = Self::default();
    }

    pub fn apply(&mut self, ev: ControlEvent) -> ApplyOutcome {
        let mut out = ApplyOutcome::default();
        match ev {
            ControlEvent::SetAlpha(alpha) => {
                if self.alpha.is_frozen() {
                    out.rejected.push(FixtureProperty::Alpha);
                } else {
                    self.alpha = FixtureValue::literal_u8(alpha);
                    out.changed.push(FixtureProperty::Alpha);
                }
            }
            ControlEvent::SetStrobeSpeed(speed) => {
                if self.strobe_speed.is_frozen() {
                    out.rejected.push(FixtureProperty::Strobe);
                } else {
                    self.strobe_speed = FixtureValue::literal_u8(speed);
                    out.changed.push(FixtureProperty::Strobe);
                }
            }
            ControlEvent::SetFocus(v) => {
                if self.focus.is_frozen() {
                    out.rejected.push(FixtureProperty::Focus);
                } else {
                    self.focus = FixtureValue::literal_u8(v);
                    out.changed.push(FixtureProperty::Focus);
                }
            }
            ControlEvent::SetPan(pan) => {
                if self.pan.is_frozen() {
                    out.rejected.push(FixtureProperty::Pan);
                } else {
                    self.pan = FixtureValue::literal_u8(pan);
                    out.changed.push(FixtureProperty::Pan);
                }
            }
            ControlEvent::SetTilt(tilt) => {
                if self.tilt.is_frozen() {
                    out.rejected.push(FixtureProperty::Tilt);
                } else {
                    self.tilt = FixtureValue::literal_u8(tilt);
                    out.changed.push(FixtureProperty::Tilt);
                }
            }
            ControlEvent::SetColor(clr) => {
                let rgb: RGBColor = clr.into();
                let hsv: HSVColor = rgb.into();
                if self.color_h.is_frozen() {
                    out.rejected.push(FixtureProperty::ColorHue);
                } else {
                    self.color_h = FixtureValue::Literal(hsv.h.clamp(0.0, 360.0) as u16);
                    out.changed.push(FixtureProperty::ColorHue);
                }
                if self.color_s.is_frozen() {
                    out.rejected.push(FixtureProperty::ColorSaturation);
                } else {
                    self.color_s =
                        FixtureValue::Literal(hsv.s.map_range(0.0..1.0, 0.0..255.0) as u16);
                    out.changed.push(FixtureProperty::ColorSaturation);
                }
                if self.color_v.is_frozen() {
                    out.rejected.push(FixtureProperty::ColorValue);
                } else {
                    self.color_v =
                        FixtureValue::Literal(hsv.v.map_range(0.0..1.0, 0.0..255.0) as u16);
                    out.changed.push(FixtureProperty::ColorValue);
                }
            }
            ControlEvent::SetColorHue(hue) => {
                if self.color_h.is_frozen() {
                    out.rejected.push(FixtureProperty::ColorHue);
                } else {
                    self.color_h = FixtureValue::Literal((hue as u16).min(360));
                    out.changed.push(FixtureProperty::ColorHue);
                }
            }
            ControlEvent::SetColorSaturation(sat) => {
                if self.color_s.is_frozen() {
                    out.rejected.push(FixtureProperty::ColorSaturation);
                } else {
                    self.color_s = FixtureValue::literal_u8(sat);
                    out.changed.push(FixtureProperty::ColorSaturation);
                }
            }
            ControlEvent::SetColorValue(val) => {
                if self.color_v.is_frozen() {
                    out.rejected.push(FixtureProperty::ColorValue);
                } else {
                    self.color_v = FixtureValue::literal_u8(val);
                    out.changed.push(FixtureProperty::ColorValue);
                }
            }
            ControlEvent::AssignPaletteToProperty(property, palette_id) => {
                self.bind_palette(property, palette_id);
                out.changed.push(property);
            }
            ControlEvent::UnassignPaletteFromProperty(property) => {
                *self.slot_mut(property) = FixtureValue::Literal(0);
                out.changed.push(property);
            }
            other => {
                tracing::warn!("Ignoring unsupported fixture event: {other:?}");
            }
        }
        out
    }
}

/// Result of `FixtureState::apply`. `rejected` lists slots that the event
/// targeted but were left untouched because they are bound to a palette.
#[derive(Debug, Default, Clone)]
pub struct ApplyOutcome {
    pub changed: Vec<FixtureProperty>,
    pub rejected: Vec<FixtureProperty>,
}

impl ApplyOutcome {
    pub fn any_rejected(&self) -> bool {
        !self.rejected.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unsupported_fixture_event_is_ignored() {
        let mut state = FixtureState::default();
        let outcome = state.apply(ControlEvent::SetEnabled(true));

        assert!(outcome.changed.is_empty());
        assert!(outcome.rejected.is_empty());
    }
}
