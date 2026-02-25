use std::sync::Arc;

use egui_glow::glow::{self, HasContext};

use super::constants::{
    BEAM_ANGLE_DEG, DEBUG_TEXT_OVERLAY, FIXTURE_BODY_COLOR, FIXTURE_EDGE_COLOR,
    GENERIC_FIXTURE_SIZE, HEAD_BODY_COLOR, JOINT_COLOR, TAKEOFF_BACK_COLOR, TAKEOFF_LOGO_BACK_PADDING,
    TAKEOFF_LOGO_BACK_THICKNESS, TAKEOFF_LOGO_DEPTH, TAKEOFF_LOGO_SCALE, TAKEOFF_LOGO_Y_OFFSET,
    TAKEOFF_TEXT, TAKEOFF_TEXT_HEIGHT, TAKEOFF_TEXT_Y, TILT_JOINT_LENGTH, TILT_JOINT_RADIUS,
    YOKE_HEIGHT, YOKE_RADIUS, BASE_SIZE, PAN_JOINT_HEIGHT, PAN_JOINT_RADIUS, HEAD_SIZE,
};
use super::data::{
    compute_fixture_pose, BeamCone, FixturePose, RenderFixture, RenderFixtureKind,
};
use super::gl::{
    create_axes, create_cube, create_cube_edges, create_cylinder, create_dynamic_mesh,
    create_font_texture, create_grid, create_program, create_quad, create_text_buffers,
    create_text_program,
};
use super::math::{
    basis_from_dir, mat4_identity, mat4_look_at, mat4_mul, mat4_perspective, mat4_rotation_euler,
    mat4_rotation_z, mat4_scale, mat4_translation, Vec3,
};
use super::text::{build_text_vertices, text_line_width};
use super::VisualizerSettings;

pub(super) struct GlowRenderer {
    gl: Arc<glow::Context>,
    program: glow::Program,
    text_program: glow::Program,
    cube_vao: glow::VertexArray,
    cube_vbo: glow::Buffer,
    cube_ebo: glow::Buffer,
    cube_index_count: i32,
    text_vao: glow::VertexArray,
    text_vbo: glow::Buffer,
    text_texture: glow::Texture,
    grid_vao: glow::VertexArray,
    grid_vbo: glow::Buffer,
    grid_vertex_count: i32,
    axes_vao: glow::VertexArray,
    axes_vbo: glow::Buffer,
    axes_vertex_count: i32,
    cylinder_vao: glow::VertexArray,
    cylinder_vbo: glow::Buffer,
    cylinder_ebo: glow::Buffer,
    cylinder_index_count: i32,
    cone_vao: glow::VertexArray,
    cone_vbo: glow::Buffer,
    cone_vertex_count: i32,
    quad_vao: glow::VertexArray,
    quad_vbo: glow::Buffer,
    quad_vertex_count: i32,
    cube_edges_vao: glow::VertexArray,
    cube_edges_vbo: glow::Buffer,
    cube_edges_vertex_count: i32,
    u_mvp: Option<glow::UniformLocation>,
    u_brightness: Option<glow::UniformLocation>,
    u_color: Option<glow::UniformLocation>,
    u_use_vertex_color: Option<glow::UniformLocation>,
    text_u_mvp: Option<glow::UniformLocation>,
    text_u_color: Option<glow::UniformLocation>,
    text_u_tex: Option<glow::UniformLocation>,
}

impl GlowRenderer {
    pub(super) fn new(gl: &Arc<glow::Context>) -> Result<Self, String> {
        unsafe {
            let program = create_program(gl)?;
            let text_program = create_text_program(gl)?;
            let (cube_vao, cube_vbo, cube_ebo, cube_index_count) = create_cube(gl)?;
            let (text_vao, text_vbo) = create_text_buffers(gl)?;
            let text_texture = create_font_texture(gl)?;
            let (cube_edges_vao, cube_edges_vbo, cube_edges_vertex_count) =
                create_cube_edges(gl)?;
            let (grid_vao, grid_vbo, grid_vertex_count) = create_grid(gl)?;
            let (axes_vao, axes_vbo, axes_vertex_count) = create_axes(gl)?;
            let (cylinder_vao, cylinder_vbo, cylinder_ebo, cylinder_index_count) =
                create_cylinder(gl, 24)?;
            let (cone_vao, cone_vbo) = create_dynamic_mesh(gl)?;
            let (quad_vao, quad_vbo, quad_vertex_count) = create_quad(gl)?;
            let u_mvp = gl.get_uniform_location(program, "u_mvp");
            let u_brightness = gl.get_uniform_location(program, "u_brightness");
            let u_color = gl.get_uniform_location(program, "u_color");
            let u_use_vertex_color = gl.get_uniform_location(program, "u_use_vertex_color");
            let text_u_mvp = gl.get_uniform_location(text_program, "u_mvp");
            let text_u_color = gl.get_uniform_location(text_program, "u_color");
            let text_u_tex = gl.get_uniform_location(text_program, "u_tex");

            Ok(Self {
                gl: gl.clone(),
                program,
                text_program,
                cube_vao,
                cube_vbo,
                cube_ebo,
                cube_index_count,
                text_vao,
                text_vbo,
                text_texture,
                grid_vao,
                grid_vbo,
                grid_vertex_count,
                axes_vao,
                axes_vbo,
                axes_vertex_count,
                cylinder_vao,
                cylinder_vbo,
                cylinder_ebo,
                cylinder_index_count,
                cone_vao,
                cone_vbo,
                cone_vertex_count: 0,
                quad_vao,
                quad_vbo,
                quad_vertex_count,
                cube_edges_vao,
                cube_edges_vbo,
                cube_edges_vertex_count,
                u_mvp,
                u_brightness,
                u_color,
                u_use_vertex_color,
                text_u_mvp,
                text_u_color,
                text_u_tex,
            })
        }
    }

    pub(super) fn render(
        &mut self,
        gl: &Arc<glow::Context>,
        settings: &VisualizerSettings,
        yaw: f32,
        pitch: f32,
        radius: f32,
        _time: f32,
        fixtures: &[RenderFixture],
        info: egui::PaintCallbackInfo,
    ) {
        let viewport = info.viewport_in_pixels();
        if viewport.width_px <= 0 || viewport.height_px <= 0 {
            return;
        }

        unsafe {
            let clip = info.clip_rect_in_pixels();
            gl.enable(glow::SCISSOR_TEST);
            gl.scissor(
                clip.left_px,
                clip.from_bottom_px,
                clip.width_px,
                clip.height_px,
            );
            gl.viewport(
                viewport.left_px,
                viewport.from_bottom_px,
                viewport.width_px,
                viewport.height_px,
            );

            gl.enable(glow::DEPTH_TEST);
            gl.depth_func(glow::LEQUAL);

            gl.clear_color(0.05, 0.05, 0.08, 1.0);
            gl.clear(glow::COLOR_BUFFER_BIT | glow::DEPTH_BUFFER_BIT);

            gl.use_program(Some(self.program));

            if let Some(loc) = &self.u_brightness {
                gl.uniform_1_f32(Some(loc), settings.brightness);
            }

            let aspect = viewport.width_px as f32 / viewport.height_px as f32;
            let projection = mat4_perspective(45.0_f32.to_radians(), aspect, 0.1, 200.0);

            let eye = {
                let cy = yaw.cos();
                let sy = yaw.sin();
                let cp = pitch.cos();
                let sp = pitch.sin();
                Vec3::new(radius * sy * cp, radius * sp, radius * cy * cp)
            };
            let view = mat4_look_at(eye, Vec3::new(0.0, 0.0, 0.0), Vec3::new(0.0, 1.0, 0.0));

            let mut head_fixtures: Vec<&RenderFixture> = Vec::new();
            let mut poses: Vec<FixturePose> = Vec::new();
            let mut cones: Vec<BeamCone> = Vec::new();
            let mut takeoff_fixtures: Vec<&RenderFixture> = Vec::new();
            let mut generic_fixtures: Vec<&RenderFixture> = Vec::new();
            for fixture in fixtures {
                match fixture.kind {
                    RenderFixtureKind::MovingHead => {
                        let pose = compute_fixture_pose(fixture);
                        let beam_angle = BEAM_ANGLE_DEG.to_radians();
                        let radius = (fixture.beam_len * beam_angle.tan()).max(0.05);
                        if fixture.beam_strength > 0.01 {
                            cones.push(BeamCone {
                                apex: pose.lens_pos,
                                dir: pose.head_forward,
                                len: fixture.beam_len,
                                radius,
                                color: fixture.beam_color,
                                strength: fixture.beam_strength,
                            });
                        }
                        head_fixtures.push(fixture);
                        poses.push(pose);
                    }
                    RenderFixtureKind::TakeOffLogo => {
                        takeoff_fixtures.push(fixture);
                    }
                    RenderFixtureKind::GenericLight | RenderFixtureKind::Dimmer => {
                        generic_fixtures.push(fixture);
                    }
                }
            }

            if settings.show_grid {
                set_use_vertex_color(gl, &self.u_use_vertex_color, true);
                gl.bind_vertex_array(Some(self.grid_vao));
                let mvp = mat4_mul(projection, mat4_mul(view, mat4_identity()));
                if let Some(loc) = &self.u_mvp {
                    gl.uniform_matrix_4_f32_slice(Some(loc), false, &mvp);
                }
                gl.draw_arrays(glow::LINES, 0, self.grid_vertex_count);
            }

            if settings.show_axes {
                set_use_vertex_color(gl, &self.u_use_vertex_color, true);
                gl.bind_vertex_array(Some(self.axes_vao));
                let mvp = mat4_mul(projection, mat4_mul(view, mat4_identity()));
                if let Some(loc) = &self.u_mvp {
                    gl.uniform_matrix_4_f32_slice(Some(loc), false, &mvp);
                }
                gl.draw_arrays(glow::LINES, 0, self.axes_vertex_count);
            }

            self.update_cones(gl, &cones);
            if self.cone_vertex_count > 0 {
                set_use_vertex_color(gl, &self.u_use_vertex_color, true);
                gl.bind_vertex_array(Some(self.cone_vao));
                let mvp = mat4_mul(projection, mat4_mul(view, mat4_identity()));
                if let Some(loc) = &self.u_mvp {
                    gl.uniform_matrix_4_f32_slice(Some(loc), false, &mvp);
                }
                gl.enable(glow::BLEND);
                gl.blend_func(glow::ONE, glow::ONE);
                gl.depth_mask(false);
                gl.draw_arrays(glow::TRIANGLES, 0, self.cone_vertex_count);
                gl.depth_mask(true);
                gl.disable(glow::BLEND);
            }

            if !head_fixtures.is_empty() {
                set_use_vertex_color(gl, &self.u_use_vertex_color, false);
                let index_stride = std::mem::size_of::<u16>() as i32;
                for (fixture, pose) in head_fixtures.iter().zip(poses.iter()) {
                    gl.bind_vertex_array(Some(self.cube_vao));

                    if let Some(loc) = &self.u_color {
                        gl.uniform_3_f32(
                            Some(loc),
                            FIXTURE_BODY_COLOR[0],
                            FIXTURE_BODY_COLOR[1],
                            FIXTURE_BODY_COLOR[2],
                        );
                    }
                    let base_model = mat4_mul(
                        mat4_translation(
                            pose.base_center.x,
                            pose.base_center.y,
                            pose.base_center.z,
                        ),
                        mat4_mul(
                            pose.base_rot,
                            mat4_scale(BASE_SIZE.x, BASE_SIZE.y, BASE_SIZE.z),
                        ),
                    );
                    let base_mvp = mat4_mul(projection, mat4_mul(view, base_model));
                    if let Some(loc) = &self.u_mvp {
                        gl.uniform_matrix_4_f32_slice(Some(loc), false, &base_mvp);
                    }
                    gl.draw_elements(
                        glow::TRIANGLES,
                        self.cube_index_count,
                        glow::UNSIGNED_SHORT,
                        0,
                    );

                    gl.bind_vertex_array(Some(self.cylinder_vao));
                    if let Some(loc) = &self.u_color {
                        gl.uniform_3_f32(
                            Some(loc),
                            JOINT_COLOR[0],
                            JOINT_COLOR[1],
                            JOINT_COLOR[2],
                        );
                    }
                    let pan_model = mat4_mul(
                        mat4_translation(pose.pan_center.x, pose.pan_center.y, pose.pan_center.z),
                        mat4_mul(
                            pose.pan_rot,
                            mat4_scale(
                                PAN_JOINT_RADIUS,
                                PAN_JOINT_HEIGHT * 0.5,
                                PAN_JOINT_RADIUS,
                            ),
                        ),
                    );
                    let pan_mvp = mat4_mul(projection, mat4_mul(view, pan_model));
                    if let Some(loc) = &self.u_mvp {
                        gl.uniform_matrix_4_f32_slice(Some(loc), false, &pan_mvp);
                    }
                    gl.draw_elements(
                        glow::TRIANGLES,
                        self.cylinder_index_count,
                        glow::UNSIGNED_SHORT,
                        0,
                    );

                    let yoke_model = mat4_mul(
                        mat4_translation(pose.yoke_center.x, pose.yoke_center.y, pose.yoke_center.z),
                        mat4_mul(
                            pose.pan_rot,
                            mat4_scale(YOKE_RADIUS, YOKE_HEIGHT * 0.5, YOKE_RADIUS),
                        ),
                    );
                    let yoke_mvp = mat4_mul(projection, mat4_mul(view, yoke_model));
                    if let Some(loc) = &self.u_mvp {
                        gl.uniform_matrix_4_f32_slice(Some(loc), false, &yoke_mvp);
                    }
                    gl.draw_elements(
                        glow::TRIANGLES,
                        self.cylinder_index_count,
                        glow::UNSIGNED_SHORT,
                        0,
                    );

                    let tilt_model = mat4_mul(
                        mat4_translation(pose.tilt_pivot.x, pose.tilt_pivot.y, pose.tilt_pivot.z),
                        mat4_mul(
                            pose.pan_rot,
                            mat4_mul(
                                mat4_rotation_z(std::f32::consts::FRAC_PI_2),
                                mat4_scale(
                                    TILT_JOINT_RADIUS,
                                    TILT_JOINT_LENGTH * 0.5,
                                    TILT_JOINT_RADIUS,
                                ),
                            ),
                        ),
                    );
                    let tilt_mvp = mat4_mul(projection, mat4_mul(view, tilt_model));
                    if let Some(loc) = &self.u_mvp {
                        gl.uniform_matrix_4_f32_slice(Some(loc), false, &tilt_mvp);
                    }
                    gl.draw_elements(
                        glow::TRIANGLES,
                        self.cylinder_index_count,
                        glow::UNSIGNED_SHORT,
                        0,
                    );

                    gl.bind_vertex_array(Some(self.cube_vao));
                    if let Some(loc) = &self.u_color {
                        gl.uniform_3_f32(
                            Some(loc),
                            HEAD_BODY_COLOR[0],
                            HEAD_BODY_COLOR[1],
                            HEAD_BODY_COLOR[2],
                        );
                    }
                    let head_model = mat4_mul(
                        mat4_translation(
                            pose.head_center.x,
                            pose.head_center.y,
                            pose.head_center.z,
                        ),
                        mat4_mul(
                            pose.head_rot,
                            mat4_scale(HEAD_SIZE.x, HEAD_SIZE.y, HEAD_SIZE.z),
                        ),
                    );
                    let head_mvp = mat4_mul(projection, mat4_mul(view, head_model));
                    if let Some(loc) = &self.u_mvp {
                        gl.uniform_matrix_4_f32_slice(Some(loc), false, &head_mvp);
                    }
                    gl.draw_elements(
                        glow::TRIANGLES,
                        self.cube_index_count,
                        glow::UNSIGNED_SHORT,
                        0,
                    );

                    let lens_intensity = (0.2 + 0.8 * fixture.beam_strength).clamp(0.0, 1.0);
                    if let Some(loc) = &self.u_color {
                        gl.uniform_3_f32(
                            Some(loc),
                            (fixture.beam_color[0] * lens_intensity).clamp(0.0, 1.0),
                            (fixture.beam_color[1] * lens_intensity).clamp(0.0, 1.0),
                            (fixture.beam_color[2] * lens_intensity).clamp(0.0, 1.0),
                        );
                    }
                    gl.draw_elements(
                        glow::TRIANGLES,
                        6,
                        glow::UNSIGNED_SHORT,
                        6 * index_stride,
                    );

                    gl.bind_vertex_array(Some(self.cube_edges_vao));
                    if let Some(loc) = &self.u_color {
                        gl.uniform_3_f32(
                            Some(loc),
                            FIXTURE_EDGE_COLOR[0],
                            FIXTURE_EDGE_COLOR[1],
                            FIXTURE_EDGE_COLOR[2],
                        );
                    }
                    let base_edge_mvp = mat4_mul(projection, mat4_mul(view, base_model));
                    if let Some(loc) = &self.u_mvp {
                        gl.uniform_matrix_4_f32_slice(Some(loc), false, &base_edge_mvp);
                    }
                    gl.line_width(1.2);
                    gl.draw_arrays(glow::LINES, 0, self.cube_edges_vertex_count);

                    let head_edge_mvp = mat4_mul(projection, mat4_mul(view, head_model));
                    if let Some(loc) = &self.u_mvp {
                        gl.uniform_matrix_4_f32_slice(Some(loc), false, &head_edge_mvp);
                    }
                    gl.draw_arrays(glow::LINES, 0, self.cube_edges_vertex_count);
                }
            }

            if !takeoff_fixtures.is_empty() {
                set_use_vertex_color(gl, &self.u_use_vertex_color, false);
                for fixture in takeoff_fixtures {
                    self.draw_takeoff_sign(gl, projection, view, eye, fixture);
                }
            }

            if !generic_fixtures.is_empty() {
                set_use_vertex_color(gl, &self.u_use_vertex_color, false);
                gl.bind_vertex_array(Some(self.cube_vao));
                for fixture in generic_fixtures {
                    let rotation = mat4_rotation_euler(fixture.rotation);
                    let model = mat4_mul(
                        mat4_translation(fixture.pos.x, fixture.pos.y, fixture.pos.z),
                        mat4_mul(
                            rotation,
                            mat4_scale(
                                GENERIC_FIXTURE_SIZE,
                                GENERIC_FIXTURE_SIZE,
                                GENERIC_FIXTURE_SIZE,
                            ),
                        ),
                    );
                    let mvp = mat4_mul(projection, mat4_mul(view, model));
                    if let Some(loc) = &self.u_mvp {
                        gl.uniform_matrix_4_f32_slice(Some(loc), false, &mvp);
                    }
                    let intensity = (0.15 + 0.85 * fixture.beam_strength).clamp(0.0, 1.0);
                    if let Some(loc) = &self.u_color {
                        gl.uniform_3_f32(
                            Some(loc),
                            (fixture.beam_color[0] * intensity).clamp(0.0, 1.0),
                            (fixture.beam_color[1] * intensity).clamp(0.0, 1.0),
                            (fixture.beam_color[2] * intensity).clamp(0.0, 1.0),
                        );
                    }
                    gl.draw_elements(
                        glow::TRIANGLES,
                        self.cube_index_count,
                        glow::UNSIGNED_SHORT,
                        0,
                    );
                }
            }

            if DEBUG_TEXT_OVERLAY {
                let debug_mvp = mat4_identity();
                unsafe {
                    gl.disable(glow::DEPTH_TEST);
                }
                self.draw_text(
                    gl,
                    debug_mvp,
                    -0.95,
                    0.85,
                    0.02,
                    "DEBUG",
                    [1.0, 1.0, 1.0, 1.0],
                );
                unsafe {
                    gl.enable(glow::DEPTH_TEST);
                }
            }

            gl.disable(glow::DEPTH_TEST);

            gl.bind_vertex_array(None);
            gl.use_program(None);
            gl.disable(glow::SCISSOR_TEST);
        }
    }

    fn update_cones(&mut self, gl: &Arc<glow::Context>, cones: &[BeamCone]) {
        let mut verts: Vec<f32> = Vec::new();
        for cone in cones {
            if cone.strength <= 0.01 {
                continue;
            }
            let forward = cone.dir.normalize();
            let (right, up) = basis_from_dir(forward);
            let base_center = cone.apex.add(forward.scale(cone.len));
            let apex_intensity = (0.7 * cone.strength).clamp(0.0, 1.0);
            let rim_intensity = (0.08 * cone.strength).clamp(0.0, 1.0);
            let apex_color = [
                (cone.color[0] * apex_intensity).clamp(0.0, 1.0),
                (cone.color[1] * apex_intensity).clamp(0.0, 1.0),
                (cone.color[2] * apex_intensity).clamp(0.0, 1.0),
            ];
            let rim_color = [
                (cone.color[0] * rim_intensity).clamp(0.0, 1.0),
                (cone.color[1] * rim_intensity).clamp(0.0, 1.0),
                (cone.color[2] * rim_intensity).clamp(0.0, 1.0),
            ];

            let segment_step = std::f32::consts::TAU / super::constants::CONE_SEGMENTS as f32;
            for i in 0..super::constants::CONE_SEGMENTS {
                let a0 = i as f32 * segment_step;
                let a1 = (i + 1) as f32 * segment_step;
                let offset0 = right
                    .scale(a0.cos() * cone.radius)
                    .add(up.scale(a0.sin() * cone.radius));
                let offset1 = right
                    .scale(a1.cos() * cone.radius)
                    .add(up.scale(a1.sin() * cone.radius));
                let p0 = base_center.add(offset0);
                let p1 = base_center.add(offset1);

                verts.extend_from_slice(&[
                    cone.apex.x,
                    cone.apex.y,
                    cone.apex.z,
                    apex_color[0],
                    apex_color[1],
                    apex_color[2],
                ]);
                verts.extend_from_slice(&[
                    p0.x, p0.y, p0.z, rim_color[0], rim_color[1], rim_color[2],
                ]);
                verts.extend_from_slice(&[
                    p1.x, p1.y, p1.z, rim_color[0], rim_color[1], rim_color[2],
                ]);
            }
        }

        self.cone_vertex_count = (verts.len() / 6) as i32;
        unsafe {
            gl.bind_vertex_array(Some(self.cone_vao));
            gl.bind_buffer(glow::ARRAY_BUFFER, Some(self.cone_vbo));
            gl.buffer_data_u8_slice(
                glow::ARRAY_BUFFER,
                bytemuck::cast_slice(&verts),
                glow::DYNAMIC_DRAW,
            );
            gl.bind_vertex_array(None);
        }
    }

    fn draw_takeoff_sign(
        &self,
        gl: &Arc<glow::Context>,
        projection: [f32; 16],
        view: [f32; 16],
        eye: Vec3,
        fixture: &RenderFixture,
    ) {
        let base_rot = mat4_rotation_euler(fixture.rotation);
        let up = super::math::mat4_transform_dir(base_rot, Vec3::new(0.0, 1.0, 0.0)).normalize();
        let forward =
            super::math::mat4_transform_dir(base_rot, Vec3::new(0.0, 0.0, 1.0)).normalize();
        let sign_center = fixture.pos.add(up.scale(TAKEOFF_LOGO_Y_OFFSET));
        let sign_base = mat4_mul(
            mat4_translation(sign_center.x, sign_center.y, sign_center.z),
            mat4_mul(
                base_rot,
                mat4_scale(TAKEOFF_LOGO_SCALE, TAKEOFF_LOGO_SCALE, TAKEOFF_LOGO_SCALE),
            ),
        );

        let text_scale = TAKEOFF_TEXT_HEIGHT / super::constants::FONT_GLYPH_H as f32;
        let text_width = text_line_width(TAKEOFF_TEXT, text_scale);
        let text_height = super::constants::FONT_GLYPH_H as f32 * text_scale;
        let word_width = text_width;
        let word_height = text_height;
        let back_model = mat4_mul(
            sign_base,
            mat4_mul(
                mat4_translation(
                    0.0,
                    0.0,
                    -(TAKEOFF_LOGO_DEPTH + TAKEOFF_LOGO_BACK_THICKNESS) * 0.5,
                ),
                mat4_scale(
                    word_width + TAKEOFF_LOGO_BACK_PADDING * 2.0,
                    word_height + TAKEOFF_LOGO_BACK_PADDING * 2.0,
                    TAKEOFF_LOGO_BACK_THICKNESS,
                ),
            ),
        );
        let back_mvp = mat4_mul(projection, mat4_mul(view, back_model));

        unsafe {
            gl.bind_vertex_array(Some(self.cube_vao));
            if let Some(loc) = &self.u_color {
                gl.uniform_3_f32(
                    Some(loc),
                    TAKEOFF_BACK_COLOR[0],
                    TAKEOFF_BACK_COLOR[1],
                    TAKEOFF_BACK_COLOR[2],
                );
            }
            if let Some(loc) = &self.u_mvp {
                gl.uniform_matrix_4_f32_slice(Some(loc), false, &back_mvp);
            }
            gl.draw_elements(
                glow::TRIANGLES,
                self.cube_index_count,
                glow::UNSIGNED_SHORT,
                0,
            );
        }

        let light_strength = fixture.beam_strength.clamp(0.0, 1.0);
        let text_color = [
            (fixture.beam_color[0] * light_strength).clamp(0.0, 1.0),
            (fixture.beam_color[1] * light_strength).clamp(0.0, 1.0),
            (fixture.beam_color[2] * light_strength).clamp(0.0, 1.0),
            light_strength,
        ];
        let view_dir = eye.sub(sign_center).normalize();
        let face_sign = if forward.dot(view_dir) >= 0.0 { 1.0 } else { -1.0 };
        let face_rot = if face_sign > 0.0 {
            mat4_identity()
        } else {
            super::math::mat4_rotation_y(std::f32::consts::PI)
        };
        let face_width = word_width + TAKEOFF_LOGO_BACK_PADDING * 2.0;
        let face_height = word_height + TAKEOFF_LOGO_BACK_PADDING * 2.0;
        let face_thickness = 0.001;
        let front_face_z = -TAKEOFF_LOGO_DEPTH * 0.5;
        let back_face_z = -TAKEOFF_LOGO_DEPTH * 0.5 - TAKEOFF_LOGO_BACK_THICKNESS;
        let face_z = if face_sign > 0.0 {
            front_face_z
        } else {
            back_face_z
        };
        let text_plane_z = face_z + face_sign * 0.001;
        let face_model = mat4_mul(
            sign_base,
            mat4_mul(
                mat4_translation(0.0, 0.0, face_z),
                mat4_scale(face_width, face_height, face_thickness),
            ),
        );
        let face_mvp = mat4_mul(projection, mat4_mul(view, face_model));
        unsafe {
            gl.bind_vertex_array(Some(self.quad_vao));
            if let Some(loc) = &self.u_color {
                gl.uniform_3_f32(Some(loc), 0.35, 0.35, 0.36);
            }
            if let Some(loc) = &self.u_mvp {
                gl.uniform_matrix_4_f32_slice(Some(loc), false, &face_mvp);
            }
            gl.draw_arrays(glow::TRIANGLES, 0, self.quad_vertex_count);
        }
        let text_model = mat4_mul(
            sign_base,
            mat4_mul(
                mat4_translation(0.0, TAKEOFF_TEXT_Y, text_plane_z),
                mat4_mul(face_rot, mat4_scale(1.0, -1.0, 1.0)),
            ),
        );
        let text_mvp = mat4_mul(projection, mat4_mul(view, text_model));
        unsafe {
            gl.enable(glow::POLYGON_OFFSET_FILL);
            gl.polygon_offset(-1.0, -1.0);
        }
        self.draw_text(
            gl,
            text_mvp,
            -text_width * 0.5,
            -text_height * 0.5,
            text_scale,
            TAKEOFF_TEXT,
            text_color,
        );
        unsafe {
            gl.disable(glow::POLYGON_OFFSET_FILL);
        }
        unsafe {
            gl.use_program(Some(self.program));
            gl.bind_vertex_array(Some(self.cube_vao));
        }
    }

    fn draw_text(
        &self,
        gl: &Arc<glow::Context>,
        mvp: [f32; 16],
        x: f32,
        y: f32,
        scale: f32,
        text: &str,
        color: [f32; 4],
    ) {
        let verts = build_text_vertices(text, x, y, scale);
        if verts.is_empty() {
            return;
        }
        unsafe {
            gl.use_program(Some(self.text_program));
            gl.bind_vertex_array(Some(self.text_vao));
            gl.bind_buffer(glow::ARRAY_BUFFER, Some(self.text_vbo));
            gl.buffer_data_u8_slice(
                glow::ARRAY_BUFFER,
                bytemuck::cast_slice(&verts),
                glow::DYNAMIC_DRAW,
            );

            if let Some(loc) = &self.text_u_mvp {
                gl.uniform_matrix_4_f32_slice(Some(loc), false, &mvp);
            }
            if let Some(loc) = &self.text_u_color {
                gl.uniform_4_f32(Some(loc), color[0], color[1], color[2], color[3]);
            }
            if let Some(loc) = &self.text_u_tex {
                gl.uniform_1_i32(Some(loc), 0);
            }

            gl.active_texture(glow::TEXTURE0);
            gl.bind_texture(glow::TEXTURE_2D, Some(self.text_texture));
            gl.enable(glow::BLEND);
            gl.blend_func(glow::SRC_ALPHA, glow::ONE_MINUS_SRC_ALPHA);
            gl.draw_arrays(glow::TRIANGLES, 0, (verts.len() / 5) as i32);
            gl.disable(glow::BLEND);
            gl.bind_texture(glow::TEXTURE_2D, None);
            gl.bind_vertex_array(None);
        }
    }
}

impl Drop for GlowRenderer {
    fn drop(&mut self) {
        unsafe {
            self.gl.delete_program(self.program);
            self.gl.delete_program(self.text_program);
            self.gl.delete_vertex_array(self.cube_vao);
            self.gl.delete_buffer(self.cube_vbo);
            self.gl.delete_buffer(self.cube_ebo);
            self.gl.delete_vertex_array(self.text_vao);
            self.gl.delete_buffer(self.text_vbo);
            self.gl.delete_texture(self.text_texture);
            self.gl.delete_vertex_array(self.cube_edges_vao);
            self.gl.delete_buffer(self.cube_edges_vbo);
            self.gl.delete_vertex_array(self.grid_vao);
            self.gl.delete_buffer(self.grid_vbo);
            self.gl.delete_vertex_array(self.axes_vao);
            self.gl.delete_buffer(self.axes_vbo);
            self.gl.delete_vertex_array(self.cylinder_vao);
            self.gl.delete_buffer(self.cylinder_vbo);
            self.gl.delete_buffer(self.cylinder_ebo);
            self.gl.delete_vertex_array(self.cone_vao);
            self.gl.delete_buffer(self.cone_vbo);
            self.gl.delete_vertex_array(self.quad_vao);
            self.gl.delete_buffer(self.quad_vbo);
        }
    }
}

fn set_use_vertex_color(
    gl: &Arc<glow::Context>,
    loc: &Option<glow::UniformLocation>,
    use_vertex: bool,
) {
    if let Some(loc) = loc {
        unsafe {
            gl.uniform_1_f32(Some(loc), if use_vertex { 1.0 } else { 0.0 });
        }
    }
}
