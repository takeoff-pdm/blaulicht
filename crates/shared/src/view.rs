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

/// A saved "what is playing" preset: an ordered overlay stack plus the master
/// values each of those scenes starts at.
///
/// Views deliberately do *not* own the render base -- that slot belongs to the
/// ephemeral `BLANK` scene (or, in live mode, to whatever is being programmed).
/// Applying a view therefore never disturbs the operator's current selection.
#[derive(Clone, PartialEq, Eq, Hash, Debug, Serialize, Deserialize, Encode, Decode)]
pub struct View {
    pub name: String,
    pub overlays: Vec<u8>,
    /// Keyed by scene id; covers every overlay.
    /// A missing entry means the defaults (100 % / 1x).
    #[serde(default)]
    pub masters: BTreeMap<u8, ViewSceneMasters>,
}

impl View {
    pub fn new(name: String, overlays: Vec<u8>) -> Self {
        Self {
            name,
            overlays,
            masters: BTreeMap::new(),
        }
    }

    /// The scenes this view activates, least-significant first.
    pub fn scene_ids(&self) -> impl Iterator<Item = u8> + '_ {
        self.overlays.iter().copied()
    }

    pub fn masters_for(&self, scene_id: u8) -> ViewSceneMasters {
        self.masters.get(&scene_id).copied().unwrap_or_default()
    }

    /// Drops master entries for scenes that are no longer overlays.
    pub fn prune_masters(&mut self) {
        let overlays = self.overlays.clone();
        self.masters
            .retain(|scene_id, _| overlays.contains(scene_id));
    }

    /// The events that activate this view: the overlay stack, then the master
    /// alpha / speed of every scene it contains. Wrap in a `Transaction`.
    pub fn apply_events(&self) -> Vec<ControlEvent> {
        let mut events = vec![ControlEvent::SetOverlays(self.overlays.clone())];
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
    fn apply_events_sets_masters_for_every_overlay() {
        let mut view = View::new("v".into(), vec![1, 2, 3]);
        view.masters.insert(
            2,
            ViewSceneMasters {
                master_alpha: 60,
                master_speed: AnimationSpeedModifier::_2,
            },
        );

        let events = view.apply_events();
        // A view never touches the scene the operator is editing.
        assert!(
            !events
                .iter()
                .any(|e| matches!(e, ControlEvent::SetSceneFocus(_)))
        );
        assert!(matches!(&events[0], ControlEvent::SetOverlays(o) if *o == vec![1, 2, 3]));

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
        assert_eq!(events.len(), 1 + 3 * 2);
    }

    #[test]
    fn prune_masters_drops_unused_scenes() {
        let mut view = View::new("v".into(), vec![1, 2]);
        view.masters.insert(1, ViewSceneMasters::default());
        view.masters.insert(2, ViewSceneMasters::default());
        view.masters.insert(9, ViewSceneMasters::default());
        view.prune_masters();
        assert_eq!(view.masters.keys().copied().collect::<Vec<_>>(), vec![1, 2]);
    }

    #[test]
    fn view_without_masters_deserializes() {
        let view: View = serde_json::from_str(r#"{"name":"v","overlays":[2]}"#).unwrap();
        assert!(view.masters.is_empty());
        assert_eq!(view.masters_for(2), ViewSceneMasters::default());
    }
}
