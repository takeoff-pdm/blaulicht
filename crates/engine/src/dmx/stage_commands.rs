//! The stage as show state: the visualizer's room, objects, trusses and
//! fixture attachments live next to the engine state (`EngineState::stage`),
//! are edited by host commands (see [`HostCommand`]) and saved in the
//! showfile's `stage` section.
//!
//! The engine places fixtures attached to trusses ([`EngineState::reconcile_stage`])
//! whenever the stage or the fixture patch changes, instead of a frontend
//! doing it every frame.
//!
//! [`HostCommand`]: crate::hub::HostCommand

use std::{
    collections::BTreeMap,
    path::{Path, PathBuf},
    sync::Arc,
};

use super::EngineState;
use crate::{
    hub::{CommandOutcome, CreatedId},
    stage::{
        geometry::attachment_pose, FixturePlacement, StageObject, StageObjectKind, StagePatch,
        StageScene,
    },
};

fn ok(created: Option<CreatedId>) -> CommandOutcome {
    CommandOutcome::Ok { created }
}

fn refused(reason: impl Into<String>) -> CommandOutcome {
    CommandOutcome::Refused(reason.into())
}

type Pose = ([f32; 3], [f32; 3]);

/// The stage and its engine-side bookkeeping (`EngineState`'s fourth field).
#[derive(Debug, Clone, Default)]
pub struct StageState {
    /// Shared, so the per-frame UI snapshots (`clone_for_ui`) copy a pointer.
    scene: Arc<StageScene>,
    /// Counts changes, so a frontend can tell cheaply whether its copy is
    /// current.
    revision: u64,
    /// The open showfile, `None` for a show that was never saved. The
    /// engine's only copy: the stage's model assets sit next to it, and the
    /// persistence service saves to it and writes it into the config's
    /// `last_open_showfile`.
    showfile: Option<PathBuf>,
    /// Where the engine last put each attached fixture. A fixture found
    /// anywhere else was moved by something else (Setup, a hand placement),
    /// which takes it off its truss. Not saved.
    placed: BTreeMap<(u8, u8), Pose>,
}

impl EngineState {
    /// The visualizer's stage.
    pub fn stage(&self) -> &StageScene {
        &self.3.scene
    }

    /// Changes whenever the stage does.
    pub fn stage_revision(&self) -> u64 {
        self.3.revision
    }

    /// The open showfile, if the show has one. The stage's model assets are
    /// stored next to it.
    pub fn showfile(&self) -> Option<&Path> {
        self.3.showfile.as_deref()
    }

    /// Loads the stage of a showfile at `showfile` (`None` for a new show)
    /// and places the attached fixtures on their trusses.
    pub fn load_stage(&mut self, mut stage: StageScene, showfile: Option<PathBuf>) {
        stage.sanitize();
        self.3.scene = Arc::new(stage);
        self.3.showfile = showfile;
        self.3.placed.clear();
        self.3.revision = self.3.revision.wrapping_add(1);
        self.reconcile_stage();
    }

    /// Sets a remote client's copy of the stage (see
    /// [`EngineState::set_mirrored_show`]) as the engine sent it.
    pub fn set_mirrored_stage(
        &mut self,
        scene: StageScene,
        revision: u64,
        showfile: Option<PathBuf>,
    ) {
        self.3 = StageState {
            scene: Arc::new(scene),
            revision,
            showfile,
            placed: BTreeMap::new(),
        };
    }

    /// The show is saved as `showfile` from now on, with its model assets
    /// copied there: points the stage's assets at the copies in `relocated`
    /// (by asset id; the rest of the stage is kept, so an edit made while the
    /// copies were written survives).
    pub fn relocate_stage(&mut self, relocated: &StageScene, showfile: PathBuf) {
        let scene = Arc::make_mut(&mut self.3.scene);
        for (id, asset) in &relocated.assets {
            if let Some(current) = scene.assets.get_mut(id) {
                current.relative_path.clone_from(&asset.relative_path);
            }
        }
        self.3.showfile = Some(showfile);
        self.3.revision = self.3.revision.wrapping_add(1);
    }

    fn edit_stage(&mut self, edit: impl FnOnce(&mut StageScene)) {
        edit(Arc::make_mut(&mut self.3.scene));
        self.3.revision = self.3.revision.wrapping_add(1);
    }

    fn fixture_exists(&self, (group, fixture): (u8, u8)) -> bool {
        self.0
            .groups
            .get(&group)
            .is_some_and(|g| g.fixtures.contains_key(&fixture))
    }

    /// Puts every fixture attached to a truss at its mount point. An
    /// attachment whose fixture is gone, or whose fixture was moved by
    /// something else since the last placement, is dropped. Runs after every
    /// stage command and every patch change.
    pub fn reconcile_stage(&mut self) {
        let desired: Vec<_> = self
            .3
            .scene
            .fixture_attachments
            .iter()
            .filter_map(|attachment| {
                Some((
                    (attachment.group_id, attachment.fixture_id),
                    attachment_pose(&self.3.scene, attachment)?,
                ))
            })
            .collect();
        let mut detached = std::collections::BTreeSet::new();
        for (key, (position, rotation)) in desired {
            let Some(fixture) = self
                .0
                .groups
                .get_mut(&key.0)
                .and_then(|group| group.fixtures.get_mut(&key.1))
            else {
                detached.insert(key);
                continue;
            };
            let actual = (
                [fixture.pos.x, fixture.pos.y, fixture.pos.z],
                [fixture.rotation.x, fixture.rotation.y, fixture.rotation.z],
            );
            if let Some(placed) = self.3.placed.get(&key) {
                if !pose_near(actual, *placed) {
                    detached.insert(key);
                    continue;
                }
            }
            [fixture.pos.x, fixture.pos.y, fixture.pos.z] = position;
            [fixture.rotation.x, fixture.rotation.y, fixture.rotation.z] = rotation;
            self.3.placed.insert(key, (position, rotation));
        }
        if !detached.is_empty() {
            self.edit_stage(|stage| {
                stage.fixture_attachments.retain(|attachment| {
                    !detached.contains(&(attachment.group_id, attachment.fixture_id))
                })
            });
        }
        let attached: std::collections::BTreeSet<_> = self
            .3
            .scene
            .fixture_attachments
            .iter()
            .map(|attachment| (attachment.group_id, attachment.fixture_id))
            .collect();
        self.3.placed.retain(|key, _| attached.contains(key));
    }

    /// Fixtures `a` and `b` of `group` swapped ids
    /// (`HostCommand::SwapFixtures`): their attachments follow them.
    pub(super) fn swap_stage_fixtures(&mut self, group: u8, a: u8, b: u8) {
        self.rekey_stage_fixtures(|key| match key {
            (g, f) if g == group && f == a => (group, b),
            (g, f) if g == group && f == b => (group, a),
            other => other,
        });
    }

    /// A fixture moved to another group and got a new id
    /// (`HostCommand::MoveFixtureToGroup`): its attachment follows it.
    pub(super) fn move_stage_fixture(&mut self, from: (u8, u8), to: (u8, u8)) {
        self.rekey_stage_fixtures(|key| if key == from { to } else { key });
    }

    fn rekey_stage_fixtures(&mut self, rekey: impl Fn((u8, u8)) -> (u8, u8)) {
        let affected = self.3.scene.fixture_attachments.iter().any(|attachment| {
            let key = (attachment.group_id, attachment.fixture_id);
            rekey(key) != key
        });
        if !affected {
            return;
        }
        self.edit_stage(|stage| {
            for attachment in &mut stage.fixture_attachments {
                (attachment.group_id, attachment.fixture_id) =
                    rekey((attachment.group_id, attachment.fixture_id));
            }
        });
        self.3.placed = std::mem::take(&mut self.3.placed)
            .into_iter()
            .map(|(key, pose)| (rekey(key), pose))
            .collect();
    }

    /// Writes hand placements into the patch (non-finite values keep the
    /// old ones). Every fixture must exist.
    fn place_fixtures(&mut self, placements: &[FixturePlacement]) {
        for placement in placements {
            let Some(fixture) = self
                .0
                .groups
                .get_mut(&placement.group)
                .and_then(|group| group.fixtures.get_mut(&placement.fixture))
            else {
                continue;
            };
            let position = [&mut fixture.pos.x, &mut fixture.pos.y, &mut fixture.pos.z];
            for (value, new) in position.into_iter().zip(placement.position) {
                if new.is_finite() {
                    *value = new;
                }
            }
            let rotation = [
                &mut fixture.rotation.x,
                &mut fixture.rotation.y,
                &mut fixture.rotation.z,
            ];
            for (value, new) in rotation.into_iter().zip(placement.rotation) {
                if new.is_finite() {
                    *value = new;
                }
            }
        }
    }

    /// `HostCommand::AddStageObject`.
    pub(super) fn add_stage_object_command(&mut self, object: StageObject) -> CommandOutcome {
        let mut id = 0;
        self.edit_stage(|stage| {
            id = stage.add_object(object);
            stage.sanitize();
        });
        ok(Some(CreatedId::StageObject(id)))
    }

    /// `HostCommand::RemoveStageObjects`.
    pub(super) fn remove_stage_objects_command(&mut self, ids: &[u64]) -> CommandOutcome {
        if ids.iter().any(|id| !self.3.scene.objects.contains_key(id)) {
            return refused("Stage object not found");
        }
        self.edit_stage(|stage| {
            for id in ids {
                stage.remove_object(*id);
            }
        });
        self.reconcile_stage();
        ok(None)
    }

    /// `HostCommand::PatchStage`: validated on a copy, then applied whole.
    pub(super) fn patch_stage_command(&mut self, patch: StagePatch) -> CommandOutcome {
        let StagePatch {
            room,
            objects,
            disconnect,
            connect,
            detach,
            attach,
            fixtures,
        } = patch;
        let mut stage = (*self.3.scene).clone();
        if objects
            .iter()
            .any(|(id, _)| !stage.objects.contains_key(id))
        {
            return refused("Stage object not found");
        }
        let placed_keys = fixtures.iter().map(|p| (p.group, p.fixture));
        let attached_keys = attach.iter().map(|a| (a.group_id, a.fixture_id));
        if !placed_keys
            .chain(attached_keys)
            .all(|key| self.fixture_exists(key))
        {
            return refused("Fixture not found");
        }

        if let Some(room) = room {
            stage.room = room;
        }
        for (id, object) in objects {
            stage.objects.insert(id, object);
        }
        let disconnect: Vec<_> = disconnect.into_iter().map(|c| c.normalized()).collect();
        stage
            .truss_connections
            .retain(|connection| !disconnect.contains(&connection.normalized()));
        stage.truss_connections.extend(connect.iter().copied());
        let replaced: Vec<_> = attach.iter().map(|a| (a.group_id, a.fixture_id)).collect();
        stage.fixture_attachments.retain(|attachment| {
            let key = (attachment.group_id, attachment.fixture_id);
            !detach.contains(&key) && !replaced.contains(&key)
        });
        if attach.iter().any(|attachment| {
            stage
                .truss_spec(attachment.truss_id)
                .and_then(|spec| spec.branch_length(attachment.branch))
                .is_none()
        }) {
            return refused("Truss not found");
        }
        stage.fixture_attachments.extend(attach);
        stage.sanitize();
        let connected = connect
            .iter()
            .all(|connection| stage.truss_connections.contains(&connection.normalized()));
        if !connected {
            return refused("Trusses cannot be connected");
        }

        for key in detach.iter().chain(&replaced) {
            self.3.placed.remove(key);
        }
        self.3.scene = Arc::new(stage);
        self.3.revision = self.3.revision.wrapping_add(1);
        self.place_fixtures(&fixtures);
        self.reconcile_stage();
        ok(None)
    }

    /// `HostCommand::RestoreStage`.
    pub(super) fn restore_stage_command(
        &mut self,
        mut stage: StageScene,
        fixtures: &[FixturePlacement],
    ) -> CommandOutcome {
        stage.sanitize();
        self.3.scene = Arc::new(stage);
        self.3.revision = self.3.revision.wrapping_add(1);
        self.3.placed.clear();
        self.place_fixtures(fixtures);
        self.reconcile_stage();
        ok(None)
    }

    /// `HostCommand::ImportStageModel`. Writes the file on the engine
    /// thread; the frontend has already parsed it.
    pub(super) fn import_stage_model_command(
        &mut self,
        file_name: &str,
        bytes: &[u8],
    ) -> CommandOutcome {
        let Some(showfile) = self.3.showfile.clone() else {
            return refused("Save the showfile before importing models");
        };
        let asset = match crate::stage_assets::install_glb(file_name, bytes, &showfile) {
            Ok(asset) => asset,
            Err(error) => return refused(format!("{error:#}")),
        };
        let mut object_id = 0;
        self.edit_stage(|stage| {
            let name = asset.original_name.clone();
            let asset_id = stage.add_asset(asset);
            let index = stage.objects.len() + 1;
            let mut object =
                StageObject::preset(StageObjectKind::ImportedModel { asset_id }, index);
            object.name = name;
            object_id = stage.add_object(object);
        });
        ok(Some(CreatedId::StageObject(object_id)))
    }

    /// `HostCommand::RemoveStageAssets`.
    pub(super) fn remove_stage_assets_command(&mut self, ids: &[u64]) -> CommandOutcome {
        if ids.iter().any(|id| !self.3.scene.assets.contains_key(id)) {
            return refused("Model not found");
        }
        self.edit_stage(|stage| {
            stage.objects.retain(|_, object| {
                !matches!(
                    object.kind,
                    StageObjectKind::ImportedModel { asset_id } if ids.contains(&asset_id)
                )
            });
            stage.assets.retain(|id, _| !ids.contains(id));
        });
        ok(None)
    }
}

fn pose_near(a: Pose, b: Pose) -> bool {
    a.0.into_iter()
        .chain(a.1)
        .zip(b.0.into_iter().chain(b.1))
        .all(|(left, right)| (left - right).abs() <= 0.001)
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    use super::Pose;
    use crate::{
        client::{
            testing::{small_rig, TwoClients},
            EngineClient,
        },
        dmx::EngineState,
        hub::{CommandOutcome, CreatedId, HostCommand, ModelBytes},
        stage::{
            geometry::attachment_pose, FixturePlacement, FixtureTrussAttachment, StageObject,
            StageObjectKind, StagePatch, StageScene, TrussConnection, TrussEndpointRef,
            TrussProfile, TrussSpec,
        },
    };
    use blaulicht_shared::fixture::{light::Light, state::Fixture, FixtureType};

    const OK: CommandOutcome = CommandOutcome::Ok { created: None };

    fn refused(reason: &str) -> CommandOutcome {
        CommandOutcome::Refused(reason.to_string())
    }

    fn pose(state: &EngineState, (group, fixture): (u8, u8)) -> Pose {
        let fixture = &state.0.groups[&group].fixtures[&fixture];
        (
            [fixture.pos.x, fixture.pos.y, fixture.pos.z],
            [fixture.rotation.x, fixture.rotation.y, fixture.rotation.z],
        )
    }

    fn mount(state: &EngineState, (group, fixture): (u8, u8)) -> Option<Pose> {
        let attachment = state
            .stage()
            .fixture_attachments
            .iter()
            .find(|a| (a.group_id, a.fixture_id) == (group, fixture))?;
        attachment_pose(state.stage(), attachment)
    }

    fn attachment(key: (u8, u8), truss_id: u64, distance_m: f32) -> FixtureTrussAttachment {
        FixtureTrussAttachment {
            group_id: key.0,
            fixture_id: key.1,
            truss_id,
            branch: 0,
            distance_m,
            rotation_offset: [0.0; 3],
        }
    }

    fn patch(edit: impl FnOnce(&mut StagePatch)) -> HostCommand {
        let mut patch = StagePatch::default();
        edit(&mut patch);
        HostCommand::PatchStage(Box::new(patch))
    }

    /// Adds a 3 m truss of `profile`, sent by `client`, and returns its id.
    fn add_truss(h: &mut TwoClients, profile: TrussProfile, x: f32) -> u64 {
        let mut truss = StageObject::truss_preset(
            1,
            TrussSpec {
                profile,
                ..Default::default()
            },
        );
        truss.transform.translation = [x, 4.0, 0.0];
        match h
            .engine
            .round_trip(&h.remote, HostCommand::AddStageObject(truss))
        {
            CommandOutcome::Ok {
                created: Some(CreatedId::StageObject(id)),
            } => id,
            other => panic!("unexpected {other:?}"),
        }
    }

    #[test]
    fn stage_objects_are_added_edited_and_removed_by_id() {
        let mut h = TwoClients::start(small_rig);
        let revision = h.desk.with_show(|show| show.stage_revision());
        let a = add_truss(&mut h, TrussProfile::Square, 0.0);
        let b = add_truss(&mut h, TrussProfile::Square, 5.0);
        assert_ne!(a, b);
        assert!(h.desk.with_show(|show| show.stage_revision()) > revision);

        let mut moved = h.desk.with_show(|show| show.stage().objects[&a].clone());
        moved.transform.translation[2] = 2.0;
        moved.name = "Front truss".to_string();
        let edit = patch(|p| p.objects = vec![(a, moved.clone())]);
        assert_eq!(h.engine.round_trip(&h.desk, edit), OK);
        assert_eq!(
            h.remote.with_show(|show| show.stage().objects[&a].clone()),
            moved
        );

        let missing = patch(|p| p.objects = vec![(99, moved.clone())]);
        assert_eq!(
            h.engine.round_trip(&h.desk, missing),
            refused("Stage object not found")
        );
        assert_eq!(
            h.engine
                .round_trip(&h.desk, HostCommand::RemoveStageObjects(vec![b, 99])),
            refused("Stage object not found")
        );
        assert!(h
            .desk
            .with_show(|show| show.stage().objects.contains_key(&b)));
        assert_eq!(
            h.engine
                .round_trip(&h.desk, HostCommand::RemoveStageObjects(vec![b])),
            OK
        );
        let ids: Vec<u64> = h
            .desk
            .with_show(|show| show.stage().objects.keys().copied().collect());
        assert_eq!(ids, vec![a]);
    }

    #[test]
    fn an_attached_fixture_hangs_on_its_truss_and_follows_it() {
        let mut h = TwoClients::start(small_rig);
        let truss = add_truss(&mut h, TrussProfile::Square, 0.0);
        let wall = match h.engine.round_trip(
            &h.remote,
            HostCommand::AddStageObject(StageObject::preset(StageObjectKind::Wall, 2)),
        ) {
            CommandOutcome::Ok {
                created: Some(CreatedId::StageObject(id)),
            } => id,
            other => panic!("unexpected {other:?}"),
        };

        let attach = patch(|p| p.attach = vec![attachment((0, 1), truss, 1.0)]);
        assert_eq!(h.engine.round_trip(&h.remote, attach), OK);
        let hanging = h
            .desk
            .with_show(|show| (pose(show, (0, 1)), mount(show, (0, 1))));
        assert_eq!(Some(hanging.0), hanging.1);
        assert!((hanging.0 .0[1] - (4.0 + 0.0)).abs() < 0.5, "{hanging:?}");

        // Moving the truss moves the fixture with it.
        let mut moved = h
            .desk
            .with_show(|show| show.stage().objects[&truss].clone());
        moved.transform.translation[0] = 3.0;
        assert_eq!(
            h.engine
                .round_trip(&h.desk, patch(|p| p.objects = vec![(truss, moved)])),
            OK
        );
        let (after, mounted) = h
            .desk
            .with_show(|show| (pose(show, (0, 1)), mount(show, (0, 1))));
        assert_eq!(Some(after), mounted);
        assert!((after.0[0] - hanging.0 .0[0] - 3.0).abs() < 0.001);

        // Refusals change nothing.
        let revision = h.desk.with_show(|show| show.stage_revision());
        for (command, reason) in [
            (
                patch(|p| p.attach = vec![attachment((0, 9), truss, 1.0)]),
                "Fixture not found",
            ),
            (
                patch(|p| p.attach = vec![attachment((1, 0), wall, 1.0)]),
                "Truss not found",
            ),
            (
                patch(|p| {
                    p.detach = vec![(0, 1)];
                    p.fixtures = vec![FixturePlacement {
                        group: 7,
                        fixture: 0,
                        position: [0.0; 3],
                        rotation: [0.0; 3],
                    }];
                }),
                "Fixture not found",
            ),
        ] {
            assert_eq!(h.engine.round_trip(&h.remote, command), refused(reason));
        }
        assert_eq!(h.desk.with_show(|show| show.stage_revision()), revision);
        assert_eq!(
            h.desk
                .with_show(|show| show.stage().fixture_attachments.len()),
            1
        );

        // Detaching leaves the fixture where it hung.
        assert_eq!(
            h.engine
                .round_trip(&h.desk, patch(|p| p.detach = vec![(0, 1)])),
            OK
        );
        assert_eq!(h.desk.with_show(|show| pose(show, (0, 1))), after);
        assert!(h
            .desk
            .with_show(|show| show.stage().fixture_attachments.is_empty()));
    }

    #[test]
    fn moving_an_attached_fixture_elsewhere_takes_it_off_its_truss() {
        let mut h = TwoClients::start(small_rig);
        let truss = add_truss(&mut h, TrussProfile::Square, 0.0);
        let attach = patch(|p| {
            p.attach = vec![
                attachment((0, 0), truss, 0.5),
                attachment((0, 1), truss, 2.0),
            ]
        });
        assert_eq!(h.engine.round_trip(&h.desk, attach), OK);

        // A Setup edit that keeps the position keeps the attachment.
        let mut renamed = h
            .desk
            .with_show(|show| show.0.groups[&0].fixtures[&0].clone());
        renamed.name = "Renamed".to_string();
        let update = HostCommand::UpdateFixture {
            group: 0,
            fixture: 0,
            value: renamed.clone(),
        };
        assert_eq!(h.engine.round_trip(&h.remote, update), OK);
        assert_eq!(
            h.desk
                .with_show(|show| show.stage().fixture_attachments.len()),
            2
        );

        // One that moves it detaches it, and it stays where Setup put it.
        renamed.pos.x += 1.0;
        let update = HostCommand::UpdateFixture {
            group: 0,
            fixture: 0,
            value: renamed.clone(),
        };
        assert_eq!(h.engine.round_trip(&h.remote, update), OK);
        let keys = |show: &EngineState| -> Vec<(u8, u8)> {
            show.stage()
                .fixture_attachments
                .iter()
                .map(|a| (a.group_id, a.fixture_id))
                .collect()
        };
        assert_eq!(h.desk.with_show(keys), vec![(0, 1)]);
        assert_eq!(
            h.desk.with_show(|show| pose(show, (0, 0)).0[0]),
            renamed.pos.x
        );

        // So does a hand placement in the visualizer.
        let place = patch(|p| {
            p.fixtures = vec![FixturePlacement {
                group: 0,
                fixture: 1,
                position: [1.0, 2.0, 3.0],
                rotation: [0.0, 90.0, f32::NAN],
            }]
        });
        let before = h.desk.with_show(|show| pose(show, (0, 1)));
        assert_eq!(h.engine.round_trip(&h.desk, place), OK);
        assert!(h.desk.with_show(keys).is_empty());
        assert_eq!(
            h.desk.with_show(|show| pose(show, (0, 1))),
            ([1.0, 2.0, 3.0], [0.0, 90.0, before.1[2]])
        );
    }

    #[test]
    fn patch_changes_from_setup_keep_attachments_in_step() {
        let mut h = TwoClients::start(small_rig);
        let truss = add_truss(&mut h, TrussProfile::Square, 0.0);
        let attach = patch(|p| {
            p.attach = vec![
                attachment((0, 0), truss, 0.5),
                attachment((1, 0), truss, 1.5),
                attachment((2, 1), truss, 2.5),
            ]
        });
        assert_eq!(h.engine.round_trip(&h.desk, attach), OK);
        let keys = |show: &EngineState| -> Vec<((u8, u8), f32)> {
            let mut keys: Vec<_> = show
                .stage()
                .fixture_attachments
                .iter()
                .map(|a| ((a.group_id, a.fixture_id), a.distance_m))
                .collect();
            keys.sort_by_key(|(key, _)| *key);
            keys
        };

        // Swapping reorders ids: each attachment follows its fixture.
        let swap = HostCommand::SwapFixtures {
            group: 0,
            a: 0,
            b: 1,
        };
        assert_eq!(h.engine.round_trip(&h.remote, swap), OK);
        assert_eq!(
            h.desk.with_show(keys),
            vec![((0, 1), 0.5), ((1, 0), 1.5), ((2, 1), 2.5)]
        );

        // Moving a fixture to another group: its attachment follows.
        let created = h.engine.round_trip(
            &h.remote,
            HostCommand::MoveFixtureToGroup {
                group: 1,
                fixture: 0,
                target: 2,
            },
        );
        let CommandOutcome::Ok {
            created: Some(CreatedId::Fixture { group: 2, fixture }),
        } = created
        else {
            panic!("unexpected {created:?}");
        };
        assert_eq!(
            h.desk.with_show(keys),
            vec![((0, 1), 0.5), ((2, 1), 2.5), ((2, fixture), 1.5)]
        );

        // Deleting a fixture or its group drops its attachment.
        let delete = HostCommand::DeleteFixture {
            group: 0,
            fixture: 1,
        };
        assert_eq!(h.engine.round_trip(&h.remote, delete), OK);
        assert_eq!(
            h.engine.round_trip(&h.remote, HostCommand::DeleteGroup(2)),
            OK
        );
        assert!(h.desk.with_show(keys).is_empty());

        // A fixture added later is not attached to anything.
        let light = Fixture::new(
            0,
            100,
            "New".to_string(),
            FixtureType::from(Light::Generic3ChanNoAlpha),
        );
        let add = HostCommand::AddFixtures {
            group: 0,
            fixtures: vec![light],
        };
        assert_eq!(h.engine.round_trip(&h.remote, add), OK);
        assert!(h.desk.with_show(keys).is_empty());
    }

    #[test]
    fn trusses_connect_only_where_they_fit() {
        let mut h = TwoClients::start(small_rig);
        let a = add_truss(&mut h, TrussProfile::Square, 0.0);
        let b = add_truss(&mut h, TrussProfile::Square, 3.0);
        let triangle = add_truss(&mut h, TrussProfile::Triangular, 6.0);
        let connection = |x: u64, y: u64| TrussConnection {
            a: TrussEndpointRef {
                object_id: x,
                endpoint: 1,
            },
            b: TrussEndpointRef {
                object_id: y,
                endpoint: 0,
            },
        };

        let incompatible = patch(|p| p.connect = vec![connection(b, triangle)]);
        assert_eq!(
            h.engine.round_trip(&h.desk, incompatible),
            refused("Trusses cannot be connected")
        );
        assert!(h
            .desk
            .with_show(|show| show.stage().truss_connections.is_empty()));

        assert_eq!(
            h.engine
                .round_trip(&h.desk, patch(|p| p.connect = vec![connection(a, b)])),
            OK
        );
        assert_eq!(
            h.desk.with_show(|show| show.stage().connected_trusses(a)),
            [a, b].into()
        );
        assert_eq!(
            h.engine
                .round_trip(&h.remote, patch(|p| p.disconnect = vec![connection(a, b)])),
            OK
        );
        assert!(h
            .desk
            .with_show(|show| show.stage().truss_connections.is_empty()));
    }

    #[test]
    fn undo_restores_the_whole_stage_and_the_fixtures() {
        let mut h = TwoClients::start(small_rig);
        let truss = add_truss(&mut h, TrussProfile::Square, 0.0);
        let (saved_stage, saved_pose) = h
            .desk
            .with_show(|show| (show.stage().clone(), pose(show, (0, 0))));

        let attach = patch(|p| p.attach = vec![attachment((0, 0), truss, 1.0)]);
        assert_eq!(h.engine.round_trip(&h.desk, attach), OK);
        add_truss(&mut h, TrussProfile::Ladder, 4.0);
        assert_ne!(h.desk.with_show(|show| pose(show, (0, 0))), saved_pose);

        let restore = HostCommand::RestoreStage {
            stage: Box::new(saved_stage.clone()),
            fixtures: vec![FixturePlacement {
                group: 0,
                fixture: 0,
                position: saved_pose.0,
                rotation: saved_pose.1,
            }],
        };
        assert_eq!(h.engine.round_trip(&h.desk, restore), OK);
        assert_eq!(h.remote.with_show(|show| show.stage().clone()), saved_stage);
        assert_eq!(h.remote.with_show(|show| pose(show, (0, 0))), saved_pose);

        // Restoring a stage with an attachment hangs the fixture on its truss.
        let mut attached = saved_stage;
        attached
            .fixture_attachments
            .push(attachment((0, 0), truss, 2.0));
        let restore = HostCommand::RestoreStage {
            stage: Box::new(attached),
            fixtures: Vec::new(),
        };
        assert_eq!(h.engine.round_trip(&h.desk, restore), OK);
        let (now, mounted) = h
            .desk
            .with_show(|show| (pose(show, (0, 0)), mount(show, (0, 0))));
        assert_eq!(Some(now), mounted);
    }

    #[test]
    fn a_model_is_stored_next_to_the_showfile_and_removed_with_its_objects() {
        let mut h = TwoClients::start(small_rig);
        let bytes =
            std::fs::read(Path::new(env!("CARGO_MANIFEST_DIR")).join("../../test-assets/Box.glb"))
                .unwrap();
        let import = || HostCommand::ImportStageModel {
            file_name: "Box.glb".to_string(),
            bytes: ModelBytes(bytes.clone().into()),
        };
        assert_eq!(
            h.engine.round_trip(&h.remote, import()),
            refused("Save the showfile before importing models")
        );

        let directory = tempdir::TempDir::new("blaulicht-stage-import").unwrap();
        let showfile = directory.path().join("show.json");
        h.engine
            .edit_show(|show| show.load_stage(StageScene::default(), Some(showfile.clone())));
        let object = match h.engine.round_trip(&h.remote, import()) {
            CommandOutcome::Ok {
                created: Some(CreatedId::StageObject(id)),
            } => id,
            other => panic!("unexpected {other:?}"),
        };
        let (asset_id, asset) = h.desk.with_show(|show| {
            let StageObjectKind::ImportedModel { asset_id } = show.stage().objects[&object].kind
            else {
                panic!("not a model");
            };
            (asset_id, show.stage().assets[&asset_id].clone())
        });
        assert_eq!(asset.original_name, "Box.glb");
        let stored = crate::stage_assets::resolve_asset(&showfile, &asset).unwrap();
        assert!(stored.starts_with(directory.path().join("show.assets")));
        assert_eq!(std::fs::read(stored).unwrap(), bytes);

        let bad = HostCommand::ImportStageModel {
            file_name: "notes.glb".to_string(),
            bytes: ModelBytes(b"not a model".to_vec().into()),
        };
        assert!(matches!(
            h.engine.round_trip(&h.remote, bad),
            CommandOutcome::Refused(_)
        ));

        assert_eq!(
            h.engine
                .round_trip(&h.desk, HostCommand::RemoveStageAssets(vec![asset_id + 1])),
            refused("Model not found")
        );
        assert_eq!(
            h.engine
                .round_trip(&h.desk, HostCommand::RemoveStageAssets(vec![asset_id])),
            OK
        );
        assert!(h.desk.with_show(|show| {
            show.stage().objects.is_empty() && show.stage().assets.is_empty()
        }));
    }

    #[test]
    fn a_loaded_stage_hangs_its_fixtures_and_drops_missing_ones() {
        let mut state = EngineState::default();
        small_rig(&mut state);
        let mut stage = StageScene::default();
        let truss = stage.add_object(StageObject::truss_preset(1, TrussSpec::default()));
        stage.fixture_attachments = vec![
            attachment((1, 1), truss, 1.0),
            attachment((5, 0), truss, 2.0),
        ];

        state.load_stage(stage, Some("/shows/show.json".into()));

        assert_eq!(state.showfile(), Some(Path::new("/shows/show.json")));
        assert_eq!(state.stage().fixture_attachments.len(), 1);
        assert_eq!(Some(pose(&state, (1, 1))), mount(&state, (1, 1)));
        // A UI snapshot shares the stage instead of copying it.
        assert!(std::ptr::eq(state.clone_for_ui().stage(), state.stage()));
    }
}
