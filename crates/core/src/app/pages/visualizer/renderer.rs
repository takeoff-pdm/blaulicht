use std::{collections::HashMap, sync::Arc};

use egui_glow::glow::{self, HasContext};

use super::constants::{
    BASE_SIZE, BEAM_ANGLE_DEG, DEBUG_TEXT_OVERLAY, DEFAULT_ROOM_DEPTH, DEFAULT_ROOM_WIDTH,
    FIXTURE_BODY_COLOR, FIXTURE_EDGE_COLOR, GENERIC_FIXTURE_SIZE, HEAD_BODY_COLOR, HEAD_SIZE,
    JOINT_COLOR, MAX_ROOM_DIMENSION, MIN_ROOM_DIMENSION, PAN_JOINT_HEIGHT, PAN_JOINT_RADIUS,
    ROOM_FLOOR_COLOR, ROOM_WALL_COLOR, TAKEOFF_BACK_COLOR, TAKEOFF_LOGO_BACK_PADDING,
    TAKEOFF_LOGO_BACK_THICKNESS, TAKEOFF_LOGO_DEPTH, TAKEOFF_LOGO_SCALE, TAKEOFF_LOGO_Y_OFFSET,
    TAKEOFF_TEXT, TAKEOFF_TEXT_HEIGHT, TAKEOFF_TEXT_Y, TILT_JOINT_LENGTH, TILT_JOINT_RADIUS,
    YOKE_HEIGHT, YOKE_RADIUS,
};
use super::data::{
    compute_fixture_pose, BeamCone, FixturePose, RenderFixture, RenderFixtureKind,
    RenderSceneSnapshot, RenderStageObject,
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
    pub(super) gl: Arc<glow::Context>,
    three_context: three_d::Context,
    imported_models: HashMap<u64, ImportedGpuModel>,
    runtime_error: Option<String>,
    program: glow::Program,
    text_program: glow::Program,
    cube_vao: glow::VertexArray,
    cube_vbo: glow::Buffer,
    cube_vertex_count: i32,
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
    cylinder_vertex_count: i32,
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
    u_model: Option<glow::UniformLocation>,
    u_brightness: Option<glow::UniformLocation>,
    u_alpha: Option<glow::UniformLocation>,
    u_color: Option<glow::UniformLocation>,
    u_use_vertex_color: Option<glow::UniformLocation>,
    u_light_direction: Option<glow::UniformLocation>,
    u_use_lighting: Option<glow::UniformLocation>,
    text_u_mvp: Option<glow::UniformLocation>,
    text_u_color: Option<glow::UniformLocation>,
    text_u_tex: Option<glow::UniformLocation>,
}

struct ImportedGpuModel {
    asset_key: String,
    model: three_d::Model<three_d::PhysicalMaterial>,
    base_transforms: Vec<three_d::Mat4>,
}

impl GlowRenderer {
    pub(super) fn new(gl: &Arc<glow::Context>) -> Result<Self, String> {
        unsafe {
            let three_context = three_d::Context::from_gl_context(gl.clone())
                .map_err(|error| format!("3D context initialization failed: {error}"))?;
            let program = create_program(gl)?;
            let text_program = create_text_program(gl)?;
            let (cube_vao, cube_vbo, cube_vertex_count) = create_cube(gl)?;
            let (text_vao, text_vbo) = create_text_buffers(gl)?;
            let text_texture = create_font_texture(gl)?;
            let (cube_edges_vao, cube_edges_vbo, cube_edges_vertex_count) = create_cube_edges(gl)?;
            let (grid_vao, grid_vbo, grid_vertex_count) = create_grid(gl)?;
            let (axes_vao, axes_vbo, axes_vertex_count) = create_axes(gl)?;
            let (cylinder_vao, cylinder_vbo, cylinder_vertex_count) = create_cylinder(gl, 24)?;
            let (cone_vao, cone_vbo) = create_dynamic_mesh(gl)?;
            let (quad_vao, quad_vbo, quad_vertex_count) = create_quad(gl)?;
            let u_mvp = gl.get_uniform_location(program, "u_mvp");
            let u_model = gl.get_uniform_location(program, "u_model");
            let u_brightness = gl.get_uniform_location(program, "u_brightness");
            let u_alpha = gl.get_uniform_location(program, "u_alpha");
            let u_color = gl.get_uniform_location(program, "u_color");
            let u_use_vertex_color = gl.get_uniform_location(program, "u_use_vertex_color");
            let u_light_direction = gl.get_uniform_location(program, "u_light_direction");
            let u_use_lighting = gl.get_uniform_location(program, "u_use_lighting");
            let text_u_mvp = gl.get_uniform_location(text_program, "u_mvp");
            let text_u_color = gl.get_uniform_location(text_program, "u_color");
            let text_u_tex = gl.get_uniform_location(text_program, "u_tex");

            Ok(Self {
                gl: gl.clone(),
                three_context,
                imported_models: HashMap::new(),
                runtime_error: None,
                program,
                text_program,
                cube_vao,
                cube_vbo,
                cube_vertex_count,
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
                cylinder_vertex_count,
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
                u_model,
                u_brightness,
                u_alpha,
                u_color,
                u_use_vertex_color,
                u_light_direction,
                u_use_lighting,
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
        free_camera: bool,
        camera_position: Vec3,
        snapshot: &RenderSceneSnapshot,
        info: egui::PaintCallbackInfo,
    ) {
        let viewport = info.viewport_in_pixels();
        if viewport.width_px <= 0 || viewport.height_px <= 0 {
            return;
        }

        unsafe {
            let clip = info.clip_rect_in_pixels();
            let scissor_left = viewport.left_px.max(clip.left_px);
            let scissor_bottom = viewport.from_bottom_px.max(clip.from_bottom_px);
            let scissor_right =
                (viewport.left_px + viewport.width_px).min(clip.left_px + clip.width_px);
            let scissor_top = (viewport.from_bottom_px + viewport.height_px)
                .min(clip.from_bottom_px + clip.height_px);
            let scissor_width = (scissor_right - scissor_left).max(0);
            let scissor_height = (scissor_top - scissor_bottom).max(0);
            if scissor_width == 0 || scissor_height == 0 {
                return;
            }
            gl.enable(glow::SCISSOR_TEST);
            gl.scissor(scissor_left, scissor_bottom, scissor_width, scissor_height);
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

            if let Some(loc) = &self.u_light_direction {
                // A fixed overhead key light gives the virtual world readable depth
                // without introducing per-fixture lighting state or extra meshes.
                gl.uniform_3_f32(Some(loc), 0.35, 1.0, 0.25);
            }

            if let Some(loc) = &self.u_brightness {
                gl.uniform_1_f32(
                    Some(loc),
                    if settings.brightness.is_finite() {
                        settings.brightness.clamp(0.0, 8.0)
                    } else {
                        1.0
                    },
                );
            }
            set_alpha(gl, &self.u_alpha, 1.0);
            set_model(gl, &self.u_model, mat4_identity());

            let aspect = viewport.width_px as f32 / viewport.height_px as f32;
            let projection = mat4_perspective(45.0_f32.to_radians(), aspect, 0.1, 200.0);

            let yaw = if yaw.is_finite() { yaw } else { 0.0 };
            let pitch = if pitch.is_finite() {
                pitch.clamp(-1.5, 1.2)
            } else {
                -0.3
            };
            let radius = if radius.is_finite() {
                radius.clamp(0.1, 200.0)
            } else {
                12.0
            };
            let orbit_eye = {
                let cy = yaw.cos();
                let sy = yaw.sin();
                let cp = pitch.cos();
                let sp = pitch.sin();
                Vec3::new(radius * sy * cp, radius * sp, radius * cy * cp)
            };
            let (eye, target) = if free_camera
                && camera_position.x.is_finite()
                && camera_position.y.is_finite()
                && camera_position.z.is_finite()
            {
                let forward = Vec3::new(
                    pitch.cos() * yaw.sin(),
                    pitch.sin(),
                    -pitch.cos() * yaw.cos(),
                );
                (camera_position, camera_position.add(forward))
            } else {
                (orbit_eye, Vec3::new(0.0, 0.0, 0.0))
            };
            let view = mat4_look_at(eye, target, Vec3::new(0.0, 1.0, 0.0));

            if settings.show_room {
                self.draw_room(
                    gl,
                    projection,
                    view,
                    settings.room_width,
                    settings.room_depth,
                    settings.room_height,
                );
            }

            for object in snapshot.stage_objects.iter() {
                if object.cpu_model.is_none() {
                    self.draw_stage_object(gl, projection, view, object);
                }
            }

            let fixtures = snapshot.fixtures.as_ref();
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
                        if settings.show_beams && fixture.beam_strength > 0.01 {
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
                set_use_lighting(gl, &self.u_use_lighting, false);
                gl.bind_vertex_array(Some(self.grid_vao));
                let mvp = mat4_mul(projection, mat4_mul(view, mat4_identity()));
                if let Some(loc) = &self.u_mvp {
                    gl.uniform_matrix_4_f32_slice(Some(loc), false, &mvp);
                }
                gl.draw_arrays(glow::LINES, 0, self.grid_vertex_count);
            }

            if settings.show_axes {
                set_use_vertex_color(gl, &self.u_use_vertex_color, true);
                set_use_lighting(gl, &self.u_use_lighting, false);
                gl.bind_vertex_array(Some(self.axes_vao));
                let mvp = mat4_mul(projection, mat4_mul(view, mat4_identity()));
                if let Some(loc) = &self.u_mvp {
                    gl.uniform_matrix_4_f32_slice(Some(loc), false, &mvp);
                }
                gl.draw_arrays(glow::LINES, 0, self.axes_vertex_count);
            }

            if settings.show_beams {
                self.update_cones(gl, &cones);
            } else {
                self.cone_vertex_count = 0;
            }
            if self.cone_vertex_count > 0 {
                set_use_vertex_color(gl, &self.u_use_vertex_color, true);
                set_use_lighting(gl, &self.u_use_lighting, false);
                set_alpha(gl, &self.u_alpha, 0.42);
                gl.bind_vertex_array(Some(self.cone_vao));
                let mvp = mat4_mul(projection, mat4_mul(view, mat4_identity()));
                if let Some(loc) = &self.u_mvp {
                    gl.uniform_matrix_4_f32_slice(Some(loc), false, &mvp);
                }
                gl.enable(glow::BLEND);
                gl.blend_func(glow::SRC_ALPHA, glow::ONE_MINUS_SRC_ALPHA);
                gl.depth_mask(false);
                gl.line_width(1.0);
                gl.draw_arrays(glow::LINES, 0, self.cone_vertex_count);
                gl.depth_mask(true);
                gl.disable(glow::BLEND);
                set_alpha(gl, &self.u_alpha, 1.0);
            }

            if settings.show_bodies && !head_fixtures.is_empty() {
                set_use_vertex_color(gl, &self.u_use_vertex_color, false);
                set_use_lighting(gl, &self.u_use_lighting, true);
                for (fixture, pose) in head_fixtures.iter().zip(poses.iter()) {
                    set_use_vertex_color(gl, &self.u_use_vertex_color, false);
                    set_use_lighting(gl, &self.u_use_lighting, true);
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
                    set_model(gl, &self.u_model, base_model);
                    if let Some(loc) = &self.u_mvp {
                        gl.uniform_matrix_4_f32_slice(Some(loc), false, &base_mvp);
                    }
                    gl.draw_arrays(glow::TRIANGLES, 0, self.cube_vertex_count);

                    gl.bind_vertex_array(Some(self.cylinder_vao));
                    if let Some(loc) = &self.u_color {
                        gl.uniform_3_f32(Some(loc), JOINT_COLOR[0], JOINT_COLOR[1], JOINT_COLOR[2]);
                    }
                    let pan_model = mat4_mul(
                        mat4_translation(pose.pan_center.x, pose.pan_center.y, pose.pan_center.z),
                        mat4_mul(
                            pose.pan_rot,
                            mat4_scale(PAN_JOINT_RADIUS, PAN_JOINT_HEIGHT * 0.5, PAN_JOINT_RADIUS),
                        ),
                    );
                    let pan_mvp = mat4_mul(projection, mat4_mul(view, pan_model));
                    set_model(gl, &self.u_model, pan_model);
                    if let Some(loc) = &self.u_mvp {
                        gl.uniform_matrix_4_f32_slice(Some(loc), false, &pan_mvp);
                    }
                    gl.draw_arrays(glow::TRIANGLES, 0, self.cylinder_vertex_count);

                    let yoke_model = mat4_mul(
                        mat4_translation(
                            pose.yoke_center.x,
                            pose.yoke_center.y,
                            pose.yoke_center.z,
                        ),
                        mat4_mul(
                            pose.pan_rot,
                            mat4_scale(YOKE_RADIUS, YOKE_HEIGHT * 0.5, YOKE_RADIUS),
                        ),
                    );
                    let yoke_mvp = mat4_mul(projection, mat4_mul(view, yoke_model));
                    set_model(gl, &self.u_model, yoke_model);
                    if let Some(loc) = &self.u_mvp {
                        gl.uniform_matrix_4_f32_slice(Some(loc), false, &yoke_mvp);
                    }
                    gl.draw_arrays(glow::TRIANGLES, 0, self.cylinder_vertex_count);

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
                    set_model(gl, &self.u_model, tilt_model);
                    if let Some(loc) = &self.u_mvp {
                        gl.uniform_matrix_4_f32_slice(Some(loc), false, &tilt_mvp);
                    }
                    gl.draw_arrays(glow::TRIANGLES, 0, self.cylinder_vertex_count);

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
                    set_model(gl, &self.u_model, head_model);
                    if let Some(loc) = &self.u_mvp {
                        gl.uniform_matrix_4_f32_slice(Some(loc), false, &head_mvp);
                    }
                    gl.draw_arrays(glow::TRIANGLES, 0, self.cube_vertex_count);

                    let lens_intensity = (0.2 + 0.8 * fixture.beam_strength).clamp(0.0, 1.0);
                    if let Some(loc) = &self.u_color {
                        gl.uniform_3_f32(
                            Some(loc),
                            (fixture.beam_color[0] * lens_intensity).clamp(0.0, 1.0),
                            (fixture.beam_color[1] * lens_intensity).clamp(0.0, 1.0),
                            (fixture.beam_color[2] * lens_intensity).clamp(0.0, 1.0),
                        );
                    }
                    let lens_model = mat4_mul(
                        mat4_translation(pose.lens_pos.x, pose.lens_pos.y, pose.lens_pos.z),
                        mat4_mul(
                            pose.head_rot,
                            mat4_scale(HEAD_SIZE.x * 0.68, HEAD_SIZE.y * 0.68, 0.035),
                        ),
                    );
                    let lens_mvp = mat4_mul(projection, mat4_mul(view, lens_model));
                    set_model(gl, &self.u_model, lens_model);
                    if let Some(loc) = &self.u_mvp {
                        gl.uniform_matrix_4_f32_slice(Some(loc), false, &lens_mvp);
                    }
                    gl.draw_arrays(glow::TRIANGLES, 0, self.cube_vertex_count);

                    gl.bind_vertex_array(Some(self.cube_edges_vao));
                    set_use_vertex_color(gl, &self.u_use_vertex_color, true);
                    set_use_lighting(gl, &self.u_use_lighting, false);
                    set_model(gl, &self.u_model, mat4_identity());
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
                    gl.line_width(1.0);
                    gl.draw_arrays(glow::LINES, 0, self.cube_edges_vertex_count);

                    let head_edge_mvp = mat4_mul(projection, mat4_mul(view, head_model));
                    if let Some(loc) = &self.u_mvp {
                        gl.uniform_matrix_4_f32_slice(Some(loc), false, &head_edge_mvp);
                    }
                    gl.draw_arrays(glow::LINES, 0, self.cube_edges_vertex_count);
                }
            }

            if settings.show_bodies && !takeoff_fixtures.is_empty() {
                set_use_vertex_color(gl, &self.u_use_vertex_color, false);
                set_use_lighting(gl, &self.u_use_lighting, true);
                for fixture in takeoff_fixtures {
                    self.draw_takeoff_sign(gl, projection, view, eye, fixture);
                }
            }

            if settings.show_bodies && !generic_fixtures.is_empty() {
                set_use_vertex_color(gl, &self.u_use_vertex_color, false);
                set_use_lighting(gl, &self.u_use_lighting, true);
                for fixture in generic_fixtures {
                    set_use_vertex_color(gl, &self.u_use_vertex_color, false);
                    set_use_lighting(gl, &self.u_use_lighting, true);
                    gl.bind_vertex_array(Some(self.cube_vao));
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
                    set_model(gl, &self.u_model, model);
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
                    gl.draw_arrays(glow::TRIANGLES, 0, self.cube_vertex_count);

                    // The same edge mesh is reused for all fixture kinds so
                    // generic lights remain separable at a glance.
                    gl.bind_vertex_array(Some(self.cube_edges_vao));
                    set_use_vertex_color(gl, &self.u_use_vertex_color, true);
                    set_use_lighting(gl, &self.u_use_lighting, false);
                    set_model(gl, &self.u_model, mat4_identity());
                    if let Some(loc) = &self.u_color {
                        gl.uniform_3_f32(
                            Some(loc),
                            FIXTURE_EDGE_COLOR[0],
                            FIXTURE_EDGE_COLOR[1],
                            FIXTURE_EDGE_COLOR[2],
                        );
                    }
                    gl.line_width(1.0);
                    gl.draw_arrays(glow::LINES, 0, self.cube_edges_vertex_count);
                }
            }

            if settings.show_labels {
                for fixture in fixtures {
                    let label_model =
                        mat4_translation(fixture.pos.x, fixture.pos.y + 1.0, fixture.pos.z);
                    let label_mvp = mat4_mul(projection, mat4_mul(view, label_model));
                    self.draw_text(
                        gl,
                        label_mvp,
                        -(fixture.name.len() as f32) * 0.025,
                        0.0,
                        0.05,
                        &fixture.name,
                        [0.9, 0.9, 0.9, 1.0],
                    );
                }
                gl.use_program(Some(self.program));
            }

            if self.runtime_error.is_none()
                && std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                    self.draw_imported_models(snapshot, info, eye, target);
                }))
                .is_err()
            {
                self.imported_models.clear();
                self.runtime_error = Some(
                    "Imported-model renderer failed; retry the renderer or remove the model"
                        .to_string(),
                );
            }
            gl.use_program(Some(self.program));

            if DEBUG_TEXT_OVERLAY {
                let debug_mvp = mat4_identity();
                gl.disable(glow::DEPTH_TEST);
                self.draw_text(
                    gl,
                    debug_mvp,
                    -0.95,
                    0.85,
                    0.02,
                    "DEBUG",
                    [1.0, 1.0, 1.0, 1.0],
                );
                gl.enable(glow::DEPTH_TEST);
            }

            gl.disable(glow::DEPTH_TEST);

            gl.disable(glow::BLEND);
            gl.disable(glow::POLYGON_OFFSET_FILL);
            gl.depth_mask(true);
            gl.bind_buffer(glow::ARRAY_BUFFER, None);
            gl.active_texture(glow::TEXTURE0);

            gl.bind_vertex_array(None);
            gl.use_program(None);
            gl.disable(glow::SCISSOR_TEST);
        }
    }

    pub(super) fn take_runtime_error(&mut self) -> Option<String> {
        self.runtime_error.take()
    }

    fn draw_room(
        &self,
        gl: &Arc<glow::Context>,
        projection: [f32; 16],
        view: [f32; 16],
        requested_width: f32,
        requested_depth: f32,
        requested_height: f32,
    ) {
        let (width, depth, height) =
            room_dimensions(requested_width, requested_depth, requested_height);
        let half_width = width * 0.5;
        let half_depth = depth * 0.5;
        let wall_thickness = 0.08;

        let floor = mat4_mul(
            mat4_translation(0.0, -0.06, 0.0),
            mat4_scale(half_width, 0.05, half_depth),
        );
        let back = mat4_mul(
            mat4_translation(0.0, height * 0.5, -half_depth),
            mat4_scale(half_width, height * 0.5, wall_thickness),
        );
        let left = mat4_mul(
            mat4_translation(-half_width, height * 0.5, 0.0),
            mat4_scale(wall_thickness, height * 0.5, half_depth),
        );
        let right = mat4_mul(
            mat4_translation(half_width, height * 0.5, 0.0),
            mat4_scale(wall_thickness, height * 0.5, half_depth),
        );

        self.draw_colored_cube(gl, projection, view, floor, ROOM_FLOOR_COLOR);
        // The camera-facing side stays open as a stage-style cutaway. This
        // preserves the sense of a room without allowing a wall to hide the
        // fixtures while orbiting or entering free-flight mode.
        for wall in [back, left, right] {
            self.draw_colored_cube(gl, projection, view, wall, ROOM_WALL_COLOR);
        }
    }

    fn draw_imported_models(
        &mut self,
        snapshot: &RenderSceneSnapshot,
        info: egui::PaintCallbackInfo,
        eye: Vec3,
        target: Vec3,
    ) {
        use three_d::{degrees, vec3, Light, Object};

        let imported: Vec<_> = snapshot
            .stage_objects
            .iter()
            .filter_map(|object| {
                Some((
                    object,
                    object.model_key.as_ref()?,
                    object.cpu_model.as_ref()?,
                ))
            })
            .collect();
        if imported.is_empty() {
            return;
        }

        self.imported_models
            .retain(|id, _| imported.iter().any(|(object, _, _)| object.id == *id));
        for (object, asset_key, cpu_model) in &imported {
            let rebuild = self
                .imported_models
                .get(&object.id)
                .map(|cached| cached.asset_key != **asset_key)
                .unwrap_or(true);
            if rebuild {
                let Ok(model) = three_d::Model::<three_d::PhysicalMaterial>::new(
                    &self.three_context,
                    cpu_model,
                ) else {
                    continue;
                };
                let base_transforms = model.iter().map(|part| part.transformation()).collect();
                self.imported_models.insert(
                    object.id,
                    ImportedGpuModel {
                        asset_key: (*asset_key).clone(),
                        model,
                        base_transforms,
                    },
                );
            }

            let transform =
                three_d::Mat4::from_translation(vec3(object.pos.x, object.pos.y, object.pos.z))
                    * three_d::Mat4::from_angle_x(degrees(object.rotation.x.to_degrees()))
                    * three_d::Mat4::from_angle_y(degrees(object.rotation.y.to_degrees()))
                    * three_d::Mat4::from_angle_z(degrees(object.rotation.z.to_degrees()))
                    * three_d::Mat4::from_nonuniform_scale(
                        object.scale.x,
                        object.scale.y,
                        object.scale.z,
                    );
            if let Some(cached) = self.imported_models.get_mut(&object.id) {
                for (part, base) in cached.model.iter_mut().zip(&cached.base_transforms) {
                    part.set_transformation(transform * *base);
                }
            }
        }

        let viewport = info.viewport_in_pixels();
        let camera = three_d::Camera::new_perspective(
            three_d::Viewport {
                x: viewport.left_px,
                y: viewport.from_bottom_px,
                width: viewport.width_px as u32,
                height: viewport.height_px as u32,
            },
            vec3(eye.x, eye.y, eye.z),
            vec3(target.x, target.y, target.z),
            vec3(0.0, 1.0, 0.0),
            degrees(45.0),
            0.1,
            200.0,
        );
        let ambient = three_d::AmbientLight::new(&self.three_context, 0.65, three_d::Srgba::WHITE);
        let directional = three_d::DirectionalLight::new(
            &self.three_context,
            2.0,
            three_d::Srgba::WHITE,
            vec3(-0.35, -1.0, -0.25),
        );
        let objects: Vec<&dyn Object> = imported
            .iter()
            .filter_map(|(object, _, _)| self.imported_models.get(&object.id))
            .flat_map(|cached| cached.model.iter().map(|part| part as &dyn Object))
            .collect();
        let lights: [&dyn Light; 2] = [&ambient, &directional];
        let screen_width = (viewport.left_px + viewport.width_px).max(1) as u32;
        let screen_height = (viewport.from_bottom_px + viewport.height_px).max(1) as u32;
        three_d::RenderTarget::screen(&self.three_context, screen_width, screen_height)
            .render(camera, objects, &lights);
    }

    fn draw_stage_object(
        &self,
        gl: &Arc<glow::Context>,
        projection: [f32; 16],
        view: [f32; 16],
        object: &RenderStageObject,
    ) {
        let rotation = mat4_rotation_euler(object.rotation);
        let model = mat4_mul(
            mat4_translation(
                object.pos.x,
                object.pos.y + object.scale.y * 0.5,
                object.pos.z,
            ),
            mat4_mul(
                rotation,
                mat4_scale(
                    object.scale.x * 0.5,
                    object.scale.y * 0.5,
                    object.scale.z * 0.5,
                ),
            ),
        );
        self.draw_colored_cube(gl, projection, view, model, object.color);

        let edge_mvp = mat4_mul(projection, mat4_mul(view, model));
        unsafe {
            gl.bind_vertex_array(Some(self.cube_edges_vao));
            set_use_vertex_color(gl, &self.u_use_vertex_color, false);
            set_use_lighting(gl, &self.u_use_lighting, false);
            set_model(gl, &self.u_model, model);
            if let Some(loc) = &self.u_color {
                gl.uniform_3_f32(Some(loc), 0.65, 0.7, 0.78);
            }
            if let Some(loc) = &self.u_mvp {
                gl.uniform_matrix_4_f32_slice(Some(loc), false, &edge_mvp);
            }
            gl.line_width(1.0);
            gl.draw_arrays(glow::LINES, 0, self.cube_edges_vertex_count);
        }
    }

    fn draw_colored_cube(
        &self,
        gl: &Arc<glow::Context>,
        projection: [f32; 16],
        view: [f32; 16],
        model: [f32; 16],
        color: [f32; 3],
    ) {
        let mvp = mat4_mul(projection, mat4_mul(view, model));
        unsafe {
            gl.use_program(Some(self.program));
            gl.bind_vertex_array(Some(self.cube_vao));
            set_use_vertex_color(gl, &self.u_use_vertex_color, false);
            set_use_lighting(gl, &self.u_use_lighting, true);
            set_alpha(gl, &self.u_alpha, 1.0);
            set_model(gl, &self.u_model, model);
            if let Some(loc) = &self.u_color {
                gl.uniform_3_f32(Some(loc), color[0], color[1], color[2]);
            }
            if let Some(loc) = &self.u_mvp {
                gl.uniform_matrix_4_f32_slice(Some(loc), false, &mvp);
            }
            gl.draw_arrays(glow::TRIANGLES, 0, self.cube_vertex_count);
        }
    }

    fn update_cones(&mut self, gl: &Arc<glow::Context>, cones: &[BeamCone]) {
        let verts = build_beam_lines(cones);

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
            set_model(gl, &self.u_model, back_model);
            gl.draw_arrays(glow::TRIANGLES, 0, self.cube_vertex_count);
        }

        let light_strength = fixture.beam_strength.clamp(0.0, 1.0);
        let text_color = [
            (fixture.beam_color[0] * light_strength).clamp(0.0, 1.0),
            (fixture.beam_color[1] * light_strength).clamp(0.0, 1.0),
            (fixture.beam_color[2] * light_strength).clamp(0.0, 1.0),
            light_strength,
        ];
        let view_dir = eye.sub(sign_center).normalize();
        let face_sign = if forward.dot(view_dir) >= 0.0 {
            1.0
        } else {
            -1.0
        };
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
            set_model(gl, &self.u_model, face_model);
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

fn build_beam_lines(cones: &[BeamCone]) -> Vec<f32> {
    let mut verts = Vec::new();
    for cone in cones {
        if cone.strength <= 0.01
            || !cone.len.is_finite()
            || !cone.radius.is_finite()
            || cone.len <= 0.0
            || cone.radius <= 0.0
        {
            continue;
        }
        let forward = cone.dir.normalize();
        if forward.norm() <= f32::EPSILON {
            continue;
        }
        let (right, up) = basis_from_dir(forward);
        let base_center = cone.apex.add(forward.scale(cone.len));
        let apex_intensity = (0.8 * cone.strength).clamp(0.0, 1.0);
        let rim_intensity = (0.35 * cone.strength).clamp(0.0, 1.0);
        let apex_color = cone
            .color
            .map(|channel| (channel * apex_intensity).clamp(0.0, 1.0));
        let rim_color = cone
            .color
            .map(|channel| (channel * rim_intensity).clamp(0.0, 1.0));
        let segment_step = std::f32::consts::TAU / super::constants::CONE_SEGMENTS as f32;

        for i in 0..super::constants::CONE_SEGMENTS {
            let a0 = i as f32 * segment_step;
            let a1 = (i + 1) as f32 * segment_step;
            let offset0 = right
                .scale(a0.cos() * cone.radius * 0.6)
                .add(up.scale(a0.sin() * cone.radius * 0.6));
            let offset1 = right
                .scale(a1.cos() * cone.radius * 0.6)
                .add(up.scale(a1.sin() * cone.radius * 0.6));
            let p0 = base_center.add(offset0);
            let p1 = base_center.add(offset1);

            if i % (super::constants::CONE_SEGMENTS / 4).max(1) == 0 {
                push_colored_vertex(&mut verts, cone.apex, apex_color);
                push_colored_vertex(&mut verts, p0, rim_color);
            }
            push_colored_vertex(&mut verts, p0, rim_color);
            push_colored_vertex(&mut verts, p1, rim_color);
        }
    }
    verts
}

fn room_dimensions(
    requested_width: f32,
    requested_depth: f32,
    requested_height: f32,
) -> (f32, f32, f32) {
    let width = if requested_width.is_finite() {
        requested_width.clamp(MIN_ROOM_DIMENSION, MAX_ROOM_DIMENSION)
    } else {
        DEFAULT_ROOM_WIDTH
    };
    let depth = if requested_depth.is_finite() {
        requested_depth.clamp(MIN_ROOM_DIMENSION, MAX_ROOM_DIMENSION)
    } else {
        DEFAULT_ROOM_DEPTH
    };
    let height = if requested_height.is_finite() {
        requested_height.clamp(MIN_ROOM_DIMENSION, 50.0)
    } else {
        8.0
    };
    (width, depth, height)
}

impl Drop for GlowRenderer {
    fn drop(&mut self) {
        unsafe {
            self.gl.delete_program(self.program);
            self.gl.delete_program(self.text_program);
            self.gl.delete_vertex_array(self.cube_vao);
            self.gl.delete_buffer(self.cube_vbo);
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
            self.gl.delete_vertex_array(self.cone_vao);
            self.gl.delete_buffer(self.cone_vbo);
            self.gl.delete_vertex_array(self.quad_vao);
            self.gl.delete_buffer(self.quad_vbo);
        }
    }
}

fn push_colored_vertex(vertices: &mut Vec<f32>, position: Vec3, color: [f32; 3]) {
    vertices.extend_from_slice(&[
        position.x, position.y, position.z, color[0], color[1], color[2],
    ]);
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

fn set_alpha(gl: &Arc<glow::Context>, loc: &Option<glow::UniformLocation>, alpha: f32) {
    if let Some(loc) = loc {
        unsafe {
            gl.uniform_1_f32(Some(loc), alpha.clamp(0.0, 1.0));
        }
    }
}

fn set_model(gl: &Arc<glow::Context>, loc: &Option<glow::UniformLocation>, model: [f32; 16]) {
    if let Some(loc) = loc {
        unsafe {
            gl.uniform_matrix_4_f32_slice(Some(loc), false, &model);
        }
    }
}

fn set_use_lighting(
    gl: &Arc<glow::Context>,
    loc: &Option<glow::UniformLocation>,
    use_lighting: bool,
) {
    if let Some(loc) = loc {
        unsafe {
            gl.uniform_1_f32(Some(loc), if use_lighting { 1.0 } else { 0.0 });
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn beam_overlay_is_line_pairs_with_finite_vertices() {
        let vertices = build_beam_lines(&[BeamCone {
            apex: Vec3::new(0.0, 1.0, 0.0),
            dir: Vec3::new(0.0, 0.0, 1.0),
            len: 10.0,
            radius: 1.0,
            color: [1.0, 0.5, 0.25],
            strength: 1.0,
        }]);

        let vertex_count = vertices.len() / 6;
        assert_eq!(vertex_count % 2, 0);
        assert_eq!(vertex_count, super::super::constants::CONE_SEGMENTS * 2 + 8);
        assert!(vertices.iter().all(|value| value.is_finite()));
    }

    #[test]
    fn invalid_beams_do_not_reach_gl_upload() {
        let vertices = build_beam_lines(&[BeamCone {
            apex: Vec3::new(0.0, 0.0, 0.0),
            dir: Vec3::new(0.0, 0.0, 0.0),
            len: f32::NAN,
            radius: 1.0,
            color: [1.0, 1.0, 1.0],
            strength: 1.0,
        }]);
        assert!(vertices.is_empty());
    }

    #[test]
    fn room_dimensions_are_bounded_and_finite() {
        assert_eq!(
            room_dimensions(f32::NAN, f32::NAN, f32::NAN).0,
            DEFAULT_ROOM_WIDTH
        );
        assert_eq!(
            room_dimensions(f32::NAN, f32::NAN, f32::NAN).1,
            DEFAULT_ROOM_DEPTH
        );
        assert_eq!(room_dimensions(1.0, 1.0, 1.0).0, MIN_ROOM_DIMENSION);
        assert_eq!(
            room_dimensions(1000.0, 1000.0, 1000.0).1,
            MAX_ROOM_DIMENSION
        );
        assert!(room_dimensions(24.0, 18.0, 8.0).2.is_finite());
    }
}
