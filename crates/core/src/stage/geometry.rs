//! Truss geometry in stage coordinates: branches, endpoints and fixture mount
//! points. Plain math on [`crate::stage`] types, so the engine can place
//! attached fixtures and the visualizer can draw and snap trusses.

use super::math::{
    mat4_mul, mat4_rotation_euler, mat4_transform_dir, mat4_transform_point, mat4_translation, Vec3,
};
use super::{
    FixtureTrussAttachment, StageObjectKind, StageScene, StageTransform, TrussPart, TrussSpec,
    TRUSS_PROFILE_SIZE_M,
};

#[derive(Debug, Clone, Copy)]
pub struct TrussEndpoint {
    pub position: Vec3,
    pub direction: Vec3,
}

#[derive(Debug, Clone, Copy)]
pub struct TrussBranch {
    pub start: Vec3,
    pub end: Vec3,
}

pub fn truss_branches(spec: TrussSpec) -> Vec<TrussBranch> {
    let y = TRUSS_PROFILE_SIZE_M * 0.5;
    match spec.part {
        TrussPart::Straight => vec![TrussBranch {
            start: Vec3::new(-spec.length_m * 0.5, y, 0.0),
            end: Vec3::new(spec.length_m * 0.5, y, 0.0),
        }],
        TrussPart::Corner90 => vec![
            branch(Vec3::new(0.0, y, 0.0), Vec3::new(0.5, y, 0.0)),
            branch(Vec3::new(0.0, y, 0.0), Vec3::new(0.0, y, 0.5)),
        ],
        TrussPart::TJunction => vec![
            branch(Vec3::new(0.0, y, 0.0), Vec3::new(-0.5, y, 0.0)),
            branch(Vec3::new(0.0, y, 0.0), Vec3::new(0.5, y, 0.0)),
            branch(Vec3::new(0.0, y, 0.0), Vec3::new(0.0, y, 0.5)),
        ],
        TrussPart::Cross => vec![
            branch(Vec3::new(0.0, y, 0.0), Vec3::new(-0.5, y, 0.0)),
            branch(Vec3::new(0.0, y, 0.0), Vec3::new(0.5, y, 0.0)),
            branch(Vec3::new(0.0, y, 0.0), Vec3::new(0.0, y, -0.5)),
            branch(Vec3::new(0.0, y, 0.0), Vec3::new(0.0, y, 0.5)),
        ],
        TrussPart::BasePlate => vec![branch(Vec3::new(0.0, 0.05, 0.0), Vec3::new(0.0, 0.3, 0.0))],
    }
}

pub fn truss_bounds(spec: TrussSpec) -> Vec3 {
    match spec.part {
        TrussPart::Straight => Vec3::new(spec.length_m, TRUSS_PROFILE_SIZE_M, TRUSS_PROFILE_SIZE_M),
        TrussPart::Corner90 | TrussPart::TJunction | TrussPart::Cross => {
            Vec3::new(1.0, TRUSS_PROFILE_SIZE_M, 1.0)
        }
        TrussPart::BasePlate => Vec3::new(0.56, 0.3, 0.56),
    }
}

pub fn truss_endpoints(spec: TrussSpec) -> Vec<TrussEndpoint> {
    let branches = truss_branches(spec);
    match spec.part {
        TrussPart::Straight => vec![
            endpoint(branches[0].start, branches[0].start.sub(branches[0].end)),
            endpoint(branches[0].end, branches[0].end.sub(branches[0].start)),
        ],
        TrussPart::BasePlate => vec![endpoint(
            branches[0].end,
            branches[0].end.sub(branches[0].start),
        )],
        _ => branches
            .iter()
            .map(|branch| endpoint(branch.end, branch.end.sub(branch.start)))
            .collect(),
    }
}

pub fn object_matrix(transform: &StageTransform) -> [f32; 16] {
    mat4_mul(
        mat4_translation(
            transform.translation[0],
            transform.translation[1],
            transform.translation[2],
        ),
        mat4_rotation_euler(Vec3::new(
            transform.rotation[0].to_radians(),
            transform.rotation[1].to_radians(),
            transform.rotation[2].to_radians(),
        )),
    )
}

pub fn world_endpoints(spec: TrussSpec, transform: &StageTransform) -> Vec<TrussEndpoint> {
    let matrix = object_matrix(transform);
    truss_endpoints(spec)
        .into_iter()
        .map(|endpoint| TrussEndpoint {
            position: mat4_transform_point(matrix, endpoint.position),
            direction: mat4_transform_dir(matrix, endpoint.direction).normalize(),
        })
        .collect()
}

pub fn world_branches(spec: TrussSpec, transform: &StageTransform) -> Vec<TrussBranch> {
    let matrix = object_matrix(transform);
    truss_branches(spec)
        .into_iter()
        .map(|branch| TrussBranch {
            start: mat4_transform_point(matrix, branch.start),
            end: mat4_transform_point(matrix, branch.end),
        })
        .collect()
}

pub fn world_mount(
    spec: TrussSpec,
    transform: &StageTransform,
    branch_index: u8,
    distance_m: f32,
) -> Option<Vec3> {
    let branch = *truss_branches(spec).get(branch_index as usize)?;
    let delta = branch.end.sub(branch.start);
    let length = delta.norm();
    if length <= 0.001 {
        return None;
    }
    let point = branch
        .start
        .add(delta.normalize().scale(distance_m.clamp(0.0, length)))
        .add(Vec3::new(0.0, -TRUSS_PROFILE_SIZE_M * 0.5, 0.0));
    Some(mat4_transform_point(object_matrix(transform), point))
}

pub fn nearest_mount(
    spec: TrussSpec,
    transform: &StageTransform,
    world_point: Vec3,
) -> Option<(u8, f32, Vec3)> {
    let matrix = object_matrix(transform);
    truss_branches(spec)
        .into_iter()
        .enumerate()
        .filter_map(|(index, branch)| {
            let start = mat4_transform_point(matrix, branch.start);
            let end = mat4_transform_point(matrix, branch.end);
            let delta = end.sub(start);
            let length = delta.norm();
            if length <= 0.001 {
                return None;
            }
            let direction = delta.scale(1.0 / length);
            let distance = world_point.sub(start).dot(direction).clamp(0.0, length);
            let center = start.add(direction.scale(distance));
            let mount = world_mount(spec, transform, index as u8, distance)?;
            Some((world_point.sub(center).norm(), index as u8, distance, mount))
        })
        .min_by(|a, b| a.0.total_cmp(&b.0))
        .map(|(_, branch, distance, mount)| (branch, distance, mount))
}

fn branch(start: Vec3, end: Vec3) -> TrussBranch {
    TrussBranch { start, end }
}

fn endpoint(position: Vec3, direction: Vec3) -> TrussEndpoint {
    TrussEndpoint {
        position,
        direction: direction.normalize(),
    }
}

/// Where fixture `attachment` hangs in `stage`: its world position and
/// rotation (degrees), or `None` if its truss or branch doesn't exist.
pub fn attachment_pose(
    stage: &StageScene,
    attachment: &FixtureTrussAttachment,
) -> Option<([f32; 3], [f32; 3])> {
    let object = stage.objects.get(&attachment.truss_id)?;
    let StageObjectKind::ProceduralTruss(spec) = object.kind else {
        return None;
    };
    let mount = world_mount(
        spec,
        &object.transform,
        attachment.branch,
        attachment.distance_m,
    )?;
    Some((
        [mount.x, mount.y, mount.z],
        [
            object.transform.rotation[0] + 180.0 + attachment.rotation_offset[0],
            object.transform.rotation[1] + attachment.rotation_offset[1],
            object.transform.rotation[2] + attachment.rotation_offset[2],
        ],
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn nearest_mount_clamps_to_branch_and_tracks_world_transform() {
        let spec = TrussSpec {
            length_m: 2.0,
            ..Default::default()
        };
        let transform = StageTransform {
            translation: [4.0, 5.0, 6.0],
            rotation: [0.0, 90.0, 0.0],
            scale: [1.0; 3],
        };

        let world_branch = world_branches(spec, &transform)[0];
        let direction = world_branch.end.sub(world_branch.start).normalize();
        let query = world_branch.end.add(direction.scale(100.0));
        let (branch, distance, mount) = nearest_mount(spec, &transform, query).unwrap();

        assert_eq!(branch, 0);
        assert!((distance - 2.0).abs() < 0.001);
        assert!((mount.x - world_branch.end.x).abs() < 0.001);
        assert!((mount.y - 5.0).abs() < 0.001);
        assert!((mount.z - world_branch.end.z).abs() < 0.001);
    }
}
