use blaulicht_shared::{
    fixture::{light::Light, FixtureType},
    RGBColor,
};
use std::collections::HashMap;
use std::sync::Arc;

use crate::stage::{StageObjectKind, StageScene};

use super::constants::{BASE_SIZE, HEAD_SIZE, PAN_JOINT_HEIGHT, YOKE_HEIGHT};
use super::math::{
    mat4_mul, mat4_rotation_euler, mat4_rotation_x, mat4_rotation_y, mat4_transform_dir, Vec3,
};

#[derive(Debug, Clone)]
pub(super) struct RenderFixture {
    pub(super) id: (u8, u8),
    pub(super) name: String,
    pub(super) pos: Vec3,
    pub(super) rotation: Vec3,
    pub(super) pan_rad: f32,
    pub(super) tilt_rad: f32,
    pub(super) beam_len: f32,
    pub(super) beam_color: [f32; 3],
    pub(super) beam_strength: f32,
    pub(super) kind: RenderFixtureKind,
}

#[derive(Debug, Clone)]
pub(super) struct RenderStageObject {
    pub(super) id: u64,
    pub(super) pos: Vec3,
    pub(super) rotation: Vec3,
    pub(super) scale: Vec3,
    pub(super) color: [f32; 3],
    pub(super) model_key: Option<String>,
    pub(super) cpu_model: Option<Arc<three_d_asset::Model>>,
}

#[derive(Debug, Clone, Copy, Default)]
pub(super) struct SceneBounds {
    pub(super) radius: f32,
}

#[derive(Debug, Clone)]
pub(super) struct RenderSceneSnapshot {
    pub(super) fixtures: Arc<[RenderFixture]>,
    pub(super) stage_objects: Arc<[RenderStageObject]>,
    pub(super) bounds: SceneBounds,
    pub(super) skipped_fixtures: usize,
    pub(super) error: Option<String>,
}

impl Default for RenderSceneSnapshot {
    fn default() -> Self {
        Self {
            fixtures: Arc::from([]),
            stage_objects: Arc::from([]),
            bounds: SceneBounds { radius: 1.0 },
            skipped_fixtures: 0,
            error: None,
        }
    }
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

pub(super) fn collect_fixtures(
    app_state: &crate::state::AppState,
    stage: &StageScene,
    model_cache: &HashMap<String, Arc<three_d_asset::Model>>,
) -> RenderSceneSnapshot {
    let mut fixtures = Vec::new();

    // Source from the final, merged DMX output (base scene + overlays + palettes +
    // master alpha + overrides) rather than a single scene's pre-merge sink, and
    // decode each fixture's state back from the wire values via `state_from_dmx`.
    let engine = match app_state.dmx_engine.read() {
        Ok(engine) => engine,
        Err(_) => {
            return RenderSceneSnapshot {
                error: Some("DMX engine state is temporarily unavailable".to_string()),
                ..Default::default()
            };
        }
    };

    let mut skipped_fixtures = 0;

    for (group_id, group) in &engine.0.groups {
        for (fixture_id, fixture) in &group.fixtures {
            let Some(universe) = app_state.dmx_universes.get(fixture.universe_no) else {
                skipped_fixtures += 1;
                continue;
            };
            let state = {
                let Ok(buffer) = universe.read() else {
                    skipped_fixtures += 1;
                    continue;
                };
                fixture
                    .state_from_dmx(&buffer.dmx_buffer)
                    .resolve(&engine.0.palettes)
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
            let pos = Vec3::new(fixture.pos.x, fixture.pos.y, fixture.pos.z);

            if ![pos.x, pos.y, pos.z, rotation.x, rotation.y, rotation.z]
                .iter()
                .all(|value| value.is_finite())
            {
                skipped_fixtures += 1;
                continue;
            }

            let beam_len = if kind == RenderFixtureKind::MovingHead {
                10.0
            } else {
                0.0
            };

            fixtures.push(RenderFixture {
                id: (*group_id, *fixture_id),
                name: fixture.name.clone(),
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

    let stage_objects: Vec<RenderStageObject> = stage
        .objects
        .iter()
        .filter(|(_, object)| object.visible)
        .map(|(id, object)| {
            let model_key = match object.kind {
                StageObjectKind::ImportedModel { asset_id } => stage
                    .assets
                    .get(&asset_id)
                    .map(|asset| asset.content_hash.clone()),
                _ => None,
            };
            let cpu_model = model_key
                .as_ref()
                .and_then(|key| model_cache.get(key))
                .cloned();
            RenderStageObject {
                id: *id,
                pos: Vec3::new(
                    object.transform.translation[0],
                    object.transform.translation[1],
                    object.transform.translation[2],
                ),
                rotation: Vec3::new(
                    object.transform.rotation[0].to_radians(),
                    object.transform.rotation[1].to_radians(),
                    object.transform.rotation[2].to_radians(),
                ),
                scale: Vec3::new(
                    object.transform.scale[0],
                    object.transform.scale[1],
                    object.transform.scale[2],
                ),
                color: object.color.map(|channel| channel as f32 / 255.0),
                model_key,
                cpu_model,
            }
        })
        .collect();

    if fixtures.is_empty() {
        let radius = stage_objects
            .iter()
            .map(|object| object.pos.norm() + object.scale.norm() * 0.5)
            .fold(1.0_f32, f32::max);
        return RenderSceneSnapshot {
            fixtures: fixtures.into(),
            stage_objects: stage_objects.into(),
            bounds: SceneBounds { radius },
            skipped_fixtures,
            ..Default::default()
        };
    }

    let mut radius: f32 = 1.0;
    for fixture in &fixtures {
        // Camera framing is based on physical fixtures. Beam reach is an
        // optional overlay and must not make the fixtures appear tiny.
        radius = radius.max(fixture.pos.norm() + 1.5);
    }
    for object in &stage_objects {
        radius = radius.max(object.pos.norm() + object.scale.norm() * 0.5);
    }

    RenderSceneSnapshot {
        fixtures: fixtures.into(),
        stage_objects: stage_objects.into(),
        bounds: SceneBounds { radius },
        skipped_fixtures,
        error: None,
    }
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
