use std::collections::BTreeMap;

use bincode::{Decode, Encode};
use serde::{Deserialize, Serialize};

use crate::{AnimationSpeedModifier, ControlEvent};

/// Master values a view assigns to one of its scenes when it is applied.
/// They are a starting point: the performance page / MIDI can still move
/// the scene's masters afterwards.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug, Serialize, Deserialize, Encode, Decode)]
pub struct ViewSceneMasters {
    /// 0..=100 percent.
    pub master_alpha: u8,
    pub master_speed: AnimationSpeedModifier,
}

impl Default for ViewSceneMasters {
    fn default() -> Self {
        Self {
            master_alpha: 100,
            master_speed: AnimationSpeedModifier::_1,
        }
    }
}

#[derive(Clone, PartialEq, Eq, Hash, Debug, Serialize, Deserialize, Encode, Decode)]
pub struct View {
    pub name: String,
    pub base_scene: u8,
    pub overlays: Vec<u8>,
    /// Keyed by scene id; covers the base scene and every overlay.
    /// A missing entry means the defaults (100 % / 1x).
    #[serde(default)]
    pub masters: BTreeMap<u8, ViewSceneMasters>,
}

impl View {
    pub fn new(name: String, base_scene: u8, overlays: Vec<u8>) -> Self {
        Self {
            name,
            base_scene,
            overlays,
            masters: BTreeMap::new(),
        }
    }

    /// Base scene followed by the overlays.
    pub fn scene_ids(&self) -> impl Iterator<Item = u8> + '_ {
        std::iter::once(self.base_scene).chain(self.overlays.iter().copied())
    }

    pub fn masters_for(&self, scene_id: u8) -> ViewSceneMasters {
        self.masters.get(&scene_id).copied().unwrap_or_default()
    }

    /// Drops master entries for scenes that are neither the base nor an overlay.
    pub fn prune_masters(&mut self) {
        let base = self.base_scene;
        let overlays = self.overlays.clone();
        self.masters
            .retain(|scene_id, _| *scene_id == base || overlays.contains(scene_id));
    }

    /// The events that activate this view: focus + overlays, then the master
    /// alpha / speed of every scene it contains. Wrap in a `Transaction`.
    pub fn apply_events(&self) -> Vec<ControlEvent> {
        let mut events = vec![
            ControlEvent::SetSceneFocus(self.base_scene),
            ControlEvent::SetOverlays(self.overlays.clone()),
        ];
        for scene_id in self.scene_ids() {
            let masters = self.masters_for(scene_id);
            events.push(ControlEvent::SetSceneMasterAlpha(
                scene_id,
                masters.master_alpha.min(100),
            ));
            events.push(ControlEvent::SetSceneMasterSpeed(
                scene_id,
                masters.master_speed,
            ));
        }
        events
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn apply_events_sets_masters_for_base_and_overlays() {
        let mut view = View::new("v".into(), 1, vec![2, 3]);
        view.masters.insert(
            2,
            ViewSceneMasters {
                master_alpha: 60,
                master_speed: AnimationSpeedModifier::_2,
            },
        );

        let events = view.apply_events();
        assert!(matches!(events[0], ControlEvent::SetSceneFocus(1)));
        assert!(matches!(&events[1], ControlEvent::SetOverlays(o) if *o == vec![2, 3]));

        let has = |pred: &dyn Fn(&ControlEvent) -> bool| events.iter().any(pred);
        assert!(has(&|e| matches!(
            e,
            ControlEvent::SetSceneMasterAlpha(1, 100)
        )));
        assert!(has(&|e| matches!(
            e,
            ControlEvent::SetSceneMasterSpeed(1, AnimationSpeedModifier::_1)
        )));
        assert!(has(&|e| matches!(
            e,
            ControlEvent::SetSceneMasterAlpha(2, 60)
        )));
        assert!(has(&|e| matches!(
            e,
            ControlEvent::SetSceneMasterSpeed(2, AnimationSpeedModifier::_2)
        )));
        assert!(has(&|e| matches!(
            e,
            ControlEvent::SetSceneMasterAlpha(3, 100)
        )));
        assert_eq!(events.len(), 2 + 3 * 2);
    }

    #[test]
    fn prune_masters_drops_unused_scenes() {
        let mut view = View::new("v".into(), 1, vec![2]);
        view.masters.insert(1, ViewSceneMasters::default());
        view.masters.insert(2, ViewSceneMasters::default());
        view.masters.insert(9, ViewSceneMasters::default());
        view.prune_masters();
        assert_eq!(view.masters.keys().copied().collect::<Vec<_>>(), vec![1, 2]);
    }

    #[test]
    fn old_showfile_without_masters_deserializes() {
        let view: View =
            serde_json::from_str(r#"{"name":"v","base_scene":1,"overlays":[2]}"#).unwrap();
        assert!(view.masters.is_empty());
        assert_eq!(view.masters_for(2), ViewSceneMasters::default());
    }
}
