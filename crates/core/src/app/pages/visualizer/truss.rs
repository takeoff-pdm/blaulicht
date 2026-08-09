use crate::stage::{
    StageTransform, TrussPart, TrussProfile, TrussSpec, TRUSS_CHORD_DIAMETER_M,
    TRUSS_LENGTH_STEP_M, TRUSS_PROFILE_SIZE_M,
};

use super::math::{
    mat4_mul, mat4_rotation_euler, mat4_scale, mat4_transform_dir, mat4_transform_point,
    mat4_translation, Vec3,
};

pub(super) const MAX_TRUSS_PRIMITIVES_PER_SCENE: usize = 100_000;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(super) struct TrussGeometryKey {
    profile: u8,
    part: u8,
    quarter_metres: u16,
}

impl TrussGeometryKey {
    pub(super) fn new(spec: TrussSpec) -> Self {
        Self {
            profile: spec.profile as u8,
            part: spec.part as u8,
            quarter_metres: (spec.length_m / TRUSS_LENGTH_STEP_M)
                .round()
                .clamp(1.0, u16::MAX as f32) as u16,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum TrussPrimitiveKind {
    Cylinder,
    Cube,
}

#[derive(Debug, Clone, Copy)]
pub(super) struct TrussPrimitive {
    pub(super) kind: TrussPrimitiveKind,
    pub(super) model: [f32; 16],
}

#[derive(Debug, Clone, Copy)]
pub(super) struct TrussEndpoint {
    pub(super) position: Vec3,
    pub(super) direction: Vec3,
}

#[derive(Debug, Clone, Copy)]
pub(super) struct TrussBranch {
    pub(super) start: Vec3,
    pub(super) end: Vec3,
}

pub(super) fn truss_branches(spec: TrussSpec) -> Vec<TrussBranch> {
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

pub(super) fn truss_bounds(spec: TrussSpec) -> Vec3 {
    match spec.part {
        TrussPart::Straight => Vec3::new(spec.length_m, TRUSS_PROFILE_SIZE_M, TRUSS_PROFILE_SIZE_M),
        TrussPart::Corner90 | TrussPart::TJunction | TrussPart::Cross => {
            Vec3::new(1.0, TRUSS_PROFILE_SIZE_M, 1.0)
        }
        TrussPart::BasePlate => Vec3::new(0.56, 0.3, 0.56),
    }
}

pub(super) fn truss_endpoints(spec: TrussSpec) -> Vec<TrussEndpoint> {
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

pub(super) fn object_matrix(transform: &StageTransform) -> [f32; 16] {
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

pub(super) fn world_endpoints(spec: TrussSpec, transform: &StageTransform) -> Vec<TrussEndpoint> {
    let matrix = object_matrix(transform);
    truss_endpoints(spec)
        .into_iter()
        .map(|endpoint| TrussEndpoint {
            position: mat4_transform_point(matrix, endpoint.position),
            direction: mat4_transform_dir(matrix, endpoint.direction).normalize(),
        })
        .collect()
}

pub(super) fn world_branches(spec: TrussSpec, transform: &StageTransform) -> Vec<TrussBranch> {
    let matrix = object_matrix(transform);
    truss_branches(spec)
        .into_iter()
        .map(|branch| TrussBranch {
            start: mat4_transform_point(matrix, branch.start),
            end: mat4_transform_point(matrix, branch.end),
        })
        .collect()
}

pub(super) fn world_mount(
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

pub(super) fn nearest_mount(
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

pub(super) fn generate_truss(spec: TrussSpec) -> Vec<TrussPrimitive> {
    let mut primitives = Vec::new();
    if spec.part == TrussPart::BasePlate {
        primitives.push(TrussPrimitive {
            kind: TrussPrimitiveKind::Cube,
            model: mat4_mul(
                mat4_translation(0.0, 0.025, 0.0),
                mat4_scale(0.28, 0.025, 0.28),
            ),
        });
    }
    for branch in truss_branches(spec) {
        generate_branch(spec.profile, branch, &mut primitives);
    }
    primitives
}

fn generate_branch(
    profile: TrussProfile,
    branch: TrussBranch,
    primitives: &mut Vec<TrussPrimitive>,
) {
    let direction = branch.end.sub(branch.start);
    let length = direction.norm();
    if !length.is_finite() || length <= 0.001 {
        return;
    }
    let forward = direction.normalize();
    let reference = if forward.y.abs() > 0.9 {
        Vec3::new(1.0, 0.0, 0.0)
    } else {
        Vec3::new(0.0, 1.0, 0.0)
    };
    let lateral = reference.cross(forward).normalize();
    let vertical = forward.cross(lateral).normalize();
    let half = (TRUSS_PROFILE_SIZE_M - TRUSS_CHORD_DIAMETER_M) * 0.5;
    let offsets: Vec<Vec3> = match profile {
        TrussProfile::Square => vec![
            lateral.scale(-half).add(vertical.scale(-half)),
            lateral.scale(-half).add(vertical.scale(half)),
            lateral.scale(half).add(vertical.scale(-half)),
            lateral.scale(half).add(vertical.scale(half)),
        ],
        TrussProfile::Triangular => vec![
            vertical.scale(half),
            lateral.scale(-half).add(vertical.scale(-half)),
            lateral.scale(half).add(vertical.scale(-half)),
        ],
        TrussProfile::Ladder => vec![vertical.scale(-half), vertical.scale(half)],
    };
    let faces: &[(usize, usize)] = match profile {
        TrussProfile::Square => &[(0, 1), (1, 3), (3, 2), (2, 0)],
        TrussProfile::Triangular => &[(0, 1), (1, 2), (2, 0)],
        TrussProfile::Ladder => &[(0, 1)],
    };
    let chord_radius = TRUSS_CHORD_DIAMETER_M * 0.5;
    let brace_radius = chord_radius * 0.42;

    for offset in &offsets {
        primitives.push(tube(
            branch.start.add(*offset),
            branch.end.add(*offset),
            chord_radius,
        ));
    }

    let bays = (length / TRUSS_LENGTH_STEP_M).ceil().max(1.0) as usize;
    for bay in 0..bays {
        let t0 = bay as f32 / bays as f32;
        let t1 = (bay + 1) as f32 / bays as f32;
        let p0 = branch.start.add(direction.scale(t0));
        let p1 = branch.start.add(direction.scale(t1));
        for (face_index, (a, b)) in faces.iter().copied().enumerate() {
            let reverse = (bay + face_index) % 2 == 1;
            let (from, to) = if reverse {
                (p0.add(offsets[b]), p1.add(offsets[a]))
            } else {
                (p0.add(offsets[a]), p1.add(offsets[b]))
            };
            primitives.push(tube(from, to, brace_radius));
        }
    }

    let mut boundary = 0.0_f32;
    while boundary <= length + 0.001 {
        let center = branch.start.add(forward.scale(boundary.min(length)));
        for (a, b) in faces {
            primitives.push(tube(
                center.add(offsets[*a]),
                center.add(offsets[*b]),
                brace_radius,
            ));
        }
        boundary += TRUSS_LENGTH_STEP_M;
    }
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

fn tube(start: Vec3, end: Vec3, radius: f32) -> TrussPrimitive {
    let delta = end.sub(start);
    let length = delta.norm().max(0.001);
    let axis = delta.normalize();
    let reference = if axis.y.abs() > 0.9 {
        Vec3::new(1.0, 0.0, 0.0)
    } else {
        Vec3::new(0.0, 1.0, 0.0)
    };
    let x = reference.cross(axis).normalize().scale(radius);
    let z = axis.cross(x.normalize()).normalize().scale(radius);
    let y = axis.scale(length * 0.5);
    let midpoint = start.add(end).scale(0.5);
    TrussPrimitive {
        kind: TrussPrimitiveKind::Cylinder,
        model: [
            x.x, x.y, x.z, 0.0, y.x, y.y, y.z, 0.0, z.x, z.y, z.z, 0.0, midpoint.x, midpoint.y,
            midpoint.z, 1.0,
        ],
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn generated_profiles_are_finite_and_have_expected_endpoints() {
        for profile in [
            TrussProfile::Square,
            TrussProfile::Triangular,
            TrussProfile::Ladder,
        ] {
            let spec = TrussSpec {
                profile,
                ..Default::default()
            };
            let primitives = generate_truss(spec);
            assert!(!primitives.is_empty());
            assert!(primitives
                .iter()
                .all(|primitive| primitive.model.iter().all(|value| value.is_finite())));
            assert_eq!(truss_endpoints(spec).len(), 2);
        }
    }

    #[test]
    fn every_catalog_part_generates_geometry_and_endpoints() {
        for part in [
            TrussPart::Straight,
            TrussPart::Corner90,
            TrussPart::TJunction,
            TrussPart::Cross,
            TrussPart::BasePlate,
        ] {
            let spec = TrussSpec {
                part,
                ..Default::default()
            };
            assert!(!generate_truss(spec).is_empty());
            assert_eq!(truss_endpoints(spec).len(), spec.endpoint_count() as usize);
        }
    }

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
