use bevy_ecs::prelude::*;
use blaulicht_shared::RGBColor;
use crate::components::{FixtureVisual, FixtureTypeVisual, Selected};
use crate::resources::{CachedEngineState, FixtureEntityMap};

pub fn sync_fixtures_system(
    mut commands: Commands,
    mut state: ResMut<CachedEngineState>,
    mut entity_map: ResMut<FixtureEntityMap>,
    mut fixtures: Query<(Entity, &mut FixtureVisual)>,
    selection_query: Query<Entity, With<Selected>>,
) {
    if !state.needs_sync {
        return;
    }

    for existing_entity in selection_query.iter() {
        commands.entity(existing_entity).remove::<Selected>();
    }

    let mut existing_fixtures = std::collections::HashSet::new();

    for (group_id, group) in &state.state.groups {
        for (fixture_id, fixture) in &group.fixtures {
            existing_fixtures.insert((*group_id, *fixture_id));

            let fixture_visual_type = FixtureTypeVisual::from(&fixture.type_);
            
            let position = (
                fixture.pos.x as f32 * 50.0,
                fixture.pos.y as f32 * 50.0,
            );

            if let Some(entity) = entity_map.get(*group_id, *fixture_id) {
                if let Ok((_, mut visual)) = fixtures.get_mut(entity) {
                    visual.position = position;
                }
            } else {
                let new_fixture = FixtureVisual::new(
                    *group_id,
                    *fixture_id,
                    fixture_visual_type,
                    position,
                );

                let entity = commands.spawn(new_fixture).id();
                entity_map.insert(*group_id, *fixture_id, entity);
            }
        }
    }

    let all_keys: Vec<_> = entity_map.entities.keys().copied().collect();
    for (group_id, fixture_id) in all_keys {
        if !existing_fixtures.contains(&(group_id, fixture_id)) {
            if let Some(entity) = entity_map.remove(group_id, fixture_id) {
                commands.entity(entity).despawn();
            }
        }
    }

    update_selection_markers(&mut commands, &state.state, &entity_map);

    state.mark_synced();
}

pub fn update_fixture_states_system(
    state: Res<CachedEngineState>,
    entity_map: Res<FixtureEntityMap>,
    mut fixtures: Query<&mut FixtureVisual>,
) {
    if let Some(scene) = state.state.scenes.get(&state.state.current_scene_focus) {
        for ((group_id, fixture_id), fixture_state) in &scene.sink.fixture_states {
            if let Some(entity) = entity_map.get(*group_id, *fixture_id) {
                if let Ok(mut visual) = fixtures.get_mut(entity) {
                    let rgb: RGBColor = fixture_state.color.into();
                    visual.current_color = rgb.tup();
                    visual.current_intensity = fixture_state.alpha;
                    visual.current_pan = fixture_state.orientation.pan;
                    visual.current_tilt = fixture_state.orientation.tilt;
                }
            }
        }
    }
}



fn update_selection_markers(
    commands: &mut Commands,
    state: &blaulicht_shared::EngineState,
    entity_map: &FixtureEntityMap,
) {
    for group_id in &state.selection.group_ids {
        if state.selection.fixtures_in_group.is_empty() {
            if let Some(group) = state.groups.get(group_id) {
                for fixture_id in group.fixtures.keys() {
                    if let Some(entity) = entity_map.get(*group_id, *fixture_id) {
                        commands.entity(entity).insert(Selected::group_only());
                    }
                }
            }
        } else {
            for fixture_id in &state.selection.fixtures_in_group {
                if let Some(entity) = entity_map.get(*group_id, *fixture_id) {
                    commands.entity(entity).insert(Selected::fixture());
                }
            }
        }
    }
}
