use blaulicht_plugin_framework as bpf;
use blaulicht_plugin_framework::prelude::println;
use blaulicht_plugin_framework::Plugin;
use blaulicht_shared::{TickInput, EngineState, ControlEvent, ControlEventMessage, PluginUiEvent};
use std::collections::HashMap;

/// Configuration for the 3D visualization renderer
#[derive(Clone)]
struct VisualizationConfig {
    canvas_width: i32,
    canvas_height: i32,
    show_labels: bool,
    show_grid: bool,
    brightness: f32,
    camera_rotation_x: f32,
    camera_rotation_y: f32,
    camera_distance: f32,
    default_fixture_pan: f32,
    default_fixture_tilt: f32,
}

/// Represents which axis or rotation handle is being dragged
#[derive(Clone, Copy, PartialEq, Debug)]
enum DragHandle {
    XAxis,
    YAxis,
    ZAxis,
    RotationX,
    RotationY,
    RotationZ,
}

/// Camera interaction mode
#[derive(Clone, Copy, PartialEq, Debug)]
enum CameraDragMode {
    None,
    Orbit,
}

impl Default for VisualizationConfig {
    fn default() -> Self {
        Self {
            canvas_width: 1200,
            canvas_height: 800,
            show_labels: true,
            show_grid: true,
            brightness: 1.15,
            camera_rotation_x: 30.0,
            camera_rotation_y: 45.0,
            camera_distance: 500.0,
            default_fixture_pan: 0.0,
            default_fixture_tilt: 0.0,
        }
    }
}

/// Runtime data for a single DMX fixture in the 3D scene
#[derive(Clone)]
struct FixtureData {
    group_id: u8,
    fixture_id: u8,
    x: f32,
    y: f32,
    z: f32,
    pan: f32,
    tilt: f32,
    color: (u8, u8, u8),
    alpha: u8,
}

/// 3D Visualizer Plugin
/// 
/// Provides real-time 3D visualization of DMX fixtures with interactive controls.
/// Features include:
/// - Perspective 3D rendering with orbit camera
/// - Solid cuboid fixtures with back-face culling
/// - Volumetric beam rendering
/// - 6-axis manipulation (translate XYZ, rotate XYZ)
/// - Dynamic floor lighting from fixture beams
/// - Real-time pan/tilt visualization
pub struct VisualizerPlugin {
    config: VisualizationConfig,
    fixtures: HashMap<(u8, u8), FixtureData>,
    selected_fixture: Option<(u8, u8)>,
    custom_positions: HashMap<(u8, u8), (f32, f32, f32)>,
    custom_rotations: HashMap<(u8, u8), (f32, f32, f32)>,
    drag_handle: Option<DragHandle>,
    drag_start_pos: Option<(f32, f32, f32)>,
    drag_start_mouse: Option<(i32, i32)>,
    is_dragging: bool,
    camera_drag_mode: CameraDragMode,
    camera_drag_start: Option<(i32, i32)>,
    camera_rotation_start: Option<(f32, f32)>,
}

impl Default for VisualizerPlugin {
    fn default() -> Self {
        Self {
            config: VisualizationConfig::default(),
            fixtures: HashMap::new(),
            selected_fixture: None,
            custom_positions: HashMap::new(),
            custom_rotations: HashMap::new(),
            drag_handle: None,
            drag_start_pos: None,
            drag_start_mouse: None,
            is_dragging: false,
            camera_drag_mode: CameraDragMode::None,
            camera_drag_start: None,
            camera_rotation_start: None,
        }
    }
}

impl VisualizerPlugin {
    /// Projects 3D world coordinates to 2D screen space
    /// 
    /// Returns `(screen_x, screen_y, scale)` if the point is in front of the camera, `None` otherwise
    fn project_3d_to_2d(&self, x: f32, y: f32, z: f32) -> Option<(i32, i32, f32)> {
        let cx = self.config.canvas_width as f32 / 2.0;
        let cy = self.config.canvas_height as f32 / 2.0;
        
        let rot_x = self.config.camera_rotation_x.to_radians();
        let rot_y = self.config.camera_rotation_y.to_radians();
        
        let cos_x = rot_x.cos();
        let sin_x = rot_x.sin();
        let cos_y = rot_y.cos();
        let sin_y = rot_y.sin();
        
        let x1 = x * cos_y - z * sin_y;
        let z1 = x * sin_y + z * cos_y;
        let y1 = y * cos_x - z1 * sin_x;
        let z2 = y * sin_x + z1 * cos_x + self.config.camera_distance;
        
        if z2 <= 0.0 {
            return None;
        }
        
        let scale = self.config.camera_distance / z2;
        let screen_x = (x1 * scale + cx) as i32;
        let screen_y = (y1 * scale + cy) as i32;
        
        Some((screen_x, screen_y, scale))
    }
    
    /// Calculates ambient occlusion for a point based on proximity to other fixtures
    fn calculate_ambient_occlusion(&self, x: f32, y: f32, z: f32) -> f32 {
        let mut occlusion = 0.0;
        let samples = 4;
        let radius = 100.0;
        
        for i in 0..samples {
            let angle = (i as f32 / samples as f32) * std::f32::consts::PI * 2.0;
            let offset_x = angle.cos() * radius;
            let offset_z = angle.sin() * radius;
            
            for fixture in self.fixtures.values() {
                let dx = (fixture.x + offset_x) - x;
                let dy = fixture.y - y;
                let dz = (fixture.z + offset_z) - z;
                let dist = (dx * dx + dy * dy + dz * dz).sqrt();
                
                if dist < radius && dist > 0.1 {
                    occlusion += (1.0 - dist / radius) * 0.15;
                }
            }
        }
        
        (1.0 - occlusion.min(0.6)).max(0.3)
    }

    /// Automatically adjusts camera distance and rotation to frame all fixtures in view
    fn autoframe(&mut self) {
        if self.fixtures.is_empty() {
            return;
        }

        let mut min_x = f32::MAX;
        let mut max_x = f32::MIN;
        let mut min_y = f32::MAX;
        let mut max_y = f32::MIN;
        let mut min_z = f32::MAX;
        let mut max_z = f32::MIN;

        for fixture in self.fixtures.values() {
            min_x = min_x.min(fixture.x);
            max_x = max_x.max(fixture.x);
            min_y = min_y.min(fixture.y);
            max_y = max_y.max(fixture.y);
            min_z = min_z.min(fixture.z);
            max_z = max_z.max(fixture.z);
        }

        let center_x = (min_x + max_x) / 2.0;
        let center_y = (min_y + max_y) / 2.0;
        let center_z = (min_z + max_z) / 2.0;

        let extent_x = (max_x - min_x).max(1.0);
        let extent_y = (max_y - min_y).max(1.0);
        let extent_z = (max_z - min_z).max(1.0);
        let max_extent = extent_x.max(extent_y).max(extent_z);

        self.config.camera_distance = max_extent * 2.0 + 200.0;
        
        self.config.camera_rotation_x = 30.0;
        self.config.camera_rotation_y = 45.0;
    }

    /// Finds the fixture at the given screen position, if any
    fn find_fixture_at_screen_pos(&self, screen_x: i32, screen_y: i32) -> Option<(u8, u8)> {
        for (key, fixture) in &self.fixtures {
            if let Some((sx, sy, scale)) = self.project_3d_to_2d(fixture.x, fixture.y, fixture.z) {
                let body_width = (30.0 * scale) as i32;
                let body_height = (40.0 * scale) as i32;
                
                let left = sx - body_width / 2;
                let right = sx + body_width / 2;
                let top = sy - body_height;
                let bottom = sy;
                
                if screen_x >= left && screen_x <= right && screen_y >= top && screen_y <= bottom {
                    return Some(*key);
                }
            }
        }
        
        None
    }

    /// Determines which drag handle (translation or rotation axis) is at the given screen position
    fn find_drag_handle_at_screen_pos(&self, screen_x: i32, screen_y: i32, fixture_key: (u8, u8)) -> Option<DragHandle> {
        let fixture = self.fixtures.get(&fixture_key)?;
        let (fx, fy, _) = self.project_3d_to_2d(fixture.x, fixture.y, fixture.z)?;
        
        let handle_length = 50.0;
        let hit_radius = 10.0;
        let rotation_handle_distance = 80.0;
        
        let (x_end_x, x_end_y, _) = self.project_3d_to_2d(fixture.x + handle_length, fixture.y, fixture.z)?;
        let dx = (x_end_x - screen_x) as f32;
        let dy = (x_end_y - screen_y) as f32;
        if (dx * dx + dy * dy).sqrt() <= hit_radius {
            return Some(DragHandle::XAxis);
        }
        
        let (y_end_x, y_end_y, _) = self.project_3d_to_2d(fixture.x, fixture.y + handle_length, fixture.z)?;
        let dx = (y_end_x - screen_x) as f32;
        let dy = (y_end_y - screen_y) as f32;
        if (dx * dx + dy * dy).sqrt() <= hit_radius {
            return Some(DragHandle::YAxis);
        }
        
        let (z_end_x, z_end_y, _) = self.project_3d_to_2d(fixture.x, fixture.y, fixture.z + handle_length)?;
        let dx = (z_end_x - screen_x) as f32;
        let dy = (z_end_y - screen_y) as f32;
        if (dx * dx + dy * dy).sqrt() <= hit_radius {
            return Some(DragHandle::ZAxis);
        }
        
        let rot_x_handle_x = fx + rotation_handle_distance as i32;
        let rot_x_handle_y = fy - 60;
        let dx = (rot_x_handle_x - screen_x) as f32;
        let dy = (rot_x_handle_y - screen_y) as f32;
        if (dx * dx + dy * dy).sqrt() <= hit_radius {
            return Some(DragHandle::RotationX);
        }
        
        let rot_y_handle_x = fx;
        let rot_y_handle_y = fy - 80;
        let dx = (rot_y_handle_x - screen_x) as f32;
        let dy = (rot_y_handle_y - screen_y) as f32;
        if (dx * dx + dy * dy).sqrt() <= hit_radius {
            return Some(DragHandle::RotationY);
        }
        
        let rot_z_handle_x = fx - rotation_handle_distance as i32;
        let rot_z_handle_y = fy - 60;
        let dx = (rot_z_handle_x - screen_x) as f32;
        let dy = (rot_z_handle_y - screen_y) as f32;
        if (dx * dx + dy * dy).sqrt() <= hit_radius {
            return Some(DragHandle::RotationZ);
        }
        
        None
    }

    /// Converts 2D screen coordinates back to 3D world space at a given Z depth
    fn unproject_screen_to_3d(&self, screen_x: i32, screen_y: i32, z_3d: f32) -> (f32, f32, f32) {
        let cx = self.config.canvas_width as f32 / 2.0;
        let cy = self.config.canvas_height as f32 / 2.0;
        
        let rot_x = self.config.camera_rotation_x.to_radians();
        let rot_y = self.config.camera_rotation_y.to_radians();
        
        let z2 = z_3d + self.config.camera_distance;
        let scale = self.config.camera_distance / z2;
        
        let x1 = ((screen_x as f32 - cx) / scale);
        let y1 = ((screen_y as f32 - cy) / scale);
        
        let cos_x = rot_x.cos();
        let sin_x = rot_x.sin();
        let cos_y = rot_y.cos();
        let sin_y = rot_y.sin();
        
        let z1 = (z2 - y1 * sin_x) / cos_x;
        let y = y1 * cos_x + z1 * sin_x;
        let x = x1 * cos_y + z1 * sin_y;
        let z = z1 * cos_y - x1 * sin_y;
        
        (x, y, z)
    }

    /// Updates the fixture data from the current scene state
    fn update_fixtures(&mut self, state: &EngineState) {
        if state.scenes.is_empty() {
            return;
        }
        
        let current_scene_id = state.current_scene_focus;
        let scene = match state.scenes.get(&current_scene_id) {
            Some(s) => s,
            None => return,
        };

        self.fixtures.clear();
        
        let mut fixture_index = 0;
        for ((group_id, fixture_id), fixture_state) in &scene.sink.fixture_states {
            let key = (*group_id, *fixture_id);
            
            let (x, y, z) = if let Some(&(cx, cy, cz)) = self.custom_positions.get(&key) {
                (cx, cy, cz)
            } else {
                (
                    (fixture_index % 10) as f32 * 80.0 - 400.0,
                    0.0,
                    (fixture_index / 10) as f32 * 80.0
                )
            };
            
            let color = fixture_state.color;
            let rgb: blaulicht_shared::RGBColor = color.into();
            let alpha = fixture_state.alpha;
            
            let pan = (fixture_state.orientation.pan as f32 / 255.0) * 540.0 - 270.0;
            let tilt = (fixture_state.orientation.tilt as f32 / 255.0) * 270.0 - 135.0;
            
            self.fixtures.insert((*group_id, *fixture_id), FixtureData {
                group_id: *group_id,
                fixture_id: *fixture_id,
                x,
                y,
                z,
                pan,
                tilt,
                color: (rgb.r, rgb.g, rgb.b),
                alpha,
            });
            
            fixture_index += 1;
        }
    }

    /// Processes UI interaction events (clicks, drags, pinches)
    fn handle_events(&mut self, events: &[ControlEventMessage]) {
        for e in events {
            if let ControlEvent::PluginUi(ui_ev) = e.body() {
                match ui_ev {
                    PluginUiEvent::Checkbox { id, checked } if id == 1 => {
                        self.config.show_labels = checked;
                    }
                    PluginUiEvent::Checkbox { id, checked } if id == 2 => {
                        self.config.show_grid = checked;
                    }
                    PluginUiEvent::Slider { id, value } if id == 11 => {
                        // Brightness slider: range 80..200 -> 0.8..2.0
                        self.config.brightness = (value as f32 / 100.0).clamp(0.5, 3.0);
                    }
                    PluginUiEvent::Button { id } if id == 10 => {
                        self.autoframe();
                    }
                    PluginUiEvent::CanvasClick { id, x, y } if id == 10 => {
                        if let Some(fixture_at_click) = self.find_fixture_at_screen_pos(x, y) {
                            self.selected_fixture = Some(fixture_at_click);
                            self.drag_handle = None;
                            self.drag_start_pos = None;
                            self.drag_start_mouse = None;
                            self.is_dragging = false;
                        } else if let Some(selected) = self.selected_fixture {
                            if let Some(handle) = self.find_drag_handle_at_screen_pos(x, y, selected) {
                                self.drag_handle = Some(handle);
                                self.drag_start_mouse = Some((x, y));
                                self.is_dragging = true;
                                if let Some(fixture) = self.fixtures.get(&selected) {
                                    self.drag_start_pos = Some((fixture.x, fixture.y, fixture.z));
                                }
                            } else {
                                self.selected_fixture = None;
                                self.drag_handle = None;
                                self.drag_start_pos = None;
                                self.drag_start_mouse = None;
                                self.is_dragging = false;
                                self.camera_drag_mode = CameraDragMode::Orbit;
                                self.camera_drag_start = Some((x, y));
                                self.camera_rotation_start = Some((self.config.camera_rotation_x, self.config.camera_rotation_y));
                            }
                        } else {
                            self.camera_drag_mode = CameraDragMode::Orbit;
                            self.camera_drag_start = Some((x, y));
                            self.camera_rotation_start = Some((self.config.camera_rotation_x, self.config.camera_rotation_y));
                        }
                    }
                    PluginUiEvent::CanvasDrag { id, x, y, dx, dy } if id == 10 => {
                        if self.is_dragging {
                            if let Some(selected) = self.selected_fixture {
                                if let (Some(handle), Some(start_pos), Some((start_mx, start_my))) = 
                                    (self.drag_handle, self.drag_start_pos, self.drag_start_mouse) {
                                    
                                    let current_dx = x - start_mx;
                                    let current_dy = y - start_my;
                                    let delta_scale = 1.5;
                                    
                                    match handle {
                                        DragHandle::XAxis => {
                                            let new_pos = (start_pos.0 + (current_dx as f32 * delta_scale), start_pos.1, start_pos.2);
                                            self.custom_positions.insert(selected, new_pos);
                                        }
                                        DragHandle::YAxis => {
                                            let new_pos = (start_pos.0, start_pos.1 - (current_dy as f32 * delta_scale), start_pos.2);
                                            self.custom_positions.insert(selected, new_pos);
                                        }
                                        DragHandle::ZAxis => {
                                            let new_pos = (start_pos.0, start_pos.1, start_pos.2 - (current_dy as f32 * delta_scale));
                                            self.custom_positions.insert(selected, new_pos);
                                        }
                                        DragHandle::RotationX => {
                                            let rotation_speed = 1.0;
                                            let current_rot = self.custom_rotations.get(&selected).copied().unwrap_or((0.0, 0.0, 0.0));
                                            let new_rot_x = current_rot.0 + (current_dx as f32 * rotation_speed);
                                            self.custom_rotations.insert(selected, (new_rot_x, current_rot.1, current_rot.2));
                                        }
                                        DragHandle::RotationY => {
                                            let rotation_speed = 1.0;
                                            let current_rot = self.custom_rotations.get(&selected).copied().unwrap_or((0.0, 0.0, 0.0));
                                            let new_rot_y = current_rot.1 - (current_dy as f32 * rotation_speed);
                                            self.custom_rotations.insert(selected, (current_rot.0, new_rot_y, current_rot.2));
                                        }
                                        DragHandle::RotationZ => {
                                            let rotation_speed = 1.0;
                                            let current_rot = self.custom_rotations.get(&selected).copied().unwrap_or((0.0, 0.0, 0.0));
                                            let new_rot_z = current_rot.2 + (current_dx as f32 * rotation_speed);
                                            self.custom_rotations.insert(selected, (current_rot.0, current_rot.1, new_rot_z));
                                        }
                                    }
                                }
                            }
                        } else if self.camera_drag_mode == CameraDragMode::Orbit {
                            if let (Some((start_x, start_y)), Some((start_rot_x, start_rot_y))) = 
                                (self.camera_drag_start, self.camera_rotation_start) {
                                let dx = x - start_x;
                                let dy = y - start_y;
                                
                                self.config.camera_rotation_y = start_rot_y + (dx as f32 * 0.5);
                                self.config.camera_rotation_x = (start_rot_x - (dy as f32 * 0.5)).clamp(-89.0, 89.0);
                            }
                        } else {
                            self.is_dragging = false;
                            self.drag_handle = None;
                            self.drag_start_pos = None;
                            self.drag_start_mouse = None;
                            self.camera_drag_mode = CameraDragMode::None;
                            self.camera_drag_start = None;
                            self.camera_rotation_start = None;
                        }
                    }
                    PluginUiEvent::CanvasPinch { id, x, y, delta } if id == 10 => {
                        self.config.camera_distance = (self.config.camera_distance - delta * 50.0).clamp(100.0, 2000.0);
                    }
                    _ => {}
                }
            }
        }
    }

    /// Draws the UI including controls and visualization canvas
    fn draw_ui(&mut self) {
        println!("[Visualizer] Drawing UI, fixtures: {}", self.fixtures.len());
        bpf::ui::begin();
        bpf::ui::begin_frame_styled(1, "3D Visualizer", 8, 8, 4, 4);
        
        bpf::ui::label("Display Options");
        bpf::ui::separator();
        
        bpf::ui::checkbox("Show Labels", 1, self.config.show_labels);
        bpf::ui::checkbox("Show Grid", 2, self.config.show_grid);

        bpf::ui::begin_horizontal();
        bpf::ui::label("Brightness");
        let brightness_val = (self.config.brightness * 100.0) as u8;
        bpf::ui::slider("brightness", 11, 80, 200, brightness_val);
        bpf::ui::end_horizontal();
        
        bpf::ui::separator();
        
        bpf::ui::button("Autoframe", 10);
        
        bpf::ui::separator();
        self.render_visualization();
        
        bpf::ui::end_frame();
    }

    /// Main rendering method for the 3D visualization
    fn render_visualization(&self) {
        println!("[Visualizer] Rendering canvas: {}x{}", self.config.canvas_width, self.config.canvas_height);
        let canvas_id = 10;
        
        bpf::ui::painter_begin(canvas_id, self.config.canvas_width, self.config.canvas_height);

        // Slightly brighten background with global brightness factor
        let br = self.config.brightness;
        let bg_r = ((5.0 * br).min(255.0)) as u8;
        let bg_g = ((5.0 * br).min(255.0)) as u8;
        let bg_b = ((10.0 * br).min(255.0)) as u8;
        bpf::ui::painter_rect(0, 0, self.config.canvas_width, self.config.canvas_height, bg_r, bg_g, bg_b, 255);

        self.render_floor();
        
        if self.config.show_grid {
            self.render_grid();
        }

        let mut fixtures_with_depth: Vec<_> = self.fixtures.values()
            .filter_map(|f| {
                self.project_3d_to_2d(f.x, f.y, f.z).map(|(x, y, s)| (f, x, y, s, f.z))
            })
            .collect();
        fixtures_with_depth.sort_by(|a, b| b.4.partial_cmp(&a.4).unwrap());

        for (fixture, screen_x, screen_y, scale, _) in &fixtures_with_depth {
            self.render_volumetric_beam(fixture, *screen_x, *screen_y, *scale);
        }

        for (fixture, _, _, _, _) in &fixtures_with_depth {
            self.render_fixture_housing(fixture);
        }

        bpf::ui::painter_end();
    }

    /// Renders the 3D grid on the floor plane
    fn render_grid(&self) {
        let grid_size = 100.0;
        let grid_count = 20;
        
        for i in 0..=grid_count {
            let offset = (i as f32 - grid_count as f32 / 2.0) * grid_size;
            
            let start_x = -grid_count as f32 * grid_size / 2.0;
            let end_x = grid_count as f32 * grid_size / 2.0;
            
            if let (Some((x1, y1, _)), Some((x2, y2, _))) = (
                self.project_3d_to_2d(start_x, 0.0, offset),
                self.project_3d_to_2d(end_x, 0.0, offset)
            ) {
                let br = self.config.brightness;
                let lr = ((60.0 * br).min(255.0)) as u8;
                let lg = ((60.0 * br).min(255.0)) as u8;
                let lb = ((70.0 * br).min(255.0)) as u8;
                bpf::ui::painter_line(x1, y1, x2, y2, lr, lg, lb, 80, 1);
            }
            
            if let (Some((x1, y1, _)), Some((x2, y2, _))) = (
                self.project_3d_to_2d(offset, 0.0, start_x),
                self.project_3d_to_2d(offset, 0.0, end_x)
            ) {
                let br = self.config.brightness;
                let lr = ((60.0 * br).min(255.0)) as u8;
                let lg = ((60.0 * br).min(255.0)) as u8;
                let lb = ((70.0 * br).min(255.0)) as u8;
                bpf::ui::painter_line(x1, y1, x2, y2, lr, lg, lb, 80, 1);
            }
        }
    }
    
    /// Renders the checkered floor with dynamic lighting from fixture beams
    fn render_floor(&self) {
        let floor_y = 0.0;
        let floor_size = 2000.0;
        let segments = 8;
        
        for sy in 0..segments {
            for sx in 0..segments {
                let x1 = -floor_size / 2.0 + (sx as f32 * floor_size / segments as f32);
                let z1 = -floor_size / 2.0 + (sy as f32 * floor_size / segments as f32);
                let x2 = x1 + floor_size / segments as f32;
                let z2 = z1 + floor_size / segments as f32;
                
                if let (Some(p1), Some(p2), Some(p3), Some(p4)) = (
                    self.project_3d_to_2d(x1, floor_y, z1),
                    self.project_3d_to_2d(x2, floor_y, z1),
                    self.project_3d_to_2d(x2, floor_y, z2),
                    self.project_3d_to_2d(x1, floor_y, z2),
                ) {
                    let base_color = if (sx + sy) % 2 == 0 { 18 } else { 22 };
                    
                    let light_intensity = self.calculate_floor_lighting(
                        x1 + floor_size / segments as f32 / 2.0,
                        z1 + floor_size / segments as f32 / 2.0
                    );
                    
                    let br = self.config.brightness;
                    let r = ((base_color as f32 + light_intensity * 200.0) * br).min(255.0) as u8;
                    let g = ((base_color as f32 + light_intensity * 200.0) * br).min(255.0) as u8;
                    let b = (((base_color + 5) as f32 + light_intensity * 200.0) * br).min(255.0) as u8;
                    
                    bpf::ui::painter_line(p1.0, p1.1, p2.0, p2.1, r, g, b, 255, 1);
                    bpf::ui::painter_line(p2.0, p2.1, p3.0, p3.1, r, g, b, 255, 1);
                    bpf::ui::painter_line(p3.0, p3.1, p4.0, p4.1, r, g, b, 255, 1);
                    bpf::ui::painter_line(p4.0, p4.1, p1.0, p1.1, r, g, b, 255, 1);
                }
            }
        }
    }
    
    /// Calculates lighting contribution from all fixture beams at a floor position
    fn calculate_floor_lighting(&self, floor_x: f32, floor_z: f32) -> f32 {
        let mut total_light = 0.0;
        
        for fixture in self.fixtures.values() {
            if fixture.alpha == 0 {
                continue;
            }
            
            let fixture_key = (fixture.group_id, fixture.fixture_id);
            let custom_rot = self.custom_rotations.get(&fixture_key).copied().unwrap_or((0.0, 0.0, 0.0));
            let pan_rad = (fixture.pan + self.config.default_fixture_pan + custom_rot.0).to_radians();
            let tilt_rad = (fixture.tilt + self.config.default_fixture_tilt + custom_rot.1).to_radians();
            
            let dir_x = pan_rad.sin() * tilt_rad.cos();
            let dir_y = -tilt_rad.sin();
            let dir_z = pan_rad.cos() * tilt_rad.cos();
            
            if dir_y >= 0.0 {
                continue;
            }
            
            let t = -fixture.y / dir_y;
            let hit_x = fixture.x + dir_x * t;
            let hit_z = fixture.z + dir_z * t;
            
            let dx = floor_x - hit_x;
            let dz = floor_z - hit_z;
            let dist = (dx * dx + dz * dz).sqrt();
            
            let beam_radius = 150.0;
            if dist < beam_radius {
                let intensity = (1.0 - dist / beam_radius) * (fixture.alpha as f32 / 255.0);
                total_light += intensity * 0.3;
            }
        }
        
        total_light.min(1.0)
    }

    /// Renders the 3D fixture housing as a solid cuboid with proper lighting and rotation
    fn render_fixture_housing(&self, fixture: &FixtureData) {
        let fixture_key = (fixture.group_id, fixture.fixture_id);
        let custom_rot = self.custom_rotations.get(&fixture_key).copied().unwrap_or((0.0, 0.0, 0.0));
        let rot_x_rad = custom_rot.0.to_radians();
        let rot_y_rad = custom_rot.1.to_radians();
        let rot_z_rad = custom_rot.2.to_radians();
        
        let body_width = 30.0;
        let body_height = 40.0;
        let body_depth = 20.0;
        
        let base_corners = [
            (-body_width / 2.0, -body_height, -body_depth / 2.0),
            (body_width / 2.0, -body_height, -body_depth / 2.0),
            (body_width / 2.0, -body_height, body_depth / 2.0),
            (-body_width / 2.0, -body_height, body_depth / 2.0),
            (-body_width / 2.0, 0.0, -body_depth / 2.0),
            (body_width / 2.0, 0.0, -body_depth / 2.0),
            (body_width / 2.0, 0.0, body_depth / 2.0),
            (-body_width / 2.0, 0.0, body_depth / 2.0),
        ];
        
        let mut rotated_corners = [(0.0, 0.0, 0.0); 8];
        for i in 0..8 {
            let (lx, ly, lz) = base_corners[i];
            
            let x1 = lx;
            let y1 = ly * rot_x_rad.cos() - lz * rot_x_rad.sin();
            let z1 = ly * rot_x_rad.sin() + lz * rot_x_rad.cos();
            
            let x2 = x1 * rot_y_rad.cos() + z1 * rot_y_rad.sin();
            let y2 = y1;
            let z2 = -x1 * rot_y_rad.sin() + z1 * rot_y_rad.cos();
            
            let x3 = x2 * rot_z_rad.cos() - y2 * rot_z_rad.sin();
            let y3 = x2 * rot_z_rad.sin() + y2 * rot_z_rad.cos();
            let z3 = z2;
            
            rotated_corners[i] = (
                fixture.x + x3,
                fixture.y + y3,
                fixture.z + z3,
            );
        }
        
        let mut projected = [None; 8];
        for i in 0..8 {
            projected[i] = self.project_3d_to_2d(rotated_corners[i].0, rotated_corners[i].1, rotated_corners[i].2);
        }
        
        if projected.iter().any(|p| p.is_none()) {
            return;
        }
        
        let screen_coords: Vec<_> = projected.iter().map(|p| p.unwrap()).collect();
        
        let ao = self.calculate_ambient_occlusion(fixture.x, fixture.y, fixture.z);
        
        let ambient = 0.18 * ao * self.config.brightness.max(1.0);
        let light_dir = (0.3_f32, -0.7_f32, 0.2_f32);
        
        let base_r = 30;
        let base_g = 30;
        let base_b = 35;
        
        let cam_x = self.config.camera_rotation_y.to_radians().sin() * self.config.camera_distance;
        let cam_z = self.config.camera_rotation_y.to_radians().cos() * self.config.camera_distance;
        let view_dir = (cam_x - fixture.x, 0.0, cam_z - fixture.z);
        let view_len = (view_dir.0 * view_dir.0 + view_dir.2 * view_dir.2).sqrt();
        let view_dir_norm = (view_dir.0 / view_len, view_dir.1, view_dir.2 / view_len);
        
        let faces = [
            ([0, 1, 2, 3], (0.0, -1.0, 0.0)),
            ([4, 5, 6, 7], (0.0, 1.0, 0.0)),
            ([0, 1, 5, 4], (0.0, 0.0, -1.0)),
            ([2, 3, 7, 6], (0.0, 0.0, 1.0)),
            ([0, 3, 7, 4], (-1.0, 0.0, 0.0)),
            ([1, 2, 6, 5], (1.0, 0.0, 0.0)),
        ];
        
        struct FaceData {
            indices: [usize; 4],
            color: (u8, u8, u8),
            depth: f32,
        }
        
        let mut visible_faces = Vec::new();
        
        for (face_indices, base_normal) in &faces {
            let (nx, ny, nz) = *base_normal;
            
            let rx1 = nx;
            let ry1 = ny * rot_x_rad.cos() - nz * rot_x_rad.sin();
            let rz1 = ny * rot_x_rad.sin() + nz * rot_x_rad.cos();
            
            let rx2 = rx1 * rot_y_rad.cos() + rz1 * rot_y_rad.sin();
            let ry2 = ry1;
            let rz2 = -rx1 * rot_y_rad.sin() + rz1 * rot_y_rad.cos();
            
            let final_nx = rx2 * rot_z_rad.cos() - ry2 * rot_z_rad.sin();
            let final_ny = rx2 * rot_z_rad.sin() + ry2 * rot_z_rad.cos();
            let final_nz = rz2;
            
            let dot_view = final_nx * view_dir_norm.0 + final_ny * view_dir_norm.1 + final_nz * view_dir_norm.2;
            
            if dot_view > 0.0 {
                let diffuse = (final_nx * light_dir.0 + final_ny * light_dir.1 + final_nz * light_dir.2).max(0.0) * ao;
                let lighting = (ambient + diffuse * 0.8) * self.config.brightness;
                
                let face_r = (base_r as f32 * lighting).min(255.0) as u8;
                let face_g = (base_g as f32 * lighting).min(255.0) as u8;
                let face_b = (base_b as f32 * lighting).min(255.0) as u8;
                
                let avg_z = (rotated_corners[face_indices[0]].2 + rotated_corners[face_indices[1]].2 + 
                            rotated_corners[face_indices[2]].2 + rotated_corners[face_indices[3]].2) / 4.0;
                
                visible_faces.push(FaceData {
                    indices: *face_indices,
                    color: (face_r, face_g, face_b),
                    depth: avg_z,
                });
            }
        }
        
        visible_faces.sort_by(|a, b| b.depth.partial_cmp(&a.depth).unwrap());
        
        for face in &visible_faces {
            let p0 = screen_coords[face.indices[0]];
            let p1 = screen_coords[face.indices[1]];
            let p2 = screen_coords[face.indices[2]];
            let p3 = screen_coords[face.indices[3]];
            
            let points = [
                (p0.0, p0.1),
                (p1.0, p1.1),
                (p2.0, p2.1),
                (p3.0, p3.1),
            ];
            
            let min_x = points.iter().map(|p| p.0).min().unwrap();
            let max_x = points.iter().map(|p| p.0).max().unwrap();
            let min_y = points.iter().map(|p| p.1).min().unwrap();
            let max_y = points.iter().map(|p| p.1).max().unwrap();
            
            bpf::ui::painter_rect(
                min_x, min_y,
                max_x - min_x, max_y - min_y,
                face.color.0, face.color.1, face.color.2, 255
            );
            
            let er = ((20.0 * self.config.brightness).min(255.0)) as u8;
            let eg = ((20.0 * self.config.brightness).min(255.0)) as u8;
            let eb = ((25.0 * self.config.brightness).min(255.0)) as u8;
            bpf::ui::painter_line(p0.0, p0.1, p1.0, p1.1, er, eg, eb, 255, 1);
            bpf::ui::painter_line(p1.0, p1.1, p2.0, p2.1, er, eg, eb, 255, 1);
            bpf::ui::painter_line(p2.0, p2.1, p3.0, p3.1, er, eg, eb, 255, 1);
            bpf::ui::painter_line(p3.0, p3.1, p0.0, p0.1, er, eg, eb, 255, 1);
        }
        
        let is_selected = self.selected_fixture == Some((fixture.group_id, fixture.fixture_id));
        if is_selected {
            let min_x = screen_coords.iter().map(|p| p.0).min().unwrap() - 3;
            let max_x = screen_coords.iter().map(|p| p.0).max().unwrap() + 3;
            let min_y = screen_coords.iter().map(|p| p.1).min().unwrap() - 3;
            let max_y = screen_coords.iter().map(|p| p.1).max().unwrap() + 3;
            
            bpf::ui::painter_rect_stroke(
                min_x, min_y,
                max_x - min_x, max_y - min_y,
                255, 200, 0, 255, 2
            );
            
            if let Some((screen_x, screen_y, _)) = self.project_3d_to_2d(fixture.x, fixture.y, fixture.z) {
                self.render_drag_handles(fixture, screen_x, screen_y);
            }
        }

        if self.config.show_labels {
            if let Some((screen_x, screen_y, _)) = self.project_3d_to_2d(fixture.x, fixture.y, fixture.z) {
                let label = format!("{}:{}", fixture.group_id, fixture.fixture_id);
                bpf::ui::painter_text(
                    screen_x - 15, screen_y + 5,
                    12, 200, 200, 200, 255,
                    &label
                );
            }
        }
    }

    /// Renders a volumetric light beam emanating from a fixture with fog effects
    fn render_volumetric_beam(&self, fixture: &FixtureData, screen_x: i32, screen_y: i32, _scale: f32) {
        let fixture_key = (fixture.group_id, fixture.fixture_id);
        let custom_rot = self.custom_rotations.get(&fixture_key).copied().unwrap_or((0.0, 0.0, 0.0));
        let pan_rad = (fixture.pan + self.config.default_fixture_pan + custom_rot.0).to_radians();
        let tilt_rad = (fixture.tilt + self.config.default_fixture_tilt + custom_rot.1).to_radians();
        
        let beam_length = 400.0;
        
        let dir_x = pan_rad.sin() * tilt_rad.cos();
        let dir_y = -tilt_rad.sin();
        let dir_z = pan_rad.cos() * tilt_rad.cos();
        
        let segments = 40;
        let cone_angle = 15.0_f32.to_radians();
        
        let mut beam_segments = Vec::with_capacity(segments);
        for seg in 0..segments {
            let t = seg as f32 / segments as f32;
            let next_t = (seg + 1) as f32 / segments as f32;
            
            let beam_x = fixture.x + dir_x * beam_length * t;
            let beam_y = fixture.y + dir_y * beam_length * t;
            let beam_z = fixture.z + dir_z * beam_length * t;
            
            let next_beam_x = fixture.x + dir_x * beam_length * next_t;
            let next_beam_y = fixture.y + dir_y * beam_length * next_t;
            let next_beam_z = fixture.z + dir_z * beam_length * next_t;
            
            if let (Some(projected), Some(next_projected)) = (
                self.project_3d_to_2d(beam_x, beam_y, beam_z),
                self.project_3d_to_2d(next_beam_x, next_beam_y, next_beam_z)
            ) {
                beam_segments.push((projected, next_projected, t, next_t, seg));
            }
        }
        
        for ((bx, by, bs), (nbx, nby, _nbs), t, next_t, seg) in beam_segments {
                let radius = (t * beam_length * cone_angle.tan() * bs * 0.5) as i32;
                
                let fog_density = 0.015;
                let distance = t * beam_length;
                let fog = 1.0 - (-fog_density * distance).exp();
                let fog = fog.min(0.9);
                
                let brightness_curve = (1.0 - t * 0.7) * (1.0 - fog * 0.5);
                let mut intensity = (fixture.alpha as f32 / 255.0) * brightness_curve * self.config.brightness;
                if intensity > 1.0 { intensity = 1.0; }
                
                let base_alpha = ((intensity * 120.0).min(255.0)) as u8;
                
                let ambient_r = (5.0 * fog) as u8;
                let ambient_g = (5.0 * fog) as u8;
                let ambient_b = (8.0 * fog) as u8;
                
                let r = ((fixture.color.0 as f32 * intensity + ambient_r as f32 * fog).min(255.0)) as u8;
                let g = ((fixture.color.1 as f32 * intensity + ambient_g as f32 * fog).min(255.0)) as u8;
                let b = ((fixture.color.2 as f32 * intensity + ambient_b as f32 * fog).min(255.0)) as u8;
                
                let num_rays = 8;
                for ray in 0..num_rays {
                    let angle = (ray as f32 / num_rays as f32) * std::f32::consts::PI * 2.0;
                    let offset_x = (angle.cos() * radius as f32) as i32;
                    let offset_y = (angle.sin() * radius as f32) as i32;
                    
                    let next_radius = (next_t * beam_length * cone_angle.tan() * bs * 0.5) as i32;
                    let next_offset_x = (angle.cos() * next_radius as f32) as i32;
                    let next_offset_y = (angle.sin() * next_radius as f32) as i32;
                    
                    let alpha = (base_alpha as f32 * 0.3) as u8;
                    bpf::ui::painter_line(
                        bx + offset_x, by + offset_y,
                        nbx + next_offset_x, nby + next_offset_y,
                        r, g, b, alpha, 1
                    );
                }
                
                if seg % 3 == 0 {
                    let core_alpha = (base_alpha as f32 * 1.5).min(255.0) as u8;
                    let core_r = ((fixture.color.0 as f32 * 1.2).min(255.0)) as u8;
                    let core_g = ((fixture.color.1 as f32 * 1.2).min(255.0)) as u8;
                    let core_b = ((fixture.color.2 as f32 * 1.2).min(255.0)) as u8;
                    
                    bpf::ui::painter_line(
                        bx, by, nbx, nby,
                        core_r, core_g, core_b, core_alpha, 2
                    );
                }
                
                if seg % 5 == 0 && radius > 0 {
                    let slice_alpha = (base_alpha as f32 * 0.2) as u8;
                    bpf::ui::painter_circle(bx, by, radius, r, g, b, slice_alpha);
                }
        }
    }

    /// Renders the 6-axis manipulation handles (XYZ translation, XYZ rotation)
    fn render_drag_handles(&self, fixture: &FixtureData, screen_x: i32, screen_y: i32) {
        let handle_length = 50.0;
        
        if let Some((x_end_x, x_end_y, _)) = self.project_3d_to_2d(fixture.x + handle_length, fixture.y, fixture.z) {
            let is_x_active = self.drag_handle == Some(DragHandle::XAxis);
            let (r, g, b) = if is_x_active { (255, 100, 100) } else { (255, 50, 50) };
            let thickness = if is_x_active { 3 } else { 2 };
            
            bpf::ui::painter_line(
                screen_x, screen_y,
                x_end_x, x_end_y,
                r, g, b, 255, thickness
            );
            
            bpf::ui::painter_circle(x_end_x, x_end_y, 5, r, g, b, 255);
        }
        
        if let Some((y_end_x, y_end_y, _)) = self.project_3d_to_2d(fixture.x, fixture.y + handle_length, fixture.z) {
            let is_y_active = self.drag_handle == Some(DragHandle::YAxis);
            let (r, g, b) = if is_y_active { (100, 255, 100) } else { (50, 255, 50) };
            let thickness = if is_y_active { 3 } else { 2 };
            
            bpf::ui::painter_line(
                screen_x, screen_y,
                y_end_x, y_end_y,
                r, g, b, 255, thickness
            );
            
            bpf::ui::painter_circle(y_end_x, y_end_y, 5, r, g, b, 255);
        }
        
        if let Some((z_end_x, z_end_y, _)) = self.project_3d_to_2d(fixture.x, fixture.y, fixture.z + handle_length) {
            let is_z_active = self.drag_handle == Some(DragHandle::ZAxis);
            let (r, g, b) = if is_z_active { (100, 100, 255) } else { (50, 50, 255) };
            let thickness = if is_z_active { 3 } else { 2 };
            
            bpf::ui::painter_line(
                screen_x, screen_y,
                z_end_x, z_end_y,
                r, g, b, 255, thickness
            );
            
            bpf::ui::painter_circle(z_end_x, z_end_y, 5, r, g, b, 255);
        }
        
        let is_rot_x_active = self.drag_handle == Some(DragHandle::RotationX);
        let rot_x_color = if is_rot_x_active { (255, 100, 100) } else { (200, 50, 50) };
        let rot_x_radius = if is_rot_x_active { 12 } else { 10 };
        bpf::ui::painter_circle(screen_x + 80, screen_y - 60, rot_x_radius, rot_x_color.0, rot_x_color.1, rot_x_color.2, 255);
        bpf::ui::painter_circle(screen_x + 80, screen_y - 60, rot_x_radius - 3, 0, 0, 0, 255);
        
        let is_rot_y_active = self.drag_handle == Some(DragHandle::RotationY);
        let rot_y_color = if is_rot_y_active { (100, 255, 100) } else { (50, 200, 50) };
        let rot_y_radius = if is_rot_y_active { 12 } else { 10 };
        bpf::ui::painter_circle(screen_x, screen_y - 80, rot_y_radius, rot_y_color.0, rot_y_color.1, rot_y_color.2, 255);
        bpf::ui::painter_circle(screen_x, screen_y - 80, rot_y_radius - 3, 0, 0, 0, 255);
        
        let is_rot_z_active = self.drag_handle == Some(DragHandle::RotationZ);
        let rot_z_color = if is_rot_z_active { (100, 100, 255) } else { (50, 50, 200) };
        let rot_z_radius = if is_rot_z_active { 12 } else { 10 };
        bpf::ui::painter_circle(screen_x - 80, screen_y - 60, rot_z_radius, rot_z_color.0, rot_z_color.1, rot_z_color.2, 255);
        bpf::ui::painter_circle(screen_x - 80, screen_y - 60, rot_z_radius - 3, 0, 0, 0, 255);
    }
}

impl Plugin for VisualizerPlugin {
    fn initialize(&mut self, _input: TickInput) {
        println!("Bevy Visualizer Plugin initialized");
    }

    fn run(&mut self, input: TickInput) {
        let state = bpf::get_dmx();
        self.update_fixtures(&state);
        
        self.handle_events(&input.events.events);
        self.draw_ui();
    }
}

#[no_mangle]
#[inline(never)]
pub extern "C" fn __wasm_bp() {
    core::sync::atomic::compiler_fence(core::sync::atomic::Ordering::SeqCst);
}

#[no_mangle]
extern "C" fn main() {
    __wasm_bp();
    bpf::hook_plugin(Box::new(VisualizerPlugin::default()));
}
