use crate::app::{
    components::{self, Dialog},
    BlaulichtApp,
};
use blaulicht_shared::{
    scene_graph::{
        GraphId, NodeId, SceneEdge, SceneGraph, SceneGraphNode, SceneGraphState,
        TransitionCondition,
    },
    AnimationSpeedModifier,
};
use egui::{Color32, Context, Painter, Pos2, Rect, Stroke, Style, Ui, Vec2};
use egui_snarl::{
    ui::{BackgroundPattern, Grid, PinInfo, SnarlStyle, SnarlViewer},
    InPin, InPinId, OutPin, OutPinId, Snarl,
};
use fdg_sim::{
    glam::Vec3, Dimensions, ForceGraph, ForceGraphHelper, Simulation, SimulationParameters,
};
use std::collections::BTreeMap;

const CANVAS_HALF_EXTENT: f32 = 1500.0;

#[derive(Clone, serde::Serialize, serde::Deserialize)]
pub struct SnarlNode {
    pub node_id: NodeId,
    pub graph_id: GraphId,
}

pub struct SceneGraphUI {
    pub snarl: Snarl<SnarlNode>,
    pub style: SnarlStyle,
    pub selected_node: Option<NodeId>,
    pub new_graph_name: String,
    pub add_graph_open: bool,
    last_synced_graph: Option<GraphId>,
    synced: bool,
    recenter_view: bool,
    zoom_level: f32,
    pending_zoom_override: Option<f32>,
    delete_graph_id: Option<GraphId>,
    delete_node: Option<(GraphId, NodeId)>,
}

impl Default for SceneGraphUI {
    fn default() -> Self {
        let mut style = SnarlStyle::new();
        style.bg_pattern = Some(BackgroundPattern::Grid(Grid::new(Vec2::splat(40.0), 0.0)));
        style.bg_pattern_stroke = Some(Stroke::new(1.0, Color32::from_gray(55)));
        Self {
            snarl: Snarl::new(),
            style,
            selected_node: None,
            new_graph_name: "New Graph".to_string(),
            add_graph_open: false,
            last_synced_graph: None,
            synced: false,
            recenter_view: false,
            zoom_level: 1.0,
            pending_zoom_override: None,
            delete_graph_id: None,
            delete_node: None,
        }
    }
}

pub struct SceneGraphViewer<'a> {
    pub state: &'a mut SceneGraphState,
    pub graph_id: GraphId,
    pub selected_node: &'a mut Option<NodeId>,
    pub recenter_view: bool,
    pub viewport_rect: Rect,
    pub zoom_level: &'a mut f32,
    pub pending_zoom_override: Option<f32>,
    /// Milliseconds until the active node's soonest time-based edge fires.
    pub active_countdown_ms: Option<u64>,
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

    fn show_input(&mut self, pin: &InPin, _ui: &mut Ui, snarl: &mut Snarl<SnarlNode>) -> PinInfo {
        let snarl_node = snarl.get_node(pin.id.node);
        let graph = self.state.graphs.get(&self.graph_id);
        let is_active = graph.map_or(false, |g| {
            snarl_node.map_or(false, |n| g.active_node == Some(n.node_id))
        });

        // Indicate the active node purely via the pin fill color. Adding/removing
        // an extra in-node widget here changes the pin's size and egui's auto-id
        // counter when the active node switches, which resizes the node and trips
        // egui's "changed id between passes" warning.
        let fill = if is_active {
            Color32::from_rgb(100, 255, 100)
        } else {
            Color32::from_rgb(150, 150, 150)
        };
        PinInfo::circle().with_fill(fill)
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
        let is_active = self
            .state
            .graphs
            .get(&self.graph_id)
            .map_or(false, |g| g.active_node == Some(node_id));
        let label = self.title(&snarl[node].clone());

        // Append a live countdown to the active node when a time-based edge is
        // ticking, so it's clear when the next transition will fire.
        let label = match (is_active, self.active_countdown_ms) {
            (true, Some(ms)) => format!("{label}  \u{23F1} {:.1}s", ms as f32 / 1000.0),
            _ => label,
        };

        // Highlight the currently active node via title color only (no extra
        // widgets / size changes, so layout and widget ids stay stable).
        let mut text = egui::RichText::new(&label).strong();
        if is_active {
            text = text.color(Color32::from_rgb(100, 255, 100));
        }

        if ui.selectable_label(is_selected, text).clicked() {
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

    fn current_transform(
        &mut self,
        to_global: &mut egui::emath::TSTransform,
        snarl: &mut Snarl<SnarlNode>,
    ) {
        if self.recenter_view {
            let mut bb = Rect::NOTHING;
            for (_, pos, n) in snarl.nodes_pos_ids() {
                if n.graph_id == self.graph_id {
                    bb.extend_with(pos);
                }
            }
            if bb.is_finite() {
                let bb = bb.expand(150.0);
                let scaling2 = self.viewport_rect.size() / bb.size();
                let scaling = scaling2.min_elem().clamp(0.1, 1.0);
                let translation =
                    self.viewport_rect.center().to_vec2() - scaling * bb.center().to_vec2();
                *to_global = egui::emath::TSTransform::new(translation, scaling);
            }
        } else if let Some(new_scaling) = self.pending_zoom_override {
            // Zoom around the viewport center while preserving the graph point under it.
            let center = self.viewport_rect.center();
            let graph_anchor = to_global.inverse().mul_pos(center);
            let translation = center.to_vec2() - new_scaling * graph_anchor.to_vec2();
            *to_global = egui::emath::TSTransform::new(translation, new_scaling);
        }
        *self.zoom_level = to_global.scaling;
    }

    fn draw_background(
        &mut self,
        background: Option<&BackgroundPattern>,
        _viewport: &Rect,
        snarl_style: &SnarlStyle,
        style: &Style,
        painter: &Painter,
        _snarl: &Snarl<SnarlNode>,
    ) {
        let bounds = Rect::from_min_max(
            Pos2::new(-CANVAS_HALF_EXTENT, -CANVAS_HALF_EXTENT),
            Pos2::new(CANVAS_HALF_EXTENT, CANVAS_HALF_EXTENT),
        );
        painter.rect_filled(bounds, 0.0, Color32::from_gray(28));
        if let Some(background) = background {
            let clipped = painter.with_clip_rect(bounds);
            background.draw(&bounds, snarl_style, style, &clipped);
        }
        painter.rect_stroke(
            bounds,
            0.0,
            Stroke::new(2.0, Color32::from_gray(90)),
            egui::StrokeKind::Outside,
        );
    }

    fn disconnect(&mut self, from: &OutPin, to: &InPin, snarl: &mut Snarl<SnarlNode>) {
        let from_node = &snarl[from.id.node];
        let to_node = &snarl[to.id.node];
        let graph_id = from_node.graph_id;
        let from_id = from_node.node_id;
        let to_id = to_node.node_id;

        if let Some(graph) = self.state.graphs.get_mut(&graph_id) {
            graph
                .edges
                .retain(|e| !(e.from == from_id && e.to == to_id));
        }

        snarl.disconnect(from.id, to.id);
    }
}

impl BlaulichtApp {
    fn render_scene_graph_delete_dialogs(&mut self, ctx: &Context) {
        if let Some(graph_id) = self.scene_graph_ui_state.delete_graph_id {
            Dialog::new("Delete Graph".to_string(), egui::vec2(260.0, 130.0))
                .with_backdrop()
                .show(ctx, |ui| {
                    ui.heading(egui::RichText::new("Delete this graph?").strong());
                    ui.add_space(12.0);
                    ui.horizontal(|ui| {
                        if ui.button("Confirm").clicked() {
                            let mut state = self.data.state.dmx_engine.write().unwrap();
                            state.0.scene_graphs.graphs.remove(&graph_id);
                            if state.0.scene_graphs.focused_graph == Some(graph_id) {
                                state.0.scene_graphs.focused_graph = None;
                            }
                            self.scene_graph_ui_state.delete_graph_id = None;
                            self.scene_graph_ui_state.selected_node = None;
                            self.scene_graph_ui_state.synced = false;
                        }
                        if ui.button("Cancel").clicked() {
                            self.scene_graph_ui_state.delete_graph_id = None;
                        }
                    });
                });
            return;
        }

        if let Some((graph_id, node_id)) = self.scene_graph_ui_state.delete_node {
            Dialog::new("Delete Node".to_string(), egui::vec2(260.0, 130.0))
                .with_backdrop()
                .show(ctx, |ui| {
                    ui.heading(egui::RichText::new("Delete this node?").strong());
                    ui.add_space(12.0);
                    ui.horizontal(|ui| {
                        if ui.button("Confirm").clicked() {
                            let mut state = self.data.state.dmx_engine.write().unwrap();
                            if let Some(graph) = state.0.scene_graphs.graphs.get_mut(&graph_id) {
                                graph.nodes.remove(&node_id);
                                graph.edges.retain(|e| e.from != node_id && e.to != node_id);
                                if graph.active_node == Some(node_id) {
                                    graph.active_node = None;
                                }
                            }
                            self.scene_graph_ui_state.delete_node = None;
                            self.scene_graph_ui_state.selected_node = None;
                            self.scene_graph_ui_state.synced = false;
                        }
                        if ui.button("Cancel").clicked() {
                            self.scene_graph_ui_state.delete_node = None;
                        }
                    });
                });
        }
    }

    pub fn scene_graph_ui(
        &mut self,
        ui: &mut Ui,
        ctx: &Context,
        render_context: crate::app::page::PageRenderContext,
    ) {
        self.render_scene_graph_delete_dialogs(ctx);
        let mut state = self.data.state.dmx_engine.write().unwrap();
        let scenes = state.0.scenes.clone();

        render_context.horizontal(ui, egui::Align::Min, |ui| {
            ui.label("Graphs:");

            let graph_ids: Vec<_> = state.0.scene_graphs.graphs.keys().copied().collect();
            let focused = state.0.scene_graphs.focused_graph;
            for &gid in &graph_ids {
                let Some(graph) = state.0.scene_graphs.graphs.get(&gid) else {
                    continue;
                };
                let selected = focused == Some(gid);
                if ui.selectable_label(selected, &graph.name).clicked() {
                    state.0.scene_graphs.focused_graph = Some(gid);
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
                    let new_id =
                        (0..=u8::MAX).find(|id| !state.0.scene_graphs.graphs.contains_key(id));
                    if let Some(new_id) = new_id {
                        let mut graph = SceneGraph::default();
                        graph.name = self.scene_graph_ui_state.new_graph_name.clone();
                        state.0.scene_graphs.graphs.insert(new_id, graph);
                        state.0.scene_graphs.focused_graph = Some(new_id);
                    }
                    self.scene_graph_ui_state.add_graph_open = false;
                }
            }
        });

        let Some(graph_id) = state.0.scene_graphs.focused_graph else {
            ui.label("Select or create a graph.");
            return;
        };

        if !state.0.scene_graphs.graphs.contains_key(&graph_id) {
            state.0.scene_graphs.focused_graph = None;
            return;
        }

        // Resync snarl if the focused graph changed.
        if self.scene_graph_ui_state.last_synced_graph != Some(graph_id) {
            self.scene_graph_ui_state.synced = false;
            self.scene_graph_ui_state.selected_node = None;
            self.scene_graph_ui_state.last_synced_graph = Some(graph_id);
        }

        // Sync snarl from engine state if needed.
        if !self.scene_graph_ui_state.synced {
            self.scene_graph_ui_state.snarl = Snarl::new();
            let Some(graph) = state.0.scene_graphs.graphs.get(&graph_id) else {
                self.scene_graph_ui_state.synced = false;
                self.scene_graph_ui_state.selected_node = None;
                return;
            };

            let mut node_to_snarl: BTreeMap<NodeId, egui_snarl::NodeId> = BTreeMap::new();
            for (i, (&node_id, node)) in graph.nodes.iter().enumerate() {
                let pos = if node.pos_x == 0.0 && node.pos_y == 0.0 {
                    Pos2::new(200.0 * i as f32, 100.0)
                } else {
                    Pos2::new(node.pos_x, node.pos_y)
                };
                let snarl_id = self
                    .scene_graph_ui_state
                    .snarl
                    .insert_node(pos, SnarlNode { node_id, graph_id });
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
        let current_section = self.collector_snapshot.section_state;
        let mut do_auto_layout = false;
        render_context.horizontal(ui, egui::Align::Min, |ui| {
            let Some(graph) = state.0.scene_graphs.graphs.get_mut(&graph_id) else {
                self.scene_graph_ui_state.selected_node = None;
                self.scene_graph_ui_state.synced = false;
                return;
            };
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

            if ui.button("Auto Layout").clicked() {
                do_auto_layout = true;
            }

            components::toolbar_separator(ui, ui.spacing().interact_size.y);
            ui.label("Zoom:");
            let mut zoom = self.scene_graph_ui_state.zoom_level;
            if ui
                .add(
                    egui::Slider::new(&mut zoom, 0.1..=2.0)
                        .logarithmic(true)
                        .show_value(false),
                )
                .changed()
            {
                self.scene_graph_ui_state.pending_zoom_override = Some(zoom);
            }
            ui.label(format!("{:.2}x", self.scene_graph_ui_state.zoom_level));

            components::toolbar_separator(ui, ui.spacing().interact_size.y);
            let (section_text, section_color) =
                crate::app::components::section_label(current_section);
            ui.label(
                egui::RichText::new(section_text)
                    .strong()
                    .color(section_color)
                    .monospace(),
            );

            if ui.button("Delete Graph").clicked() {
                self.scene_graph_ui_state.delete_graph_id = Some(graph_id);
            }
        });

        if do_auto_layout {
            if let Some(graph) = state.0.scene_graphs.graphs.get_mut(&graph_id) {
                auto_layout_graph(graph, &mut self.scene_graph_ui_state.snarl);
                self.scene_graph_ui_state.recenter_view = true;
            }
        }

        if !state.0.scene_graphs.graphs.contains_key(&graph_id) {
            return;
        }

        ui.separator();

        // Persist snarl positions back to scene graph nodes (captures user drags), clamped to canvas.
        let snarl_updates: Vec<(egui_snarl::NodeId, SnarlNode, Pos2)> = self
            .scene_graph_ui_state
            .snarl
            .nodes_pos_ids()
            .map(|(id, pos, n)| {
                let clamped = Pos2::new(
                    pos.x.clamp(-CANVAS_HALF_EXTENT, CANVAS_HALF_EXTENT),
                    pos.y.clamp(-CANVAS_HALF_EXTENT, CANVAS_HALF_EXTENT),
                );
                (id, n.clone(), clamped)
            })
            .collect();
        for (snarl_id, snarl_node, clamped) in &snarl_updates {
            if let Some(info) = self.scene_graph_ui_state.snarl.get_node_info_mut(*snarl_id) {
                info.pos = *clamped;
            }
            if let Some(graph) = state.0.scene_graphs.graphs.get_mut(&snarl_node.graph_id) {
                if let Some(node) = graph.nodes.get_mut(&snarl_node.node_id) {
                    node.pos_x = clamped.x;
                    node.pos_y = clamped.y;
                }
            }
        }

        // Use a vertical split in narrow dynamic tiles so both workspaces remain usable.
        let recenter_view = std::mem::take(&mut self.scene_graph_ui_state.recenter_view);
        let pending_zoom_override =
            std::mem::take(&mut self.scene_graph_ui_state.pending_zoom_override);
        let active_countdown_ms = state
            .0
            .scene_graphs
            .active_countdowns
            .get(&graph_id)
            .copied();
        let available = ui.available_size_before_wrap().max(egui::vec2(1.0, 1.0));
        let (workspace_rect, _) = ui.allocate_exact_size(available, egui::Sense::hover());
        let gap = ui.spacing().item_spacing.x;
        let (graph_rect, editor_rect) = if render_context.is_narrow_dynamic() {
            let graph_height = (workspace_rect.height() * 0.58).max(1.0);
            (
                Rect::from_min_max(
                    workspace_rect.min,
                    egui::pos2(workspace_rect.max.x, workspace_rect.min.y + graph_height),
                ),
                Rect::from_min_max(
                    egui::pos2(
                        workspace_rect.min.x,
                        (workspace_rect.min.y + graph_height + gap).min(workspace_rect.max.y),
                    ),
                    workspace_rect.max,
                ),
            )
        } else {
            let graph_fraction = match (render_context.is_dynamic(), render_context.width_class) {
                (true, crate::app::page::PageWidthClass::Wide) => 0.68,
                (true, _) => 0.62,
                (false, _) => 0.5,
            };
            let graph_width = (workspace_rect.width() * graph_fraction).max(1.0);
            (
                Rect::from_min_max(
                    workspace_rect.min,
                    egui::pos2(workspace_rect.min.x + graph_width, workspace_rect.max.y),
                ),
                Rect::from_min_max(
                    egui::pos2(
                        (workspace_rect.min.x + graph_width + gap).min(workspace_rect.max.x),
                        workspace_rect.min.y,
                    ),
                    workspace_rect.max,
                ),
            )
        };
        let mut graph_ui = ui.new_child(
            egui::UiBuilder::new()
                .max_rect(graph_rect)
                .layout(egui::Layout::top_down(egui::Align::Min)),
        );
        let mut editor_ui = ui.new_child(
            egui::UiBuilder::new()
                .max_rect(editor_rect)
                .layout(egui::Layout::top_down(egui::Align::Min)),
        );
        let viewport_rect =
            Rect::from_min_size(graph_ui.cursor().min, graph_ui.available_size_before_wrap());

        // Left: snarl graph.
        let mut viewer = SceneGraphViewer {
            state: &mut state.0.scene_graphs,
            graph_id,
            selected_node: &mut self.scene_graph_ui_state.selected_node,
            recenter_view,
            viewport_rect,
            zoom_level: &mut self.scene_graph_ui_state.zoom_level,
            pending_zoom_override,
            active_countdown_ms,
        };

        self.scene_graph_ui_state.snarl.show(
            &mut viewer,
            &self.scene_graph_ui_state.style,
            egui::Id::new("scene_graph_snarl"),
            &mut graph_ui,
        );

        // Right: node editor panel.
        let Some(selected_node_id) = self.scene_graph_ui_state.selected_node else {
            editor_ui.centered_and_justified(|ui| {
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

        let ui = &mut editor_ui;

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
                self.scene_graph_ui_state.delete_node = Some((graph_id, selected_node_id));
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
                        TransitionCondition::OnEnterDrop => 5,
                        TransitionCondition::OnEnterBreakdown => 6,
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
                            5 => "On Enter Drop",
                            6 => "On Enter Breakdown",
                            _ => "???",
                        })
                        .show_ui(ui, |ui| {
                            ui.selectable_value(&mut condition_idx, 0, "Manual");
                            ui.selectable_value(&mut condition_idx, 1, "After Duration");
                            ui.selectable_value(&mut condition_idx, 2, "After Beats");
                            ui.selectable_value(&mut condition_idx, 3, "On Beat Drop");
                            ui.selectable_value(&mut condition_idx, 4, "On Non-Beat");
                            ui.selectable_value(&mut condition_idx, 5, "On Enter Drop");
                            ui.selectable_value(&mut condition_idx, 6, "On Enter Breakdown");
                        });

                    if condition_idx != prev {
                        edge.condition = match condition_idx {
                            1 => TransitionCondition::AfterDuration(1000),
                            2 => TransitionCondition::AfterBeats(4),
                            3 => TransitionCondition::OnBeatDrop,
                            4 => TransitionCondition::OnNonBeat,
                            5 => TransitionCondition::OnEnterDrop,
                            6 => TransitionCondition::OnEnterBreakdown,
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
                        if ui
                            .add(egui::DragValue::new(&mut p).range(0..=255))
                            .changed()
                        {
                            edge.priority = p as u8;
                        }
                    });
                });
            }
        }
    }
}

fn auto_layout_graph(graph: &mut SceneGraph, snarl: &mut Snarl<SnarlNode>) {
    if graph.nodes.is_empty() {
        return;
    }

    let mut force_graph: ForceGraph<NodeId, ()> = ForceGraph::default();
    let mut node_to_fdg: BTreeMap<NodeId, fdg_sim::petgraph::graph::NodeIndex> = BTreeMap::new();

    for &node_id in graph.nodes.keys() {
        let idx = force_graph.add_force_node(format!("{}", node_id), node_id);
        node_to_fdg.insert(node_id, idx);
    }

    for edge in &graph.edges {
        if let (Some(&from), Some(&to)) = (node_to_fdg.get(&edge.from), node_to_fdg.get(&edge.to)) {
            force_graph.add_edge(from, to, ());
        }
    }

    let mut params: SimulationParameters<NodeId, ()> = SimulationParameters::default();
    params.node_start_size = 400.0;
    params.dimensions = Dimensions::Two;
    let mut sim = Simulation::from_graph(force_graph, params);
    for _ in 0..300 {
        sim.update(0.035);
    }

    let scale = 2.5;
    let mut positions: BTreeMap<NodeId, Vec3> = BTreeMap::new();
    let result_graph = sim.get_graph();
    for idx in result_graph.node_indices() {
        let node = &result_graph[idx];
        positions.insert(node.data, node.location);
    }

    let clamp = |v: f32| v.clamp(-CANVAS_HALF_EXTENT, CANVAS_HALF_EXTENT);
    for (node_id, loc) in &positions {
        if let Some(node) = graph.nodes.get_mut(node_id) {
            node.pos_x = clamp(loc.x * scale);
            node.pos_y = clamp(loc.y * scale);
        }
    }

    for (snarl_id, _, snarl_node) in snarl
        .nodes_pos_ids()
        .map(|(id, p, n)| (id, p, n.clone()))
        .collect::<Vec<_>>()
    {
        if let Some(loc) = positions.get(&snarl_node.node_id) {
            if let Some(info) = snarl.get_node_info_mut(snarl_id) {
                info.pos = Pos2::new(clamp(loc.x * scale), clamp(loc.y * scale));
            }
        }
    }
}
