//! The engine side of the View and Scene graph pages' host commands (see
//! [`HostCommand`]). Each runs in `DmxEngine::apply_host`, with the sender's
//! session swapped in, and names what it acts on by id.
//!
//! Scene graph commands edit one node, edge or position at a time. The
//! engine rewrites `scene_graphs` every tick (the active node, countdowns),
//! so a frontend must never write back a whole graph it read earlier.
//!
//! [`HostCommand`]: crate::hub::HostCommand

use super::EngineState;
use crate::hub::{CommandOutcome, CreatedId, SceneGraphEdgeParam, SceneGraphNodeParam};
use blaulicht_shared::{
    scene::BLANK_SCENE_ID,
    scene_graph::{GraphId, NodeId, SceneEdge, SceneGraph, SceneGraphNode, TransitionCondition},
    view::{View, ViewSceneMasters},
};

/// Half the width (and height) of the scene graph canvas, centred on the
/// origin. Node positions are clamped to it.
pub const SCENE_GRAPH_CANVAS_HALF_EXTENT: f32 = 1500.0;

fn ok(created: Option<CreatedId>) -> CommandOutcome {
    CommandOutcome::Ok { created }
}

fn refused(reason: impl Into<String>) -> CommandOutcome {
    CommandOutcome::Refused(reason.into())
}

fn clamp_to_canvas(v: f32) -> f32 {
    v.clamp(
        -SCENE_GRAPH_CANVAS_HALF_EXTENT,
        SCENE_GRAPH_CANVAS_HALF_EXTENT,
    )
}

const GRAPH_NOT_FOUND: &str = "Scene graph not found";
const NODE_NOT_FOUND: &str = "Node not found";
const EDGE_NOT_FOUND: &str = "Edge not found";
const VIEW_NOT_FOUND: &str = "View not found";

impl EngineState {
    //
    // Views.
    //

    /// `HostCommand::CreateView` (`capture`) and `HostCommand::CreateEmptyView`.
    pub(super) fn create_view_command(&mut self, name: &str, capture: bool) -> CommandOutcome {
        let name = name.trim();
        if name.is_empty() {
            return refused("Name is empty");
        }
        let Some(id) = (0..=u8::MAX).find(|id| !self.0.views.contains_key(id)) else {
            return refused("View limit reached");
        };
        // A view captures what is playing, which is the overlay stack -- the
        // base slot belongs to BLANK.
        let overlays = if capture {
            self.0.current_overlay_scenes.clone()
        } else {
            Vec::new()
        };
        self.0
            .views
            .insert(id, View::new(name.to_string(), overlays));
        ok(Some(CreatedId::View(id)))
    }

    /// `HostCommand::DeleteView`.
    pub(super) fn delete_view_command(&mut self, view: u8) -> CommandOutcome {
        if !self.0.views.contains_key(&view) {
            return refused(VIEW_NOT_FOUND);
        }
        if self.0.views.len() <= 1 {
            return refused("At least one view must remain");
        }
        self.0.views.remove(&view);
        ok(None)
    }

    /// `HostCommand::RenameView`.
    pub(super) fn rename_view_command(&mut self, view: u8, name: &str) -> CommandOutcome {
        let name = name.trim();
        if name.is_empty() {
            return refused("Name is empty");
        }
        let Some(view) = self.0.views.get_mut(&view) else {
            return refused(VIEW_NOT_FOUND);
        };
        view.name = name.to_string();
        ok(None)
    }

    /// `HostCommand::AddViewOverlay`.
    pub(super) fn add_view_overlay_command(&mut self, view: u8, scene: u8) -> CommandOutcome {
        if scene == BLANK_SCENE_ID || !self.0.scenes.contains_key(&scene) {
            return refused("Scene not found");
        }
        let Some(view) = self.0.views.get_mut(&view) else {
            return refused(VIEW_NOT_FOUND);
        };
        if !view.overlays.contains(&scene) {
            view.overlays.push(scene);
        }
        ok(None)
    }

    /// `HostCommand::RemoveViewOverlay`.
    pub(super) fn remove_view_overlay_command(&mut self, view: u8, scene: u8) -> CommandOutcome {
        let Some(view) = self.0.views.get_mut(&view) else {
            return refused(VIEW_NOT_FOUND);
        };
        view.overlays.retain(|id| *id != scene);
        view.prune_masters();
        ok(None)
    }

    /// `HostCommand::SetViewSceneMasters`.
    pub(super) fn set_view_scene_masters_command(
        &mut self,
        view: u8,
        scene: u8,
        masters: ViewSceneMasters,
    ) -> CommandOutcome {
        let Some(view) = self.0.views.get_mut(&view) else {
            return refused(VIEW_NOT_FOUND);
        };
        if !view.overlays.contains(&scene) {
            return refused("Scene is not an overlay of this view");
        }
        view.masters.insert(scene, masters);
        ok(None)
    }

    //
    // Scene graphs.
    //

    fn graph_mut(&mut self, graph: GraphId) -> Result<&mut SceneGraph, CommandOutcome> {
        self.0
            .scene_graphs
            .graphs
            .get_mut(&graph)
            .ok_or_else(|| refused(GRAPH_NOT_FOUND))
    }

    fn node_mut(
        &mut self,
        graph: GraphId,
        node: NodeId,
    ) -> Result<&mut SceneGraphNode, CommandOutcome> {
        self.graph_mut(graph)?
            .nodes
            .get_mut(&node)
            .ok_or_else(|| refused(NODE_NOT_FOUND))
    }

    /// `HostCommand::CreateSceneGraph`: creates the graph and focuses it in
    /// the sender's session (swapped in), as the page did.
    pub(super) fn create_scene_graph_command(&mut self, name: String) -> CommandOutcome {
        let graphs = &mut self.0.scene_graphs;
        let Some(id) = (0..=u8::MAX).find(|id| !graphs.graphs.contains_key(id)) else {
            return refused("Scene graph limit reached");
        };
        graphs.graphs.insert(
            id,
            SceneGraph {
                name,
                ..SceneGraph::default()
            },
        );
        graphs.focused_graph = Some(id);
        ok(Some(CreatedId::SceneGraph(id)))
    }

    /// `HostCommand::DeleteSceneGraph`.
    pub(super) fn delete_scene_graph_command(&mut self, graph: GraphId) -> CommandOutcome {
        let graphs = &mut self.0.scene_graphs;
        if graphs.graphs.remove(&graph).is_none() {
            return refused(GRAPH_NOT_FOUND);
        }
        graphs.active_countdowns.remove(&graph);
        if graphs.focused_graph == Some(graph) {
            graphs.focused_graph = None;
        }
        // Other operator sessions focused on it, too.
        self.repair_sessions();
        ok(None)
    }

    /// `HostCommand::AddSceneGraphNode`.
    pub(super) fn add_scene_graph_node_command(
        &mut self,
        graph: GraphId,
        x: f32,
        y: f32,
    ) -> CommandOutcome {
        let graph_id = graph;
        let graph = match self.graph_mut(graph) {
            Ok(graph) => graph,
            Err(refusal) => return refusal,
        };
        let Some(node) = (0..=u8::MAX).find(|id| !graph.nodes.contains_key(id)) else {
            return refused("Node limit reached");
        };
        graph.nodes.insert(
            node,
            SceneGraphNode {
                pos_x: clamp_to_canvas(x),
                pos_y: clamp_to_canvas(y),
                ..SceneGraphNode::default()
            },
        );
        ok(Some(CreatedId::SceneGraphNode {
            graph: graph_id,
            node,
        }))
    }

    /// `HostCommand::RemoveSceneGraphNode`.
    pub(super) fn remove_scene_graph_node_command(
        &mut self,
        graph: GraphId,
        node: NodeId,
    ) -> CommandOutcome {
        let graph = match self.graph_mut(graph) {
            Ok(graph) => graph,
            Err(refusal) => return refusal,
        };
        if graph.nodes.remove(&node).is_none() {
            return refused(NODE_NOT_FOUND);
        }
        graph.edges.retain(|e| e.from != node && e.to != node);
        if graph.active_node == Some(node) {
            graph.active_node = None;
        }
        ok(None)
    }

    /// `HostCommand::MoveSceneGraphNodes`: all or none.
    pub(super) fn move_scene_graph_nodes_command(
        &mut self,
        graph: GraphId,
        positions: &[(NodeId, f32, f32)],
    ) -> CommandOutcome {
        let graph = match self.graph_mut(graph) {
            Ok(graph) => graph,
            Err(refusal) => return refusal,
        };
        if positions
            .iter()
            .any(|(node, _, _)| !graph.nodes.contains_key(node))
        {
            return refused(NODE_NOT_FOUND);
        }
        for &(node, x, y) in positions {
            let node = graph.nodes.get_mut(&node).expect("checked above");
            node.pos_x = clamp_to_canvas(x);
            node.pos_y = clamp_to_canvas(y);
        }
        ok(None)
    }

    /// `HostCommand::ConnectSceneGraphNodes`.
    pub(super) fn connect_scene_graph_nodes_command(
        &mut self,
        graph: GraphId,
        from: NodeId,
        to: NodeId,
    ) -> CommandOutcome {
        let graph = match self.graph_mut(graph) {
            Ok(graph) => graph,
            Err(refusal) => return refusal,
        };
        if !graph.nodes.contains_key(&from) || !graph.nodes.contains_key(&to) {
            return refused(NODE_NOT_FOUND);
        }
        if !graph.edges.iter().any(|e| e.from == from && e.to == to) {
            graph.edges.push(SceneEdge {
                from,
                to,
                condition: TransitionCondition::Manual,
                priority: 0,
            });
        }
        ok(None)
    }

    /// `HostCommand::DisconnectSceneGraphNodes`.
    pub(super) fn disconnect_scene_graph_nodes_command(
        &mut self,
        graph: GraphId,
        from: NodeId,
        to: NodeId,
    ) -> CommandOutcome {
        let graph = match self.graph_mut(graph) {
            Ok(graph) => graph,
            Err(refusal) => return refusal,
        };
        let before = graph.edges.len();
        graph.edges.retain(|e| !(e.from == from && e.to == to));
        if graph.edges.len() == before {
            return refused(EDGE_NOT_FOUND);
        }
        ok(None)
    }

    /// `HostCommand::SetSceneGraphNode`.
    pub(super) fn set_scene_graph_node_command(
        &mut self,
        graph: GraphId,
        node: NodeId,
        param: SceneGraphNodeParam,
    ) -> CommandOutcome {
        if let SceneGraphNodeParam::AddScene(scene) = param {
            if !self.0.scenes.contains_key(&scene) {
                return refused("Scene not found");
            }
        }
        let node = match self.node_mut(graph, node) {
            Ok(node) => node,
            Err(refusal) => return refusal,
        };
        match param {
            SceneGraphNodeParam::Name(name) => node.name = name,
            SceneGraphNodeParam::MasterAlpha(alpha) => node.master_alpha = alpha.min(100),
            SceneGraphNodeParam::MasterSpeed(speed) => node.master_speed = speed,
            SceneGraphNodeParam::AddScene(scene) => {
                if !node.scenes.contains(&scene) {
                    node.scenes.push(scene);
                }
            }
            SceneGraphNodeParam::RemoveScene(scene) => node.scenes.retain(|id| *id != scene),
        }
        ok(None)
    }

    /// `HostCommand::SetSceneGraphEdge`.
    pub(super) fn set_scene_graph_edge_command(
        &mut self,
        graph: GraphId,
        from: NodeId,
        to: NodeId,
        param: SceneGraphEdgeParam,
    ) -> CommandOutcome {
        let graph = match self.graph_mut(graph) {
            Ok(graph) => graph,
            Err(refusal) => return refusal,
        };
        let Some(edge) = graph
            .edges
            .iter_mut()
            .find(|e| e.from == from && e.to == to)
        else {
            return refused(EDGE_NOT_FOUND);
        };
        match param {
            SceneGraphEdgeParam::Condition(condition) => edge.condition = condition,
            SceneGraphEdgeParam::Priority(priority) => edge.priority = priority,
        }
        ok(None)
    }

    /// `HostCommand::SetSceneGraphStart`.
    pub(super) fn set_scene_graph_start_command(
        &mut self,
        graph: GraphId,
        node: NodeId,
    ) -> CommandOutcome {
        let graph = match self.graph_mut(graph) {
            Ok(graph) => graph,
            Err(refusal) => return refusal,
        };
        if !graph.nodes.contains_key(&node) {
            return refused(NODE_NOT_FOUND);
        }
        graph.active_node = Some(node);
        ok(None)
    }
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use crate::{
        client::{
            testing::{small_rig, TwoClients},
            EngineClient,
        },
        dmx::EngineState,
        hub::{CommandOutcome, CreatedId, HostCommand, SceneGraphEdgeParam, SceneGraphNodeParam},
    };
    use blaulicht_shared::{
        scene::BLANK_SCENE_ID,
        scene_graph::{SceneEdge, SceneGraph, SceneGraphNode, TransitionCondition},
        view::{View, ViewSceneMasters},
        AnimationSpeedModifier, ControlEvent, ControlEventMessage, EventOriginator,
    };

    const OK: CommandOutcome = CommandOutcome::Ok { created: None };

    fn web(event: ControlEvent) -> ControlEventMessage {
        ControlEventMessage::new(EventOriginator::Web, event)
    }

    fn refused(reason: &str) -> CommandOutcome {
        CommandOutcome::Refused(reason.to_string())
    }

    fn graph(state: &EngineState, id: u8) -> SceneGraph {
        state.0.scene_graphs.graphs[&id].clone()
    }

    fn edge(from: u8, to: u8, condition: TransitionCondition) -> SceneEdge {
        SceneEdge {
            from,
            to,
            condition,
            priority: 0,
        }
    }

    /// Graph 0, enabled: node 0 (scene 1) -> node 1 (scene 2) at once, then
    /// back after a minute. Node 0 is the start.
    fn running_graph(state: &mut EngineState) {
        small_rig(state);
        let node = |scene: u8| SceneGraphNode {
            scenes: vec![scene],
            ..SceneGraphNode::default()
        };
        state.0.scene_graphs.graphs.insert(
            0,
            SceneGraph {
                name: "Show".to_string(),
                nodes: [(0, node(1)), (1, node(2))].into(),
                edges: vec![
                    edge(0, 1, TransitionCondition::AfterDuration(0)),
                    edge(1, 0, TransitionCondition::AfterDuration(60_000)),
                ],
                active_node: Some(0),
                enabled: true,
            },
        );
    }

    /// Graph 0 with nodes 0 and 1 and an edge 0 -> 1, not running.
    fn idle_graph(state: &mut EngineState) {
        small_rig(state);
        state.0.scene_graphs.graphs.insert(
            0,
            SceneGraph {
                nodes: [
                    (0, SceneGraphNode::default()),
                    (1, SceneGraphNode::default()),
                ]
                .into(),
                edges: vec![edge(0, 1, TransitionCondition::Manual)],
                ..SceneGraph::default()
            },
        );
    }

    #[test]
    fn a_graph_edit_while_the_engine_ticks_the_graph_loses_no_runtime_state() {
        let mut h = TwoClients::start(running_graph);
        // What the page drew before the engine ran the graph.
        let stale = h.desk.with_show(|show| graph(show, 0));
        assert_eq!(stale.active_node, Some(0));

        // The engine activates node 0, then moves on to node 1 by itself.
        for _ in 0..100 {
            h.engine.tick();
            if h.desk.with_show(|show| graph(show, 0).active_node) == Some(1) {
                break;
            }
            std::thread::sleep(Duration::from_millis(1));
        }
        std::thread::sleep(Duration::from_millis(30));
        h.engine.tick();
        let countdown = |h: &TwoClients| {
            h.desk
                .with_show(|show| show.0.scene_graphs.active_countdowns[&0])
        };
        let before = countdown(&h);
        assert!(before <= 60_000 - 30, "{before}");

        // Edits from pages that drew the stale graph.
        std::thread::sleep(Duration::from_millis(30));
        let edits = [
            HostCommand::SetSceneGraphNode {
                graph: 0,
                node: 0,
                param: SceneGraphNodeParam::Name("Intro".to_string()),
            },
            HostCommand::MoveSceneGraphNodes {
                graph: 0,
                positions: vec![(1, 300.0, 40.0)],
            },
            HostCommand::SetSceneGraphEdge {
                graph: 0,
                from: 0,
                to: 1,
                param: SceneGraphEdgeParam::Priority(3),
            },
            HostCommand::AddSceneGraphNode {
                graph: 0,
                x: 0.0,
                y: 200.0,
            },
        ];
        for (i, edit) in edits.into_iter().enumerate() {
            let client = if i % 2 == 0 { &h.desk } else { &h.remote };
            assert!(
                matches!(h.engine.round_trip(client, edit), CommandOutcome::Ok { .. }),
                "edit {i}"
            );
        }

        let (edited, overlays) = h
            .desk
            .with_show(|show| (graph(show, 0), show.0.current_overlay_scenes.clone()));
        // The edits landed...
        assert_eq!(edited.nodes[&0].name, "Intro");
        assert_eq!(
            (edited.nodes[&1].pos_x, edited.nodes[&1].pos_y),
            (300.0, 40.0)
        );
        assert_eq!(edited.edges[0].priority, 3);
        assert_eq!(edited.nodes.len(), 3);
        // ...and the engine's own state survived them: the active node, its
        // overlay and its clock (the countdown kept running, not restarted).
        assert_eq!(edited.active_node, Some(1));
        assert!(
            overlays.contains(&2) && !overlays.contains(&1),
            "{overlays:?}"
        );
        let after = countdown(&h);
        assert!(after + 30 <= before, "restarted? {before} -> {after}");
    }

    #[test]
    fn a_new_graph_is_focused_for_its_creator_only() {
        let mut h = TwoClients::start(small_rig);
        let create = HostCommand::CreateSceneGraph {
            name: "Set".to_string(),
        };
        assert!(create.edits_show());
        assert_eq!(
            h.engine.round_trip(&h.remote, create),
            CommandOutcome::Ok {
                created: Some(CreatedId::SceneGraph(0))
            }
        );
        assert_eq!(
            h.remote.with_session(|_, me| me.focused_graph),
            Some(Some(0))
        );
        assert_eq!(h.desk.with_session(|_, me| me.focused_graph), Some(None));
        let created = h.desk.with_show(|show| graph(show, 0));
        assert_eq!(created.name, "Set");
        assert!(!created.enabled);
    }

    #[test]
    fn deleting_a_graph_unfocuses_every_session() {
        let mut h = TwoClients::start(idle_graph);
        for client in [&h.desk, &h.remote] {
            let focus = web(ControlEvent::SetFocusedSceneGraph(Some(0)));
            assert_eq!(h.engine.round_trip(client, focus), OK);
        }
        assert_eq!(
            h.engine
                .round_trip(&h.remote, HostCommand::DeleteSceneGraph(0)),
            OK
        );
        assert!(h
            .desk
            .with_show(|show| show.0.scene_graphs.graphs.is_empty()));
        for client in [&h.desk, &h.remote] {
            assert_eq!(client.with_session(|_, me| me.focused_graph), Some(None));
        }
        assert_eq!(
            h.engine
                .round_trip(&h.desk, HostCommand::DeleteSceneGraph(0)),
            refused("Scene graph not found")
        );
    }

    #[test]
    fn nodes_are_added_moved_and_removed_by_id() {
        let mut h = TwoClients::start(idle_graph);
        assert_eq!(
            h.engine.round_trip(
                &h.desk,
                HostCommand::AddSceneGraphNode {
                    graph: 0,
                    x: 5000.0,
                    y: -20.0,
                }
            ),
            CommandOutcome::Ok {
                created: Some(CreatedId::SceneGraphNode { graph: 0, node: 2 })
            }
        );
        let added = h.desk.with_show(|show| graph(show, 0).nodes[&2].clone());
        assert_eq!((added.pos_x, added.pos_y), (1500.0, -20.0));
        assert_eq!(
            h.engine.round_trip(
                &h.desk,
                HostCommand::AddSceneGraphNode {
                    graph: 9,
                    x: 0.0,
                    y: 0.0,
                }
            ),
            refused("Scene graph not found")
        );

        // A move is all or none.
        let refused_move = HostCommand::MoveSceneGraphNodes {
            graph: 0,
            positions: vec![(0, 1.0, 2.0), (7, 3.0, 4.0)],
        };
        assert_eq!(
            h.engine.round_trip(&h.desk, refused_move),
            refused("Node not found")
        );
        assert_eq!(h.desk.with_show(|show| graph(show, 0).nodes[&0].pos_x), 0.0);
        let moved = HostCommand::MoveSceneGraphNodes {
            graph: 0,
            positions: vec![(0, 1.0, 2.0), (1, 3.0, -9000.0)],
        };
        assert_eq!(h.engine.round_trip(&h.desk, moved), OK);
        let nodes = h.desk.with_show(|show| graph(show, 0).nodes);
        assert_eq!((nodes[&0].pos_x, nodes[&0].pos_y), (1.0, 2.0));
        assert_eq!((nodes[&1].pos_x, nodes[&1].pos_y), (3.0, -1500.0));

        // Removing the start node takes its edges and the start with it.
        let start = HostCommand::SetSceneGraphStart { graph: 0, node: 1 };
        assert_eq!(h.engine.round_trip(&h.desk, start), OK);
        let remove = HostCommand::RemoveSceneGraphNode { graph: 0, node: 1 };
        assert_eq!(h.engine.round_trip(&h.remote, remove.clone()), OK);
        let after = h.desk.with_show(|show| graph(show, 0));
        assert!(!after.nodes.contains_key(&1));
        assert!(after.edges.is_empty());
        assert_eq!(after.active_node, None);
        assert_eq!(
            h.engine.round_trip(&h.remote, remove),
            refused("Node not found")
        );
        assert_eq!(
            h.engine.round_trip(
                &h.desk,
                HostCommand::SetSceneGraphStart { graph: 0, node: 1 }
            ),
            refused("Node not found")
        );
    }

    #[test]
    fn edges_are_connected_edited_and_disconnected() {
        let mut h = TwoClients::start(idle_graph);
        let connect = |from, to| HostCommand::ConnectSceneGraphNodes { graph: 0, from, to };
        assert_eq!(h.engine.round_trip(&h.desk, connect(1, 0)), OK);
        // A second connect of the same pins adds nothing.
        assert_eq!(h.engine.round_trip(&h.desk, connect(1, 0)), OK);
        assert_eq!(
            h.engine.round_trip(&h.desk, connect(1, 4)),
            refused("Node not found")
        );
        let edges = h.desk.with_show(|show| graph(show, 0).edges);
        assert_eq!(edges.len(), 2);
        assert_eq!(edges[1].condition, TransitionCondition::Manual);

        let set = |param| HostCommand::SetSceneGraphEdge {
            graph: 0,
            from: 1,
            to: 0,
            param,
        };
        let beats = SceneGraphEdgeParam::Condition(TransitionCondition::AfterBeats(8));
        assert_eq!(h.engine.round_trip(&h.remote, set(beats)), OK);
        assert_eq!(
            h.engine
                .round_trip(&h.remote, set(SceneGraphEdgeParam::Priority(7))),
            OK
        );
        let edited = h.desk.with_show(|show| graph(show, 0).edges[1].clone());
        assert_eq!(edited.condition, TransitionCondition::AfterBeats(8));
        assert_eq!(edited.priority, 7);

        let disconnect = HostCommand::DisconnectSceneGraphNodes {
            graph: 0,
            from: 1,
            to: 0,
        };
        assert_eq!(h.engine.round_trip(&h.desk, disconnect.clone()), OK);
        assert_eq!(h.desk.with_show(|show| graph(show, 0).edges.len()), 1);
        assert_eq!(
            h.engine.round_trip(&h.desk, disconnect),
            refused("Edge not found")
        );
        assert_eq!(
            h.engine
                .round_trip(&h.desk, set(SceneGraphEdgeParam::Priority(1))),
            refused("Edge not found")
        );
    }

    #[test]
    fn node_parameters_are_set_one_at_a_time() {
        let mut h = TwoClients::start(idle_graph);
        let set = |param| HostCommand::SetSceneGraphNode {
            graph: 0,
            node: 0,
            param,
        };
        for param in [
            SceneGraphNodeParam::Name("Verse".to_string()),
            SceneGraphNodeParam::MasterAlpha(180),
            SceneGraphNodeParam::MasterSpeed(AnimationSpeedModifier::_2),
            SceneGraphNodeParam::AddScene(1),
            SceneGraphNodeParam::AddScene(2),
            SceneGraphNodeParam::AddScene(1),
            SceneGraphNodeParam::RemoveScene(2),
        ] {
            assert_eq!(h.engine.round_trip(&h.remote, set(param)), OK);
        }
        let node = h.desk.with_show(|show| graph(show, 0).nodes[&0].clone());
        assert_eq!(node.name, "Verse");
        assert_eq!(node.master_alpha, 100);
        assert_eq!(node.master_speed, AnimationSpeedModifier::_2);
        assert_eq!(node.scenes, vec![1]);

        assert_eq!(
            h.engine
                .round_trip(&h.remote, set(SceneGraphNodeParam::AddScene(77))),
            refused("Scene not found")
        );
        let missing = HostCommand::SetSceneGraphNode {
            graph: 0,
            node: 9,
            param: SceneGraphNodeParam::MasterAlpha(1),
        };
        assert_eq!(
            h.engine.round_trip(&h.remote, missing),
            refused("Node not found")
        );
    }

    #[test]
    fn views_are_created_edited_and_deleted_by_id() {
        let mut h = TwoClients::start(small_rig);
        let overlays = web(ControlEvent::SetOverlays(vec![1, 2]));
        assert_eq!(h.engine.round_trip(&h.desk, overlays), OK);

        let create = HostCommand::CreateView {
            name: "  Chorus ".to_string(),
        };
        assert!(create.edits_show());
        let CommandOutcome::Ok {
            created: Some(CreatedId::View(chorus)),
        } = h.engine.round_trip(&h.remote, create)
        else {
            panic!("no view created");
        };
        let view = h.desk.with_show(|show| show.0.views[&chorus].clone());
        assert_eq!(view, View::new("Chorus".to_string(), vec![1, 2]));
        let empty = HostCommand::CreateEmptyView {
            name: "Bridge".to_string(),
        };
        assert!(empty.edits_show());
        let CommandOutcome::Ok {
            created: Some(CreatedId::View(bridge)),
        } = h.engine.round_trip(&h.remote, empty)
        else {
            panic!("no empty view created");
        };
        let view = h.desk.with_show(|show| show.0.views[&bridge].clone());
        assert_eq!(view, View::new("Bridge".to_string(), vec![]));
        assert_eq!(
            h.engine
                .round_trip(&h.remote, HostCommand::DeleteView(bridge)),
            OK
        );
        assert_eq!(
            h.engine.round_trip(
                &h.remote,
                HostCommand::CreateView {
                    name: " ".to_string()
                }
            ),
            refused("Name is empty")
        );

        let rename = HostCommand::RenameView {
            view: chorus,
            name: " Verse ".to_string(),
        };
        assert!(rename.edits_show());
        assert_eq!(h.engine.round_trip(&h.remote, rename), OK);
        assert_eq!(
            h.desk.with_show(|show| show.0.views[&chorus].name.clone()),
            "Verse"
        );
        assert_eq!(
            h.engine.round_trip(
                &h.remote,
                HostCommand::RenameView {
                    view: chorus,
                    name: " ".to_string()
                }
            ),
            refused("Name is empty")
        );
        assert_eq!(
            h.engine.round_trip(
                &h.remote,
                HostCommand::RenameView {
                    view: 250,
                    name: "X".to_string()
                }
            ),
            refused("View not found")
        );

        // Overlays and their masters.
        let masters = ViewSceneMasters {
            master_alpha: 40,
            master_speed: AnimationSpeedModifier::_2,
        };
        let set_masters = |scene| HostCommand::SetViewSceneMasters {
            view: chorus,
            scene,
            masters,
        };
        assert_eq!(h.engine.round_trip(&h.desk, set_masters(2)), OK);
        assert_eq!(
            h.engine.round_trip(&h.desk, set_masters(0)),
            refused("Scene is not an overlay of this view")
        );
        let add = |scene| HostCommand::AddViewOverlay {
            view: chorus,
            scene,
        };
        assert_eq!(h.engine.round_trip(&h.desk, add(0)), OK);
        assert_eq!(h.engine.round_trip(&h.desk, add(0)), OK);
        assert_eq!(
            h.engine.round_trip(&h.desk, add(BLANK_SCENE_ID)),
            refused("Scene not found")
        );
        let remove = HostCommand::RemoveViewOverlay {
            view: chorus,
            scene: 2,
        };
        assert_eq!(h.engine.round_trip(&h.desk, remove), OK);
        let view = h.desk.with_show(|show| show.0.views[&chorus].clone());
        assert_eq!(view.overlays, vec![1, 0]);
        assert!(view.masters.is_empty(), "{:?}", view.masters);
        assert_eq!(
            h.engine
                .round_trip(&h.desk, HostCommand::AddViewOverlay { view: 99, scene: 1 }),
            refused("View not found")
        );

        // Deleting keeps at least one view.
        let views = h.desk.with_show(|show| show.0.views.len());
        for _ in views..2 {
            let extra = HostCommand::CreateView {
                name: "Extra".to_string(),
            };
            assert!(matches!(
                h.engine.round_trip(&h.desk, extra),
                CommandOutcome::Ok { .. }
            ));
        }
        let ids: Vec<u8> = h
            .desk
            .with_show(|show| show.0.views.keys().copied().collect());
        for id in &ids[..ids.len() - 1] {
            assert_eq!(
                h.engine.round_trip(&h.remote, HostCommand::DeleteView(*id)),
                OK
            );
        }
        assert_eq!(
            h.engine
                .round_trip(&h.remote, HostCommand::DeleteView(ids[ids.len() - 1])),
            refused("At least one view must remain")
        );
        assert_eq!(
            h.engine
                .round_trip(&h.remote, HostCommand::DeleteView(ids[0])),
            refused("View not found")
        );
    }

    #[test]
    fn the_grand_master_is_set_and_clamped() {
        let mut h = TwoClients::start(small_rig);
        assert_eq!(h.remote.grand_master_percent(), 100);
        let set = HostCommand::SetGrandMaster(40);
        assert!(!set.edits_show());
        assert_eq!(h.engine.round_trip(&h.remote, set), OK);
        assert_eq!(h.desk.grand_master_percent(), 40);
        assert_eq!(
            h.engine
                .round_trip(&h.desk, HostCommand::SetGrandMaster(250)),
            OK
        );
        assert_eq!(h.remote.grand_master_percent(), 100);
    }

    fn buffer_of_remote(h: &TwoClients) -> String {
        h.remote
            .with_session(|_, me| format!("{:?}", me.control_buffer))
            .unwrap()
    }

    /// The Scene graph page focuses and enables graphs with events now; they
    /// must not clear the Programmer's attribute faders (the control buffer).
    #[test]
    fn graph_focus_and_enable_keep_the_control_buffer() {
        let mut h = TwoClients::start(idle_graph);
        assert_eq!(
            h.engine
                .round_trip(&h.desk, web(ControlEvent::SelectGroup(0))),
            OK
        );
        let hue = web(ControlEvent::SetColorHue(200));
        assert_eq!(h.engine.round_trip(&h.desk, hue), OK);
        let buffer = |h: &TwoClients| {
            h.desk
                .with_session(|_, me| format!("{:?}", me.control_buffer))
                .unwrap()
        };
        let before = buffer(&h);
        assert_ne!(
            before,
            buffer_of_remote(&h),
            "the hue is in the desk's buffer"
        );
        for event in [
            ControlEvent::SetFocusedSceneGraph(Some(0)),
            ControlEvent::SetSceneGraphEnabled(0, true),
        ] {
            assert_eq!(h.engine.round_trip(&h.desk, web(event)), OK);
        }
        assert_eq!(buffer(&h), before);
    }
}
