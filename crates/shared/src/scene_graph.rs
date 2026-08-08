use bincode::{Decode, Encode};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, HashSet};

use crate::{AnimationSpeedModifier, SectionState};

pub type NodeId = u8;
pub type GraphId = u8;

#[derive(Debug, Clone, Serialize, Deserialize, Encode, Decode, Default)]
pub struct SceneGraphState {
    pub graphs: BTreeMap<GraphId, SceneGraph>,
    #[serde(default)]
    pub focused_graph: Option<GraphId>,
    /// Runtime-only: milliseconds until the active node's soonest time-based
    /// (`AfterDuration`) edge fires, per graph. Recomputed every engine tick and
    /// not persisted to showfiles.
    #[serde(skip)]
    pub active_countdowns: BTreeMap<GraphId, u64>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Encode, Decode)]
pub struct SceneGraph {
    pub name: String,
    pub nodes: BTreeMap<NodeId, SceneGraphNode>,
    pub edges: Vec<SceneEdge>,
    pub active_node: Option<NodeId>,
    pub enabled: bool,
}

impl Default for SceneGraph {
    fn default() -> Self {
        Self {
            name: "New Graph".to_string(),
            nodes: BTreeMap::new(),
            edges: Vec::new(),
            active_node: None,
            enabled: false,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Encode, Decode)]
pub struct SceneGraphNode {
    pub name: String,
    pub scenes: Vec<u8>,
    pub master_alpha: u8,
    pub master_speed: AnimationSpeedModifier,
    #[serde(default)]
    pub pos_x: f32,
    #[serde(default)]
    pub pos_y: f32,
}

impl Default for SceneGraphNode {
    fn default() -> Self {
        Self {
            name: "Node".to_string(),
            scenes: Vec::new(),
            master_alpha: 100,
            master_speed: AnimationSpeedModifier::_1,
            pos_x: 0.0,
            pos_y: 0.0,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Encode, Decode)]
pub struct SceneEdge {
    pub from: NodeId,
    pub to: NodeId,
    pub condition: TransitionCondition,
    pub priority: u8,
}

#[derive(Debug, Clone, Serialize, Deserialize, Encode, Decode)]
pub enum TransitionCondition {
    AfterDuration(u64),
    AfterBeats(u32),
    OnBeatDrop,
    OnNonBeat,
    Manual,
    All(Vec<TransitionCondition>),
    Any(Vec<TransitionCondition>),
    /// Fires on the rising edge of the audio engine entering the `Drop` section.
    OnEnterDrop,
    /// Fires on the rising edge of the audio engine entering the `Breakdown` section.
    OnEnterBreakdown,
}

#[derive(Debug, Clone, Default)]
pub struct NodeActivationState {
    pub activated_at_ms: u64,
    pub beats_since_activation: u32,
}

pub struct AudioConditions {
    pub beat_active: bool,
    pub beat_trigger: bool,
    pub section_state: SectionState,
}

#[derive(Debug, Clone, Default)]
pub struct SceneGraphRuntime {
    pub runtimes: BTreeMap<GraphId, GraphRuntime>,
}

#[derive(Debug, Clone, Default)]
pub struct GraphRuntime {
    pub activation_state: NodeActivationState,
    pub pending_transition: Option<NodeId>,
    /// The node the runtime last saw as active. Used to detect external changes
    /// to `graph.active_node` (showfile load, "Set as Start", manual selection)
    /// so the activation clock is restarted instead of reusing a stale timestamp.
    pub current_node: Option<NodeId>,
    /// The audio section seen last tick, used to fire `OnEnter*` edges on the
    /// rising edge of a section change.
    pub last_section: SectionState,
}

impl SceneGraphRuntime {
    pub fn tick_all(&mut self, state: &mut SceneGraphState, now_ms: u64, audio: &AudioConditions) {
        let SceneGraphState {
            graphs,
            active_countdowns,
            ..
        } = state;
        active_countdowns.clear();
        for (id, graph) in graphs.iter_mut() {
            let runtime = self.runtimes.entry(*id).or_default();
            Self::tick_graph(graph, runtime, now_ms, audio);
            if let Some(remaining) = Self::active_node_countdown(graph, runtime, now_ms) {
                active_countdowns.insert(*id, remaining);
            }
        }
    }

    /// Milliseconds until the active node's soonest `AfterDuration` edge fires,
    /// or `None` if the graph is disabled or the active node has no timed edge.
    fn active_node_countdown(graph: &SceneGraph, runtime: &GraphRuntime, now_ms: u64) -> Option<u64> {
        if !graph.enabled {
            return None;
        }
        let active = graph.active_node?;
        let elapsed = now_ms.saturating_sub(runtime.activation_state.activated_at_ms);
        graph
            .edges
            .iter()
            .filter(|e| e.from == active)
            .filter_map(|e| match &e.condition {
                TransitionCondition::AfterDuration(ms) => Some(ms.saturating_sub(elapsed)),
                _ => None,
            })
            .min()
    }

    fn tick_graph(graph: &mut SceneGraph, runtime: &mut GraphRuntime, now_ms: u64, audio: &AudioConditions) {
        // Detect section rising edges once per tick (consumed every tick, even when
        // disabled, so re-enabling doesn't fire a stale edge).
        let entered_drop =
            audio.section_state == SectionState::Drop && runtime.last_section != SectionState::Drop;
        let entered_breakdown = audio.section_state == SectionState::Breakdown
            && runtime.last_section != SectionState::Breakdown;
        runtime.last_section = audio.section_state;

        if !graph.enabled || graph.active_node.is_none() {
            // Forget the active node so re-enabling (or re-loading) restarts the
            // activation clock from the moment it becomes active again.
            runtime.current_node = None;
            return;
        }

        // Step 0: Detect external changes to active_node (showfile load,
        // "Set as Start", manual selection). The runtime never observed this node
        // being entered, so any leftover activated_at_ms is stale -- restart the
        // activation clock from now instead of reusing it.
        if runtime.current_node != graph.active_node {
            runtime.current_node = graph.active_node;
            runtime.activation_state = NodeActivationState {
                activated_at_ms: now_ms,
                ..Default::default()
            };
            runtime.pending_transition = None;
            return;
        }

        // Step 1: Execute pending transition from last tick.
        if let Some(next_node) = runtime.pending_transition.take() {
            if graph.nodes.contains_key(&next_node) {
                graph.active_node = Some(next_node);
                runtime.current_node = Some(next_node);
                runtime.activation_state = NodeActivationState {
                    activated_at_ms: now_ms,
                    beats_since_activation: 0,
                };
            }
            return;
        }

        // Step 2: Update activation state from audio.
        if audio.beat_trigger {
            runtime.activation_state.beats_since_activation += 1;
        }

        // Step 3: Evaluate outgoing edges, pick highest priority.
        let active = graph.active_node.unwrap();
        let mut best: Option<(u8, NodeId)> = None;

        for edge in &graph.edges {
            if edge.from != active {
                continue;
            }
            if Self::evaluate_condition(
                &runtime.activation_state,
                &edge.condition,
                now_ms,
                audio,
                entered_drop,
                entered_breakdown,
            ) {
                match best {
                    None => best = Some((edge.priority, edge.to)),
                    Some((current_priority, _)) if edge.priority > current_priority => {
                        best = Some((edge.priority, edge.to));
                    }
                    _ => {}
                }
            }
        }

        // Step 4: Set pending transition (executes next tick).
        if let Some((_, target)) = best {
            runtime.pending_transition = Some(target);
        }
    }

    fn evaluate_condition(
        activation: &NodeActivationState,
        condition: &TransitionCondition,
        now_ms: u64,
        audio: &AudioConditions,
        entered_drop: bool,
        entered_breakdown: bool,
    ) -> bool {
        match condition {
            TransitionCondition::AfterDuration(duration_ms) => {
                now_ms.saturating_sub(activation.activated_at_ms) >= *duration_ms
            }
            TransitionCondition::AfterBeats(n) => activation.beats_since_activation >= *n,
            TransitionCondition::OnBeatDrop => audio.beat_trigger,
            TransitionCondition::OnNonBeat => !audio.beat_active,
            TransitionCondition::OnEnterDrop => entered_drop,
            TransitionCondition::OnEnterBreakdown => entered_breakdown,
            TransitionCondition::Manual => false,
            TransitionCondition::All(conditions) => conditions.iter().all(|c| {
                Self::evaluate_condition(activation, c, now_ms, audio, entered_drop, entered_breakdown)
            }),
            TransitionCondition::Any(conditions) => conditions.iter().any(|c| {
                Self::evaluate_condition(activation, c, now_ms, audio, entered_drop, entered_breakdown)
            }),
        }
    }

    pub fn trigger_manual_transition(&mut self, graph_id: GraphId, to: NodeId, graphs: &BTreeMap<GraphId, SceneGraph>) {
        if let Some(graph) = graphs.get(&graph_id) {
            if graph.nodes.contains_key(&to) {
                let runtime = self.runtimes.entry(graph_id).or_default();
                runtime.pending_transition = Some(to);
            }
        }
    }
}

impl SceneGraphState {
    /// Removes references to scenes that are no longer present after scene
    /// deletion or show-file loading.
    pub fn retain_scene_ids(&mut self, valid_scene_ids: &HashSet<u8>) {
        if self
            .focused_graph
            .is_some_and(|graph_id| !self.graphs.contains_key(&graph_id))
        {
            self.focused_graph = None;
        }
        self.active_countdowns
            .retain(|graph_id, _| self.graphs.contains_key(graph_id));

        for graph in self.graphs.values_mut() {
            for node in graph.nodes.values_mut() {
                node.scenes.retain(|scene_id| valid_scene_ids.contains(scene_id));
            }
            if graph.active_node.is_some_and(|node_id| !graph.nodes.contains_key(&node_id)) {
                graph.active_node = None;
            }
        }
    }

    pub fn collect_active_scene_overrides(&self) -> Vec<SceneOverride> {
        let mut overrides = Vec::new();
        for graph in self.graphs.values() {
            if !graph.enabled {
                continue;
            }
            if let Some(node) = graph.active_node.and_then(|id| graph.nodes.get(&id)) {
                for &scene_id in &node.scenes {
                    overrides.push(SceneOverride {
                        scene_id,
                        master_alpha: node.master_alpha,
                        master_speed: node.master_speed,
                    });
                }
            }
        }
        overrides
    }
}

#[derive(Debug, Clone)]
pub struct SceneOverride {
    pub scene_id: u8,
    pub master_alpha: u8,
    pub master_speed: AnimationSpeedModifier,
}
