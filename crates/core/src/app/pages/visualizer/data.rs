use blaulicht_shared::{
    fixture::{light::Light, FixtureType},
    RGBColor,
};

use super::constants::{
    BASE_SIZE, HEAD_SIZE, PAN_JOINT_HEIGHT, PAN_JOINT_RADIUS, TILT_JOINT_LENGTH, TILT_JOINT_RADIUS,
    YOKE_HEIGHT, YOKE_RADIUS,
};
use super::math::{
    mat4_mul, mat4_rotation_euler, mat4_rotation_x, mat4_rotation_y, mat4_transform_dir, Vec3,
};

#[derive(Debug, Clone, Copy)]
pub(super) struct RenderFixture {
    pub(super) pos: Vec3,
    pub(super) rotation: Vec3,
    pub(super) pan_rad: f32,
    pub(super) tilt_rad: f32,
    pub(super) beam_len: f32,
    pub(super) beam_color: [f32; 3],
    pub(super) beam_strength: f32,
    pub(super) kind: RenderFixtureKind,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum RenderFixtureKind {
    MovingHead,
    TakeOffLogo,
    GenericLight,
    Dimmer,
}

#[derive(Clone, Copy, Debug)]
pub(super) struct BeamCone {
    pub(super) apex: Vec3,
    pub(super) dir: Vec3,
    pub(super) len: f32,
    pub(super) radius: f32,
    pub(super) color: [f32; 3],
    pub(super) strength: f32,
}

#[derive(Clone, Copy, Debug)]
pub(super) struct FixturePose {
    pub(super) base_center: Vec3,
    pub(super) pan_center: Vec3,
    pub(super) yoke_center: Vec3,
    pub(super) tilt_pivot: Vec3,
    pub(super) head_center: Vec3,
    pub(super) head_forward: Vec3,
    pub(super) lens_pos: Vec3,
    pub(super) base_rot: [f32; 16],
    pub(super) pan_rot: [f32; 16],
    pub(super) head_rot: [f32; 16],
}

pub(super) fn collect_fixtures(app_state: &crate::state::AppState) -> Vec<RenderFixture> {
    let mut fixtures = Vec::new();
    let scale = 0.1;

    // Source from the final, merged DMX output (base scene + overlays + palettes +
    // master alpha + overrides) rather than a single scene's pre-merge sink, and
    // decode each fixture's state back from the wire values via `state_from_dmx`.
    let engine = app_state.dmx_engine.read().unwrap();

    let mut min_x = f32::MAX;
    let mut max_x = f32::MIN;
    let mut min_z = f32::MAX;
    let mut max_z = f32::MIN;
    let mut min_y = f32::MAX;

    for group in engine.0.groups.values() {
        for fixture in group.fixtures.values() {
            let state = {
                let buffer = app_state.dmx_universes[fixture.universe_no]
                    .read()
                    .unwrap();
                fixture.state_from_dmx(&buffer.dmx_buffer)
            };
            let rgb: RGBColor = state.color.into();
            let alpha = (state.alpha as f32 / 255.0).clamp(0.0, 1.0);
            let pan_deg = (state.orientation.pan as f32 / 255.0) * 540.0 - 270.0;
            let tilt_deg = (state.orientation.tilt as f32 / 255.0) * 270.0 - 135.0;
            let pan_rad = pan_deg.to_radians();
            let tilt_rad = tilt_deg.to_radians();
            let rotation = Vec3::new(
                fixture.rotation.x.to_radians(),
                fixture.rotation.y.to_radians(),
                fixture.rotation.z.to_radians(),
            );
            let kind = match &fixture.type_ {
                FixtureType::MovingHead(_) => RenderFixtureKind::MovingHead,
                FixtureType::Light(light) => match light {
                    Light::TakeOffLogo => RenderFixtureKind::TakeOffLogo,
                    _ => RenderFixtureKind::GenericLight,
                },
                FixtureType::Dimmer(_) => RenderFixtureKind::Dimmer,
            };
            let pos = Vec3::new(
                fixture.pos.x as f32 * scale,
                fixture.pos.z as f32 * scale,
                fixture.pos.y as f32 * scale,
            );

            min_x = min_x.min(pos.x);
            max_x = max_x.max(pos.x);
            min_z = min_z.min(pos.z);
            max_z = max_z.max(pos.z);
            min_y = min_y.min(pos.y);

            let beam_len = if kind == RenderFixtureKind::MovingHead {
                10.0
            } else {
                0.0
            };

            fixtures.push(RenderFixture {
                pos,
                rotation,
                pan_rad,
                tilt_rad,
                beam_len,
                beam_color: [
                    (rgb.r as f32 / 255.0).clamp(0.0, 1.0),
                    (rgb.g as f32 / 255.0).clamp(0.0, 1.0),
                    (rgb.b as f32 / 255.0).clamp(0.0, 1.0),
                ],
                beam_strength: alpha,
                kind,
            });
        }
    }

    if fixtures.is_empty() {
        return fixtures;
    }

    let center_x = (min_x + max_x) * 0.5;
    let center_z = (min_z + max_z) * 0.5;
    let base_y = min_y.min(0.0);

    for fixture in fixtures.iter_mut() {
        fixture.pos.x -= center_x;
        fixture.pos.z -= center_z;
        fixture.pos.y -= base_y;
    }

    fixtures
}

pub(super) fn compute_fixture_pose(fixture: &RenderFixture) -> FixturePose {
    let base_rot = mat4_rotation_euler(fixture.rotation);
    let pan_rot = mat4_rotation_y(-fixture.pan_rad);
    let tilt_rot = mat4_rotation_x(-fixture.tilt_rad);
    let pan_matrix = mat4_mul(base_rot, pan_rot);
    let head_rot = mat4_mul(pan_matrix, tilt_rot);

    let up = mat4_transform_dir(base_rot, Vec3::new(0.0, 1.0, 0.0)).normalize();
    let base_origin = fixture.pos;
    let base_center = base_origin.add(up.scale(BASE_SIZE.y * 0.5));
    let pan_center = base_origin.add(up.scale(BASE_SIZE.y + PAN_JOINT_HEIGHT * 0.5));
    let yoke_center = base_origin.add(up.scale(BASE_SIZE.y + PAN_JOINT_HEIGHT + YOKE_HEIGHT * 0.5));
    let tilt_pivot = base_origin.add(up.scale(BASE_SIZE.y + PAN_JOINT_HEIGHT + YOKE_HEIGHT));
    let head_forward = mat4_transform_dir(head_rot, Vec3::new(0.0, 0.0, 1.0)).normalize();
    let head_center = tilt_pivot.add(head_forward.scale(HEAD_SIZE.z * 0.5));
    let lens_pos = tilt_pivot.add(head_forward.scale(HEAD_SIZE.z));

    FixturePose {
        base_center,
        pan_center,
        yoke_center,
        tilt_pivot,
        head_center,
        head_forward,
        lens_pos,
        base_rot,
        pan_rot: pan_matrix,
        head_rot,
    }
}
