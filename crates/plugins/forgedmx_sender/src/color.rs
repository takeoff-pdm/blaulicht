//! Resolves color palettes (including pointer chains) to RGB for forgeDMX.

use std::collections::BTreeMap;

use blaulicht_shared::palette::{resolve_via_chain, Palette, MAX_PALETTE_CHAIN_DEPTH};
use blaulicht_shared::FixtureProperty;

pub type Rgb = [u8; 3];

/// Palettes that yield a color (plain color palettes and pointers to one),
/// sorted by id, as (id, name).
pub fn color_palettes(palettes: &BTreeMap<u8, Palette>) -> Vec<(u8, String)> {
    palettes
        .iter()
        .filter(|(_, p)| p.kind.properties(palettes).contains(&FixtureProperty::ColorHue))
        .map(|(id, p)| (*id, p.name.clone()))
        .collect()
}

/// The palette's color, or None when it is missing or not a color palette.
pub fn palette_rgb(palettes: &BTreeMap<u8, Palette>, id: u8) -> Option<Rgb> {
    let palette = palettes.get(&id)?;
    if !palette.kind.properties(palettes).contains(&FixtureProperty::ColorHue) {
        return None;
    }
    // Same scales the fixtures get: hue in degrees, saturation/value 0..=255.
    let get = |p| resolve_via_chain(palettes, id, p, MAX_PALETTE_CHAIN_DEPTH);
    let h = f64::from(get(FixtureProperty::ColorHue)) % 360.0;
    let s = f64::from(get(FixtureProperty::ColorSaturation).min(255)) / 255.0;
    let v = f64::from(get(FixtureProperty::ColorValue).min(255)) / 255.0;
    Some(hsv_to_rgb(h, s, v))
}

/// h in degrees, s and v in 0..=1.
pub fn hsv_to_rgb(h: f64, s: f64, v: f64) -> Rgb {
    let c = v * s;
    let hp = h.rem_euclid(360.0) / 60.0;
    let x = c * (1.0 - (hp % 2.0 - 1.0).abs());
    let (r, g, b) = match hp as u32 {
        0 => (c, x, 0.0),
        1 => (x, c, 0.0),
        2 => (0.0, c, x),
        3 => (0.0, x, c),
        4 => (x, 0.0, c),
        _ => (c, 0.0, x),
    };
    let m = v - c;
    let to = |n: f64| ((n + m) * 255.0).round().clamp(0.0, 255.0) as u8;
    [to(r), to(g), to(b)]
}

pub fn hex(c: Rgb) -> String {
    format!("#{:02x}{:02x}{:02x}", c[0], c[1], c[2])
}

#[cfg(test)]
mod tests {
    use super::*;
    use blaulicht_shared::palette::{PaletteKind, PaletteOp};
    use blaulicht_shared::HSVColor;

    fn color(name: &str, h: f64, s: f64, v: f64) -> Palette {
        Palette { name: name.into(), kind: PaletteKind::Color(HSVColor { h, s, v }), folder: None }
    }

    fn palettes() -> BTreeMap<u8, Palette> {
        let mut p = BTreeMap::new();
        p.insert(1, color("Red", 0.0, 1.0, 1.0));
        p.insert(
            2,
            Palette {
                name: "Red complement".into(),
                kind: PaletteKind::Pointer {
                    target: 1,
                    property: Some(FixtureProperty::ColorHue),
                    ops: vec![PaletteOp::HueComplementary],
                },
                folder: None,
            },
        );
        p.insert(
            3,
            Palette {
                name: "Narrow".into(),
                kind: PaletteKind::Beam { focus: 10, strobe_speed: 0 },
                folder: None,
            },
        );
        p
    }

    #[test]
    fn converts_hsv() {
        assert_eq!(hsv_to_rgb(0.0, 1.0, 1.0), [255, 0, 0]);
        assert_eq!(hsv_to_rgb(120.0, 1.0, 1.0), [0, 255, 0]);
        assert_eq!(hsv_to_rgb(240.0, 1.0, 0.5), [0, 0, 128]);
        assert_eq!(hsv_to_rgb(42.0, 0.0, 1.0), [255, 255, 255]);
    }

    #[test]
    fn lists_only_color_palettes() {
        let ids: Vec<u8> = color_palettes(&palettes()).iter().map(|(id, _)| *id).collect();
        assert_eq!(ids, vec![1, 2]);
    }

    #[test]
    fn resolves_plain_and_pointer_palettes() {
        let p = palettes();
        assert_eq!(palette_rgb(&p, 1), Some([255, 0, 0]));
        assert_eq!(palette_rgb(&p, 2), Some([0, 255, 255]));
        assert_eq!(palette_rgb(&p, 3), None);
        assert_eq!(palette_rgb(&p, 99), None);
    }

    #[test]
    fn formats_hex() {
        assert_eq!(hex([255, 8, 0]), "#ff0800");
    }
}
