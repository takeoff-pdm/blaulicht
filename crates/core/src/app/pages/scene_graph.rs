use crate::app::BlaulichtApp;
use blaulicht_shared::{
    scene_graph::{
        GraphId, NodeId, SceneEdge, SceneGraph, SceneGraphNode, SceneGraphState,
        TransitionCondition,
    },
    AnimationSpeedModifier,
};
use egui::{Color32, Context, Pos2, Ui};
use egui_snarl::{
    InPin, InPinId, OutPin, OutPinId, Snarl,
    ui::{PinInfo, SnarlStyle, SnarlViewer},
};
use std::collections::BTreeMap;

#[derive(Clone, serde::Serialize, serde::Deserialize)]
pub struct SnarlNode {
    pub node_id: NodeId,
    pub graph_id: GraphId,
}

pub struct SceneGraphUI {
    pub snarl: Snarl<SnarlNode>,
    pub style: SnarlStyle,
    pub selected_graph: Option<GraphId>,
    pub selected_node: Option<NodeId>,
    pub new_graph_name: String,
    pub add_graph_open: bool,
    synced: bool,
}

impl Default for SceneGraphUI {
    fn default() -> Self {
        Self {
            snarl: Snarl::new(),
            style: SnarlStyle::new(),
            selected_graph: None,
            selected_node: None,
            new_graph_name: "New Graph".to_string(),
            add_graph_open: false,
            synced: false,
        }
    }
}

pub struct SceneGraphViewer<'a> {
    pub state: &'a mut SceneGraphState,
    pub graph_id: GraphId,
    pub selected_node: &'a mut Option<NodeId>,
}

#[allow(refining_impl_trait)]
impl<'a> SnarlViewer<SnarlNode> for SceneGraphViewer<'a> {
    fn title(&mut self, node: &SnarlNode) -> String {
        let graph = self.state.graphs.get(&node.graph_id);
        graph
            .and_then(|g| g.nodes.get(&node.node_id))
            .map(|n| n.name.clone())
            .unwrap_or_else(|| "???".to_string())
    }

    fn inputs(&mut self, _node: &SnarlNode) -> usize {
        1
    }

    fn outputs(&mut self, _node: &SnarlNode) -> usize {
        1
    }

    fn show_input(
        &mut self,
        pin: &InPin,
        ui: &mut Ui,
        snarl: &mut Snarl<SnarlNode>,
    ) -> PinInfo {
        let snarl_node = snarl.get_node(pin.id.node);
        let graph = self.state.graphs.get(&self.graph_id);
        let is_active = graph.map_or(false, |g| {
            snarl_node.map_or(false, |n| g.active_node == Some(n.node_id))
        });

        if is_active {
            ui.colored_label(Color32::from_rgb(100, 255, 100), "\u{25CF}");
            PinInfo::circle().with_fill(Color32::from_rgb(100, 255, 100))
        } else {
            PinInfo::circle().with_fill(Color32::from_rgb(150, 150, 150))
        }
    }

    fn show_output(
        &mut self,
        _pin: &OutPin,
        _ui: &mut Ui,
        _snarl: &mut Snarl<SnarlNode>,
    ) -> PinInfo {
        PinInfo::circle().with_fill(Color32::from_rgb(100, 150, 255))
    }

    fn has_body(&mut self, _node: &SnarlNode) -> bool {
        false
    }

    fn show_header(
        &mut self,
        node: egui_snarl::NodeId,
        _inputs: &[InPin],
        _outputs: &[OutPin],
        ui: &mut Ui,
        snarl: &mut Snarl<SnarlNode>,
    ) {
        let snarl_node = &snarl[node];
        let node_id = snarl_node.node_id;

        let is_selected = *self.selected_node == Some(node_id);
        let label = self.title(&snarl[node].clone());

        if ui.selectable_label(is_selected, &label).clicked() {
            *self.selected_node = Some(node_id);
        }
    }

    fn connect(&mut self, from: &OutPin, to: &InPin, snarl: &mut Snarl<SnarlNode>) {
        let from_node = &snarl[from.id.node];
        let to_node = &snarl[to.id.node];

        if from_node.graph_id != to_node.graph_id {
            return;
        }

        let graph_id = from_node.graph_id;
        let from_id = from_node.node_id;
        let to_id = to_node.node_id;

        if let Some(graph) = self.state.graphs.get_mut(&graph_id) {
            let already_exists = graph
                .edges
                .iter()
                .any(|e| e.from == from_id && e.to == to_id);
            if !already_exists {
                graph.edges.push(SceneEdge {
                    from: from_id,
                    to: to_id,
                    condition: TransitionCondition::Manual,
                    priority: 0,
                });
            }
        }

        snarl.connect(from.id, to.id);
    }

    fn disconnect(&mut self, from: &OutPin, to: &InPin, snarl: &mut Snarl<SnarlNode>) {
        let from_node = &snarl[from.id.node];
        let to_node = &snarl[to.id.node];
        let graph_id = from_node.graph_id;
        let from_id = from_node.node_id;
        let to_id = to_node.node_id;

        if let Some(graph) = self.state.graphs.get_mut(&graph_id) {
            graph.edges.retain(|e| !(e.from == from_id && e.to == to_id));
        }

        snarl.disconnect(from.id, to.id);
    }
}

impl BlaulichtApp {
    pub fn scene_graph_ui(&mut self, ui: &mut Ui, _ctx: &Context) {
        let mut state = self.data.state.dmx_engine.write().unwrap();
        let scenes = state.0.scenes.clone();

        ui.horizontal(|ui| {
            ui.label("Graphs:");

            let graph_ids: Vec<_> = state.0.scene_graphs.graphs.keys().copied().collect();
            for &gid in &graph_ids {
                let graph = state.0.scene_graphs.graphs.get(&gid).unwrap();
                let selected = self.scene_graph_ui_state.selected_graph == Some(gid);
                if ui.selectable_label(selected, &graph.name).clicked() {
                    self.scene_graph_ui_state.selected_graph = Some(gid);
                    self.scene_graph_ui_state.synced = false;
                    self.scene_graph_ui_state.selected_node = None;
                }
            }

            if ui.button("+").clicked() {
                self.scene_graph_ui_state.add_graph_open =
                    !self.scene_graph_ui_state.add_graph_open;
            }

            if self.scene_graph_ui_state.add_graph_open {
                ui.text_edit_singleline(&mut self.scene_graph_ui_state.new_graph_name);
                if ui.button("Create").clicked() {
                    let new_id = (0..=u8::MAX)
                        .find(|id| !state.0.scene_graphs.graphs.contains_key(id));
                    if let Some(new_id) = new_id {
                        let mut graph = SceneGraph::default();
                        graph.name = self.scene_graph_ui_state.new_graph_name.clone();
                        state.0.scene_graphs.graphs.insert(new_id, graph);
                        self.scene_graph_ui_state.selected_graph = Some(new_id);
                        self.scene_graph_ui_state.synced = false;
                    }
                    self.scene_graph_ui_state.add_graph_open = false;
                }
            }
        });

        let Some(graph_id) = self.scene_graph_ui_state.selected_graph else {
            ui.label("Select or create a graph.");
            return;
        };

        if !state.0.scene_graphs.graphs.contains_key(&graph_id) {
            self.scene_graph_ui_state.selected_graph = None;
            return;
        }

        // Sync snarl from engine state if needed.
        if !self.scene_graph_ui_state.synced {
            self.scene_graph_ui_state.snarl = Snarl::new();
            let graph = state.0.scene_graphs.graphs.get(&graph_id).unwrap();

            let mut node_to_snarl: BTreeMap<NodeId, egui_snarl::NodeId> = BTreeMap::new();
            for (i, (&node_id, _)) in graph.nodes.iter().enumerate() {
                let pos = Pos2::new(200.0 * i as f32, 100.0);
                let snarl_id = self.scene_graph_ui_state.snarl.insert_node(
                    pos,
                    SnarlNode {
                        node_id,
                        graph_id,
                    },
                );
                node_to_snarl.insert(node_id, snarl_id);
            }

            for edge in &graph.edges {
                if let (Some(&from_snarl), Some(&to_snarl)) =
                    (node_to_snarl.get(&edge.from), node_to_snarl.get(&edge.to))
                {
                    self.scene_graph_ui_state.snarl.connect(
                        OutPinId {
                            node: from_snarl,
                            output: 0,
                        },
                        InPinId {
                            node: to_snarl,
                            input: 0,
                        },
                    );
                }
            }

            self.scene_graph_ui_state.synced = true;
        }

        // Controls bar.
        ui.horizontal(|ui| {
            let graph = state.0.scene_graphs.graphs.get_mut(&graph_id).unwrap();
            ui.checkbox(&mut graph.enabled, "Enabled");

            if ui.button("Add Node").clicked() {
                let new_id = (0..=u8::MAX).find(|id| !graph.nodes.contains_key(id));
                if let Some(new_id) = new_id {
                    graph.nodes.insert(new_id, SceneGraphNode::default());
                    let node_count = self.scene_graph_ui_state.snarl.node_ids().count();
                    self.scene_graph_ui_state.snarl.insert_node(
                        Pos2::new(200.0 * node_count as f32, 100.0),
                        SnarlNode {
                            node_id: new_id,
                            graph_id,
                        },
                    );
                }
            }

            if ui.button("Delete Graph").clicked() {
                state.0.scene_graphs.graphs.remove(&graph_id);
                self.scene_graph_ui_state.selected_graph = None;
                self.scene_graph_ui_state.selected_node = None;
                self.scene_graph_ui_state.synced = false;
                return;
            }
        });

        if !state.0.scene_graphs.graphs.contains_key(&graph_id) {
            return;
        }

        ui.separator();

        // Split: graph on left, node editor on right.
        ui.columns(2, |cols| {
            // Left: snarl graph.
            let mut viewer = SceneGraphViewer {
                state: &mut state.0.scene_graphs,
                graph_id,
                selected_node: &mut self.scene_graph_ui_state.selected_node,
            };

            self.scene_graph_ui_state.snarl.show(
                &mut viewer,
                &self.scene_graph_ui_state.style,
                egui::Id::new("scene_graph_snarl"),
                &mut cols[0],
            );

            // Right: node editor panel.
            let Some(selected_node_id) = self.scene_graph_ui_state.selected_node else {
                cols[1].centered_and_justified(|ui| {
                    ui.label("Click a node to edit it.");
                });
                return;
            };

            let Some(graph) = state.0.scene_graphs.graphs.get_mut(&graph_id) else {
                return;
            };

            let Some(scene_node) = graph.nodes.get_mut(&selected_node_id) else {
                self.scene_graph_ui_state.selected_node = None;
                return;
            };

            let ui = &mut cols[1];

            ui.heading(&scene_node.name.clone());
            ui.separator();

            ui.horizontal(|ui| {
                ui.label("Name:");
                ui.text_edit_singleline(&mut scene_node.name);
            });

            ui.add_space(4.0);

            ui.horizontal(|ui| {
                ui.label("Alpha:");
                let mut alpha = scene_node.master_alpha as f32;
                if ui
                    .add(egui::Slider::new(&mut alpha, 0.0..=100.0).suffix("%"))
                    .changed()
                {
                    scene_node.master_alpha = alpha as u8;
                }
            });

            ui.horizontal(|ui| {
                ui.label("Speed:");
                let speed_idx = scene_node.master_speed.as_index();
                let mut idx = speed_idx;
                egui::ComboBox::from_id_salt("node_speed")
                    .selected_text(scene_node.master_speed.as_str())
                    .show_ui(ui, |ui| {
                        for (i, s) in AnimationSpeedModifier::ALL.iter().enumerate() {
                            ui.selectable_value(&mut idx, i, s.as_str());
                        }
                    });
                if idx != speed_idx {
                    scene_node.master_speed = AnimationSpeedModifier::from_index(idx);
                }
            });

            ui.add_space(8.0);
            ui.label("Scenes:");
            let mut to_remove = None;
            for (i, scene_id) in scene_node.scenes.iter().enumerate() {
                ui.horizontal(|ui| {
                    let name = scenes
                        .get(scene_id)
                        .map(|s| s.name.as_str())
                        .unwrap_or("???");
                    ui.label(format!("  {} (ID {})", name, scene_id));
                    if ui.small_button("x").clicked() {
                        to_remove = Some(i);
                    }
                });
            }
            if let Some(i) = to_remove {
                scene_node.scenes.remove(i);
            }

            let mut add_scene_id: Option<u8> = None;
            egui::ComboBox::from_id_salt("node_add_scene")
                .selected_text("+ Add Scene")
                .show_ui(ui, |ui| {
                    for (id, scene) in scenes.iter() {
                        if !scene_node.scenes.contains(id)
                            && ui.selectable_label(false, &scene.name).clicked()
                        {
                            add_scene_id = Some(*id);
                        }
                    }
                });
            if let Some(id) = add_scene_id {
                scene_node.scenes.push(id);
            }

            ui.add_space(12.0);
            ui.separator();

            // Node actions.
            ui.horizontal(|ui| {
                if ui.button("Set as Start").clicked() {
                    graph.active_node = Some(selected_node_id);
                }
                if ui.button("Delete Node").clicked() {
                    graph.nodes.remove(&selected_node_id);
                    graph
                        .edges
                        .retain(|e| e.from != selected_node_id && e.to != selected_node_id);
                    if graph.active_node == Some(selected_node_id) {
                        graph.active_node = None;
                    }
                    self.scene_graph_ui_state.selected_node = None;
                    self.scene_graph_ui_state.synced = false;
                }
            });

            // Edge conditions for this node's outgoing edges.
            let outgoing_edges: Vec<_> = graph
                .edges
                .iter()
                .enumerate()
                .filter(|(_, e)| e.from == selected_node_id)
                .map(|(i, e)| (i, e.to, e.priority))
                .collect();

            if !outgoing_edges.is_empty() {
                ui.add_space(12.0);
                ui.separator();
                ui.label("Outgoing Edges:");

                for (edge_idx, to_id, _priority) in &outgoing_edges {
                    let to_name = graph
                        .nodes
                        .get(to_id)
                        .map(|n| n.name.as_str())
                        .unwrap_or("???");

                    ui.group(|ui| {
                        ui.label(format!("-> {}", to_name));

                        let edge = &mut graph.edges[*edge_idx];

                        let mut condition_idx = match &edge.condition {
                            TransitionCondition::Manual => 0,
                            TransitionCondition::AfterDuration(_) => 1,
                            TransitionCondition::AfterBeats(_) => 2,
                            TransitionCondition::OnBeatDrop => 3,
                            TransitionCondition::OnNonBeat => 4,
                            _ => 0,
                        };

                        let prev = condition_idx;
                        egui::ComboBox::from_id_salt(format!("edge_cond_{}", edge_idx))
                            .selected_text(match condition_idx {
                                0 => "Manual",
                                1 => "After Duration",
                                2 => "After Beats",
                                3 => "On Beat Drop",
                                4 => "On Non-Beat",
                                _ => "???",
                            })
                            .show_ui(ui, |ui| {
                                ui.selectable_value(&mut condition_idx, 0, "Manual");
                                ui.selectable_value(&mut condition_idx, 1, "After Duration");
                                ui.selectable_value(&mut condition_idx, 2, "After Beats");
                                ui.selectable_value(&mut condition_idx, 3, "On Beat Drop");
                                ui.selectable_value(&mut condition_idx, 4, "On Non-Beat");
                            });

                        if condition_idx != prev {
                            edge.condition = match condition_idx {
                                1 => TransitionCondition::AfterDuration(1000),
                                2 => TransitionCondition::AfterBeats(4),
                                3 => TransitionCondition::OnBeatDrop,
                                4 => TransitionCondition::OnNonBeat,
                                _ => TransitionCondition::Manual,
                            };
                        }

                        match &mut edge.condition {
                            TransitionCondition::AfterDuration(ms) => {
                                let mut secs = *ms as f32 / 1000.0;
                                if ui
                                    .add(
                                        egui::Slider::new(&mut secs, 0.1..=30.0)
                                            .suffix("s")
                                            .logarithmic(true),
                                    )
                                    .changed()
                                {
                                    *ms = (secs * 1000.0) as u64;
                                }
                            }
                            TransitionCondition::AfterBeats(n) => {
                                let mut beats = *n as i32;
                                if ui
                                    .add(egui::Slider::new(&mut beats, 1..=64).suffix(" beats"))
                                    .changed()
                                {
                                    *n = beats as u32;
                                }
                            }
                            _ => {}
                        }

                        ui.horizontal(|ui| {
                            ui.label("Priority:");
                            let mut p = edge.priority as i32;
                            if ui.add(egui::DragValue::new(&mut p).range(0..=255)).changed() {
                                edge.priority = p as u8;
                            }
                        });
                    });
                }
            }
        });
    }
}
