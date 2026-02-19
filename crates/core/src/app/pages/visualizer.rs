use crate::app::BlaulichtApp;
use blaulicht_shared::RGBColor;
use egui::{Context, Sense, Vec2};
use egui_glow::glow::{self, HasContext};
use egui_glow::CallbackFn;
use std::sync::{Arc, Mutex};

const FIXTURE_BODY_COLOR: [f32; 3] = [0.35, 0.35, 0.36];
const FIXTURE_BORDER_COLOR: [f32; 3] = [0.32, 0.32, 0.36];
const FIXTURE_EDGE_COLOR: [f32; 3] = [0.0, 0.0, 0.0];
const FIXTURE_BORDER_SCALE: f32 = 1.04;

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
    color: [f32; 3],
    size: f32,
    beam_dir: Vec3,
    beam_len: f32,
    beam_color: [f32; 3],
    beam_strength: f32,
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
            let rgb = rgb.with_alpha(state.alpha);
            let alpha = (state.alpha as f32 / 255.0).clamp(0.0, 1.0);
            let pan_deg = (state.orientation.pan as f32 / 255.0) * 540.0 - 270.0;
            let tilt_deg = (state.orientation.tilt as f32 / 255.0) * 270.0 - 135.0;
            let rotation = Vec3::new(
                fixture.rotation.x.to_radians(),
                fixture.rotation.y.to_radians(),
                fixture.rotation.z.to_radians(),
            );
            let beam_dir = rotate_vec3(dir_from_pan_tilt(pan_deg, tilt_deg), rotation);
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

            fixtures.push(RenderFixture {
                pos,
                rotation,
                color: [
                    rgb.r as f32 / 255.0,
                    rgb.g as f32 / 255.0,
                    rgb.b as f32 / 255.0,
                ],
                size: 0.6,
                beam_dir,
                beam_len: 10.0,
                beam_color: [
                    (rgb.r as f32 / 255.0).clamp(0.0, 1.0),
                    (rgb.g as f32 / 255.0).clamp(0.0, 1.0),
                    (rgb.b as f32 / 255.0).clamp(0.0, 1.0),
                ],
                beam_strength: alpha,
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

fn dir_from_pan_tilt(pan_deg: f32, tilt_deg: f32) -> Vec3 {
    let pan = pan_deg.to_radians();
    let tilt = tilt_deg.to_radians();
    let cos_tilt = tilt.cos();
    Vec3::new(pan.sin() * cos_tilt, -tilt.sin(), pan.cos() * cos_tilt).normalize()
}

fn rotate_vec3(mut v: Vec3, rot: Vec3) -> Vec3 {
    let (sx, cx) = rot.x.sin_cos();
    let y = v.y * cx - v.z * sx;
    let z = v.y * sx + v.z * cx;
    v.y = y;
    v.z = z;

    let (sy, cy) = rot.y.sin_cos();
    let x = v.x * cy - v.z * sy;
    let z = v.x * sy + v.z * cy;
    v.x = x;
    v.z = z;

    let (sz, cz) = rot.z.sin_cos();
    let x = v.x * cz - v.y * sz;
    let y = v.x * sz + v.y * cz;
    v.x = x;
    v.y = y;

    v
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
    beam_vao: glow::VertexArray,
    beam_vbo: glow::Buffer,
    beam_vertex_count: i32,
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
            let (beam_vao, beam_vbo) = create_dynamic_lines(gl)?;
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
                beam_vao,
                beam_vbo,
                beam_vertex_count: 0,
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

            self.update_beams(gl, fixtures);
            if self.beam_vertex_count > 0 {
                set_use_vertex_color(gl, &self.u_use_vertex_color, true);
                gl.bind_vertex_array(Some(self.beam_vao));
                let mvp = mat4_mul(projection, mat4_mul(view, mat4_identity()));
                if let Some(loc) = &self.u_mvp {
                    gl.uniform_matrix_4_f32_slice(Some(loc), false, &mvp);
                }
                gl.line_width(2.0);
                gl.draw_arrays(glow::LINES, 0, self.beam_vertex_count);
            }

            if !fixtures.is_empty() {
                set_use_vertex_color(gl, &self.u_use_vertex_color, false);
                for fixture in fixtures {
                    let rotation = mat4_mul(
                        mat4_rotation_z(fixture.rotation.z),
                        mat4_mul(
                            mat4_rotation_y(fixture.rotation.y),
                            mat4_rotation_x(fixture.rotation.x),
                        ),
                    );
                    let model = mat4_mul(
                        mat4_translation(fixture.pos.x, fixture.pos.y, fixture.pos.z),
                        mat4_mul(rotation, mat4_scale(fixture.size, fixture.size, fixture.size)),
                    );
                    let mvp = mat4_mul(projection, mat4_mul(view, model));

                    gl.bind_vertex_array(Some(self.cube_vao));

                    gl.enable(glow::CULL_FACE);
                    gl.cull_face(glow::FRONT);
                    if let Some(loc) = &self.u_color {
                        gl.uniform_3_f32(
                            Some(loc),
                            FIXTURE_BORDER_COLOR[0],
                            FIXTURE_BORDER_COLOR[1],
                            FIXTURE_BORDER_COLOR[2],
                        );
                    }
                    let outline_size = fixture.size * FIXTURE_BORDER_SCALE;
                    let outline_model = mat4_mul(
                        mat4_translation(fixture.pos.x, fixture.pos.y, fixture.pos.z),
                        mat4_mul(rotation, mat4_scale(outline_size, outline_size, outline_size)),
                    );
                    let outline_mvp = mat4_mul(projection, mat4_mul(view, outline_model));
                    if let Some(loc) = &self.u_mvp {
                        gl.uniform_matrix_4_f32_slice(Some(loc), false, &outline_mvp);
                    }
                    gl.draw_elements(
                        glow::TRIANGLES,
                        self.cube_index_count,
                        glow::UNSIGNED_SHORT,
                        0,
                    );
                    gl.cull_face(glow::BACK);
                    gl.disable(glow::CULL_FACE);

                    gl.enable(glow::POLYGON_OFFSET_FILL);
                    gl.polygon_offset(1.0, 1.0);

                    if let Some(loc) = &self.u_color {
                        gl.uniform_3_f32(
                            Some(loc),
                            FIXTURE_BODY_COLOR[0],
                            FIXTURE_BODY_COLOR[1],
                            FIXTURE_BODY_COLOR[2],
                        );
                    }
                    if let Some(loc) = &self.u_mvp {
                        gl.uniform_matrix_4_f32_slice(Some(loc), false, &mvp);
                    }
                    let index_stride = std::mem::size_of::<u16>() as i32;
                    gl.draw_elements(glow::TRIANGLES, 24, glow::UNSIGNED_SHORT, 0);
                    gl.draw_elements(
                        glow::TRIANGLES,
                        6,
                        glow::UNSIGNED_SHORT,
                        30 * index_stride,
                    );

                    if let Some(loc) = &self.u_color {
                        gl.uniform_3_f32(
                            Some(loc),
                            fixture.color[0],
                            fixture.color[1],
                            fixture.color[2],
                        );
                    }
                    gl.draw_elements(
                        glow::TRIANGLES,
                        6,
                        glow::UNSIGNED_SHORT,
                        24 * index_stride,
                    );

                    gl.disable(glow::POLYGON_OFFSET_FILL);

                    gl.bind_vertex_array(Some(self.cube_edges_vao));
                    if let Some(loc) = &self.u_color {
                        gl.uniform_3_f32(
                            Some(loc),
                            FIXTURE_EDGE_COLOR[0],
                            FIXTURE_EDGE_COLOR[1],
                            FIXTURE_EDGE_COLOR[2],
                        );
                    }
                    let edge_size = fixture.size;
                    let edge_model = mat4_mul(
                        mat4_translation(fixture.pos.x, fixture.pos.y, fixture.pos.z),
                        mat4_mul(rotation, mat4_scale(edge_size, edge_size, edge_size)),
                    );
                    let edge_mvp = mat4_mul(projection, mat4_mul(view, edge_model));
                    if let Some(loc) = &self.u_mvp {
                        gl.uniform_matrix_4_f32_slice(Some(loc), false, &edge_mvp);
                    }
                    gl.line_width(1.5);
                    gl.draw_arrays(glow::LINES, 0, self.cube_edges_vertex_count);
                }
            }

            gl.bind_vertex_array(None);
            gl.use_program(None);
            gl.disable(glow::DEPTH_TEST);
            gl.disable(glow::SCISSOR_TEST);
        }
    }

    fn update_beams(&mut self, gl: &Arc<glow::Context>, fixtures: &[RenderFixture]) {
        let mut verts: Vec<f32> = Vec::new();
        for fixture in fixtures {
            if fixture.beam_strength <= 0.01 {
                continue;
            }
            let start = fixture.pos;
            let end = fixture.pos.add(fixture.beam_dir.scale(fixture.beam_len));
            let intensity = (0.4 + 0.6 * fixture.beam_strength).clamp(0.0, 1.0);
            let color = [
                (fixture.beam_color[0] * intensity).clamp(0.0, 1.0),
                (fixture.beam_color[1] * intensity).clamp(0.0, 1.0),
                (fixture.beam_color[2] * intensity).clamp(0.0, 1.0),
            ];
            verts.extend_from_slice(&[start.x, start.y, start.z, color[0], color[1], color[2]]);
            verts.extend_from_slice(&[end.x, end.y, end.z, color[0], color[1], color[2]]);
        }

        self.beam_vertex_count = (verts.len() / 6) as i32;
        unsafe {
            gl.bind_vertex_array(Some(self.beam_vao));
            gl.bind_buffer(glow::ARRAY_BUFFER, Some(self.beam_vbo));
            gl.buffer_data_u8_slice(
                glow::ARRAY_BUFFER,
                bytemuck::cast_slice(&verts),
                glow::DYNAMIC_DRAW,
            );
            gl.bind_vertex_array(None);
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
            self.gl.delete_vertex_array(self.beam_vao);
            self.gl.delete_buffer(self.beam_vbo);
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

unsafe fn create_dynamic_lines(
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
