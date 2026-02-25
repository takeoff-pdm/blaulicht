use super::math::Vec3;

pub(super) const FIXTURE_BODY_COLOR: [f32; 3] = [0.35, 0.35, 0.36];
pub(super) const FIXTURE_EDGE_COLOR: [f32; 3] = [0.0, 0.0, 0.0];
pub(super) const JOINT_COLOR: [f32; 3] = [0.26, 0.26, 0.28];
pub(super) const HEAD_BODY_COLOR: [f32; 3] = [0.18, 0.18, 0.2];

pub(super) const BASE_SIZE: Vec3 = Vec3 {
    x: 0.9,
    y: 0.25,
    z: 0.9,
};
pub(super) const PAN_JOINT_HEIGHT: f32 = 0.12;
pub(super) const PAN_JOINT_RADIUS: f32 = 0.22;
pub(super) const YOKE_HEIGHT: f32 = 0.5;
pub(super) const YOKE_RADIUS: f32 = 0.18;
pub(super) const TILT_JOINT_RADIUS: f32 = 0.15;
pub(super) const TILT_JOINT_LENGTH: f32 = 0.25;
pub(super) const HEAD_SIZE: Vec3 = Vec3 {
    x: 0.45,
    y: 0.28,
    z: 0.5,
};
pub(super) const BEAM_ANGLE_DEG: f32 = 8.0;
pub(super) const CONE_SEGMENTS: usize = 24;
pub(super) const TAKEOFF_LOGO_SCALE: f32 = 0.38;
pub(super) const TAKEOFF_LOGO_DEPTH: f32 = 0.18;
pub(super) const TAKEOFF_LOGO_BACK_PADDING: f32 = 0.2;
pub(super) const TAKEOFF_LOGO_BACK_THICKNESS: f32 = 0.08;
pub(super) const TAKEOFF_LOGO_Y_OFFSET: f32 = 0.35;
pub(super) const TAKEOFF_BACK_COLOR: [f32; 3] = [0.08, 0.08, 0.09];
pub(super) const GENERIC_FIXTURE_SIZE: f32 = 0.45;
pub(super) const TAKEOFF_TEXT: &str = "Takeoff";
pub(super) const TAKEOFF_TEXT_HEIGHT: f32 = 0.32;
pub(super) const TAKEOFF_TEXT_Y: f32 = 0.0;
pub(super) const FONT_GLYPH_W: usize = 8;
pub(super) const FONT_GLYPH_H: usize = 8;
pub(super) const FONT_ATLAS_COLS: usize = 16;
pub(super) const FONT_ATLAS_ROWS: usize = 8;
pub(super) const FONT_ATLAS_W: usize = FONT_GLYPH_W * FONT_ATLAS_COLS;
pub(super) const FONT_ATLAS_H: usize = FONT_GLYPH_H * FONT_ATLAS_ROWS;
pub(super) const FONT_GLYPH_SPACING: f32 = 1.0;
pub(super) const FONT_LINE_GAP: f32 = 2.0;
pub(super) const DEBUG_TEXT_OVERLAY: bool = false;
