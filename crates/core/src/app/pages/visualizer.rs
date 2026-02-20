use crate::app::BlaulichtApp;
use blaulicht_shared::{fixture::{light::Light, FixtureType}, RGBColor};
use egui::{Context, Sense, Vec2};
use egui_glow::glow::{self, HasContext};
use egui_glow::CallbackFn;
use std::sync::{Arc, Mutex};

const FIXTURE_BODY_COLOR: [f32; 3] = [0.35, 0.35, 0.36];
const FIXTURE_EDGE_COLOR: [f32; 3] = [0.0, 0.0, 0.0];
const JOINT_COLOR: [f32; 3] = [0.26, 0.26, 0.28];
const HEAD_BODY_COLOR: [f32; 3] = [0.18, 0.18, 0.2];

const BASE_SIZE: Vec3 = Vec3 {
    x: 0.9,
    y: 0.25,
    z: 0.9,
};
const PAN_JOINT_HEIGHT: f32 = 0.12;
const PAN_JOINT_RADIUS: f32 = 0.22;
const YOKE_HEIGHT: f32 = 0.5;
const YOKE_RADIUS: f32 = 0.18;
const TILT_JOINT_RADIUS: f32 = 0.15;
const TILT_JOINT_LENGTH: f32 = 0.25;
const HEAD_SIZE: Vec3 = Vec3 {
    x: 0.45,
    y: 0.28,
    z: 0.5,
};
const BEAM_ANGLE_DEG: f32 = 8.0;
const CONE_SEGMENTS: usize = 24;
const TAKEOFF_LOGO_SCALE: f32 = 0.38;
const TAKEOFF_LOGO_DEPTH: f32 = 0.18;
const TAKEOFF_LOGO_STROKE: f32 = 0.18;
const TAKEOFF_LOGO_W: f32 = 1.0;
const TAKEOFF_LOGO_H: f32 = 1.35;
const TAKEOFF_LOGO_SPACING: f32 = 1.55;
const TAKEOFF_LOGO_KERNING_EO: f32 = 0.0;
const TAKEOFF_LOGO_KERNING_OF: f32 = 0.0;
const TAKEOFF_LOGO_KERNING_FF: f32 = 0.0;
const TAKEOFF_LOGO_BACK_PADDING: f32 = 0.2;
const TAKEOFF_LOGO_BACK_THICKNESS: f32 = 0.08;
const TAKEOFF_LOGO_Y_OFFSET: f32 = 0.35;
const TAKEOFF_BACK_COLOR: [f32; 3] = [0.08, 0.08, 0.09];
const GENERIC_FIXTURE_SIZE: f32 = 0.45;
const TAKEOFF_LOGO_LETTERS: [char; 7] = ['T', 'A', 'K', 'E', 'O', 'F', 'F'];

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct VisualizerSettings {
    pub show_grid: bool,
    pub show_axes: bool,
    pub brightness: f32,
}

impl Default for VisualizerSettings {
    fn default() -> Self {
        Self {
            show_grid: true,
            show_axes: false,
            brightness: 1.2,
        }
    }
}

pub struct VisualizerUiState {
    settings: VisualizerSettings,
    camera_yaw: f32,
    camera_pitch: f32,
    camera_radius: f32,
    drag_start_yaw: Option<f32>,
    drag_start_pitch: Option<f32>,
    drag_last_pos: Option<egui::Pos2>,
    shared: Arc<Mutex<GlowShared>>,
}

impl VisualizerUiState {
    fn new_shared() -> Arc<Mutex<GlowShared>> {
        Arc::new(Mutex::new(GlowShared {
            renderer: None,
            last_error: None,
            settings: VisualizerSettings::default(),
            camera_yaw: 0.0,
            camera_pitch: -0.3,
            camera_radius: 12.0,
            time: 0.0,
            fixtures: Vec::new(),
        }))
    }
}

#[derive(Default)]
struct GlowShared {
    renderer: Option<GlowRenderer>,
    last_error: Option<String>,
    settings: VisualizerSettings,
    camera_yaw: f32,
    camera_pitch: f32,
    camera_radius: f32,
    time: f32,
    fixtures: Vec<RenderFixture>,
}

impl GlowShared {
    fn ensure_renderer(&mut self, gl: &Arc<glow::Context>) {
        if self.renderer.is_some() || self.last_error.is_some() {
            return;
        }
        match GlowRenderer::new(gl) {
            Ok(renderer) => {
                self.renderer = Some(renderer);
            }
            Err(err) => {
                self.last_error = Some(err);
            }
        }
    }
}

impl Default for VisualizerUiState {
    fn default() -> Self {
        Self {
            settings: VisualizerSettings::default(),
            camera_yaw: 0.0,
            camera_pitch: -0.3,
            camera_radius: 12.0,
            drag_start_yaw: None,
            drag_start_pitch: None,
            drag_last_pos: None,
            shared: Self::new_shared(),
        }
    }
}

impl BlaulichtApp {
    pub fn visualizer_ui(&mut self, ui: &mut egui::Ui, _ctx: &Context) {
        ui.heading("Visualizer");
        ui.add_space(4.0);

        let spacing = ui.spacing().item_spacing;
        ui.spacing_mut().item_spacing = egui::vec2(6.0, 4.0);

        ui.horizontal(|ui| {
            ui.checkbox(&mut self.visualizer_ui_state.settings.show_grid, "Grid");
            ui.checkbox(&mut self.visualizer_ui_state.settings.show_axes, "Axes");
            if ui.small_button("Reset Cam").clicked() {
                self.visualizer_ui_state.camera_yaw = 0.0;
                self.visualizer_ui_state.camera_pitch = -0.3;
                self.visualizer_ui_state.camera_radius = 12.0;
            }
        });

        ui.horizontal(|ui| {
            ui.label("Bright");
            let mut brightness_pct =
                (self.visualizer_ui_state.settings.brightness * 100.0).round() as i32;
            if ui
                .add_sized(
                    [180.0, 18.0],
                    egui::Slider::new(&mut brightness_pct, 50..=500).show_value(false),
                )
                .changed()
            {
                self.visualizer_ui_state.settings.brightness =
                    (brightness_pct as f32 / 100.0).clamp(0.5, 5.0);
            }
            ui.label(format!("{brightness_pct}%"));
        });

        ui.add_space(6.0);

        let canvas_size = Vec2::new(ui.available_width(), (ui.available_height()).max(120.0));
        let (rect, _response) = ui.allocate_exact_size(canvas_size, Sense::hover());
        let canvas_id = ui.make_persistent_id("visualizer_canvas");
        let response = ui.interact(rect, canvas_id, Sense::drag());

        if response.drag_started_by(egui::PointerButton::Primary) {
            self.visualizer_ui_state.drag_start_yaw = Some(self.visualizer_ui_state.camera_yaw);
            self.visualizer_ui_state.drag_start_pitch = Some(self.visualizer_ui_state.camera_pitch);
            self.visualizer_ui_state.drag_last_pos = response.interact_pointer_pos();
        }

        if response.dragged_by(egui::PointerButton::Primary) {
            if let (Some(start_yaw), Some(start_pitch)) = (
                self.visualizer_ui_state.drag_start_yaw,
                self.visualizer_ui_state.drag_start_pitch,
            ) {
                if let Some(pos) = response.interact_pointer_pos() {
                    if let Some(last_pos) = self.visualizer_ui_state.drag_last_pos {
                        let delta = pos - last_pos;
                        let rotate_speed = 0.02;
                        self.visualizer_ui_state.camera_yaw -= delta.x * rotate_speed;
                        self.visualizer_ui_state.camera_pitch =
                            (self.visualizer_ui_state.camera_pitch + delta.y * rotate_speed)
                                .clamp(-1.5, 1.2);
                    } else {
                        self.visualizer_ui_state.camera_yaw = start_yaw;
                        self.visualizer_ui_state.camera_pitch = start_pitch;
                    }
                    self.visualizer_ui_state.drag_last_pos = Some(pos);
                }
            }
        }

        if response.drag_stopped_by(egui::PointerButton::Primary) {
            self.visualizer_ui_state.drag_start_yaw = None;
            self.visualizer_ui_state.drag_start_pitch = None;
            self.visualizer_ui_state.drag_last_pos = None;
        }

        if response.hovered() {
            let zoom_delta = ui.input(|i| i.smooth_scroll_delta.y);
            if zoom_delta.abs() > 0.0 {
                self.visualizer_ui_state.camera_radius =
                    (self.visualizer_ui_state.camera_radius - zoom_delta * 0.02).clamp(4.0, 60.0);
            }
        }

        let fixtures = collect_fixtures(&self.data.state.dmx_engine.read().unwrap());
        {
            let mut shared = self.visualizer_ui_state.shared.lock().unwrap();
            shared.settings = self.visualizer_ui_state.settings;
            shared.camera_yaw = self.visualizer_ui_state.camera_yaw;
            shared.camera_pitch = self.visualizer_ui_state.camera_pitch;
            shared.camera_radius = self.visualizer_ui_state.camera_radius;
            shared.time = self.animation_time;
            shared.fixtures = fixtures;
        }

        let shared = self.visualizer_ui_state.shared.clone();
        let callback = CallbackFn::new(move |info, painter| {
            let mut shared = shared.lock().unwrap();
            shared.ensure_renderer(painter.gl());
            let settings = shared.settings;
            let yaw = shared.camera_yaw;
            let pitch = shared.camera_pitch;
            let radius = shared.camera_radius;
            let time = shared.time;
            let fixtures = shared.fixtures.clone();
            if let Some(renderer) = shared.renderer.as_mut() {
                renderer.render(
                    painter.gl(),
                    &settings,
                    yaw,
                    pitch,
                    radius,
                    time,
                    &fixtures,
                    info,
                );
            }
        });

        ui.painter().add(egui::Shape::Callback(egui::PaintCallback {
            rect,
            callback: Arc::new(callback),
        }));

        if let Some(err) = self.visualizer_ui_state.shared.lock().unwrap().last_error.as_ref() {
            ui.add_space(6.0);
            ui.label(format!("Renderer error: {err}"));
        }

        ui.spacing_mut().item_spacing = spacing;
    }
}

#[derive(Debug, Clone, Copy)]
struct RenderFixture {
    pos: Vec3,
    rotation: Vec3,
    pan_rad: f32,
    tilt_rad: f32,
    beam_len: f32,
    beam_color: [f32; 3],
    beam_strength: f32,
    kind: RenderFixtureKind,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum RenderFixtureKind {
    MovingHead,
    TakeOffLogo,
    GenericLight,
    Dimmer,
}

#[derive(Clone, Copy, Debug)]
struct BeamCone {
    apex: Vec3,
    dir: Vec3,
    len: f32,
    radius: f32,
    color: [f32; 3],
    strength: f32,
}

#[derive(Clone, Copy, Debug)]
struct FixturePose {
    base_center: Vec3,
    pan_center: Vec3,
    yoke_center: Vec3,
    tilt_pivot: Vec3,
    head_center: Vec3,
    head_forward: Vec3,
    lens_pos: Vec3,
    base_rot: [f32; 16],
    pan_rot: [f32; 16],
    head_rot: [f32; 16],
}

#[derive(Clone, Copy, Debug)]
struct Stroke {
    center: Vec3,
    size: Vec3,
    rot_z: f32,
}

fn collect_fixtures(engine: &crate::dmx::EngineState) -> Vec<RenderFixture> {
    let mut fixtures = Vec::new();
    let scale = 0.1;

    let scene_id = engine.0.current_scene_focus;
    let scene = engine.0.scenes.get(&scene_id);

    let mut min_x = f32::MAX;
    let mut max_x = f32::MIN;
    let mut min_z = f32::MAX;
    let mut max_z = f32::MIN;
    let mut min_y = f32::MAX;

    for (gid, group) in &engine.0.groups {
        for (fid, fixture) in &group.fixtures {
            let state = scene
                .and_then(|s| s.sink.fixture_states.get(&(*gid, *fid)))
                .cloned()
                .unwrap_or_default();
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

fn compute_fixture_pose(fixture: &RenderFixture) -> FixturePose {
    let base_rot = mat4_rotation_euler(fixture.rotation);
    let pan_rot = mat4_rotation_y(-fixture.pan_rad);
    let tilt_rot = mat4_rotation_x(-fixture.tilt_rad);
    let pan_matrix = mat4_mul(base_rot, pan_rot);
    let head_rot = mat4_mul(pan_matrix, tilt_rot);

    let up = mat4_transform_dir(base_rot, Vec3::new(0.0, 1.0, 0.0)).normalize();
    let base_origin = fixture.pos;
    let base_center = base_origin.add(up.scale(BASE_SIZE.y * 0.5));
    let pan_center = base_origin.add(up.scale(BASE_SIZE.y + PAN_JOINT_HEIGHT * 0.5));
    let yoke_center =
        base_origin.add(up.scale(BASE_SIZE.y + PAN_JOINT_HEIGHT + YOKE_HEIGHT * 0.5));
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

struct GlowRenderer {
    gl: Arc<glow::Context>,
    program: glow::Program,
    cube_vao: glow::VertexArray,
    cube_vbo: glow::Buffer,
    cube_ebo: glow::Buffer,
    cube_index_count: i32,
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
    cube_edges_vao: glow::VertexArray,
    cube_edges_vbo: glow::Buffer,
    cube_edges_vertex_count: i32,
    u_mvp: Option<glow::UniformLocation>,
    u_brightness: Option<glow::UniformLocation>,
    u_color: Option<glow::UniformLocation>,
    u_use_vertex_color: Option<glow::UniformLocation>,
}

impl GlowRenderer {
    fn new(gl: &Arc<glow::Context>) -> Result<Self, String> {
        unsafe {
            let program = create_program(gl)?;
            let (cube_vao, cube_vbo, cube_ebo, cube_index_count) = create_cube(gl)?;
            let (cube_edges_vao, cube_edges_vbo, cube_edges_vertex_count) =
                create_cube_edges(gl)?;
            let (grid_vao, grid_vbo, grid_vertex_count) = create_grid(gl)?;
            let (axes_vao, axes_vbo, axes_vertex_count) = create_axes(gl)?;
            let (cylinder_vao, cylinder_vbo, cylinder_ebo, cylinder_index_count) =
                create_cylinder(gl, 24)?;
            let (cone_vao, cone_vbo) = create_dynamic_mesh(gl)?;
            let u_mvp = gl.get_uniform_location(program, "u_mvp");
            let u_brightness = gl.get_uniform_location(program, "u_brightness");
            let u_color = gl.get_uniform_location(program, "u_color");
            let u_use_vertex_color = gl.get_uniform_location(program, "u_use_vertex_color");

            Ok(Self {
                gl: gl.clone(),
                program,
                cube_vao,
                cube_vbo,
                cube_ebo,
                cube_index_count,
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
                cube_edges_vao,
                cube_edges_vbo,
                cube_edges_vertex_count,
                u_mvp,
                u_brightness,
                u_color,
                u_use_vertex_color,
            })
        }
    }

    fn render(
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
                let strokes = build_takeoff_logo_strokes();
                for fixture in takeoff_fixtures {
                    self.draw_takeoff_sign(gl, projection, view, fixture, &strokes);
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

            gl.bind_vertex_array(None);
            gl.use_program(None);
            gl.disable(glow::DEPTH_TEST);
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

            let segment_step = std::f32::consts::TAU / CONE_SEGMENTS as f32;
            for i in 0..CONE_SEGMENTS {
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
                verts.extend_from_slice(&[p0.x, p0.y, p0.z, rim_color[0], rim_color[1], rim_color[2]]);
                verts.extend_from_slice(&[p1.x, p1.y, p1.z, rim_color[0], rim_color[1], rim_color[2]]);
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
        fixture: &RenderFixture,
        strokes: &[Stroke],
    ) {
        let base_rot = mat4_rotation_euler(fixture.rotation);
        let up = mat4_transform_dir(base_rot, Vec3::new(0.0, 1.0, 0.0)).normalize();
        let sign_center = fixture.pos.add(up.scale(TAKEOFF_LOGO_Y_OFFSET));
        let sign_base = mat4_mul(
            mat4_translation(sign_center.x, sign_center.y, sign_center.z),
            mat4_mul(
                base_rot,
                mat4_scale(TAKEOFF_LOGO_SCALE, TAKEOFF_LOGO_SCALE, TAKEOFF_LOGO_SCALE),
            ),
        );

        let word_width = takeoff_logo_word_width();
        let word_height = TAKEOFF_LOGO_H;
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

        let intensity = (0.2 + 0.8 * fixture.beam_strength).clamp(0.0, 1.0);
        let glow_color = [
            (fixture.beam_color[0] * intensity).clamp(0.0, 1.0),
            (fixture.beam_color[1] * intensity).clamp(0.0, 1.0),
            (fixture.beam_color[2] * intensity).clamp(0.0, 1.0),
        ];

        unsafe {
            if let Some(loc) = &self.u_color {
                gl.uniform_3_f32(Some(loc), glow_color[0], glow_color[1], glow_color[2]);
            }

            gl.enable(glow::BLEND);
            gl.blend_func(glow::ONE, glow::ONE);
            for stroke in strokes {
                let stroke_model = mat4_mul(
                    sign_base,
                    mat4_mul(
                        mat4_translation(stroke.center.x, stroke.center.y, stroke.center.z),
                        mat4_mul(
                            mat4_rotation_z(stroke.rot_z),
                            mat4_scale(stroke.size.x, stroke.size.y, stroke.size.z),
                        ),
                    ),
                );
                let stroke_mvp = mat4_mul(projection, mat4_mul(view, stroke_model));
                if let Some(loc) = &self.u_mvp {
                    gl.uniform_matrix_4_f32_slice(Some(loc), false, &stroke_mvp);
                }
                gl.draw_elements(
                    glow::TRIANGLES,
                    self.cube_index_count,
                    glow::UNSIGNED_SHORT,
                    0,
                );
            }
            gl.disable(glow::BLEND);
        }
    }
}

impl Drop for GlowRenderer {
    fn drop(&mut self) {
        unsafe {
            self.gl.delete_program(self.program);
            self.gl.delete_vertex_array(self.cube_vao);
            self.gl.delete_buffer(self.cube_vbo);
            self.gl.delete_buffer(self.cube_ebo);
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

unsafe fn create_program(gl: &Arc<glow::Context>) -> Result<glow::Program, String> {
    let vertex_shader_source = r#"#version 330
layout (location = 0) in vec3 a_pos;
layout (location = 1) in vec3 a_color;
uniform mat4 u_mvp;
out vec3 v_color;
void main() {
    v_color = a_color;
    gl_Position = u_mvp * vec4(a_pos, 1.0);
}"#;

    let fragment_shader_source = r#"#version 330
in vec3 v_color;
uniform float u_brightness;
uniform vec3 u_color;
uniform float u_use_vertex_color;
out vec4 color;
void main() {
    vec3 base = mix(u_color, v_color, u_use_vertex_color);
    color = vec4(base * u_brightness, 1.0);
}"#;

    let program = gl
        .create_program()
        .map_err(|e| format!("Program create failed: {e}"))?;
    let vs = compile_shader(gl, glow::VERTEX_SHADER, vertex_shader_source)?;
    let fs = compile_shader(gl, glow::FRAGMENT_SHADER, fragment_shader_source)?;

    gl.attach_shader(program, vs);
    gl.attach_shader(program, fs);
    gl.link_program(program);

    gl.delete_shader(vs);
    gl.delete_shader(fs);

    if !gl.get_program_link_status(program) {
        let log = gl.get_program_info_log(program);
        gl.delete_program(program);
        return Err(format!("Program link failed: {log}"));
    }

    Ok(program)
}

unsafe fn compile_shader(
    gl: &Arc<glow::Context>,
    shader_type: u32,
    source: &str,
) -> Result<glow::Shader, String> {
    let shader = gl
        .create_shader(shader_type)
        .map_err(|e| format!("Shader create failed: {e}"))?;
    gl.shader_source(shader, source);
    gl.compile_shader(shader);
    if !gl.get_shader_compile_status(shader) {
        let log = gl.get_shader_info_log(shader);
        gl.delete_shader(shader);
        return Err(format!("Shader compile failed: {log}"));
    }
    Ok(shader)
}

unsafe fn create_cube(
    gl: &Arc<glow::Context>,
) -> Result<(glow::VertexArray, glow::Buffer, glow::Buffer, i32), String> {
    let vertices: [f32; 48] = [
        -1.0, -1.0, -1.0, 0.2, 0.6, 0.95, // 0
        1.0, -1.0, -1.0, 0.2, 0.6, 0.95,  // 1
        1.0, 1.0, -1.0, 0.2, 0.6, 0.95,   // 2
        -1.0, 1.0, -1.0, 0.2, 0.6, 0.95,  // 3
        -1.0, -1.0, 1.0, 0.4, 0.8, 1.0,   // 4
        1.0, -1.0, 1.0, 0.4, 0.8, 1.0,    // 5
        1.0, 1.0, 1.0, 0.4, 0.8, 1.0,     // 6
        -1.0, 1.0, 1.0, 0.4, 0.8, 1.0,    // 7
    ];

    let indices: [u16; 36] = [
        0, 1, 2, 2, 3, 0, // back
        4, 5, 6, 6, 7, 4, // front
        0, 4, 7, 7, 3, 0, // left
        1, 5, 6, 6, 2, 1, // right
        3, 2, 6, 6, 7, 3, // top
        0, 1, 5, 5, 4, 0, // bottom
    ];

    let vao = gl
        .create_vertex_array()
        .map_err(|e| format!("VAO create failed: {e}"))?;
    let vbo = gl
        .create_buffer()
        .map_err(|e| format!("VBO create failed: {e}"))?;
    let ebo = gl
        .create_buffer()
        .map_err(|e| format!("EBO create failed: {e}"))?;

    gl.bind_vertex_array(Some(vao));
    gl.bind_buffer(glow::ARRAY_BUFFER, Some(vbo));
    gl.buffer_data_u8_slice(
        glow::ARRAY_BUFFER,
        bytemuck::cast_slice(&vertices),
        glow::STATIC_DRAW,
    );
    gl.bind_buffer(glow::ELEMENT_ARRAY_BUFFER, Some(ebo));
    gl.buffer_data_u8_slice(
        glow::ELEMENT_ARRAY_BUFFER,
        bytemuck::cast_slice(&indices),
        glow::STATIC_DRAW,
    );

    let stride = 6 * std::mem::size_of::<f32>() as i32;
    gl.enable_vertex_attrib_array(0);
    gl.vertex_attrib_pointer_f32(0, 3, glow::FLOAT, false, stride, 0);
    gl.enable_vertex_attrib_array(1);
    gl.vertex_attrib_pointer_f32(1, 3, glow::FLOAT, false, stride, 12);

    gl.bind_vertex_array(None);

    Ok((vao, vbo, ebo, indices.len() as i32))
}

unsafe fn create_cylinder(
    gl: &Arc<glow::Context>,
    segments: usize,
) -> Result<(glow::VertexArray, glow::Buffer, glow::Buffer, i32), String> {
    let segs = segments.max(3);
    let mut vertices: Vec<f32> = Vec::with_capacity((segs + 3) * 12);
    let mut indices: Vec<u16> = Vec::new();

    for i in 0..=segs {
        let angle = (i as f32 / segs as f32) * std::f32::consts::TAU;
        let x = angle.cos();
        let z = angle.sin();
        vertices.extend_from_slice(&[x, 1.0, z, 1.0, 1.0, 1.0]);
        vertices.extend_from_slice(&[x, -1.0, z, 1.0, 1.0, 1.0]);
    }

    let ring_vert_count = (segs + 1) * 2;
    let top_center_index = ring_vert_count as u16;
    let bottom_center_index = top_center_index + 1;
    vertices.extend_from_slice(&[0.0, 1.0, 0.0, 1.0, 1.0, 1.0]);
    vertices.extend_from_slice(&[0.0, -1.0, 0.0, 1.0, 1.0, 1.0]);

    for i in 0..segs {
        let top_i = (i * 2) as u16;
        let bottom_i = top_i + 1;
        let top_next = ((i + 1) * 2) as u16;
        let bottom_next = top_next + 1;
        indices.extend_from_slice(&[top_i, bottom_i, top_next]);
        indices.extend_from_slice(&[top_next, bottom_i, bottom_next]);
    }

    for i in 0..segs {
        let top_i = (i * 2) as u16;
        let top_next = ((i + 1) * 2) as u16;
        indices.extend_from_slice(&[top_center_index, top_next, top_i]);
    }

    for i in 0..segs {
        let bottom_i = (i * 2 + 1) as u16;
        let bottom_next = ((i + 1) * 2 + 1) as u16;
        indices.extend_from_slice(&[bottom_center_index, bottom_i, bottom_next]);
    }

    let vao = gl
        .create_vertex_array()
        .map_err(|e| format!("Cylinder VAO create failed: {e}"))?;
    let vbo = gl
        .create_buffer()
        .map_err(|e| format!("Cylinder VBO create failed: {e}"))?;
    let ebo = gl
        .create_buffer()
        .map_err(|e| format!("Cylinder EBO create failed: {e}"))?;

    gl.bind_vertex_array(Some(vao));
    gl.bind_buffer(glow::ARRAY_BUFFER, Some(vbo));
    gl.buffer_data_u8_slice(
        glow::ARRAY_BUFFER,
        bytemuck::cast_slice(&vertices),
        glow::STATIC_DRAW,
    );
    gl.bind_buffer(glow::ELEMENT_ARRAY_BUFFER, Some(ebo));
    gl.buffer_data_u8_slice(
        glow::ELEMENT_ARRAY_BUFFER,
        bytemuck::cast_slice(&indices),
        glow::STATIC_DRAW,
    );

    let stride = 6 * std::mem::size_of::<f32>() as i32;
    gl.enable_vertex_attrib_array(0);
    gl.vertex_attrib_pointer_f32(0, 3, glow::FLOAT, false, stride, 0);
    gl.enable_vertex_attrib_array(1);
    gl.vertex_attrib_pointer_f32(1, 3, glow::FLOAT, false, stride, 12);

    gl.bind_vertex_array(None);

    Ok((vao, vbo, ebo, indices.len() as i32))
}

unsafe fn create_cube_edges(
    gl: &Arc<glow::Context>,
) -> Result<(glow::VertexArray, glow::Buffer, i32), String> {
    let color = FIXTURE_EDGE_COLOR;
    let mut verts: Vec<f32> = Vec::new();

    let mut add_edge = |a: (f32, f32, f32), b: (f32, f32, f32)| {
        verts.extend_from_slice(&[a.0, a.1, a.2, color[0], color[1], color[2]]);
        verts.extend_from_slice(&[b.0, b.1, b.2, color[0], color[1], color[2]]);
    };

    let p0 = (-1.0, -1.0, -1.0);
    let p1 = (1.0, -1.0, -1.0);
    let p2 = (1.0, 1.0, -1.0);
    let p3 = (-1.0, 1.0, -1.0);
    let p4 = (-1.0, -1.0, 1.0);
    let p5 = (1.0, -1.0, 1.0);
    let p6 = (1.0, 1.0, 1.0);
    let p7 = (-1.0, 1.0, 1.0);

    add_edge(p0, p1);
    add_edge(p1, p2);
    add_edge(p2, p3);
    add_edge(p3, p0);

    add_edge(p4, p5);
    add_edge(p5, p6);
    add_edge(p6, p7);
    add_edge(p7, p4);

    add_edge(p0, p4);
    add_edge(p1, p5);
    add_edge(p2, p6);
    add_edge(p3, p7);

    create_line_buffer(gl, &verts)
}

unsafe fn create_grid(
    gl: &Arc<glow::Context>,
) -> Result<(glow::VertexArray, glow::Buffer, i32), String> {
    let mut verts: Vec<f32> = Vec::new();
    let size = 10.0;
    let step = 1.0;
    let color = [0.26, 0.26, 0.26];

    let mut add_line = |x1: f32, z1: f32, x2: f32, z2: f32| {
        verts.extend_from_slice(&[x1, 0.0, z1, color[0], color[1], color[2]]);
        verts.extend_from_slice(&[x2, 0.0, z2, color[0], color[1], color[2]]);
    };

    let mut t = -size;
    while t <= size + 0.001 {
        add_line(t, -size, t, size);
        add_line(-size, t, size, t);
        t += step;
    }

    create_line_buffer(gl, &verts)
}

unsafe fn create_axes(
    gl: &Arc<glow::Context>,
) -> Result<(glow::VertexArray, glow::Buffer, i32), String> {
    let mut verts: Vec<f32> = Vec::new();
    let axis_len = 6.0;
    let add_axis = |verts: &mut Vec<f32>, x: f32, y: f32, z: f32, r: f32, g: f32, b: f32| {
        verts.extend_from_slice(&[0.0, 0.0, 0.0, r, g, b]);
        verts.extend_from_slice(&[x, y, z, r, g, b]);
    };

    add_axis(&mut verts, axis_len, 0.0, 0.0, 0.9, 0.1, 0.1);
    add_axis(&mut verts, 0.0, axis_len, 0.0, 0.1, 0.9, 0.1);
    add_axis(&mut verts, 0.0, 0.0, axis_len, 0.1, 0.3, 0.9);

    create_line_buffer(gl, &verts)
}

unsafe fn create_line_buffer(
    gl: &Arc<glow::Context>,
    verts: &[f32],
) -> Result<(glow::VertexArray, glow::Buffer, i32), String> {
    let vao = gl
        .create_vertex_array()
        .map_err(|e| format!("Line VAO create failed: {e}"))?;
    let vbo = gl
        .create_buffer()
        .map_err(|e| format!("Line VBO create failed: {e}"))?;

    gl.bind_vertex_array(Some(vao));
    gl.bind_buffer(glow::ARRAY_BUFFER, Some(vbo));
    gl.buffer_data_u8_slice(
        glow::ARRAY_BUFFER,
        bytemuck::cast_slice(verts),
        glow::STATIC_DRAW,
    );

    let stride = 6 * std::mem::size_of::<f32>() as i32;
    gl.enable_vertex_attrib_array(0);
    gl.vertex_attrib_pointer_f32(0, 3, glow::FLOAT, false, stride, 0);
    gl.enable_vertex_attrib_array(1);
    gl.vertex_attrib_pointer_f32(1, 3, glow::FLOAT, false, stride, 12);

    gl.bind_vertex_array(None);

    Ok((vao, vbo, (verts.len() / 6) as i32))
}

unsafe fn create_dynamic_mesh(
    gl: &Arc<glow::Context>,
) -> Result<(glow::VertexArray, glow::Buffer), String> {
    let vao = gl
        .create_vertex_array()
        .map_err(|e| format!("Dynamic VAO create failed: {e}"))?;
    let vbo = gl
        .create_buffer()
        .map_err(|e| format!("Dynamic VBO create failed: {e}"))?;

    gl.bind_vertex_array(Some(vao));
    gl.bind_buffer(glow::ARRAY_BUFFER, Some(vbo));
    gl.buffer_data_size(glow::ARRAY_BUFFER, 0, glow::DYNAMIC_DRAW);

    let stride = 6 * std::mem::size_of::<f32>() as i32;
    gl.enable_vertex_attrib_array(0);
    gl.vertex_attrib_pointer_f32(0, 3, glow::FLOAT, false, stride, 0);
    gl.enable_vertex_attrib_array(1);
    gl.vertex_attrib_pointer_f32(1, 3, glow::FLOAT, false, stride, 12);

    gl.bind_vertex_array(None);

    Ok((vao, vbo))
}

#[derive(Clone, Copy, Debug)]
struct Vec3 {
    x: f32,
    y: f32,
    z: f32,
}

impl Vec3 {
    fn new(x: f32, y: f32, z: f32) -> Self {
        Self { x, y, z }
    }

    fn sub(self, rhs: Self) -> Self {
        Self::new(self.x - rhs.x, self.y - rhs.y, self.z - rhs.z)
    }

    fn cross(self, rhs: Self) -> Self {
        Self::new(
            self.y * rhs.z - self.z * rhs.y,
            self.z * rhs.x - self.x * rhs.z,
            self.x * rhs.y - self.y * rhs.x,
        )
    }

    fn dot(self, rhs: Self) -> f32 {
        self.x * rhs.x + self.y * rhs.y + self.z * rhs.z
    }

    fn norm(self) -> f32 {
        self.dot(self).sqrt()
    }

    fn normalize(self) -> Self {
        let n = self.norm();
        if n <= f32::EPSILON {
            self
        } else {
            Self::new(self.x / n, self.y / n, self.z / n)
        }
    }

    fn add(self, rhs: Self) -> Self {
        Self::new(self.x + rhs.x, self.y + rhs.y, self.z + rhs.z)
    }

    fn scale(self, s: f32) -> Self {
        Self::new(self.x * s, self.y * s, self.z * s)
    }
}

fn basis_from_dir(dir: Vec3) -> (Vec3, Vec3) {
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

fn build_takeoff_logo_strokes() -> Vec<Stroke> {
    let centers = takeoff_logo_centers();
    let mut strokes = Vec::new();
    for (offset_x, letter) in centers
        .into_iter()
        .zip(TAKEOFF_LOGO_LETTERS.iter().copied())
    {
        add_letter_strokes(&mut strokes, letter, offset_x);
    }
    strokes
}

fn takeoff_logo_word_width() -> f32 {
    let count = TAKEOFF_LOGO_LETTERS.len() as f32;
    let mut width = TAKEOFF_LOGO_W * count + TAKEOFF_LOGO_SPACING * (count - 1.0);
    for idx in 1..TAKEOFF_LOGO_LETTERS.len() {
        width += takeoff_logo_kerning(
            TAKEOFF_LOGO_LETTERS[idx - 1],
            TAKEOFF_LOGO_LETTERS[idx],
        );
    }
    width
}

fn takeoff_logo_centers() -> Vec<f32> {
    let mut centers = Vec::with_capacity(TAKEOFF_LOGO_LETTERS.len());
    let mut cursor_x = 0.0;
    centers.push(0.0);
    for idx in 1..TAKEOFF_LOGO_LETTERS.len() {
        let kern = takeoff_logo_kerning(
            TAKEOFF_LOGO_LETTERS[idx - 1],
            TAKEOFF_LOGO_LETTERS[idx],
        );
        cursor_x += TAKEOFF_LOGO_W + TAKEOFF_LOGO_SPACING + kern;
        centers.push(cursor_x);
    }

    let half_w = TAKEOFF_LOGO_W * 0.5;
    let min_x = centers.first().copied().unwrap_or(0.0) - half_w;
    let max_x = centers.last().copied().unwrap_or(0.0) + half_w;
    let shift = -0.5 * (min_x + max_x);
    for center in &mut centers {
        *center += shift;
    }
    centers
}

fn takeoff_logo_kerning(prev: char, next: char) -> f32 {
    match (prev, next) {
        ('E', 'O') => TAKEOFF_LOGO_KERNING_EO,
        ('O', 'F') => TAKEOFF_LOGO_KERNING_OF,
        ('F', 'F') => TAKEOFF_LOGO_KERNING_FF,
        _ => 0.0,
    }
}

fn add_letter_strokes(strokes: &mut Vec<Stroke>, letter: char, offset_x: f32) {
    let half_w = TAKEOFF_LOGO_W * 0.5;
    let half_h = TAKEOFF_LOGO_H * 0.5;
    let top_y = half_h - TAKEOFF_LOGO_STROKE * 0.5;
    let bottom_y = -half_h + TAKEOFF_LOGO_STROKE * 0.5;

    match letter {
        'T' => {
            add_h(strokes, offset_x, -half_w, half_w, top_y);
            add_v(strokes, offset_x, 0.0, -half_h, half_h);
        }
        'A' => {
            add_v(strokes, offset_x, -half_w + TAKEOFF_LOGO_STROKE * 0.5, -half_h, half_h);
            add_v(strokes, offset_x, half_w - TAKEOFF_LOGO_STROKE * 0.5, -half_h, half_h);
            add_h(strokes, offset_x, -half_w, half_w, top_y);
            add_h(strokes, offset_x, -half_w * 0.6, half_w * 0.6, 0.0);
        }
        'K' => {
            add_v(strokes, offset_x, -half_w + TAKEOFF_LOGO_STROKE * 0.5, -half_h, half_h);
            let diag_len = TAKEOFF_LOGO_H * 0.95;
            add_diag(
                strokes,
                offset_x,
                0.1,
                0.25,
                diag_len,
                std::f32::consts::FRAC_PI_4,
            );
            add_diag(
                strokes,
                offset_x,
                0.1,
                -0.25,
                diag_len,
                -std::f32::consts::FRAC_PI_4,
            );
        }
        'E' => {
            add_v(strokes, offset_x, -half_w + TAKEOFF_LOGO_STROKE * 0.5, -half_h, half_h);
            add_h(strokes, offset_x, -half_w, half_w, top_y);
            add_h(strokes, offset_x, -half_w, half_w * 0.65, 0.0);
            add_h(strokes, offset_x, -half_w, half_w, bottom_y);
        }
        'O' => {
            add_v(strokes, offset_x, -half_w + TAKEOFF_LOGO_STROKE * 0.5, -half_h, half_h);
            add_v(strokes, offset_x, half_w - TAKEOFF_LOGO_STROKE * 0.5, -half_h, half_h);
            add_h(strokes, offset_x, -half_w, half_w, top_y);
            add_h(strokes, offset_x, -half_w, half_w, bottom_y);
        }
        'F' => {
            add_v(strokes, offset_x, -half_w + TAKEOFF_LOGO_STROKE * 0.5, -half_h, half_h);
            add_h(strokes, offset_x, -half_w, half_w, top_y);
            add_h(strokes, offset_x, -half_w, half_w * 0.6, 0.0);
        }
        _ => {}
    }
}

fn add_h(strokes: &mut Vec<Stroke>, offset_x: f32, x0: f32, x1: f32, y: f32) {
    let center = Vec3::new(offset_x + (x0 + x1) * 0.5, y, 0.0);
    let width = (x1 - x0).abs();
    strokes.push(Stroke {
        center,
        size: Vec3::new(width, TAKEOFF_LOGO_STROKE, TAKEOFF_LOGO_DEPTH),
        rot_z: 0.0,
    });
}

fn add_v(strokes: &mut Vec<Stroke>, offset_x: f32, x: f32, y0: f32, y1: f32) {
    let center = Vec3::new(offset_x + x, (y0 + y1) * 0.5, 0.0);
    let height = (y1 - y0).abs();
    strokes.push(Stroke {
        center,
        size: Vec3::new(TAKEOFF_LOGO_STROKE, height, TAKEOFF_LOGO_DEPTH),
        rot_z: 0.0,
    });
}

fn add_diag(
    strokes: &mut Vec<Stroke>,
    offset_x: f32,
    center_x: f32,
    center_y: f32,
    length: f32,
    angle: f32,
) {
    strokes.push(Stroke {
        center: Vec3::new(offset_x + center_x, center_y, 0.0),
        size: Vec3::new(length, TAKEOFF_LOGO_STROKE, TAKEOFF_LOGO_DEPTH),
        rot_z: angle,
    });
}

fn mat4_perspective(fov_y: f32, aspect: f32, near: f32, far: f32) -> [f32; 16] {
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

fn mat4_identity() -> [f32; 16] {
    [
        1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0,
    ]
}

fn mat4_translation(x: f32, y: f32, z: f32) -> [f32; 16] {
    [
        1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, x, y, z, 1.0,
    ]
}

fn mat4_scale(x: f32, y: f32, z: f32) -> [f32; 16] {
    [
        x, 0.0, 0.0, 0.0, 0.0, y, 0.0, 0.0, 0.0, 0.0, z, 0.0, 0.0, 0.0, 0.0, 1.0,
    ]
}

fn mat4_rotation_y(angle: f32) -> [f32; 16] {
    let c = angle.cos();
    let s = angle.sin();
    [
        c, 0.0, -s, 0.0, 0.0, 1.0, 0.0, 0.0, s, 0.0, c, 0.0, 0.0, 0.0, 0.0, 1.0,
    ]
}

fn mat4_rotation_x(angle: f32) -> [f32; 16] {
    let c = angle.cos();
    let s = angle.sin();
    [
        1.0, 0.0, 0.0, 0.0, 0.0, c, s, 0.0, 0.0, -s, c, 0.0, 0.0, 0.0, 0.0, 1.0,
    ]
}

fn mat4_rotation_z(angle: f32) -> [f32; 16] {
    let c = angle.cos();
    let s = angle.sin();
    [
        c, s, 0.0, 0.0, -s, c, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0,
    ]
}

fn mat4_rotation_euler(rot: Vec3) -> [f32; 16] {
    mat4_mul(
        mat4_rotation_z(rot.z),
        mat4_mul(mat4_rotation_y(rot.y), mat4_rotation_x(rot.x)),
    )
}

fn mat4_transform_dir(m: [f32; 16], v: Vec3) -> Vec3 {
    Vec3::new(
        m[0] * v.x + m[4] * v.y + m[8] * v.z,
        m[1] * v.x + m[5] * v.y + m[9] * v.z,
        m[2] * v.x + m[6] * v.y + m[10] * v.z,
    )
}

fn mat4_look_at(eye: Vec3, target: Vec3, up: Vec3) -> [f32; 16] {
    let f = target.sub(eye).normalize();
    let s = f.cross(up.normalize()).normalize();
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

fn mat4_mul(a: [f32; 16], b: [f32; 16]) -> [f32; 16] {
    let mut out = [0.0; 16];
    for col in 0..4 {
        for row in 0..4 {
            out[col * 4 + row] =
                a[0 * 4 + row] * b[col * 4 + 0]
                    + a[1 * 4 + row] * b[col * 4 + 1]
                    + a[2 * 4 + row] * b[col * 4 + 2]
                    + a[3 * 4 + row] * b[col * 4 + 3];
        }
    }
    out
}
