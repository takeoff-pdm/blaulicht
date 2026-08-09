#[derive(Clone, Copy, Debug)]
pub(super) struct Vec3 {
    pub(super) x: f32,
    pub(super) y: f32,
    pub(super) z: f32,
}

impl Vec3 {
    pub(super) fn new(x: f32, y: f32, z: f32) -> Self {
        Self { x, y, z }
    }

    pub(super) fn sub(self, rhs: Self) -> Self {
        Self::new(self.x - rhs.x, self.y - rhs.y, self.z - rhs.z)
    }

    pub(super) fn cross(self, rhs: Self) -> Self {
        Self::new(
            self.y * rhs.z - self.z * rhs.y,
            self.z * rhs.x - self.x * rhs.z,
            self.x * rhs.y - self.y * rhs.x,
        )
    }

    pub(super) fn dot(self, rhs: Self) -> f32 {
        self.x * rhs.x + self.y * rhs.y + self.z * rhs.z
    }

    pub(super) fn norm(self) -> f32 {
        self.dot(self).sqrt()
    }

    pub(super) fn normalize(self) -> Self {
        let n = self.norm();
        if !n.is_finite() || n <= f32::EPSILON {
            Self::new(0.0, 0.0, 0.0)
        } else {
            Self::new(self.x / n, self.y / n, self.z / n)
        }
    }

    pub(super) fn add(self, rhs: Self) -> Self {
        Self::new(self.x + rhs.x, self.y + rhs.y, self.z + rhs.z)
    }

    pub(super) fn scale(self, s: f32) -> Self {
        Self::new(self.x * s, self.y * s, self.z * s)
    }
}

pub(super) fn basis_from_dir(dir: Vec3) -> (Vec3, Vec3) {
    let forward = dir.normalize();
    let up_ref = if forward.y.abs() > 0.95 {
        Vec3::new(1.0, 0.0, 0.0)
    } else {
        Vec3::new(0.0, 1.0, 0.0)
    };
    let right = up_ref.cross(forward).normalize();
    let up = forward.cross(right).normalize();
    (right, up)
}

pub(super) fn mat4_perspective(fov_y: f32, aspect: f32, near: f32, far: f32) -> [f32; 16] {
    if !fov_y.is_finite()
        || !aspect.is_finite()
        || !near.is_finite()
        || !far.is_finite()
        || aspect <= f32::EPSILON
        || near <= f32::EPSILON
        || far <= near
    {
        return mat4_identity();
    }
    let f = 1.0 / (fov_y / 2.0).tan();
    let nf = 1.0 / (near - far);
    [
        f / aspect,
        0.0,
        0.0,
        0.0,
        0.0,
        f,
        0.0,
        0.0,
        0.0,
        0.0,
        (far + near) * nf,
        -1.0,
        0.0,
        0.0,
        (2.0 * far * near) * nf,
        0.0,
    ]
}

pub(super) fn mat4_identity() -> [f32; 16] {
    [
        1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0,
    ]
}

pub(super) fn mat4_translation(x: f32, y: f32, z: f32) -> [f32; 16] {
    [
        1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, x, y, z, 1.0,
    ]
}

pub(super) fn mat4_scale(x: f32, y: f32, z: f32) -> [f32; 16] {
    [
        x, 0.0, 0.0, 0.0, 0.0, y, 0.0, 0.0, 0.0, 0.0, z, 0.0, 0.0, 0.0, 0.0, 1.0,
    ]
}

pub(super) fn mat4_rotation_y(angle: f32) -> [f32; 16] {
    let c = angle.cos();
    let s = angle.sin();
    [
        c, 0.0, -s, 0.0, 0.0, 1.0, 0.0, 0.0, s, 0.0, c, 0.0, 0.0, 0.0, 0.0, 1.0,
    ]
}

pub(super) fn mat4_rotation_x(angle: f32) -> [f32; 16] {
    let c = angle.cos();
    let s = angle.sin();
    [
        1.0, 0.0, 0.0, 0.0, 0.0, c, s, 0.0, 0.0, -s, c, 0.0, 0.0, 0.0, 0.0, 1.0,
    ]
}

pub(super) fn mat4_rotation_z(angle: f32) -> [f32; 16] {
    let c = angle.cos();
    let s = angle.sin();
    [
        c, s, 0.0, 0.0, -s, c, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0,
    ]
}

pub(super) fn mat4_rotation_euler(rot: Vec3) -> [f32; 16] {
    mat4_mul(
        mat4_rotation_z(rot.z),
        mat4_mul(mat4_rotation_y(rot.y), mat4_rotation_x(rot.x)),
    )
}

pub(super) fn mat4_transform_dir(m: [f32; 16], v: Vec3) -> Vec3 {
    Vec3::new(
        m[0] * v.x + m[4] * v.y + m[8] * v.z,
        m[1] * v.x + m[5] * v.y + m[9] * v.z,
        m[2] * v.x + m[6] * v.y + m[10] * v.z,
    )
}

pub(super) fn mat4_transform_point(m: [f32; 16], v: Vec3) -> Vec3 {
    Vec3::new(
        m[0] * v.x + m[4] * v.y + m[8] * v.z + m[12],
        m[1] * v.x + m[5] * v.y + m[9] * v.z + m[13],
        m[2] * v.x + m[6] * v.y + m[10] * v.z + m[14],
    )
}

pub(super) fn project_point(
    projection: [f32; 16],
    view: [f32; 16],
    point: Vec3,
) -> Option<[f32; 3]> {
    let matrix = mat4_mul(projection, view);
    let x = matrix[0] * point.x + matrix[4] * point.y + matrix[8] * point.z + matrix[12];
    let y = matrix[1] * point.x + matrix[5] * point.y + matrix[9] * point.z + matrix[13];
    let z = matrix[2] * point.x + matrix[6] * point.y + matrix[10] * point.z + matrix[14];
    let w = matrix[3] * point.x + matrix[7] * point.y + matrix[11] * point.z + matrix[15];
    if ![x, y, z, w].iter().all(|value| value.is_finite()) || w <= f32::EPSILON {
        return None;
    }
    let projected = [x / w, y / w, z / w];
    (projected[2] >= -1.0 && projected[2] <= 1.0).then_some(projected)
}

pub(super) fn mat4_look_at(eye: Vec3, target: Vec3, up: Vec3) -> [f32; 16] {
    let mut f = target.sub(eye).normalize();
    if f.norm() <= f32::EPSILON {
        f = Vec3::new(0.0, 0.0, -1.0);
    }
    let mut up = up.normalize();
    if up.norm() <= f32::EPSILON || f.dot(up).abs() > 0.999 {
        up = if f.y.abs() < 0.999 {
            Vec3::new(0.0, 1.0, 0.0)
        } else {
            Vec3::new(1.0, 0.0, 0.0)
        };
    }
    let mut s = f.cross(up).normalize();
    if s.norm() <= f32::EPSILON {
        s = Vec3::new(1.0, 0.0, 0.0);
    }
    let u = s.cross(f);

    [
        s.x,
        u.x,
        -f.x,
        0.0,
        s.y,
        u.y,
        -f.y,
        0.0,
        s.z,
        u.z,
        -f.z,
        0.0,
        -s.dot(eye),
        -u.dot(eye),
        f.dot(eye),
        1.0,
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn look_at_is_finite_for_degenerate_inputs() {
        let matrix = mat4_look_at(
            Vec3::new(0.0, 0.0, 0.0),
            Vec3::new(0.0, 0.0, 0.0),
            Vec3::new(0.0, 0.0, 0.0),
        );
        assert!(matrix.iter().all(|value| value.is_finite()));
    }

    #[test]
    fn invalid_projection_uses_identity() {
        assert_eq!(mat4_perspective(1.0, 0.0, 0.1, 100.0), mat4_identity());
    }

    #[test]
    fn projects_visible_point_to_view_center() {
        let projection = mat4_perspective(45.0_f32.to_radians(), 1.0, 0.1, 100.0);
        let view = mat4_look_at(
            Vec3::new(0.0, 0.0, 5.0),
            Vec3::new(0.0, 0.0, 0.0),
            Vec3::new(0.0, 1.0, 0.0),
        );
        let point = project_point(projection, view, Vec3::new(0.0, 0.0, 0.0)).unwrap();
        assert!(point[0].abs() < 1e-5);
        assert!(point[1].abs() < 1e-5);
    }
}

pub(super) fn mat4_mul(a: [f32; 16], b: [f32; 16]) -> [f32; 16] {
    let mut out = [0.0; 16];
    for col in 0..4 {
        for row in 0..4 {
            out[col * 4 + row] = a[0 * 4 + row] * b[col * 4 + 0]
                + a[1 * 4 + row] * b[col * 4 + 1]
                + a[2 * 4 + row] * b[col * 4 + 2]
                + a[3 * 4 + row] * b[col * 4 + 3];
        }
    }
    out
}
