//! Operator sessions: selection, selection stack, control buffer, scene focus
//! and focused scene graph, one set per [`SessionId`].
//!
//! - [`SessionId::DESK`] (the native UI and every locally hosted plugin) keeps
//!   its session in the existing `blaulicht_shared::EngineState` fields, so
//!   plugins, the showfile and the UI readers are unchanged.
//! - Every other session lives in a side store in the core wrapper
//!   ([`EngineState`]), under the same lock.
//! - While the engine applies a `ControlEvent` from session S, it swaps S's
//!   fields into the `DESK` fields and back afterwards
//!   ([`EngineState::swap_session`], called from `DmxEngine::dispatch`, outside
//!   the transaction snapshot). Everything that reads or writes those fields
//!   during `apply` (selection events, `SetColor` and friends, folder
//!   selection, the control buffer reset, transaction rollback) acts on S
//!   without knowing it.
//! - Structural changes (deleting groups, fixtures, scenes or scene graphs,
//!   loading a show) repair every side-store session
//!   ([`EngineState::repair_sessions`]).
//! - Sessions are opened and closed through the hub
//!   ([`crate::hub::SystemHub::open_session`]); the engine applies those
//!   changes around each batch of bus messages.
//! - The live preview has one owner session ([`EngineState::live_owner`]);
//!   the rig's base layer is the owner's focus
//!   ([`EngineState::render_base_scene`]). Closing the owner, or deleting the
//!   scene it is focused on, ends the preview.

use std::collections::{BTreeMap, VecDeque};

use blaulicht_shared::{
    fixture::state::FixtureState,
    scene::{FixtureSelection, BLANK_SCENE_ID},
    scene_graph::GraphId,
    EngineGroups, EngineSelection,
};

use super::EngineState;
use crate::hub::SessionId;

/// One operator's state, for every session except [`SessionId::DESK`].
#[derive(Debug, Clone, Default, serde::Serialize, serde::Deserialize)]
pub struct OperatorSession {
    pub selection: EngineSelection,
    pub selection_stack: VecDeque<EngineSelection>,
    pub control_buffer: FixtureState,
    pub scene_focus: u8,
    pub focused_graph: Option<GraphId>,
}

/// The side store: every open session except [`SessionId::DESK`].
pub type SessionStore = BTreeMap<SessionId, OperatorSession>;

/// Engine-wide session bookkeeping that belongs to no single session.
#[derive(Debug, Clone, Copy)]
pub struct SessionMeta {
    /// The session whose focus the live preview renders. The shared
    /// `live_mode` flag mirrors `live_owner.is_some()`.
    pub(super) live_owner: Option<SessionId>,
    /// The session whose fields are in the `DESK` fields right now:
    /// [`SessionId::DESK`], except while `DmxEngine::dispatch` applies a
    /// message from another session (see [`EngineState::swap_session`]).
    pub(super) swapped_in: SessionId,
}

impl Default for SessionMeta {
    fn default() -> Self {
        Self {
            live_owner: None,
            swapped_in: SessionId::DESK,
        }
    }
}

impl OperatorSession {
    /// A fresh session focused on `scene_focus` and `focused_graph`, with an
    /// empty selection, selection stack and control buffer.
    pub fn new(scene_focus: u8, focused_graph: Option<GraphId>) -> Self {
        Self {
            scene_focus,
            focused_graph,
            ..Self::default()
        }
    }

    /// Swaps this session's fields with the `DESK` fields of `state`.
    fn swap_with(&mut self, state: &mut blaulicht_shared::EngineState) {
        std::mem::swap(&mut self.selection, &mut state.selection);
        std::mem::swap(&mut self.selection_stack, &mut state.selection_stack);
        std::mem::swap(&mut self.control_buffer, &mut state.control_buffer);
        std::mem::swap(&mut self.scene_focus, &mut state.current_scene_focus);
        std::mem::swap(
            &mut self.focused_graph,
            &mut state.scene_graphs.focused_graph,
        );
    }

    /// Drops what no longer exists in `state` from the selection and the
    /// stack, and moves a focus that points at a missing scene (or `BLANK`)
    /// to the first user scene, as `delete_scene` does for `DESK`. A missing
    /// focused scene graph becomes `None`.
    fn repair(&mut self, state: &blaulicht_shared::EngineState) {
        prune_selection(&mut self.selection, &state.groups);
        for selection in self.selection_stack.iter_mut() {
            prune_selection(selection, &state.groups);
        }
        if self.scene_focus == BLANK_SCENE_ID || !state.scenes.contains_key(&self.scene_focus) {
            if let Some((id, _)) = state.user_scenes().next() {
                self.scene_focus = *id;
            }
        }
        if self
            .focused_graph
            .is_some_and(|graph| !state.scene_graphs.graphs.contains_key(&graph))
        {
            self.focused_graph = None;
        }
    }
}

/// Removes deleted groups and fixtures from `selection`. If every fixture of
/// a fixture filter is gone, the group goes too: the selection must not widen
/// to the whole group.
fn prune_selection(selection: &mut EngineSelection, groups: &EngineGroups) {
    selection.group_ids.retain(|gid| groups.contains_key(gid));
    if selection.fixtures_in_group.is_empty() {
        return;
    }
    let group = match selection.group_ids.iter().next() {
        Some(gid) if selection.group_ids.len() == 1 => &groups[gid],
        _ => {
            selection.fixtures_in_group.clear();
            return;
        }
    };
    selection
        .fixtures_in_group
        .retain(|fid| group.fixtures.contains_key(fid));
    if selection.fixtures_in_group.is_empty() {
        selection.group_ids.clear();
    }
}

/// One session's state, borrowed from the engine; see [`EngineState::session`].
#[derive(Debug, Clone, Copy)]
pub struct SessionView<'a> {
    pub selection: &'a EngineSelection,
    pub selection_stack: &'a VecDeque<EngineSelection>,
    pub control_buffer: &'a FixtureState,
    pub scene_focus: u8,
    pub focused_graph: Option<GraphId>,
}

impl SessionView<'_> {
    /// The fixtures this session's selection covers: the session-aware form
    /// of [`EngineState::get_selection`] (which reads the `DESK` fields).
    pub fn fixture_selection(&self, groups: &EngineGroups) -> FixtureSelection {
        super::state::resolve_selection(self.selection, groups)
    }

    /// An owned copy, for sending it to a remote client.
    pub fn to_session(&self) -> OperatorSession {
        OperatorSession {
            selection: self.selection.clone(),
            selection_stack: self.selection_stack.clone(),
            control_buffer: self.control_buffer.clone(),
            scene_focus: self.scene_focus,
            focused_graph: self.focused_graph,
        }
    }
}

impl EngineState {
    /// Creates session `id`, focused on `DESK`'s scene and scene graph, with
    /// an empty selection. Returns `false` (and changes nothing) for
    /// [`SessionId::DESK`] or a session that is already open.
    pub fn open_session(&mut self, id: SessionId) -> bool {
        if id == SessionId::DESK || self.1.contains_key(&id) {
            return false;
        }
        let session = OperatorSession::new(
            self.0.current_scene_focus,
            self.0.scene_graphs.focused_graph,
        );
        self.1.insert(id, session);
        true
    }

    /// Drops session `id`'s state, and ends the live preview if `id` owns
    /// it. Returns `false` if it was not open. [`SessionId::DESK`] cannot be
    /// closed.
    pub fn close_session(&mut self, id: SessionId) -> bool {
        let closed = self.1.remove(&id).is_some();
        if closed && self.2.live_owner == Some(id) {
            self.set_live_owner(None);
        }
        closed
    }

    /// `true` for [`SessionId::DESK`] and every open session.
    pub fn has_session(&self, id: SessionId) -> bool {
        id == SessionId::DESK || self.1.contains_key(&id)
    }

    /// Session `id`'s state: the `DESK` fields for [`SessionId::DESK`], the
    /// side store otherwise. `None` if the session is not open. Correct while
    /// a session is swapped in, too.
    pub fn session(&self, id: SessionId) -> Option<SessionView<'_>> {
        let swapped_in = self.2.swapped_in;
        if id == swapped_in {
            return Some(SessionView {
                selection: &self.0.selection,
                selection_stack: &self.0.selection_stack,
                control_buffer: &self.0.control_buffer,
                scene_focus: self.0.current_scene_focus,
                focused_graph: self.0.scene_graphs.focused_graph,
            });
        }
        // While another session is swapped in, `DESK`'s fields sit in its slot.
        let slot = if id == SessionId::DESK {
            swapped_in
        } else {
            id
        };
        self.1.get(&slot).map(|session| SessionView {
            selection: &session.selection,
            selection_stack: &session.selection_stack,
            control_buffer: &session.control_buffer,
            scene_focus: session.scene_focus,
            focused_graph: session.focused_graph,
        })
    }

    /// The scene session `id` edits (its focus). `None` if it is not open.
    pub fn scene_focus_of(&self, id: SessionId) -> Option<u8> {
        self.session(id).map(|session| session.scene_focus)
    }

    /// A [`Self::clone_for_ui`] snapshot as session `id` sees the show: its
    /// selection, stack, control buffer, scene focus and focused graph are in
    /// the `DESK` fields, so `selection()`, `get_selection()`, `curr_scene()`
    /// and `.0.current_scene_focus` read `id`'s (its focused scene is the
    /// one kept whole). Lookups by id ([`Self::session`], `live_owner()`)
    /// stay correct. `None` if `id` is not open.
    ///
    /// For pages that draw from one snapshot per frame; like
    /// `clone_for_ui`, it must not be mutated.
    pub fn clone_for_session(&self, id: SessionId) -> Option<Self> {
        let focus = self.scene_focus_of(id)?;
        let mut snapshot = self.clone_keeping_scene(focus);
        let swapped_in = snapshot.2.swapped_in;
        if id != swapped_in {
            // Back to `DESK` first (a no-op outside `dispatch`), then `id`.
            snapshot.swap_session(swapped_in);
            snapshot.swap_session(id);
        }
        Some(snapshot)
    }

    /// Every open session except [`SessionId::DESK`].
    pub fn remote_sessions(&self) -> impl Iterator<Item = (SessionId, &OperatorSession)> {
        self.1.iter().map(|(id, session)| (*id, session))
    }

    pub(super) fn remote_sessions_mut(&mut self) -> impl Iterator<Item = &mut OperatorSession> {
        self.1.values_mut()
    }

    /// Swaps session `id`'s state into the `DESK` fields of `self.0` (and
    /// `DESK`'s into the store). Calling it again swaps them back; every
    /// caller must, before it releases the lock. A no-op for
    /// [`SessionId::DESK`]. Returns `false` (and changes nothing) if `id` is
    /// not open.
    pub(super) fn swap_session(&mut self, id: SessionId) -> bool {
        if id == SessionId::DESK {
            return true;
        }
        let EngineState(state, sessions, meta, _) = self;
        match sessions.get_mut(&id) {
            Some(session) => {
                session.swap_with(state);
                debug_assert!(
                    meta.swapped_in == SessionId::DESK || meta.swapped_in == id,
                    "{id} swapped in while {} is",
                    meta.swapped_in
                );
                meta.swapped_in = if meta.swapped_in == id {
                    SessionId::DESK
                } else {
                    id
                };
                true
            }
            None => false,
        }
    }

    /// The session whose fields are in the `DESK` fields right now: the
    /// sender of the message being applied, `DESK` outside of
    /// `DmxEngine::dispatch`.
    pub(super) fn applying_session(&self) -> SessionId {
        self.2.swapped_in
    }

    /// The session whose focus the live preview renders, if any.
    pub fn live_owner(&self) -> Option<SessionId> {
        self.2.live_owner
    }

    //
    // A remote client's copy (`client::RemoteClient`): the engine sends the
    // shared state, the client's own session and the stage separately, and
    // the client keeps them in an `EngineState` so frontends read it as they
    // read a local engine.
    //

    /// Replaces the shared state (`.0`) with the one of a show snapshot.
    pub fn set_mirrored_show(&mut self, state: blaulicht_shared::EngineState) {
        self.0 = state;
    }

    /// Sets the one session a remote client's copy holds (its own) and the
    /// live preview's owner.
    pub fn set_mirrored_session(
        &mut self,
        id: SessionId,
        session: OperatorSession,
        live_owner: Option<SessionId>,
    ) {
        self.1.clear();
        if id != SessionId::DESK {
            self.1.insert(id, session);
        }
        self.2.live_owner = live_owner;
    }

    /// Makes `owner` the live preview's owner (`None` ends the preview) and
    /// mirrors it into the shared `live_mode` flag. Returns `false` if
    /// nothing changed.
    pub(super) fn set_live_owner(&mut self, owner: Option<SessionId>) -> bool {
        if self.2.live_owner == owner {
            return false;
        }
        self.2.live_owner = owner;
        self.0.live_mode = owner.is_some();
        true
    }

    /// The scene the renderer uses as its base layer: the live preview
    /// owner's focus, or `BLANK`. Wraps
    /// `blaulicht_shared::EngineState::render_base_scene`; nothing else may
    /// recompute the base layer.
    pub fn render_base_scene(&self) -> u8 {
        let live_focus = self
            .live_owner()
            .and_then(|owner| self.scene_focus_of(owner))
            .unwrap_or(BLANK_SCENE_ID);
        self.0.render_base_scene(live_focus)
    }

    /// Repairs every side-store session after a structural change: deleted
    /// groups and fixtures leave the selections and stacks, and a focus on a
    /// deleted scene or scene graph moves on (see [`OperatorSession`]'s
    /// `repair`). `DESK` is repaired by the structural change itself, as
    /// before.
    pub fn repair_sessions(&mut self) {
        let EngineState(state, sessions, _, _) = self;
        for session in sessions.values_mut() {
            session.repair(state);
        }
    }

    /// Resets every side-store session as if it had just been opened, after
    /// a new show was loaded. The sessions stay open.
    pub(super) fn reset_sessions(&mut self) {
        let EngineState(state, sessions, _, _) = self;
        for session in sessions.values_mut() {
            *session =
                OperatorSession::new(state.current_scene_focus, state.scene_graphs.focused_graph);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use blaulicht_shared::{
        fixture::{
            light::Light,
            state::{Fixture, FixtureGroup},
            FixtureType,
        },
        scene_graph::SceneGraph,
    };
    use std::collections::{BTreeMap, HashSet};

    const REMOTE: SessionId = SessionId(1);

    fn group(fixtures: u8) -> FixtureGroup {
        FixtureGroup {
            fixtures: (0..fixtures)
                .map(|fid| {
                    (
                        fid,
                        Fixture::new(
                            0,
                            1 + 3 * fid as usize,
                            format!("F{fid}"),
                            FixtureType::from(Light::Generic3ChanNoAlpha),
                        ),
                    )
                })
                .collect(),
            ..FixtureGroup::default()
        }
    }

    fn selection(groups: &[u8], fixtures: &[u8]) -> EngineSelection {
        EngineSelection {
            group_ids: groups.iter().copied().collect(),
            fixtures_in_group: fixtures.iter().copied().collect(),
        }
    }

    fn engine() -> EngineState {
        let mut engine = EngineState::default();
        engine.0.groups = BTreeMap::from([(0, group(2)), (1, group(2))]);
        engine.0.new_scene("Second".to_string());
        engine.0.ensure_blank_scene();
        engine
    }

    #[test]
    fn a_new_session_starts_on_the_desk_focus_with_an_empty_selection() {
        let mut engine = engine();
        engine.0.current_scene_focus = 1;
        engine
            .0
            .scene_graphs
            .graphs
            .insert(4, SceneGraph::default());
        engine.0.scene_graphs.focused_graph = Some(4);
        engine.0.selection = selection(&[0], &[]);

        assert!(engine.open_session(REMOTE));
        assert!(!engine.open_session(REMOTE));
        assert!(!engine.open_session(SessionId::DESK));

        let session = engine.session(REMOTE).unwrap();
        assert!(session.selection.is_empty());
        assert!(session.selection_stack.is_empty());
        assert_eq!(session.scene_focus, 1);
        assert_eq!(session.focused_graph, Some(4));
        assert_eq!(engine.scene_focus_of(SessionId::DESK), Some(1));

        assert!(engine.close_session(REMOTE));
        assert!(!engine.has_session(REMOTE));
        assert!(engine.session(REMOTE).is_none());
        assert!(engine.has_session(SessionId::DESK));
    }

    #[test]
    fn swapping_twice_restores_both_sessions() {
        let mut engine = engine();
        engine.0.selection = selection(&[0], &[]);
        engine.open_session(REMOTE);
        engine.1.get_mut(&REMOTE).unwrap().selection = selection(&[1], &[1]);
        engine.1.get_mut(&REMOTE).unwrap().scene_focus = 1;

        assert!(engine.swap_session(REMOTE));
        assert_eq!(engine.0.selection.group_ids, HashSet::from([1]));
        assert_eq!(engine.0.current_scene_focus, 1);
        // Lookups by id still find each session while it is swapped.
        assert_eq!(engine.applying_session(), REMOTE);
        assert_eq!(engine.scene_focus_of(REMOTE), Some(1));
        assert_eq!(engine.scene_focus_of(SessionId::DESK), Some(0));
        assert_eq!(
            engine.session(SessionId::DESK).unwrap().selection.group_ids,
            HashSet::from([0])
        );
        assert!(engine.swap_session(REMOTE));
        assert_eq!(engine.applying_session(), SessionId::DESK);
        assert_eq!(engine.0.selection.group_ids, HashSet::from([0]));
        assert_eq!(engine.0.current_scene_focus, 0);
        assert_eq!(engine.scene_focus_of(REMOTE), Some(1));

        assert!(!engine.swap_session(SessionId(9)));
    }

    #[test]
    fn repair_prunes_deleted_groups_and_fixtures_without_widening() {
        let mut engine = engine();
        engine.open_session(REMOTE);
        {
            let session = engine.1.get_mut(&REMOTE).unwrap();
            session.selection = selection(&[1], &[1]);
            session.selection_stack = VecDeque::from([
                selection(&[0, 1], &[]),
                selection(&[0], &[0, 1]),
                selection(&[1], &[]),
            ]);
        }
        // Fixture 1 of group 1 goes, then group 0.
        engine.0.groups.get_mut(&1).unwrap().fixtures.remove(&1);
        engine.0.groups.remove(&0);
        engine.repair_sessions();

        let session = engine.session(REMOTE).unwrap();
        // The filter lost its only fixture: the group must not widen to all
        // of group 1.
        assert!(session.selection.is_empty());
        assert!(session.selection.fixtures_in_group.is_empty());
        let stack: Vec<_> = session
            .selection_stack
            .iter()
            .map(|s| (s.group_ids.clone(), s.fixtures_in_group.clone()))
            .collect();
        assert_eq!(
            stack,
            vec![
                (HashSet::from([1]), HashSet::new()),
                (HashSet::new(), HashSet::new()),
                (HashSet::from([1]), HashSet::new()),
            ]
        );
    }

    #[test]
    fn repair_moves_focus_off_deleted_scenes_and_graphs() {
        let mut engine = engine();
        engine
            .0
            .scene_graphs
            .graphs
            .insert(2, SceneGraph::default());
        engine.open_session(REMOTE);
        {
            let session = engine.1.get_mut(&REMOTE).unwrap();
            session.scene_focus = 1;
            session.focused_graph = Some(2);
        }
        engine.0.scenes.remove(&1);
        engine.0.scene_graphs.graphs.remove(&2);
        engine.repair_sessions();

        let session = engine.session(REMOTE).unwrap();
        assert_eq!(session.scene_focus, 0);
        assert_eq!(session.focused_graph, None);

        // A focus on BLANK is never valid either.
        engine.1.get_mut(&REMOTE).unwrap().scene_focus = BLANK_SCENE_ID;
        engine.repair_sessions();
        assert_eq!(engine.scene_focus_of(REMOTE), Some(0));
    }
}

/// Sessions through the real `DmxEngine`: every event applies to its
/// sender's session only.
#[cfg(test)]
mod engine_tests {
    use super::super::{DmxEngine, EngineState};
    use crate::{
        event::SystemEventBus,
        frontend_ui::FrontendUiMirror,
        hub::{
            BusMessage, ClientConnection, ClientId, CommandOutcome, CommandResult, SessionId,
            SystemHub,
        },
        msg::SystemMessage,
        state::AppState,
    };
    use blaulicht_shared::{
        fixture::{
            light::Light,
            state::{Fixture, FixtureGroup, FixtureState},
            value::FixtureValue,
            FixtureType,
        },
        folder::{FolderItem, FolderKind},
        scene::{EngineSink, BLANK_SCENE_ID},
        ControlEvent, ControlEventMessage, EventOriginator,
    };
    use std::{
        cell::Cell,
        collections::HashSet,
        sync::{Arc, RwLockWriteGuard},
        time::{Duration, Instant},
    };

    const DESK: SessionId = SessionId::DESK;
    const REMOTE: SessionId = SessionId(1);
    const RED: (u8, u8, u8) = (255, 0, 0);
    const BLUE: (u8, u8, u8) = (0, 0, 255);

    struct Harness {
        engine: DmxEngine,
        app: Arc<AppState>,
        seq: Cell<u64>,
        /// Not running; kept for the engine's rollback sends.
        _bus: SystemEventBus<BusMessage>,
    }

    impl Harness {
        /// Groups 0..3 with two fixtures each, user scenes 0, 1 and 2, and
        /// the session `REMOTE` open.
        fn new() -> Self {
            let app = Arc::new(AppState::new(&[]));
            let bus = SystemEventBus::<BusMessage>::new();
            let engine = DmxEngine::new(
                Arc::clone(&app),
                bus.new_connection(),
                SystemHub::new(),
                [String::new(), String::new()],
            );
            {
                let mut state = app.dmx_engine.write().unwrap();
                for gid in 0..3u8 {
                    let fixtures = (0..2u8)
                        .map(|fid| {
                            let addr = 1 + 3 * (gid as usize * 2 + fid as usize);
                            let light = FixtureType::from(Light::Generic3ChanNoAlpha);
                            (fid, Fixture::new(0, addr, format!("F{addr}"), light))
                        })
                        .collect();
                    state.0.groups.insert(
                        gid,
                        FixtureGroup {
                            name: format!("G{gid}"),
                            fixtures,
                            folder: None,
                        },
                    );
                }
                let sink = EngineSink::from_groups(&state.0.groups);
                state.0.scenes.get_mut(&0).unwrap().sink = sink;
                state.0.new_scene("A".to_string());
                state.0.new_scene("B".to_string());
                state.0.ensure_blank_scene();
                assert!(state.open_session(REMOTE));
            }
            Self {
                engine,
                app,
                seq: Cell::new(0),
                _bus: bus,
            }
        }

        fn state(&self) -> RwLockWriteGuard<'_, EngineState> {
            self.app.dmx_engine.write().unwrap()
        }

        fn send(&self, session: SessionId, event: ControlEvent) -> CommandOutcome {
            let client = match session {
                SessionId::DESK => ClientId::Desk,
                SessionId(id) => ClientId::Remote(id),
            };
            self.seq.set(self.seq.get() + 1);
            let message = BusMessage::control(
                client,
                session,
                self.seq.get(),
                ControlEventMessage::new(EventOriginator::Web, event),
            );
            let mut state = self.state();
            self.engine
                .dispatch(&mut state, message)
                .expect("a reply")
                .outcome
        }

        fn ok(&self, session: SessionId, event: ControlEvent) {
            let outcome = self.send(session, event.clone());
            assert_eq!(
                outcome,
                CommandOutcome::Ok { created: None },
                "{event:?} from {session}"
            );
        }

        fn groups(&self, session: SessionId) -> HashSet<u8> {
            self.state()
                .session(session)
                .unwrap()
                .selection
                .group_ids
                .clone()
        }

        fn fixtures(&self, session: SessionId) -> HashSet<u8> {
            self.state()
                .session(session)
                .unwrap()
                .selection
                .fixtures_in_group
                .clone()
        }

        fn stack(&self, session: SessionId) -> Vec<HashSet<u8>> {
            self.state()
                .session(session)
                .unwrap()
                .selection_stack
                .iter()
                .map(|selection| selection.group_ids.clone())
                .collect()
        }

        fn focus(&self, session: SessionId) -> u8 {
            self.state().scene_focus_of(session).unwrap()
        }

        fn fixture(&self, scene: u8, gid: u8, fid: u8) -> FixtureState {
            self.state().0.scenes[&scene].sink.fixture_states[&(gid, fid)].clone()
        }

        fn control_buffer(&self, session: SessionId) -> FixtureState {
            self.state()
                .session(session)
                .unwrap()
                .control_buffer
                .clone()
        }
    }

    fn color(state: &FixtureState) -> (FixtureValue, FixtureValue, FixtureValue) {
        (state.color_h, state.color_s, state.color_v)
    }

    fn colored(rgb: (u8, u8, u8)) -> (FixtureValue, FixtureValue, FixtureValue) {
        let mut state = FixtureState::default();
        state.apply(ControlEvent::SetColor(rgb));
        color(&state)
    }

    #[test]
    fn two_sessions_set_color_on_their_own_selection_only() {
        let h = Harness::new();
        h.ok(DESK, ControlEvent::SelectGroup(0));
        h.ok(REMOTE, ControlEvent::SelectGroup(1));
        h.ok(DESK, ControlEvent::SetColor(RED));
        h.ok(REMOTE, ControlEvent::SetColor(BLUE));

        // Both edit scene 0 (a new session starts on the desk's focus).
        for fid in 0..2 {
            assert_eq!(color(&h.fixture(0, 0, fid)), colored(RED));
            assert_eq!(color(&h.fixture(0, 1, fid)), colored(BLUE));
            assert_eq!(
                color(&h.fixture(0, 2, fid)),
                color(&FixtureState::default())
            );
        }
        assert_eq!(h.groups(DESK), HashSet::from([0]));
        assert_eq!(h.groups(REMOTE), HashSet::from([1]));
        assert_eq!(color(&h.control_buffer(DESK)), colored(RED));
        assert_eq!(color(&h.control_buffer(REMOTE)), colored(BLUE));

        // A selection event resets only its sender's control buffer.
        h.ok(REMOTE, ControlEvent::SelectGroup(2));
        assert_eq!(
            color(&h.control_buffer(REMOTE)),
            color(&FixtureState::default())
        );
        assert_eq!(color(&h.control_buffer(DESK)), colored(RED));

        // An empty selection is refused for its own session only.
        h.ok(REMOTE, ControlEvent::RemoveAllSelection);
        assert_eq!(
            h.send(REMOTE, ControlEvent::SetColor(BLUE)),
            CommandOutcome::Refused("No selected object(s)".to_string())
        );
        h.ok(DESK, ControlEvent::SetColor(BLUE));
        assert_eq!(color(&h.fixture(0, 0, 0)), colored(BLUE));
    }

    #[test]
    fn selection_stack_focus_and_folder_selection_are_per_session() {
        let h = Harness::new();
        h.ok(DESK, ControlEvent::SelectGroup(0));
        h.ok(DESK, ControlEvent::PushSelection);
        h.ok(DESK, ControlEvent::SelectGroup(1));
        h.ok(DESK, ControlEvent::SetSceneFocus(1));
        h.ok(REMOTE, ControlEvent::SelectGroup(2));
        h.ok(REMOTE, ControlEvent::PushSelection);
        h.ok(REMOTE, ControlEvent::SetSceneFocus(2));

        assert_eq!(h.groups(DESK), HashSet::from([1]));
        assert_eq!(h.stack(DESK), vec![HashSet::from([0])]);
        assert_eq!(h.focus(DESK), 1);
        assert!(h.groups(REMOTE).is_empty());
        assert_eq!(h.stack(REMOTE), vec![HashSet::from([2])]);
        assert_eq!(h.focus(REMOTE), 2);
        // The desk's state stays in the shared fields plugins and the
        // showfile read.
        assert_eq!(h.state().0.current_scene_focus, 1);
        assert_eq!(h.state().0.selection.group_ids, HashSet::from([1]));

        h.ok(REMOTE, ControlEvent::PopSelection);
        assert_eq!(h.groups(REMOTE), HashSet::from([2]));
        assert_eq!(h.stack(DESK), vec![HashSet::from([0])]);
        assert_eq!(
            h.send(REMOTE, ControlEvent::PopSelection),
            CommandOutcome::Refused("Selection stack empty".to_string())
        );

        // Each session writes into its own focused scene.
        h.ok(REMOTE, ControlEvent::SetAlpha(7));
        assert_eq!(h.fixture(2, 2, 0).alpha, FixtureValue::literal_u8(7));
        assert_eq!(h.fixture(1, 2, 0).alpha, FixtureState::default().alpha);
        h.ok(DESK, ControlEvent::SetAlpha(9));
        assert_eq!(h.fixture(1, 1, 0).alpha, FixtureValue::literal_u8(9));
        assert_eq!(h.fixture(2, 1, 0).alpha, FixtureState::default().alpha);

        // Folder selection (`blaulicht_shared::folder`) adds to the
        // sender's selection.
        h.ok(
            DESK,
            ControlEvent::CreateFolder {
                id: 0,
                kind: FolderKind::Groups,
                parent: None,
                name: "Folder".to_string(),
            },
        );
        h.ok(
            DESK,
            ControlEvent::MoveToFolder {
                item: FolderItem::Group(0),
                folder: Some(0),
            },
        );
        h.ok(REMOTE, ControlEvent::SelectFolder(0));
        assert_eq!(h.groups(REMOTE), HashSet::from([0, 2]));
        assert_eq!(h.groups(DESK), HashSet::from([1]));

        // The focused scene graph too.
        h.state()
            .0
            .scene_graphs
            .graphs
            .insert(3, Default::default());
        h.ok(REMOTE, ControlEvent::SetFocusedSceneGraph(Some(3)));
        assert_eq!(h.state().session(REMOTE).unwrap().focused_graph, Some(3));
        assert_eq!(h.state().0.scene_graphs.focused_graph, None);
    }

    #[test]
    fn deleting_groups_and_fixtures_prunes_remote_selections() {
        let h = Harness::new();
        h.ok(DESK, ControlEvent::SelectGroup(2));
        h.ok(REMOTE, ControlEvent::SelectGroup(1));
        h.ok(REMOTE, ControlEvent::PushSelection);
        h.ok(REMOTE, ControlEvent::SelectGroup(1));
        h.ok(REMOTE, ControlEvent::SelectGroup(2));

        h.state().delete_group(1);
        assert_eq!(h.groups(REMOTE), HashSet::from([2]));
        assert_eq!(h.stack(REMOTE), vec![HashSet::new()]);
        // The desk is cleared, as before.
        assert!(h.groups(DESK).is_empty());

        // A fixture filter loses the deleted fixture.
        h.ok(REMOTE, ControlEvent::DeSelectGroup(2));
        h.ok(REMOTE, ControlEvent::SelectGroup(0));
        h.ok(
            REMOTE,
            ControlEvent::LimitSelectionToFixtureInCurrentGroup(0),
        );
        h.ok(
            REMOTE,
            ControlEvent::LimitSelectionToFixtureInCurrentGroup(1),
        );
        assert!(h.state().delete_fixture_from_group(0, 0));
        assert_eq!(h.groups(REMOTE), HashSet::from([0]));
        assert_eq!(h.fixtures(REMOTE), HashSet::from([1]));

        // The remote session still works on what is left.
        h.ok(REMOTE, ControlEvent::SetAlpha(5));
        assert_eq!(h.fixture(0, 0, 1).alpha, FixtureValue::literal_u8(5));
    }

    #[test]
    fn deleting_a_scene_repairs_remote_focus() {
        let h = Harness::new();
        h.ok(DESK, ControlEvent::SetSceneFocus(1));
        h.ok(REMOTE, ControlEvent::SetSceneFocus(2));

        assert!(h.state().delete_scene(2));
        assert_eq!(h.focus(REMOTE), 0);
        assert_eq!(h.focus(DESK), 1);

        // Editing goes to the repaired focus instead of a missing scene.
        h.ok(REMOTE, ControlEvent::SelectGroup(0));
        h.ok(REMOTE, ControlEvent::SetAlpha(3));
        assert_eq!(h.fixture(0, 0, 0).alpha, FixtureValue::literal_u8(3));
    }

    #[test]
    fn a_remote_transaction_rolls_back_only_its_own_session() {
        let h = Harness::new();
        h.ok(DESK, ControlEvent::SelectGroup(0));
        h.ok(DESK, ControlEvent::SetSceneFocus(1));
        h.ok(REMOTE, ControlEvent::SelectGroup(1));

        let refused = h.send(
            REMOTE,
            ControlEvent::Transaction(vec![
                ControlEvent::SelectGroup(2),
                ControlEvent::SetSceneFocus(2),
                ControlEvent::SetAlpha(50),
                ControlEvent::SelectGroup(99),
            ]),
        );
        assert_eq!(
            refused,
            CommandOutcome::Refused("Illegal group".to_string())
        );
        assert_eq!(h.groups(REMOTE), HashSet::from([1]));
        assert_eq!(h.focus(REMOTE), 0);
        assert_eq!(h.fixture(2, 1, 0).alpha, FixtureState::default().alpha);
        assert_eq!(h.groups(DESK), HashSet::from([0]));
        assert_eq!(h.focus(DESK), 1);

        // A transaction that succeeds applies to the remote session only.
        h.ok(
            REMOTE,
            ControlEvent::Transaction(vec![
                ControlEvent::SelectGroup(2),
                ControlEvent::SetAlpha(50),
            ]),
        );
        assert_eq!(h.groups(REMOTE), HashSet::from([1, 2]));
        assert_eq!(h.fixture(0, 2, 0).alpha, FixtureValue::literal_u8(50));
        assert_eq!(h.fixture(0, 0, 0).alpha, FixtureState::default().alpha);
        assert_eq!(h.groups(DESK), HashSet::from([0]));
    }

    #[test]
    fn patch_moves_and_swaps_carry_remote_selections() {
        let h = Harness::new();
        h.ok(REMOTE, ControlEvent::SelectGroup(0));
        h.ok(
            REMOTE,
            ControlEvent::LimitSelectionToFixtureInCurrentGroup(1),
        );

        assert!(h.state().swap_fixtures_in_group(0, 0, 1));
        assert_eq!(h.fixtures(REMOTE), HashSet::from([0]));

        assert_eq!(h.state().move_fixture_to_group(0, 0, 2), Some(2));
        assert_eq!(h.groups(REMOTE), HashSet::from([2]));
        assert_eq!(h.fixtures(REMOTE), HashSet::from([2]));
    }

    #[test]
    fn loading_a_show_resets_remote_sessions() {
        let h = Harness::new();
        h.ok(REMOTE, ControlEvent::SelectGroup(1));
        h.ok(REMOTE, ControlEvent::PushSelection);
        h.ok(REMOTE, ControlEvent::SetSceneFocus(2));

        let show = h.state().0.clone();
        h.state().load_showfile(show, false);
        let state = h.state();
        let session = state.session(REMOTE).expect("still open");
        assert!(session.selection.is_empty());
        assert!(session.selection_stack.is_empty());
        assert_eq!(session.scene_focus, state.0.current_scene_focus);
    }

    #[test]
    fn messages_from_a_session_that_is_not_open_are_refused() {
        let h = Harness::new();
        assert_eq!(
            h.send(SessionId(42), ControlEvent::SelectGroup(0)),
            CommandOutcome::Refused("Unknown session".to_string())
        );
        assert!(h.groups(DESK).is_empty());
        assert!(h.groups(REMOTE).is_empty());
    }

    /// The first fixture of group 0 (DMX channels 1..=3) as the renderer
    /// wrote it.
    fn rendered_rgb(h: &mut Harness) -> [u8; 3] {
        h.engine.render_universes();
        let buffer = h.app.dmx_universes[0].read().unwrap();
        [
            buffer.dmx_buffer[1],
            buffer.dmx_buffer[2],
            buffer.dmx_buffer[3],
        ]
    }

    /// Desk focus 0 is blue, `REMOTE`'s focus 2 is red, and nothing plays.
    fn preview_harness() -> Harness {
        let h = Harness::new();
        h.ok(DESK, ControlEvent::SelectGroup(0));
        h.ok(DESK, ControlEvent::SetColor(BLUE));
        h.ok(DESK, ControlEvent::SetAlpha(255));
        h.ok(REMOTE, ControlEvent::SetSceneFocus(2));
        h.ok(REMOTE, ControlEvent::SelectGroup(0));
        h.ok(REMOTE, ControlEvent::SetColor(RED));
        h.ok(REMOTE, ControlEvent::SetAlpha(255));
        h.state().0.current_overlay_scenes.clear();
        h
    }

    #[test]
    fn the_live_preview_renders_its_owners_focus() {
        let mut h = preview_harness();
        let dark = rendered_rgb(&mut h);
        assert_eq!(h.state().render_base_scene(), BLANK_SCENE_ID);

        // One operator, as before: the desk's focus takes the base slot.
        h.ok(DESK, ControlEvent::ToggleLiveMode);
        assert_eq!(h.state().live_owner(), Some(DESK));
        assert_eq!(h.state().render_base_scene(), 0);
        let blue = rendered_rgb(&mut h);
        assert!(blue[2] > blue[0], "{blue:?}");
        h.ok(DESK, ControlEvent::ToggleLiveMode);
        assert_eq!(h.state().live_owner(), None);
        assert!(!h.state().0.live_mode);
        assert_eq!(rendered_rgb(&mut h), dark);

        // A remote session previews its own focus, not the desk's.
        h.ok(REMOTE, ControlEvent::SetLiveMode(true));
        {
            let state = h.state();
            assert_eq!(state.live_owner(), Some(REMOTE));
            assert_eq!(state.render_base_scene(), 2);
            // Plugins see `live_mode` together with the desk's focus.
            assert!(state.0.live_mode);
            assert_eq!(state.0.current_scene_focus, 0);
        }
        let red = rendered_rgb(&mut h);
        assert!(red[0] > red[2], "{red:?}");

        // The preview follows its owner's focus.
        h.ok(REMOTE, ControlEvent::SetSceneFocus(0));
        assert_eq!(rendered_rgb(&mut h), blue);
        h.ok(REMOTE, ControlEvent::SetSceneFocus(2));

        // Toggling from another session takes the preview over; toggling
        // again as the owner ends it.
        h.ok(DESK, ControlEvent::ToggleLiveMode);
        assert_eq!(h.state().live_owner(), Some(DESK));
        assert_eq!(rendered_rgb(&mut h), blue);
        h.ok(REMOTE, ControlEvent::ToggleLiveMode);
        assert_eq!(h.state().live_owner(), Some(REMOTE));
        assert_eq!(rendered_rgb(&mut h), red);
        h.ok(REMOTE, ControlEvent::SetLiveMode(true));
        assert_eq!(h.state().live_owner(), Some(REMOTE));
        h.ok(REMOTE, ControlEvent::ToggleLiveMode);
        assert_eq!(h.state().live_owner(), None);

        // Turning it off ends it, whoever owns it.
        h.ok(REMOTE, ControlEvent::SetLiveMode(true));
        h.ok(DESK, ControlEvent::SetLiveMode(false));
        assert_eq!(h.state().live_owner(), None);
        assert!(!h.state().0.live_mode);
        assert_eq!(rendered_rgb(&mut h), dark);
    }

    #[test]
    fn closing_the_owner_session_ends_the_preview() {
        let mut h = preview_harness();
        let dark = rendered_rgb(&mut h);
        h.ok(REMOTE, ControlEvent::SetLiveMode(true));
        assert_ne!(rendered_rgb(&mut h), dark);

        assert!(h.state().close_session(REMOTE));
        assert_eq!(h.state().live_owner(), None);
        assert!(!h.state().0.live_mode);
        assert_eq!(rendered_rgb(&mut h), dark);

        // Closing a session that does not own the preview leaves it alone.
        assert!(h.state().open_session(SessionId(2)));
        h.ok(DESK, ControlEvent::SetLiveMode(true));
        assert!(h.state().close_session(SessionId(2)));
        assert_eq!(h.state().live_owner(), Some(DESK));
    }

    #[test]
    fn deleting_the_owners_focused_scene_ends_the_preview() {
        let mut h = preview_harness();
        let dark = rendered_rgb(&mut h);
        h.ok(REMOTE, ControlEvent::SetLiveMode(true));

        // Another scene goes: the preview stays.
        assert!(h.state().delete_scene(1));
        assert_eq!(h.state().live_owner(), Some(REMOTE));
        assert_eq!(h.state().render_base_scene(), 2);

        // The previewed scene goes: the preview ends instead of jumping to
        // wherever the owner's focus is repaired to.
        assert!(h.state().delete_scene(2));
        assert_eq!(h.state().live_owner(), None);
        assert!(!h.state().0.live_mode);
        assert_eq!(h.focus(REMOTE), 0);
        assert_eq!(rendered_rgb(&mut h), dark);

        // The same for the desk.
        h.ok(DESK, ControlEvent::SetLiveMode(true));
        h.state().0.new_scene("C".to_string());
        assert!(h.state().delete_scene(0));
        assert_eq!(h.state().live_owner(), None);
    }

    #[test]
    fn a_failed_transaction_restores_the_preview_owner() {
        let h = preview_harness();
        h.ok(DESK, ControlEvent::SetLiveMode(true));
        let refused = h.send(
            REMOTE,
            ControlEvent::Transaction(vec![
                ControlEvent::SetLiveMode(true),
                ControlEvent::SelectGroup(99),
            ]),
        );
        assert_eq!(
            refused,
            CommandOutcome::Refused("Illegal group".to_string())
        );
        assert_eq!(h.state().live_owner(), Some(DESK));
        assert!(h.state().0.live_mode);
    }

    #[test]
    fn loading_a_show_ends_the_preview() {
        let h = preview_harness();
        h.ok(REMOTE, ControlEvent::SetLiveMode(true));
        let show = h.state().0.clone();
        h.state().load_showfile(show, false);
        assert_eq!(h.state().live_owner(), None);
        assert!(!h.state().0.live_mode);
    }

    fn results(client: &ClientConnection) -> Vec<CommandResult> {
        std::iter::from_fn(|| client.try_recv_system())
            .filter_map(|message| match message {
                SystemMessage::CommandResult(result) => Some(result),
                _ => None,
            })
            .collect()
    }

    #[test]
    fn sessions_open_and_close_through_the_hub() {
        let app = Arc::new(AppState::new(&[]));
        app.dmx_engine.write().unwrap().0.new_scene("A".to_string());
        app.dmx_engine
            .write()
            .unwrap()
            .0
            .groups
            .insert(0, Default::default());
        let hub = SystemHub::new();
        let mut bus = SystemEventBus::<BusMessage>::new();
        let handle = bus.handle();
        let mut engine = DmxEngine::new(
            Arc::clone(&app),
            handle.connect(),
            hub.clone(),
            [String::new(), String::new()],
        );
        let mirror = FrontendUiMirror::new();
        std::thread::spawn(move || bus.run());
        app.dmx_engine.write().unwrap().0.current_scene_focus = 1;

        // A remote client in a session of its own, used right away.
        let remote =
            ClientConnection::connect_new_session(hub.allocate_client(), &hub, &handle, &mirror);
        let session = remote.session();
        assert_ne!(session, SessionId::DESK);
        let seq = remote.send(ControlEventMessage::new(
            EventOriginator::Web,
            ControlEvent::SelectGroup(0),
        ));
        let deadline = Instant::now() + Duration::from_secs(5);
        let reply = loop {
            assert!(Instant::now() < deadline, "no reply");
            engine.apply_bus_messages();
            if let Some(reply) = results(&remote).pop() {
                break reply;
            }
            std::thread::sleep(Duration::from_millis(1));
        };
        assert_eq!(reply.seq, seq);
        assert_eq!(reply.outcome, CommandOutcome::Ok { created: None });
        {
            let state = app.dmx_engine.read().unwrap();
            let view = state.session(session).expect("opened by the engine");
            assert_eq!(view.selection.group_ids, HashSet::from([0]));
            assert_eq!(view.scene_focus, 1);
            assert!(state.0.selection.is_empty());
        }

        // The remote client starts a live preview ...
        let seq = remote.send(ControlEventMessage::new(
            EventOriginator::Web,
            ControlEvent::SetLiveMode(true),
        ));
        let reply = loop {
            assert!(Instant::now() < deadline, "no reply");
            engine.apply_bus_messages();
            if let Some(reply) = results(&remote).pop() {
                break reply;
            }
            std::thread::sleep(Duration::from_millis(1));
        };
        assert_eq!(reply.seq, seq);
        assert_eq!(app.dmx_engine.read().unwrap().live_owner(), Some(session));

        // ... and dropping the client closes its session, which ends it.
        drop(remote);
        engine.apply_bus_messages();
        {
            let state = app.dmx_engine.read().unwrap();
            assert!(!state.has_session(session));
            assert_eq!(state.live_owner(), None);
            assert!(!state.0.live_mode);
        }

        // Ids are never reused.
        let next = hub.open_session();
        assert!(next.id() > session);
    }
}
