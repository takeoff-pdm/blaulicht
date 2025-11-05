use bevy_ecs::prelude::*;
use blaulicht_shared::fixture::FixtureType;

#[derive(Component, Debug, Clone)]
pub struct FixtureVisual {
    pub group_id: u8,
    pub fixture_id: u8,
    pub fixture_type: FixtureTypeVisual,
    pub position: (f32, f32),
    pub current_color: (u8, u8, u8),
    pub current_intensity: u8,
    pub current_pan: u8,
    pub current_tilt: u8,
}

impl FixtureVisual {
    pub fn new(group_id: u8, fixture_id: u8, fixture_type: FixtureTypeVisual, position: (f32, f32)) -> Self {
        Self {
            group_id,
            fixture_id,
            fixture_type,
            position,
            current_color: (0, 0, 0),
            current_intensity: 0,
            current_pan: 127,
            current_tilt: 127,
        }
    }

    pub fn render_radius(&self) -> f32 {
        match &self.fixture_type {
            FixtureTypeVisual::MovingHead { .. } => 20.0,
            FixtureTypeVisual::Light { .. } => 15.0,
            FixtureTypeVisual::Dimmer => 10.0,
        }
    }

    pub fn light_radius(&self) -> f32 {
        let base = match &self.fixture_type {
            FixtureTypeVisual::MovingHead { beam_angle } => 80.0 + beam_angle * 0.5,
            FixtureTypeVisual::Light { form_factor } => match form_factor {
                LightFormFactor::Spot => 60.0,
                LightFormFactor::Wash => 100.0,
                LightFormFactor::Par => 70.0,
            },
            FixtureTypeVisual::Dimmer => 50.0,
        };
        
        base * (self.current_intensity as f32 / 255.0)
    }
}

#[derive(Debug, Clone)]
pub enum FixtureTypeVisual {
    MovingHead { beam_angle: f32 },
    Light { form_factor: LightFormFactor },
    Dimmer,
}

impl From<&FixtureType> for FixtureTypeVisual {
    fn from(ft: &FixtureType) -> Self {
        match ft {
            FixtureType::MovingHead(_) => FixtureTypeVisual::MovingHead { beam_angle: 30.0 },
            FixtureType::Light(_) => FixtureTypeVisual::Light { form_factor: LightFormFactor::Par },
            FixtureType::Dimmer(_) => FixtureTypeVisual::Dimmer,
        }
    }
}

#[derive(Debug, Clone)]
pub enum LightFormFactor {
    Spot,
    Wash,
    Par,
}
