use egui::{Color32, Ui};

pub fn text_color_for_bg(ui: &Ui, bg: Color32) -> Color32 {
    // Default text color from visuals
    let default_text = ui.visuals().widgets.inactive.fg_stroke.color;

    // Compute contrast ratio
    if is_contrasting(default_text, bg) {
        default_text
    } else {
        contrasting_bw(bg)
    }
}

/// Check if two colors have "enough" contrast
fn is_contrasting(fg: Color32, bg: Color32) -> bool {
    let contrast = contrast_ratio(fg, bg);
    contrast > 3.0 // WCAG "minimum readable" threshold
}

/// Get contrast ratio between two colors
fn contrast_ratio(c1: Color32, c2: Color32) -> f32 {
    let l1 = luminance(c1);
    let l2 = luminance(c2);
    let (bright, dark) = if l1 > l2 { (l1, l2) } else { (l2, l1) };
    (bright + 0.05) / (dark + 0.05)
}

/// Relative luminance (perceived brightness)
fn luminance(c: Color32) -> f32 {
    fn chan(v: u8) -> f32 {
        let v = v as f32 / 255.0;
        if v <= 0.03928 {
            v / 12.92
        } else {
            ((v + 0.055) / 1.055).powf(2.4)
        }
    }
    0.2126 * chan(c.r()) + 0.7152 * chan(c.g()) + 0.0722 * chan(c.b())
}

/// Simple fallback: black or white text depending on bg brightness
fn contrasting_bw(bg: Color32) -> Color32 {
    let brightness = 0.299 * bg.r() as f32 + 0.587 * bg.g() as f32 + 0.114 * bg.b() as f32;
    if brightness > 128.0 {
        Color32::BLACK
    } else {
        Color32::WHITE
    }
}

// ---------------------------------------------------------------------------
// HSV triangle picker (desktop Programmer, `design/studio/programmer.html` TRI)
// ---------------------------------------------------------------------------

/// The picker's side in points.
pub const HSV_TRIANGLE_SIZE: f32 = 176.0;
const RING_OUTER: f32 = 88.0;
const RING_INNER: f32 = 72.0;
const TRI_RADIUS: f32 = 68.0;
const RING_SEGMENTS: usize = 180;

/// A change made with [`hsv_triangle`]: hue in degrees (0..360), or
/// saturation and value (0..1).
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum HsvChange {
    Hue(f32),
    SatValue { s: f32, v: f32 },
}

/// Pure hue `h` (degrees) as RGB.
pub fn hue_rgb(h: f32) -> Color32 {
    let f = |n: f32| {
        let q = (n + h / 60.0).rem_euclid(6.0);
        (255.0 * (1.0 - q.min(4.0 - q).clamp(0.0, 1.0))).round() as u8
    };
    Color32::from_rgb(f(5.0), f(3.0), f(1.0))
}

/// Point on a circle around `c`: angle 0 at the top, clockwise.
fn at(c: egui::Pos2, angle: f32, r: f32) -> egui::Pos2 {
    egui::pos2(c.x + r * angle.sin(), c.y - r * angle.cos())
}

/// The fixed triangle's corners: pure hue (right), white (top left), black
/// (bottom left).
pub fn triangle_corners(c: egui::Pos2) -> [egui::Pos2; 3] {
    use std::f32::consts::{FRAC_PI_2, PI};
    [
        at(c, FRAC_PI_2, TRI_RADIUS),
        at(c, FRAC_PI_2 - 2.0 * PI / 3.0, TRI_RADIUS),
        at(c, FRAC_PI_2 + 2.0 * PI / 3.0, TRI_RADIUS),
    ]
}

/// Barycentric weights of `p` for the corners (hue, white, black).
fn bary(p: egui::Pos2, [h, w, b]: [egui::Pos2; 3]) -> [f32; 3] {
    let d = (w.y - b.y) * (h.x - b.x) + (b.x - w.x) * (h.y - b.y);
    let a = ((w.y - b.y) * (p.x - b.x) + (b.x - w.x) * (p.y - b.y)) / d;
    let ww = ((b.y - h.y) * (p.x - b.x) + (h.x - b.x) * (p.y - b.y)) / d;
    [a, ww, 1.0 - a - ww]
}

/// Saturation and value at `p` inside the triangle (clamped onto it).
pub fn sat_value_at(p: egui::Pos2, c: egui::Pos2) -> (Option<f32>, f32) {
    let [a, w, b] = bary(p, triangle_corners(c)).map(|n| n.max(0.0));
    let t = (a + w + b).max(f32::EPSILON);
    let (a, w) = (a / t, w / t);
    let v = (a + w).clamp(0.0, 1.0);
    // Black has no saturation: keep the one it had.
    let s = (v > 0.004).then(|| (a / v).clamp(0.0, 1.0));
    (s, v)
}

/// Where saturation `s` and value `v` sit in the triangle.
pub fn sat_value_pos(c: egui::Pos2, s: f32, v: f32) -> egui::Pos2 {
    let [h, w, b] = triangle_corners(c);
    b + v * (w - b) + v * s * (h - w)
}

/// Hue (degrees) of the ring point at `p`.
pub fn hue_at(p: egui::Pos2, c: egui::Pos2) -> f32 {
    let a = (p.x - c.x).atan2(-(p.y - c.y)) / std::f32::consts::TAU;
    (a.rem_euclid(1.0) * 360.0).rem_euclid(360.0)
}

fn marker(p: &egui::Painter, at: egui::Pos2) {
    p.circle_stroke(at, 5.0, egui::Stroke::new(2.0_f32, Color32::BLACK));
    p.circle_stroke(at, 5.0, egui::Stroke::new(1.0_f32, Color32::WHITE));
}

/// Colour as a hue ring around a fixed saturation/value triangle, drawn as a
/// mesh with vertex colours. A drag that starts on the ring sets the hue,
/// one that starts inside sets saturation and value. `h` is in degrees,
/// `s` and `v` in 0..1. Desktop only.
pub fn hsv_triangle(
    ui: &mut Ui,
    h: f32,
    s: f32,
    v: f32,
    hue_enabled: bool,
    sv_enabled: bool,
) -> (egui::Response, Option<HsvChange>) {
    use egui::{epaint::Mesh, Sense};
    let size = egui::vec2(HSV_TRIANGLE_SIZE, HSV_TRIANGLE_SIZE);
    let (rect, response) = ui.allocate_exact_size(size, Sense::click_and_drag());
    let c = rect.center();
    let p = ui.painter();
    let h = h.rem_euclid(360.0);

    // Ring.
    let mut ring = Mesh::default();
    for i in 0..=RING_SEGMENTS {
        let t = i as f32 / RING_SEGMENTS as f32;
        let a = t * std::f32::consts::TAU;
        let col = hue_rgb(t * 360.0);
        ring.colored_vertex(at(c, a, RING_OUTER), col);
        ring.colored_vertex(at(c, a, RING_INNER), col);
        if i > 0 {
            let k = (2 * i) as u32;
            ring.add_triangle(k - 2, k - 1, k);
            ring.add_triangle(k - 1, k + 1, k);
        }
    }
    p.add(ring);

    // Triangle: hue, white, black; the mix between them is HSV.
    let corners = triangle_corners(c);
    let mut tri = Mesh::default();
    tri.colored_vertex(corners[0], hue_rgb(h));
    tri.colored_vertex(corners[1], Color32::WHITE);
    tri.colored_vertex(corners[2], Color32::BLACK);
    tri.add_triangle(0, 1, 2);
    p.add(tri);

    marker(p, at(c, h.to_radians(), (RING_OUTER + RING_INNER) / 2.0));
    marker(p, sat_value_pos(c, s.clamp(0.0, 1.0), v.clamp(0.0, 1.0)));

    // The drag keeps to where it started.
    let ring_id = response.id.with("on_ring");
    let mut change = None;
    if let Some(pos) = response.interact_pointer_pos() {
        if response.drag_started() || response.clicked() || response.is_pointer_button_down_on() {
            let on_ring =
                if response.drag_started() || ui.data(|d| d.get_temp::<bool>(ring_id)).is_none() {
                    let r = (pos - c).length() >= RING_INNER - 3.0;
                    ui.data_mut(|d| d.insert_temp(ring_id, r));
                    r
                } else {
                    ui.data(|d| d.get_temp::<bool>(ring_id)).unwrap_or(false)
                };
            if on_ring && hue_enabled {
                change = Some(HsvChange::Hue(hue_at(pos, c)));
            } else if !on_ring && sv_enabled {
                let (ns, nv) = sat_value_at(pos, c);
                change = Some(HsvChange::SatValue {
                    s: ns.unwrap_or(s),
                    v: nv,
                });
            }
        }
    }
    if !response.is_pointer_button_down_on() {
        ui.data_mut(|d| d.remove::<bool>(ring_id));
    }
    (response, change)
}

#[cfg(test)]
mod hsv_tests {
    use super::*;

    #[test]
    fn corners_round_trip() {
        let c = egui::pos2(88.0, 88.0);
        let [h, w, b] = triangle_corners(c);
        assert!(h.x > c.x && (h.y - c.y).abs() < 0.01, "hue on the right");
        assert!(w.x < c.x && w.y < c.y, "white top left");
        assert!(b.x < c.x && b.y > c.y, "black bottom left");
        let (s, v) = sat_value_at(h, c);
        assert!((s.unwrap() - 1.0).abs() < 1e-3 && (v - 1.0).abs() < 1e-3);
        let (s, v) = sat_value_at(w, c);
        assert!(s.unwrap() < 1e-3 && (v - 1.0).abs() < 1e-3);
        assert!(sat_value_at(b, c).0.is_none());
        let p = sat_value_pos(c, 0.3, 0.6);
        let (s, v) = sat_value_at(p, c);
        assert!((s.unwrap() - 0.3).abs() < 1e-3 && (v - 0.6).abs() < 1e-3);
    }

    #[test]
    fn hue_zero_is_up_and_clockwise() {
        let c = egui::pos2(0.0, 0.0);
        assert!(hue_at(egui::pos2(0.0, -80.0), c) < 0.01);
        assert!((hue_at(egui::pos2(80.0, 0.0), c) - 90.0).abs() < 0.01);
        assert_eq!(hue_rgb(0.0), Color32::from_rgb(255, 0, 0));
        assert_eq!(hue_rgb(120.0), Color32::from_rgb(0, 255, 0));
    }
}
