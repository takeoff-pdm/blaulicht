//! Animation templates the Programmer creates: starter presets turned into
//! templates, and active animations saved as new templates. The engine runs
//! these for `HostCommand::ToggleAnimationPreset` and
//! `HostCommand::SaveAnimationAsTemplate`; the Programmer's Animations dialog
//! reads them to label its buttons.

use blaulicht_shared::{
    scene::{FixtureSelection, Scene},
    ActiveAnimation, AnimationPreset, AnimationSpec, AnimationTemplate, ControlEvent,
};
use std::collections::BTreeMap;

/// The refusal when every template id is taken.
pub const ANIMATION_LIMIT_REACHED: &str = "Animation limit reached";

/// The animations on `selection` in `scene`, keyed by template id. The
/// engine keys them by the *sorted* selection.
pub fn selection_animations<'a>(
    scene: &'a Scene,
    selection: &FixtureSelection,
) -> Option<&'a BTreeMap<u8, ActiveAnimation>> {
    scene.sink.active_animations.get(&selection.sorted())
}

/// The template a starter preset maps to: the lowest-id template named after
/// the preset.
pub fn preset_template_id(
    templates: &BTreeMap<u8, AnimationTemplate>,
    preset: AnimationPreset,
) -> Option<u8> {
    templates
        .iter()
        .find(|(_, template)| template.spec.name == preset.label())
        .map(|(id, _)| *id)
}

/// The lowest unused template id.
fn free_template_id(templates: &BTreeMap<u8, AnimationTemplate>) -> Option<u8> {
    (0..=u8::MAX).find(|id| !templates.contains_key(id))
}

/// Returns the template for `preset`, creating it from
/// `AnimationSpec::preset` if there is none. `None` when every template id is
/// taken.
pub fn ensure_preset_template(
    templates: &mut BTreeMap<u8, AnimationTemplate>,
    preset: AnimationPreset,
) -> Option<u8> {
    if let Some(id) = preset_template_id(templates, preset) {
        return Some(id);
    }
    let id = free_template_id(templates)?;
    templates.insert(
        id,
        AnimationTemplate {
            spec: AnimationSpec::preset(preset),
        },
    );
    Some(id)
}

/// Inserts `spec` as a new template named `name`. `None` when every template
/// id is taken.
pub fn save_as_template(
    templates: &mut BTreeMap<u8, AnimationTemplate>,
    mut spec: AnimationSpec,
    name: &str,
) -> Option<u8> {
    let id = free_template_id(templates)?;
    spec.name = name.trim().to_string();
    templates.insert(id, AnimationTemplate { spec });
    Some(id)
}

/// Tapping a template toggles it: applied and started if it is not on the
/// selection yet (a new animation starts paused), removed if it is. The
/// transaction rolls back as a whole when the engine rejects the add.
pub fn toggle_animation_event(template_id: u8, active: bool) -> ControlEvent {
    if active {
        ControlEvent::RemoveAnimation(template_id)
    } else {
        ControlEvent::Transaction(vec![
            ControlEvent::AddAnimation(template_id),
            ControlEvent::PlayAnimation(template_id),
        ])
    }
}

#[cfg(test)]
mod tests {
    use super::{
        ensure_preset_template, preset_template_id, save_as_template, toggle_animation_event,
    };
    use blaulicht_shared::{AnimationPreset, AnimationSpec, AnimationTemplate, ControlEvent};
    use std::collections::BTreeMap;

    fn template(name: &str) -> AnimationTemplate {
        let mut spec = AnimationSpec::empty();
        spec.name = name.to_string();
        AnimationTemplate { spec }
    }

    #[test]
    fn a_preset_reuses_the_template_named_after_it() {
        let mut templates = BTreeMap::from([(0, template("Other")), (3, template("Kick Flash"))]);
        assert_eq!(
            ensure_preset_template(&mut templates, AnimationPreset::KickFlash),
            Some(3)
        );
        assert_eq!(templates.len(), 2);
        assert_eq!(
            preset_template_id(&templates, AnimationPreset::BassPump),
            None
        );
    }

    #[test]
    fn a_preset_without_template_gets_one_at_the_lowest_free_id() {
        let mut templates = BTreeMap::from([(0, template("Other")), (2, template("More"))]);
        let id = ensure_preset_template(&mut templates, AnimationPreset::HueDrift).unwrap();
        assert_eq!(id, 1);
        assert_eq!(templates[&1].spec.name, "Hue Drift");
        assert_eq!(
            templates[&1].spec.property,
            AnimationPreset::HueDrift.property()
        );
        // Applying it again reuses it.
        assert_eq!(
            ensure_preset_template(&mut templates, AnimationPreset::HueDrift),
            Some(1)
        );
        assert_eq!(templates.len(), 3);
    }

    #[test]
    fn no_template_is_created_when_all_ids_are_taken() {
        let mut templates: BTreeMap<u8, AnimationTemplate> =
            (0..=u8::MAX).map(|id| (id, template("Full"))).collect();
        assert_eq!(
            ensure_preset_template(&mut templates, AnimationPreset::KickFlash),
            None
        );
        assert_eq!(
            save_as_template(&mut templates, AnimationSpec::empty(), "X"),
            None
        );
    }

    #[test]
    fn saving_keeps_the_spec_and_sets_the_trimmed_name() {
        let mut templates = BTreeMap::from([(0, template("A"))]);
        let spec = AnimationSpec::preset(AnimationPreset::TempoSweep);
        let id = save_as_template(&mut templates, spec, "  My Sweep ").unwrap();
        assert_eq!(id, 1);
        assert_eq!(templates[&1].spec.name, "My Sweep");
        assert_eq!(
            templates[&1].spec.property,
            AnimationPreset::TempoSweep.property()
        );
    }

    #[test]
    fn a_tap_toggles_the_animation() {
        let ControlEvent::Transaction(events) = toggle_animation_event(4, false) else {
            panic!("applying is a transaction");
        };
        assert!(matches!(
            events.as_slice(),
            [
                ControlEvent::AddAnimation(4),
                ControlEvent::PlayAnimation(4)
            ]
        ));
        assert!(matches!(
            toggle_animation_event(4, true),
            ControlEvent::RemoveAnimation(4)
        ));
    }
}
