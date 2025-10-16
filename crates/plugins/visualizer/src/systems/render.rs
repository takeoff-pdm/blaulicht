use bevy_ecs::prelude::*;
use blaulicht_plugin_framework as bpf;
use crate::components::{FixtureVisual, LightBeam, Selected};
use crate::resources::VisualizationConfig;

pub fn render_system(
    config: Res<VisualizationConfig>,
    fixtures: Query<(&FixtureVisual, Option<&Selected>)>,
    beams: Query<(&LightBeam, &FixtureVisual)>,
) {
    let canvas_id = 10;
    
    bpf::ui::painter_begin(canvas_id, config.canvas_width, config.canvas_height);

    bpf::ui::painter_rect(
        0, 0,
        config.canvas_width, config.canvas_height,
        20, 20, 25, 255
    );

    if config.show_grid {
        render_grid(&config);
    }

    if config.show_light_beams {
        for fixture in fixtures.iter() {
            render_light_beam(&config, &fixture.0);
        }
    }

    for (fixture, selected) in fixtures.iter() {
        render_fixture(&config, fixture, selected);
    }

    if config.show_labels {
        for (fixture, _) in fixtures.iter() {
            render_fixture_label(&config, fixture);
        }
    }

    bpf::ui::painter_end();
}

fn render_grid(config: &VisualizationConfig) {
    let grid_spacing = 100.0;
    let grid_color = (40, 40, 45, 255);

    for i in -10..=10 {
        let world_x = i as f32 * grid_spacing;
        let (x1, y1) = config.world_to_screen(world_x, -1000.0);
        let (x2, y2) = config.world_to_screen(world_x, 1000.0);
        
        bpf::ui::painter_line(
            x1, y1, x2, y2,
            grid_color.0, grid_color.1, grid_color.2, grid_color.3,
            1
        );
    }

    for i in -10..=10 {
        let world_y = i as f32 * grid_spacing;
        let (x1, y1) = config.world_to_screen(-1000.0, world_y);
        let (x2, y2) = config.world_to_screen(1000.0, world_y);
        
        bpf::ui::painter_line(
            x1, y1, x2, y2,
            grid_color.0, grid_color.1, grid_color.2, grid_color.3,
            1
        );
    }
}

fn render_light_beam(config: &VisualizationConfig, fixture: &FixtureVisual) {
    if fixture.current_intensity == 0 {
        return;
    }

    let (screen_x, screen_y) = config.world_to_screen(fixture.position.0, fixture.position.1);
    let radius = (fixture.light_radius() * config.scale) as i32;

    let alpha = ((fixture.current_intensity as f32 / 255.0) * 64.0) as i32;

    bpf::ui::painter_circle(
        screen_x,
        screen_y,
        radius,
        fixture.current_color.0 as i32,
        fixture.current_color.1 as i32,
        fixture.current_color.2 as i32,
        alpha.min(255).max(0)
    );
}

fn render_fixture(config: &VisualizationConfig, fixture: &FixtureVisual, selected: Option<&Selected>) {
    let (screen_x, screen_y) = config.world_to_screen(fixture.position.0, fixture.position.1);
    let radius = (fixture.render_radius() * config.scale) as i32;

    let body_color = if fixture.current_intensity > 0 {
        fixture.current_color
    } else {
        (60, 60, 70)
    };

    bpf::ui::painter_circle(
        screen_x,
        screen_y,
        radius,
        body_color.0 as i32,
        body_color.1 as i32,
        body_color.2 as i32,
        255
    );

    if let Some(sel) = selected {
        let selection_color = if sel.fixture_selected {
            (255, 200, 0, 255)
        } else {
            (100, 200, 255, 255)
        };

        bpf::ui::painter_circle_stroke(
            screen_x,
            screen_y,
            radius + 3,
            selection_color.0,
            selection_color.1,
            selection_color.2,
            selection_color.3,
            2
        );
    }

    bpf::ui::painter_circle_stroke(
        screen_x,
        screen_y,
        radius,
        200, 200, 210, 255,
        1
    );
}

fn render_fixture_label(config: &VisualizationConfig, fixture: &FixtureVisual) {
    let (screen_x, screen_y) = config.world_to_screen(fixture.position.0, fixture.position.1);
    let label_y = screen_y + (fixture.render_radius() * config.scale) as i32 + 15;

    let label = format!("{}-{}", fixture.group_id, fixture.fixture_id);
    
    bpf::ui::painter_text(
        screen_x - 10,
        label_y,
        10,
        200, 200, 200, 255,
        &label
    );
}
