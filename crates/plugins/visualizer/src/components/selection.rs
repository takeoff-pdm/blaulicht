use bevy_ecs::prelude::*;

#[derive(Component, Debug)]
pub struct Selected {
    pub group_selected: bool,
    pub fixture_selected: bool,
}

impl Selected {
    pub fn group_only() -> Self {
        Self {
            group_selected: true,
            fixture_selected: false,
        }
    }

    pub fn fixture() -> Self {
        Self {
            group_selected: true,
            fixture_selected: true,
        }
    }

    pub fn is_selected(&self) -> bool {
        self.group_selected || self.fixture_selected
    }
}
