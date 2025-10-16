use bevy_ecs::prelude::*;

#[derive(Resource, Clone)]
pub struct VisualizationConfig {
    pub view_mode: ViewMode,
    pub show_labels: bool,
    pub show_grid: bool,
    pub show_light_beams: bool,
    pub scale: f32,
    pub camera_x: f32,
    pub camera_y: f32,
    pub canvas_width: i32,
    pub canvas_height: i32,
}

impl Default for VisualizationConfig {
    fn default() -> Self {
        Self {
            view_mode: ViewMode::TopDown2D,
            show_labels: true,
            show_grid: true,
            show_light_beams: true,
            scale: 1.0,
            camera_x: 0.0,
            camera_y: 0.0,
            canvas_width: 800,
            canvas_height: 600,
        }
    }
}

impl VisualizationConfig {
    pub fn world_to_screen(&self, world_x: f32, world_y: f32) -> (i32, i32) {
        let screen_x = ((world_x - self.camera_x) * self.scale + self.canvas_width as f32 / 2.0) as i32;
        let screen_y = ((world_y - self.camera_y) * self.scale + self.canvas_height as f32 / 2.0) as i32;
        (screen_x, screen_y)
    }

    pub fn screen_to_world(&self, screen_x: i32, screen_y: i32) -> (f32, f32) {
        let world_x = (screen_x as f32 - self.canvas_width as f32 / 2.0) / self.scale + self.camera_x;
        let world_y = (screen_y as f32 - self.canvas_height as f32 / 2.0) / self.scale + self.camera_y;
        (world_x, world_y)
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum ViewMode {
    TopDown2D,
}
