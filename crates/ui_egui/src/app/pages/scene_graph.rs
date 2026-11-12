use crate::app::{
    components::{self, dropdown, Dialog},
    BlaulichtApp,
};
use blaulicht_engine::client::EngineClient;
use blaulicht_engine::dmx::SCENE_GRAPH_CANVAS_HALF_EXTENT as CANVAS_HALF_EXTENT;
use blaulicht_engine::hub::{CommandSeq, HostCommand, SceneGraphEdgeParam, SceneGraphNodeParam};
use blaulicht_shared::{
    scene_graph::{GraphId, NodeId, SceneGraph, TransitionCondition},
    AnimationSpeedModifier, ControlEvent,
};
use egui::{Color32, Context, Painter, Pos2, Rect, Stroke, Style, Ui, Vec2};
use egui_snarl::{
    ui::{BackgroundPattern, Grid, PinInfo, SnarlStyle, SnarlViewer},
    InPin, InPinId, OutPin, OutPinId, Snarl,
};
use fdg_sim::{
    glam::Vec3, Dimensions, ForceGraph, ForceGraphHelper, Simulation, SimulationParameters,
};
use std::collections::{BTreeMap, BTreeSet};

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
    /// The snarl node of each engine node of the synced graph.
    snarl_ids: BTreeMap<NodeId, egui_snarl::NodeId>,
    /// Where each node was when the snarl was last synced from the engine,
    /// or last sent to it. A snarl position that differs is an operator's
    /// drag; an engine position that differs is someone else's move.
    seen_pos: BTreeMap<NodeId, Pos2>,
    /// The edges `(from, to)` as last synced from the engine, or sent to it.
    seen_edges: BTreeSet<(NodeId, NodeId)>,
    /// The last move / connect / disconnect sent, until its reply arrives.
    /// The snarl is not synced from the engine meanwhile, so it doesn't
    /// jump back to what the engine had before it applied the edit.
    edit_reply: Option<CommandSeq>,
    /// The selected node's name while it is being typed (graph, node, text).
    name_edit: Option<(GraphId, NodeId, String)>,
    recenter_view: bool,
    zoom_level: f32,
    last_canvas_size: Vec2,
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
            snarl_ids: BTreeMap::new(),
            seen_pos: BTreeMap::new(),
            seen_edges: BTreeSet::new(),
            edit_reply: None,
            name_edit: None,
            recenter_view: false,
            zoom_level: 1.0,
            last_canvas_size: Vec2::ZERO,
            pending_zoom_override: None,
            delete_graph_id: None,
            delete_node: None,
        }
    }
}

/// Where a node is drawn: its position, or a row slot for a node that was
/// never placed (`0, 0`).
fn display_pos(index: usize, pos_x: f32, pos_y: f32) -> Pos2 {
    if pos_x == 0.0 && pos_y == 0.0 {
        Pos2::new(200.0 * index as f32, 100.0)
    } else {
        Pos2::new(pos_x, pos_y)
    }
}

impl SceneGraphUI {
    /// Brings the snarl in line with the engine's `graph` (`graph_id`), and
    /// returns the commands for what the operator changed on the snarl since
    /// the last sync: wires connected or removed, and nodes dropped at a new
    /// position (only once `pointer_down` is over, so a drag sends one
    /// move). While `waiting` for the reply to such a command, nothing is
    /// taken from the engine.
    fn sync(
        &mut self,
        graph_id: GraphId,
        graph: &SceneGraph,
        pointer_down: bool,
        waiting: bool,
    ) -> Vec<HostCommand> {
        if self.last_synced_graph != Some(graph_id) {
            self.synced = false;
            self.selected_node = None;
            self.last_synced_graph = Some(graph_id);
        }
        if !self.synced {
            self.rebuild(graph_id, graph);
            return Vec::new();
        }

        let commands = self.operator_edits(graph_id, pointer_down);
        if !commands.is_empty() || waiting {
            return commands;
        }
        self.take_engine_changes(graph_id, graph);
        Vec::new()
    }

    fn rebuild(&mut self, graph_id: GraphId, graph: &SceneGraph) {
        self.snarl = Snarl::new();
        self.snarl_ids.clear();
        self.seen_pos.clear();
        self.seen_edges.clear();
        self.take_engine_changes(graph_id, graph);
        self.synced = true;
        self.recenter_view = true;
    }

    /// The snarl's wires as engine edges.
    fn snarl_edges(&self) -> BTreeSet<(NodeId, NodeId)> {
        self.snarl
            .wires()
            .filter_map(|(out_pin, in_pin)| {
                let from = self.snarl.get_node(out_pin.node)?.node_id;
                let to = self.snarl.get_node(in_pin.node)?.node_id;
                Some((from, to))
            })
            .collect()
    }

    fn operator_edits(&mut self, graph: GraphId, pointer_down: bool) -> Vec<HostCommand> {
        let mut commands = Vec::new();
        let edges = self.snarl_edges();
        for &(from, to) in edges.difference(&self.seen_edges) {
            commands.push(HostCommand::ConnectSceneGraphNodes { graph, from, to });
        }
        for &(from, to) in self.seen_edges.difference(&edges) {
            commands.push(HostCommand::DisconnectSceneGraphNodes { graph, from, to });
        }
        self.seen_edges = edges;

        if !pointer_down {
            let mut positions = Vec::new();
            for (&node, &snarl_id) in &self.snarl_ids {
                let Some(info) = self.snarl.get_node_info(snarl_id) else {
                    continue;
                };
                if self.seen_pos.get(&node) != Some(&info.pos) {
                    positions.push((node, info.pos.x, info.pos.y));
                    self.seen_pos.insert(node, info.pos);
                }
            }
            if !positions.is_empty() {
                commands.push(HostCommand::MoveSceneGraphNodes { graph, positions });
            }
        }
        commands
    }

    fn take_engine_changes(&mut self, graph_id: GraphId, graph: &SceneGraph) {
        // Nodes added or removed (by this frontend's commands, or another's).
        let gone: Vec<NodeId> = self
            .snarl_ids
            .keys()
            .filter(|node| !graph.nodes.contains_key(node))
            .copied()
            .collect();
        for node in gone {
            if let Some(snarl_id) = self.snarl_ids.remove(&node) {
                self.snarl.remove_node(snarl_id);
            }
            self.seen_pos.remove(&node);
            self.seen_edges
                .retain(|&(from, to)| from != node && to != node);
            if self.selected_node == Some(node) {
                self.selected_node = None;
            }
        }
        for (index, (&node_id, node)) in graph.nodes.iter().enumerate() {
            let pos = display_pos(index, node.pos_x, node.pos_y);
            match self.snarl_ids.get(&node_id) {
                None => {
                    let snarl_id = self.snarl.insert_node(pos, SnarlNode { node_id, graph_id });
                    self.snarl_ids.insert(node_id, snarl_id);
                }
                // Moved elsewhere (auto layout, another frontend).
                Some(&snarl_id) if self.seen_pos.get(&node_id) != Some(&pos) => {
                    if let Some(info) = self.snarl.get_node_info_mut(snarl_id) {
                        info.pos = pos;
                    }
                }
                Some(_) => {}
            }
            self.seen_pos.insert(node_id, pos);
        }

        let edges: BTreeSet<(NodeId, NodeId)> = graph
            .edges
            .iter()
            .map(|edge| (edge.from, edge.to))
            .collect();
        if edges != self.seen_edges {
            let wired = self.snarl_edges();
            let pins = |ids: &BTreeMap<NodeId, egui_snarl::NodeId>, from, to| {
                Some((
                    OutPinId {
                        node: *ids.get(&from)?,
                        output: 0,
                    },
                    InPinId {
                        node: *ids.get(&to)?,
                        input: 0,
                    },
                ))
            };
            for &(from, to) in wired.difference(&edges) {
                if let Some((out_pin, in_pin)) = pins(&self.snarl_ids, from, to) {
                    self.snarl.disconnect(out_pin, in_pin);
                }
            }
            for &(from, to) in edges.difference(&wired) {
                if let Some((out_pin, in_pin)) = pins(&self.snarl_ids, from, to) {
                    self.snarl.connect(out_pin, in_pin);
                }
            }
            self.seen_edges = edges;
        }
    }
}

pub struct SceneGraphViewer<'a> {
    /// The engine's graph as read this frame. The viewer only changes the
    /// snarl; `SceneGraphUI::sync` turns those changes into commands.
    pub graph: &'a SceneGraph,
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
        self.graph
            .nodes
            .get(&node.node_id)
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
        let is_active = snarl_node.is_some_and(|n| self.graph.active_node == Some(n.node_id));

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
        let is_active = self.graph.active_node == Some(node_id);
        let label = self.title(&snarl[node].clone());

        // Keep both rows present so transitions never resize nodes or move their pins.
        ui.vertical(|ui| {
            ui.set_width(140.0);
            let mut text = egui::RichText::new(&label).strong();
            if is_active {
                text = text.color(Color32::from_rgb(100, 255, 100));
            }
            if ui
                .add(egui::Button::selectable(is_selected, text).truncate())
                .on_hover_text(&label)
                .clicked()
            {
                *self.selected_node = Some(node_id);
            }
            let countdown = match (is_active, self.active_countdown_ms) {
                (true, Some(ms)) => format!("⏱ {:.1}s", ms as f32 / 1000.0),
                _ => " ".to_string(),
            };
            ui.add(egui::Label::new(egui::RichText::new(countdown).small()).truncate());
        });
    }

    fn connect(&mut self, from: &OutPin, to: &InPin, snarl: &mut Snarl<SnarlNode>) {
        if snarl[from.id.node].graph_id == snarl[to.id.node].graph_id {
            snarl.connect(from.id, to.id);
        }
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
                    bb = bb.union(Rect::from_min_size(pos, Vec2::new(230.0, 100.0)));
                }
            }
            if bb.is_finite() {
                let bb = bb.expand(30.0);
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
        snarl.disconnect(from.id, to.id);
    }
}

/// What the page draws, read from the engine once per frame.
struct GraphSnapshot {
    /// Every graph's id and name.
    graphs: Vec<(GraphId, String)>,
    /// This session's focused graph, if it exists.
    focused: Option<(GraphId, SceneGraph)>,
    countdown_ms: Option<u64>,
    scene_names: BTreeMap<u8, String>,
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
                            self.send_command(HostCommand::DeleteSceneGraph(graph_id), |app, _| {
                                app.scene_graph_ui_state.selected_node = None;
                                app.scene_graph_ui_state.synced = false;
                            });
                            self.scene_graph_ui_state.delete_graph_id = None;
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
                            self.send_command(
                                HostCommand::RemoveSceneGraphNode {
                                    graph: graph_id,
                                    node: node_id,
                                },
                                move |app, _| {
                                    let ui_state = &mut app.scene_graph_ui_state;
                                    if ui_state.selected_node == Some(node_id) {
                                        ui_state.selected_node = None;
                                    }
                                },
                            );
                            self.scene_graph_ui_state.delete_node = None;
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
        let snapshot = self.client.with_session(|show, me| {
            let graphs = &show.0.scene_graphs;
            GraphSnapshot {
                graphs: graphs
                    .graphs
                    .iter()
                    .map(|(id, graph)| (*id, graph.name.clone()))
                    .collect(),
                focused: me
                    .focused_graph
                    .and_then(|id| Some((id, graphs.graphs.get(&id)?.clone()))),
                countdown_ms: me
                    .focused_graph
                    .and_then(|id| graphs.active_countdowns.get(&id).copied()),
                scene_names: show
                    .0
                    .scenes
                    .iter()
                    .map(|(id, scene)| (*id, scene.name.clone()))
                    .collect(),
            }
        });
        let Some(snapshot) = snapshot else {
            ui.label("Not connected.");
            return;
        };
        self.render_scene_graph(ui, render_context, snapshot);
    }

    fn render_scene_graph(
        &mut self,
        ui: &mut Ui,
        render_context: crate::app::page::PageRenderContext,
        snapshot: GraphSnapshot,
    ) {
        let GraphSnapshot {
            graphs,
            focused,
            countdown_ms: active_countdown_ms,
            scene_names,
        } = snapshot;

        render_context.horizontal(ui, egui::Align::Min, |ui| {
            components::column_header(ui, "Graphs:");

            let focused_id = focused.as_ref().map(|(id, _)| *id);
            for (gid, name) in &graphs {
                let selected = focused_id == Some(*gid);
                if ui.selectable_label(selected, name).clicked() && !selected {
                    self.client
                        .send_event(ControlEvent::SetFocusedSceneGraph(Some(*gid)));
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
                    // The engine focuses the new graph for this session.
                    let name = self.scene_graph_ui_state.new_graph_name.clone();
                    self.send_command(HostCommand::CreateSceneGraph { name }, |_, _| {});
                    self.scene_graph_ui_state.add_graph_open = false;
                }
            }
        });

        let Some((graph_id, graph)) = focused else {
            ui.label("Select or create a graph.");
            return;
        };

        // Sync the snarl with the engine, and send what the operator changed
        // on it (wires, dropped nodes).
        let pointer_down = ui.input(|input| input.pointer.any_down());
        let waiting = self
            .scene_graph_ui_state
            .edit_reply
            .is_some_and(|seq| self.reply_handlers.is_waiting(seq));
        let edits = self
            .scene_graph_ui_state
            .sync(graph_id, &graph, pointer_down, waiting);
        for edit in edits {
            let seq = self.send_command(edit, |_, _| {});
            self.scene_graph_ui_state.edit_reply = Some(seq);
        }

        // Controls bar.
        let current_section = self.collector_snapshot.section_state;
        let mut do_auto_layout = false;
        render_context.horizontal(ui, egui::Align::Min, |ui| {
            let mut enabled = graph.enabled;
            if ui.checkbox(&mut enabled, "Enabled").changed() {
                self.client
                    .send_event(ControlEvent::SetSceneGraphEnabled(graph_id, enabled));
            }

            if ui.button("Add Node").clicked() {
                let node_count = graph.nodes.len();
                self.send_command(
                    HostCommand::AddSceneGraphNode {
                        graph: graph_id,
                        x: 200.0 * node_count as f32,
                        y: 100.0,
                    },
                    |_, _| {},
                );
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

        // The new positions go to the engine with the next sync, like a drag.
        if do_auto_layout {
            auto_layout_graph(&graph, &mut self.scene_graph_ui_state.snarl);
            self.scene_graph_ui_state.recenter_view = true;
        }

        ui.separator();

        // Keep nodes on the canvas while they are dragged.
        let snarl = &mut self.scene_graph_ui_state.snarl;
        let snarl_ids: Vec<egui_snarl::NodeId> = snarl.node_ids().map(|(id, _)| id).collect();
        for snarl_id in snarl_ids {
            if let Some(info) = snarl.get_node_info_mut(snarl_id) {
                info.pos = Pos2::new(
                    info.pos.x.clamp(-CANVAS_HALF_EXTENT, CANVAS_HALF_EXTENT),
                    info.pos.y.clamp(-CANVAS_HALF_EXTENT, CANVAS_HALF_EXTENT),
                );
            }
        }

        // Use a vertical split in narrow dynamic tiles so both workspaces remain usable.
        let pending_zoom_override =
            std::mem::take(&mut self.scene_graph_ui_state.pending_zoom_override);
        let available = ui.available_size_before_wrap().max(egui::vec2(1.0, 1.0));
        let (workspace_rect, _) = ui.allocate_exact_size(available, egui::Sense::hover());
        let gap = ui.spacing().item_spacing.x;
        let (graph_rect, editor_rect) = if self.scene_graph_ui_state.selected_node.is_none() {
            (
                workspace_rect,
                Rect::from_min_max(workspace_rect.max, workspace_rect.max),
            )
        } else if render_context.is_narrow_dynamic() {
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

        if self.scene_graph_ui_state.last_canvas_size != viewport_rect.size() {
            self.scene_graph_ui_state.last_canvas_size = viewport_rect.size();
            self.scene_graph_ui_state.recenter_view = true;
        }
        let recenter_view = std::mem::take(&mut self.scene_graph_ui_state.recenter_view);

        // Left: snarl graph.
        let mut viewer = SceneGraphViewer {
            graph: &graph,
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
            return;
        };
        let Some(scene_node) = graph.nodes.get(&selected_node_id) else {
            self.scene_graph_ui_state.selected_node = None;
            return;
        };

        let ui = &mut editor_ui;
        let mut commands: Vec<HostCommand> = Vec::new();
        let node_param = |param| HostCommand::SetSceneGraphNode {
            graph: graph_id,
            node: selected_node_id,
            param,
        };

        // The name being typed is kept here until the field loses focus, so
        // keystrokes the engine hasn't applied yet don't get lost.
        let name_edit = &mut self.scene_graph_ui_state.name_edit;
        let mut name = match name_edit {
            Some((g, n, text)) if *g == graph_id && *n == selected_node_id => text.clone(),
            _ => scene_node.name.clone(),
        };

        ui.heading(&name);
        ui.separator();

        ui.horizontal(|ui| {
            ui.label("Name:");
            let response = ui.text_edit_singleline(&mut name);
            if response.changed() {
                commands.push(node_param(SceneGraphNodeParam::Name(name.clone())));
            }
            *name_edit = response
                .has_focus()
                .then(|| (graph_id, selected_node_id, name.clone()));
        });

        ui.add_space(4.0);

        ui.horizontal(|ui| {
            ui.label("Alpha:");
            let mut alpha = scene_node.master_alpha as f32;
            if ui
                .add(egui::Slider::new(&mut alpha, 0.0..=100.0).suffix("%"))
                .changed()
            {
                commands.push(node_param(SceneGraphNodeParam::MasterAlpha(alpha as u8)));
            }
        });

        ui.horizontal(|ui| {
            ui.label("Speed:");
            let speed_idx = scene_node.master_speed.as_index();
            let mut idx = speed_idx;
            if dropdown::is_desktop(ui) {
                let list = dropdown::items(AnimationSpeedModifier::ALL.iter().map(|s| s.as_str()));
                if let Some(i) =
                    dropdown::select(ui, "node_speed", &list, Some(idx), "", Some(80.0))
                {
                    idx = i;
                }
            } else {
                dropdown::kiosk_combo(
                    ui,
                    "node_speed",
                    None,
                    scene_node.master_speed.as_str(),
                    |ui| {
                        for (i, s) in AnimationSpeedModifier::ALL.iter().enumerate() {
                            ui.selectable_value(&mut idx, i, s.as_str());
                        }
                    },
                );
            }
            if idx != speed_idx {
                commands.push(node_param(SceneGraphNodeParam::MasterSpeed(
                    AnimationSpeedModifier::from_index(idx),
                )));
            }
        });

        ui.add_space(8.0);
        components::column_header(ui, "Scenes:");
        for scene_id in &scene_node.scenes {
            ui.horizontal(|ui| {
                let name = scene_names
                    .get(scene_id)
                    .map(|s| s.as_str())
                    .unwrap_or("???");
                ui.label(format!("  {} (ID {})", name, scene_id));
                if ui.small_button("x").clicked() {
                    commands.push(node_param(SceneGraphNodeParam::RemoveScene(*scene_id)));
                }
            });
        }

        if dropdown::is_desktop(ui) {
            let addable: Vec<u8> = scene_names
                .keys()
                .filter(|id| !scene_node.scenes.contains(id))
                .copied()
                .collect();
            let list: Vec<dropdown::Item> = addable
                .iter()
                .map(|id| dropdown::Item::new(&scene_names[id]).meta(format!("#{id}")))
                .collect();
            let options = dropdown::Options {
                placeholder: "Filter scenes".into(),
                empty: "No more scenes to add.".into(),
                ..Default::default()
            };
            if let Some(i) = dropdown::action(ui, "node_add_scene", "Add scene", &list, options) {
                commands.push(node_param(SceneGraphNodeParam::AddScene(addable[i])));
            }
        } else {
            dropdown::kiosk_combo(ui, "node_add_scene", None, "+ Add Scene", |ui| {
                for (id, name) in scene_names.iter() {
                    if !scene_node.scenes.contains(id) && ui.selectable_label(false, name).clicked()
                    {
                        commands.push(node_param(SceneGraphNodeParam::AddScene(*id)));
                    }
                }
            });
        }

        ui.add_space(12.0);
        ui.separator();

        // Node actions.
        ui.horizontal(|ui| {
            if ui.button("Set as Start").clicked() {
                commands.push(HostCommand::SetSceneGraphStart {
                    graph: graph_id,
                    node: selected_node_id,
                });
            }
            if ui.button("Delete Node").clicked() {
                self.scene_graph_ui_state.delete_node = Some((graph_id, selected_node_id));
            }
        });

        // Edge conditions for this node's outgoing edges.
        let outgoing_edges: Vec<_> = graph
            .edges
            .iter()
            .filter(|e| e.from == selected_node_id)
            .collect();

        if !outgoing_edges.is_empty() {
            ui.add_space(12.0);
            ui.separator();
            ui.label("Outgoing Edges:");

            for edge in outgoing_edges {
                let to_id = edge.to;
                let to_name = graph
                    .nodes
                    .get(&to_id)
                    .map(|n| n.name.as_str())
                    .unwrap_or("???");
                let edge_param = |param| HostCommand::SetSceneGraphEdge {
                    graph: graph_id,
                    from: selected_node_id,
                    to: to_id,
                    param,
                };

                ui.group(|ui| {
                    ui.label(format!("-> {}", to_name));

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
                    const CONDITIONS: [&str; 7] = [
                        "Manual",
                        "After Duration",
                        "After Beats",
                        "On Beat Drop",
                        "On Non-Beat",
                        "On Enter Drop",
                        "On Enter Breakdown",
                    ];
                    if dropdown::is_desktop(ui) {
                        let list = dropdown::items(CONDITIONS);
                        if let Some(i) = dropdown::select(
                            ui,
                            ("edge_cond", to_id),
                            &list,
                            Some(condition_idx),
                            "",
                            Some(160.0),
                        ) {
                            condition_idx = i;
                        }
                    } else {
                        dropdown::kiosk_combo(
                            ui,
                            ("edge_cond", to_id),
                            None,
                            CONDITIONS.get(condition_idx).copied().unwrap_or("???"),
                            |ui| {
                                for (i, label) in CONDITIONS.iter().enumerate() {
                                    ui.selectable_value(&mut condition_idx, i, *label);
                                }
                            },
                        );
                    }

                    let mut condition = edge.condition.clone();
                    if condition_idx != prev {
                        condition = match condition_idx {
                            1 => TransitionCondition::AfterDuration(1000),
                            2 => TransitionCondition::AfterBeats(4),
                            3 => TransitionCondition::OnBeatDrop,
                            4 => TransitionCondition::OnNonBeat,
                            5 => TransitionCondition::OnEnterDrop,
                            6 => TransitionCondition::OnEnterBreakdown,
                            _ => TransitionCondition::Manual,
                        };
                        commands.push(edge_param(SceneGraphEdgeParam::Condition(
                            condition.clone(),
                        )));
                    }

                    match condition {
                        TransitionCondition::AfterDuration(ms) => {
                            let mut secs = ms as f32 / 1000.0;
                            if ui
                                .add(
                                    egui::Slider::new(&mut secs, 0.1..=30.0)
                                        .suffix("s")
                                        .logarithmic(true),
                                )
                                .changed()
                            {
                                commands.push(edge_param(SceneGraphEdgeParam::Condition(
                                    TransitionCondition::AfterDuration((secs * 1000.0) as u64),
                                )));
                            }
                        }
                        TransitionCondition::AfterBeats(n) => {
                            let mut beats = n as i32;
                            if ui
                                .add(egui::Slider::new(&mut beats, 1..=64).suffix(" beats"))
                                .changed()
                            {
                                commands.push(edge_param(SceneGraphEdgeParam::Condition(
                                    TransitionCondition::AfterBeats(beats as u32),
                                )));
                            }
                        }
                        _ => {}
                    }

                    ui.horizontal(|ui| {
                        ui.label("Priority:");
                        // DragValue keeps the dragged value itself, so the
                        // engine's one-tick delay loses no movement.
                        let mut p = edge.priority as i32;
                        if ui
                            .add(egui::DragValue::new(&mut p).range(0..=255))
                            .changed()
                        {
                            commands.push(edge_param(SceneGraphEdgeParam::Priority(p as u8)));
                        }
                    });
                });
            }
        }

        for command in commands {
            self.client.send(command);
        }
    }
}

/// Lays the graph out with a force simulation and moves the snarl's nodes
/// there; the next sync sends the new positions.
fn auto_layout_graph(graph: &SceneGraph, snarl: &mut Snarl<SnarlNode>) {
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

#[cfg(test)]
mod edit_tests {
    use super::*;
    use blaulicht_shared::scene_graph::{SceneEdge, SceneGraphNode};

    #[test]
    fn countdown_does_not_change_header_bounds_and_fit_contains_nodes() {
        let ctx = Context::default();
        let mut graph = SceneGraph::default();
        graph.nodes.insert(0, SceneGraphNode::default());
        let mut snarl = Snarl::new();
        let node = snarl.insert_node(
            Pos2::new(220.0, 100.0),
            SnarlNode {
                node_id: 0,
                graph_id: 0,
            },
        );
        let mut selected = None;
        let mut zoom = 1.0;
        let mut sizes = Vec::new();
        for active in [None, Some(0), None] {
            graph.active_node = active;
            let mut viewer = SceneGraphViewer {
                graph: &graph,
                graph_id: 0,
                selected_node: &mut selected,
                recenter_view: true,
                viewport_rect: Rect::from_min_size(Pos2::ZERO, Vec2::new(360.0, 400.0)),
                zoom_level: &mut zoom,
                pending_zoom_override: None,
                active_countdown_ms: Some(1999),
            };
            let mut transform = egui::emath::TSTransform::IDENTITY;
            viewer.current_transform(&mut transform, &mut snarl);
            let node_bounds = Rect::from_min_size(Pos2::new(220.0, 100.0), Vec2::new(230.0, 100.0));
            assert!(viewer
                .viewport_rect
                .contains_rect(transform.mul_rect(node_bounds)));
            let _ = ctx.run(Default::default(), |ctx| {
                egui::CentralPanel::default().show(ctx, |ui| {
                    let response =
                        ui.vertical(|ui| viewer.show_header(node, &[], &[], ui, &mut snarl));
                    sizes.push(response.response.rect.size());
                });
            });
        }
        assert!(
            sizes.windows(2).all(|pair| pair[0] == pair[1]),
            "header sizes: {sizes:?}"
        );
    }

    fn node_at(x: f32, y: f32) -> SceneGraphNode {
        SceneGraphNode {
            pos_x: x,
            pos_y: y,
            ..SceneGraphNode::default()
        }
    }

    fn edge(from: NodeId, to: NodeId) -> SceneEdge {
        SceneEdge {
            from,
            to,
            condition: TransitionCondition::Manual,
            priority: 0,
        }
    }

    /// Nodes 0 and 1, with an edge 0 -> 1.
    fn two_nodes() -> SceneGraph {
        let mut graph = SceneGraph::default();
        graph.nodes.insert(0, node_at(10.0, 20.0));
        graph.nodes.insert(1, node_at(300.0, 20.0));
        graph.edges.push(edge(0, 1));
        graph
    }

    fn snarl_pos(ui: &SceneGraphUI, node: NodeId) -> Pos2 {
        ui.snarl.get_node_info(ui.snarl_ids[&node]).unwrap().pos
    }

    fn pins(ui: &SceneGraphUI, from: NodeId, to: NodeId) -> (OutPinId, InPinId) {
        (
            OutPinId {
                node: ui.snarl_ids[&from],
                output: 0,
            },
            InPinId {
                node: ui.snarl_ids[&to],
                input: 0,
            },
        )
    }

    #[test]
    fn the_snarl_shows_the_engine_graph() {
        let mut ui = SceneGraphUI::default();
        let mut graph = two_nodes();
        assert!(ui.sync(0, &graph, false, false).is_empty());
        assert_eq!(snarl_pos(&ui, 1), Pos2::new(300.0, 20.0));
        assert_eq!(ui.snarl_edges(), BTreeSet::from([(0, 1)]));

        // Another frontend adds a node, moves one, rewires and removes one.
        graph.nodes.insert(2, node_at(0.0, 400.0));
        graph.nodes.get_mut(&0).unwrap().pos_x = 50.0;
        graph.edges = vec![edge(1, 2)];
        assert!(ui.sync(0, &graph, false, false).is_empty());
        assert_eq!(snarl_pos(&ui, 0), Pos2::new(50.0, 20.0));
        assert_eq!(snarl_pos(&ui, 2), Pos2::new(0.0, 400.0));
        assert_eq!(ui.snarl_edges(), BTreeSet::from([(1, 2)]));

        ui.selected_node = Some(1);
        graph.nodes.remove(&1);
        graph.edges.clear();
        assert!(ui.sync(0, &graph, false, false).is_empty());
        assert_eq!(ui.snarl.node_ids().count(), 2);
        assert_eq!(ui.selected_node, None);
        assert!(ui.snarl_edges().is_empty());
    }

    #[test]
    fn a_drag_is_sent_once_when_it_ends() {
        let mut ui = SceneGraphUI::default();
        let graph = two_nodes();
        ui.sync(0, &graph, false, false);
        let snarl_id = ui.snarl_ids[&1];
        ui.snarl.get_node_info_mut(snarl_id).unwrap().pos = Pos2::new(320.0, 40.0);

        // Still dragging: nothing is sent, and the engine doesn't pull it back.
        assert!(ui.sync(0, &graph, true, false).is_empty());
        assert_eq!(snarl_pos(&ui, 1), Pos2::new(320.0, 40.0));

        // Dropped: one move.
        let sent = ui.sync(0, &graph, false, false);
        assert!(
            matches!(&sent[..], [HostCommand::MoveSceneGraphNodes { graph: 0, positions }]
                if positions == &vec![(1, 320.0, 40.0)]),
            "{sent:?}"
        );
        // Until the reply, the engine's old position doesn't pull it back.
        assert!(ui.sync(0, &graph, false, true).is_empty());
        assert_eq!(snarl_pos(&ui, 1), Pos2::new(320.0, 40.0));

        // The engine applied it: nothing more to send or change.
        let mut moved = graph.clone();
        *moved.nodes.get_mut(&1).unwrap() = node_at(320.0, 40.0);
        assert!(ui.sync(0, &moved, false, false).is_empty());
        assert_eq!(snarl_pos(&ui, 1), Pos2::new(320.0, 40.0));
    }

    #[test]
    fn wires_become_connect_and_disconnect_commands() {
        let mut ui = SceneGraphUI::default();
        let graph = two_nodes();
        ui.sync(0, &graph, false, false);

        let (out_pin, in_pin) = pins(&ui, 0, 1);
        ui.snarl.disconnect(out_pin, in_pin);
        let (out_pin, in_pin) = pins(&ui, 1, 0);
        ui.snarl.connect(out_pin, in_pin);
        let sent = ui.sync(0, &graph, false, false);
        assert!(
            matches!(
                &sent[..],
                [
                    HostCommand::ConnectSceneGraphNodes {
                        graph: 0,
                        from: 1,
                        to: 0
                    },
                    HostCommand::DisconnectSceneGraphNodes {
                        graph: 0,
                        from: 0,
                        to: 1
                    },
                ]
            ),
            "{sent:?}"
        );
        // Waiting for the reply: the wires stay as the operator left them.
        assert!(ui.sync(0, &graph, false, true).is_empty());
        assert_eq!(ui.snarl_edges(), BTreeSet::from([(1, 0)]));
        // A refused edit: the engine's edges come back.
        assert!(ui.sync(0, &graph, false, false).is_empty());
        assert_eq!(ui.snarl_edges(), BTreeSet::from([(0, 1)]));
    }

    #[test]
    fn another_graph_rebuilds_the_snarl() {
        let mut ui = SceneGraphUI::default();
        ui.sync(0, &two_nodes(), false, false);
        ui.selected_node = Some(0);
        let mut other = SceneGraph::default();
        other.nodes.insert(5, node_at(0.0, 0.0));
        assert!(ui.sync(1, &other, false, false).is_empty());
        assert_eq!(ui.selected_node, None);
        assert_eq!(ui.snarl.node_ids().count(), 1);
        // A node that was never placed gets a slot, and is not moved there
        // in the engine just by being shown.
        assert_eq!(snarl_pos(&ui, 5), Pos2::new(0.0, 100.0));
        assert!(ui.sync(1, &other, false, false).is_empty());
    }
}
