use bevy_ecs::prelude::*;

#[derive(Component, Debug)]
pub struct LightBeam {
    pub parent_fixture: Entity,
    pub intensity: f32,
    pub color: (u8, u8, u8),
    pub radius: f32,
}

impl LightBeam {
    pub fn new(parent_fixture: Entity, color: (u8, u8, u8), intensity: u8, radius: f32) -> Self {
        Self {
            parent_fixture,
            intensity: intensity as f32 / 255.0,
            color,
            radius,
        }
    }

    pub fn effective_alpha(&self) -> u8 {
        (self.intensity * 128.0) as u8
    }
}
