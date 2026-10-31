use blaulicht_engine::stage::{
    TrussPart, TrussProfile, TrussSpec, TRUSS_CHORD_DIAMETER_M, TRUSS_LENGTH_STEP_M,
    TRUSS_PROFILE_SIZE_M,
};

use super::math::{mat4_mul, mat4_scale, mat4_translation, Vec3};

pub(super) use blaulicht_engine::stage::geometry::{
    nearest_mount, truss_bounds, truss_branches, world_branches, world_endpoints, TrussBranch,
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
    use blaulicht_engine::stage::geometry::truss_endpoints;

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
}
