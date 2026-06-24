use bincode::{Decode, Encode};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

use crate::AnimationSpeedModifier;

pub type NodeId = u8;
pub type GraphId = u8;

#[derive(Debug, Clone, Serialize, Deserialize, Encode, Decode, Default)]
pub struct SceneGraphState {
    pub graphs: BTreeMap<GraphId, SceneGraph>,
    #[serde(default)]
    pub focused_graph: Option<GraphId>,
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
}

#[derive(Debug, Clone, Default)]
pub struct NodeActivationState {
    pub activated_at_ms: u64,
    pub beats_since_activation: u32,
    pub has_seen_non_beat: bool,
    pub has_seen_beat: bool,
}

pub struct AudioConditions {
    pub beat_active: bool,
    pub beat_trigger: bool,
}

#[derive(Debug, Clone, Default)]
pub struct SceneGraphRuntime {
    pub runtimes: BTreeMap<GraphId, GraphRuntime>,
}

#[derive(Debug, Clone, Default)]
pub struct GraphRuntime {
    pub activation_state: NodeActivationState,
    pub pending_transition: Option<NodeId>,
}

impl SceneGraphRuntime {
    pub fn tick_all(&mut self, graphs: &mut BTreeMap<GraphId, SceneGraph>, now_ms: u64, audio: &AudioConditions) {
        for (id, graph) in graphs.iter_mut() {
            let runtime = self.runtimes.entry(*id).or_default();
            Self::tick_graph(graph, runtime, now_ms, audio);
        }
    }

    fn tick_graph(graph: &mut SceneGraph, runtime: &mut GraphRuntime, now_ms: u64, audio: &AudioConditions) {
        if !graph.enabled || graph.active_node.is_none() {
            return;
        }

        // Step 1: Execute pending transition from last tick.
        if let Some(next_node) = runtime.pending_transition.take() {
            if graph.nodes.contains_key(&next_node) {
                graph.active_node = Some(next_node);
                runtime.activation_state = NodeActivationState {
                    activated_at_ms: now_ms,
                    beats_since_activation: 0,
                    has_seen_non_beat: false,
                    has_seen_beat: false,
                };
            }
            return;
        }

        // Step 2: Update activation state from audio.
        if audio.beat_trigger {
            runtime.activation_state.beats_since_activation += 1;
        }
        if audio.beat_active {
            runtime.activation_state.has_seen_beat = true;
        } else {
            runtime.activation_state.has_seen_non_beat = true;
        }

        // Step 3: Evaluate outgoing edges, pick highest priority.
        let active = graph.active_node.unwrap();
        let mut best: Option<(u8, NodeId)> = None;

        for edge in &graph.edges {
            if edge.from != active {
                continue;
            }
            if Self::evaluate_condition(&runtime.activation_state, &edge.condition, now_ms, audio) {
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
    ) -> bool {
        match condition {
            TransitionCondition::AfterDuration(duration_ms) => {
                now_ms.saturating_sub(activation.activated_at_ms) >= *duration_ms
            }
            TransitionCondition::AfterBeats(n) => activation.beats_since_activation >= *n,
            TransitionCondition::OnBeatDrop => {
                activation.has_seen_non_beat && audio.beat_trigger
            }
            TransitionCondition::OnNonBeat => {
                activation.has_seen_beat && !audio.beat_active
            }
            TransitionCondition::Manual => false,
            TransitionCondition::All(conditions) => {
                conditions.iter().all(|c| Self::evaluate_condition(activation, c, now_ms, audio))
            }
            TransitionCondition::Any(conditions) => {
                conditions.iter().any(|c| Self::evaluate_condition(activation, c, now_ms, audio))
            }
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
