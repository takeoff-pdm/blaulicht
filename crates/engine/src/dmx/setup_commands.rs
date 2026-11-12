//! The engine side of the Setup pages' host commands: groups, scenes,
//! fixtures and animation templates (see [`HostCommand`]). Each runs in
//! `DmxEngine::apply_host`, with the sender's session swapped in, and names
//! what it acts on by id.
//!
//! [`HostCommand`]: crate::hub::HostCommand

use super::{animation::templates::ANIMATION_LIMIT_REACHED, EngineState};
use crate::hub::{CommandOutcome, CreatedId};
use blaulicht_shared::{
    fixture::state::Fixture,
    folder::{FolderId, FolderItem},
    scene::BLANK_SCENE_ID,
    AnimationSpec, AnimationTemplate,
};
use tracing::warn;

fn ok(created: Option<CreatedId>) -> CommandOutcome {
    CommandOutcome::Ok { created }
}

fn refused(reason: impl Into<String>) -> CommandOutcome {
    CommandOutcome::Refused(reason.into())
}

/// A trimmed, non-empty name for a rename.
fn rename_to(name: &str) -> Result<String, CommandOutcome> {
    match name.trim() {
        "" => Err(refused("Name is empty")),
        trimmed => Ok(trimmed.to_string()),
    }
}

impl EngineState {
    /// Moves a just-created group or scene into `folder`. A folder it can't
    /// move into leaves it at the top level, as the dialogs did before.
    fn place_new_item(&mut self, item: FolderItem, folder: Option<FolderId>) {
        let Some(folder) = folder else {
            return;
        };
        if let Err(err) = self.0.move_to_folder(item, Some(folder)) {
            warn!("New {item:?} stays at the top level: {err}");
        }
    }

    /// `HostCommand::CreateGroup`.
    pub(super) fn create_group_command(
        &mut self,
        name: String,
        folder: Option<FolderId>,
    ) -> CommandOutcome {
        let Some(group) = self.create_group(name) else {
            return refused("Group limit reached");
        };
        self.place_new_item(FolderItem::Group(group), folder);
        ok(Some(CreatedId::Group(group)))
    }

    /// `HostCommand::RenameGroup`.
    pub(super) fn rename_group_command(&mut self, group: u8, name: &str) -> CommandOutcome {
        let name = match rename_to(name) {
            Ok(name) => name,
            Err(refusal) => return refusal,
        };
        if self.rename_group(group, name) {
            ok(None)
        } else {
            refused("Group not found")
        }
    }

    /// `HostCommand::DeleteGroup`.
    pub(super) fn delete_group_command(&mut self, group: u8) -> CommandOutcome {
        if !self.0.groups.contains_key(&group) {
            return refused("Group not found");
        }
        self.delete_group(group);
        ok(None)
    }

    /// `HostCommand::CreateScene`.
    pub(super) fn create_scene_command(
        &mut self,
        name: String,
        folder: Option<FolderId>,
    ) -> CommandOutcome {
        let Some(scene) = self.new_scene(name) else {
            return refused("Scene limit reached");
        };
        self.place_new_item(FolderItem::Scene(scene), folder);
        ok(Some(CreatedId::Scene(scene)))
    }

    fn is_user_scene(&self, scene: u8) -> bool {
        scene != BLANK_SCENE_ID && self.0.scenes.contains_key(&scene)
    }

    /// `HostCommand::CloneScene`.
    pub(super) fn clone_scene_command(&mut self, scene: u8, name: String) -> CommandOutcome {
        if !self.is_user_scene(scene) {
            return refused("Scene not found");
        }
        match self.0.clone_scene_of(scene, name) {
            Some(clone) => ok(Some(CreatedId::Scene(clone))),
            None => refused("Scene limit reached"),
        }
    }

    /// `HostCommand::RenameScene`.
    pub(super) fn rename_scene_command(&mut self, scene: u8, name: &str) -> CommandOutcome {
        let name = match rename_to(name) {
            Ok(name) => name,
            Err(refusal) => return refusal,
        };
        if self.is_user_scene(scene) && self.rename_scene(scene, name) {
            ok(None)
        } else {
            refused("Scene not found")
        }
    }

    /// `HostCommand::DeleteScene`.
    pub(super) fn delete_scene_command(&mut self, scene: u8) -> CommandOutcome {
        if !self.is_user_scene(scene) {
            return refused("Scene not found");
        }
        if self.delete_scene(scene) {
            ok(None)
        } else {
            refused("At least one scene must remain")
        }
    }

    /// `HostCommand::AddFixtures`.
    pub(super) fn add_fixtures_command(
        &mut self,
        group: u8,
        fixtures: Vec<Fixture>,
    ) -> CommandOutcome {
        match self.add_fixture_batch_to_group(group, fixtures) {
            Ok(_) => ok(None),
            Err(err) => refused(err.to_string()),
        }
    }

    /// `HostCommand::UpdateFixture`.
    pub(super) fn update_fixture_command(
        &mut self,
        group: u8,
        fixture: u8,
        value: Fixture,
    ) -> CommandOutcome {
        match self.update_fixture(group, fixture, value) {
            Ok(()) => ok(None),
            Err(err) => refused(err.to_string()),
        }
    }

    /// `HostCommand::DeleteFixture`.
    pub(super) fn delete_fixture_command(&mut self, group: u8, fixture: u8) -> CommandOutcome {
        if self.delete_fixture_from_group(group, fixture) {
            ok(None)
        } else {
            refused("Fixture not found")
        }
    }

    /// `HostCommand::SwapFixtures`.
    pub(super) fn swap_fixtures_command(&mut self, group: u8, a: u8, b: u8) -> CommandOutcome {
        if self.swap_fixtures_in_group(group, a, b) {
            ok(None)
        } else {
            refused("Fixture not found")
        }
    }

    /// `HostCommand::MoveFixtureToGroup`.
    pub(super) fn move_fixture_command(
        &mut self,
        group: u8,
        fixture: u8,
        target: u8,
    ) -> CommandOutcome {
        match self.move_fixture_to_group(group, fixture, target) {
            Some(new_id) => ok(Some(CreatedId::Fixture {
                group: target,
                fixture: new_id,
            })),
            None => refused("Could not move fixture"),
        }
    }

    /// Whether every `(group, fixture)` exists (`HostCommand::InitFixtures`).
    pub(super) fn has_fixtures(&self, fixtures: &[(u8, u8)]) -> bool {
        fixtures.iter().all(|(group, fixture)| {
            self.0
                .groups
                .get(group)
                .is_some_and(|group| group.fixtures.contains_key(fixture))
        })
    }

    /// `HostCommand::CreateAnimationTemplate`.
    pub(super) fn create_animation_template_command(
        &mut self,
        spec: AnimationSpec,
    ) -> CommandOutcome {
        let templates = &mut self.0.animation_templates;
        let Some(id) = (0..=u8::MAX).find(|id| !templates.contains_key(id)) else {
            return refused(ANIMATION_LIMIT_REACHED);
        };
        templates.insert(id, AnimationTemplate { spec });
        ok(Some(CreatedId::AnimationTemplate(id)))
    }

    /// `HostCommand::UpdateAnimationTemplate`.
    pub(super) fn update_animation_template_command(
        &mut self,
        id: u8,
        spec: AnimationSpec,
    ) -> CommandOutcome {
        match self.0.animation_templates.get_mut(&id) {
            Some(template) => {
                template.spec = spec;
                ok(None)
            }
            None => refused("Animation not found"),
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::{
        client::{
            testing::{small_rig, TwoClients},
            EngineClient,
        },
        hub::{CommandOutcome, CreatedId, HostCommand},
    };
    use blaulicht_shared::{
        fixture::{light::Light, state::Fixture, FixtureType},
        folder::FolderKind,
        scene::BLANK_SCENE_ID,
        AnimationPreset, AnimationSpec, AnimationTemplate, ControlEvent, ControlEventMessage,
        EventOriginator,
    };

    const OK: CommandOutcome = CommandOutcome::Ok { created: None };

    fn web(event: ControlEvent) -> ControlEventMessage {
        ControlEventMessage::new(EventOriginator::Web, event)
    }

    fn refused(reason: &str) -> CommandOutcome {
        CommandOutcome::Refused(reason.to_string())
    }

    fn light(address: usize, name: &str) -> Fixture {
        Fixture::new(
            0,
            address,
            name.to_string(),
            FixtureType::from(Light::Generic3ChanNoAlpha),
        )
    }

    /// Folder 0 of `kind`, made by the desk.
    fn folder(h: &mut TwoClients, kind: FolderKind) {
        let create = ControlEvent::CreateFolder {
            id: 0,
            kind,
            parent: None,
            name: "Folder".to_string(),
        };
        assert_eq!(h.engine.round_trip(&h.desk, web(create)), OK);
    }

    #[test]
    fn a_new_group_lands_in_its_folder_and_reports_its_id() {
        let mut h = TwoClients::start(small_rig);
        folder(&mut h, FolderKind::Groups);
        let create = HostCommand::CreateGroup {
            name: "New".to_string(),
            folder: Some(0),
        };
        assert!(create.edits_show());
        assert_eq!(
            h.engine.round_trip(&h.remote, create),
            CommandOutcome::Ok {
                created: Some(CreatedId::Group(3))
            }
        );
        // A folder it can't go into leaves it at the top level.
        let stray = HostCommand::CreateGroup {
            name: "Stray".to_string(),
            folder: Some(42),
        };
        assert_eq!(
            h.engine.round_trip(&h.remote, stray),
            CommandOutcome::Ok {
                created: Some(CreatedId::Group(4))
            }
        );
        h.desk.with_show(|show| {
            assert_eq!(show.0.groups[&3].name, "New");
            assert_eq!(show.0.groups[&3].folder, Some(0));
            assert_eq!(show.0.groups[&4].folder, None);
        });
    }

    #[test]
    fn groups_are_renamed_and_deleted_by_id() {
        let mut h = TwoClients::start(small_rig);
        let rename = |group: u8, name: &str| HostCommand::RenameGroup {
            group,
            name: name.to_string(),
        };
        assert_eq!(h.engine.round_trip(&h.remote, rename(1, "  Truss ")), OK);
        assert_eq!(
            h.engine.round_trip(&h.remote, rename(1, "   ")),
            refused("Name is empty")
        );
        assert_eq!(
            h.engine.round_trip(&h.remote, rename(9, "X")),
            refused("Group not found")
        );
        assert_eq!(
            h.desk.with_show(|show| show.0.groups[&1].name.clone()),
            "Truss"
        );

        // The desk has groups 0 and 1 selected, the remote group 2: the
        // remote deletes group 1. Its own selection is cleared, the desk's
        // loses only group 1.
        for group in [0, 1] {
            let select = web(ControlEvent::SelectGroup(group));
            assert_eq!(h.engine.round_trip(&h.desk, select), OK);
        }
        let select = web(ControlEvent::SelectGroup(2));
        assert_eq!(h.engine.round_trip(&h.remote, select), OK);
        assert_eq!(
            h.engine.round_trip(&h.remote, HostCommand::DeleteGroup(1)),
            OK
        );
        assert_eq!(
            h.engine.round_trip(&h.remote, HostCommand::DeleteGroup(1)),
            refused("Group not found")
        );
        h.desk
            .with_session(|show, me| {
                assert!(!show.0.groups.contains_key(&1));
                assert_eq!(
                    me.selection.group_ids.iter().copied().collect::<Vec<_>>(),
                    [0]
                );
            })
            .unwrap();
        h.remote
            .with_session(|_, me| assert!(me.selection.is_empty()))
            .unwrap();
    }

    #[test]
    fn scene_commands_act_on_the_named_scene_not_the_focus() {
        let mut h = TwoClients::start(small_rig);
        folder(&mut h, FolderKind::Scenes);
        // The remote focuses scene 1; the desk stays on scene 0.
        let focus = web(ControlEvent::SetSceneFocus(1));
        assert_eq!(h.engine.round_trip(&h.remote, focus), OK);
        let alpha = web(ControlEvent::SetSceneMasterAlpha(2, 77));
        assert_eq!(h.engine.round_trip(&h.remote, alpha), OK);

        let create = HostCommand::CreateScene {
            name: "New".to_string(),
            folder: Some(0),
        };
        assert_eq!(
            h.engine.round_trip(&h.remote, create),
            CommandOutcome::Ok {
                created: Some(CreatedId::Scene(3))
            }
        );
        let clone = HostCommand::CloneScene {
            scene: 2,
            name: "B copy".to_string(),
        };
        assert_eq!(
            h.engine.round_trip(&h.remote, clone),
            CommandOutcome::Ok {
                created: Some(CreatedId::Scene(4))
            }
        );
        let rename = HostCommand::RenameScene {
            scene: 2,
            name: " Renamed ".to_string(),
        };
        assert_eq!(h.engine.round_trip(&h.remote, rename), OK);
        h.desk.with_show(|show| {
            assert_eq!(show.0.scenes[&3].name, "New");
            assert_eq!(show.0.scenes[&3].folder, Some(0));
            assert_eq!(show.0.scenes[&4].name, "B copy");
            assert_eq!(show.0.scenes[&4].sink.master_alpha_fader, 77);
            assert_eq!(show.0.scenes[&2].name, "Renamed");
            assert_eq!(show.0.scenes[&1].name, "A");
        });

        // The remote deletes the desk's focused scene: the desk moves on, the
        // remote stays on scene 1.
        assert_eq!(
            h.engine.round_trip(&h.remote, HostCommand::DeleteScene(0)),
            OK
        );
        assert!(h.desk.with_show(|show| !show.0.scenes.contains_key(&0)));
        assert_ne!(h.desk.with_session(|_, me| me.scene_focus).unwrap(), 0);
        assert_eq!(h.remote.with_session(|_, me| me.scene_focus).unwrap(), 1);

        for command in [
            HostCommand::DeleteScene(BLANK_SCENE_ID),
            HostCommand::DeleteScene(0),
            HostCommand::CloneScene {
                scene: BLANK_SCENE_ID,
                name: "X".to_string(),
            },
            HostCommand::RenameScene {
                scene: BLANK_SCENE_ID,
                name: "X".to_string(),
            },
        ] {
            assert_eq!(
                h.engine.round_trip(&h.remote, command),
                refused("Scene not found")
            );
        }
    }

    #[test]
    fn the_last_user_scene_stays() {
        let mut h = TwoClients::start(small_rig);
        for scene in [1, 2] {
            assert_eq!(
                h.engine
                    .round_trip(&h.desk, HostCommand::DeleteScene(scene)),
                OK
            );
        }
        assert_eq!(
            h.engine.round_trip(&h.desk, HostCommand::DeleteScene(0)),
            refused("At least one scene must remain")
        );
    }

    #[test]
    fn a_fixture_batch_is_added_whole_or_refused() {
        let mut h = TwoClients::start(small_rig);
        // The second fixture overlaps group 0's first (channels 1-3).
        let overlapping = HostCommand::AddFixtures {
            group: 2,
            fixtures: vec![light(100, "Ok"), light(2, "Clash")],
        };
        assert!(overlapping.edits_show());
        let CommandOutcome::Refused(why) = h.engine.round_trip(&h.remote, overlapping) else {
            panic!("an overlapping batch was added");
        };
        assert!(why.contains("Clash"), "{why}");
        assert_eq!(h.desk.with_show(|show| show.0.groups[&2].fixtures.len()), 2);

        let batch = HostCommand::AddFixtures {
            group: 2,
            fixtures: vec![light(100, "One"), light(103, "Two")],
        };
        assert_eq!(h.engine.round_trip(&h.remote, batch), OK);
        h.desk.with_show(|show| {
            let fixtures = &show.0.groups[&2].fixtures;
            assert_eq!(fixtures[&2].name, "One");
            assert_eq!(fixtures[&3].name, "Two");
            assert!(show.0.scenes[&1].sink.fixture_states.contains_key(&(2, 3)));
        });
        assert_eq!(
            h.engine.round_trip(
                &h.remote,
                HostCommand::AddFixtures {
                    group: 9,
                    fixtures: vec![light(200, "Nowhere")],
                }
            ),
            refused("Group 9 no longer exists")
        );
    }

    #[test]
    fn fixtures_are_updated_swapped_moved_and_deleted_by_id() {
        let mut h = TwoClients::start(small_rig);
        let update = |value: Fixture| HostCommand::UpdateFixture {
            group: 0,
            fixture: 1,
            value,
        };
        assert_eq!(
            h.engine.round_trip(&h.remote, update(light(300, "Moved"))),
            OK
        );
        // Channel 7 belongs to group 1's first fixture.
        let CommandOutcome::Refused(why) = h.engine.round_trip(&h.remote, update(light(7, "X")))
        else {
            panic!("an overlapping update was applied");
        };
        assert!(why.contains("F7"), "{why}");

        let swap = HostCommand::SwapFixtures {
            group: 0,
            a: 1,
            b: 0,
        };
        assert_eq!(h.engine.round_trip(&h.remote, swap), OK);
        h.desk.with_show(|show| {
            assert_eq!(show.0.groups[&0].fixtures[&0].name, "Moved");
            assert_eq!(show.0.groups[&0].fixtures[&1].name, "F1");
        });

        let move_fixture = HostCommand::MoveFixtureToGroup {
            group: 0,
            fixture: 0,
            target: 2,
        };
        assert_eq!(
            h.engine.round_trip(&h.remote, move_fixture),
            CommandOutcome::Ok {
                created: Some(CreatedId::Fixture {
                    group: 2,
                    fixture: 2
                })
            }
        );
        assert_eq!(
            h.desk
                .with_show(|show| show.0.groups[&2].fixtures[&2].name.clone()),
            "Moved"
        );
        let stray = HostCommand::MoveFixtureToGroup {
            group: 0,
            fixture: 0,
            target: 1,
        };
        assert_eq!(
            h.engine.round_trip(&h.remote, stray),
            refused("Could not move fixture")
        );

        let delete = HostCommand::DeleteFixture {
            group: 2,
            fixture: 2,
        };
        assert_eq!(h.engine.round_trip(&h.remote, delete.clone()), OK);
        assert_eq!(
            h.engine.round_trip(&h.remote, delete),
            refused("Fixture not found")
        );
        assert!(h
            .desk
            .with_show(|show| !show.0.groups[&2].fixtures.contains_key(&2)));
    }

    #[test]
    fn initializing_fixtures_queues_their_setup_run() {
        let mut h = TwoClients::start(small_rig);
        let init = HostCommand::InitFixtures(vec![(0, 1), (2, 0)]);
        assert!(!init.edits_show());
        assert_eq!(h.engine.round_trip(&h.remote, init), OK);
        let mut running = h.engine.engine().initializing_fixtures();
        running.sort();
        assert_eq!(running, [(0, 1), (2, 0)]);

        let missing = HostCommand::InitFixtures(vec![(1, 0), (9, 9)]);
        assert_eq!(
            h.engine.round_trip(&h.remote, missing),
            refused("Fixture not found")
        );
        assert!(!h.engine.engine().initializing_fixtures().contains(&(1, 0)));
    }

    #[test]
    fn identify_flashes_one_fixture_until_stopped() {
        let mut h = TwoClients::start(small_rig);
        let identify = HostCommand::IdentifyFixture(Some((0, 1)));
        assert!(!identify.edits_show());
        assert_eq!(h.engine.round_trip(&h.remote, identify), OK);
        assert_eq!(h.engine.engine().identified_fixture(), Some((0, 1)));
        // Stepping replaces the fixture.
        let next = HostCommand::IdentifyFixture(Some((2, 0)));
        assert_eq!(h.engine.round_trip(&h.remote, next), OK);
        assert_eq!(h.engine.engine().identified_fixture(), Some((2, 0)));
        let missing = HostCommand::IdentifyFixture(Some((9, 9)));
        assert_eq!(
            h.engine.round_trip(&h.remote, missing),
            refused("Fixture not found")
        );
        assert_eq!(h.engine.engine().identified_fixture(), Some((2, 0)));
        let stop = HostCommand::IdentifyFixture(None);
        assert_eq!(h.engine.round_trip(&h.remote, stop), OK);
        assert_eq!(h.engine.engine().identified_fixture(), None);
    }

    #[test]
    fn animation_templates_are_created_updated_and_deleted() {
        let mut h = TwoClients::start(small_rig);
        let create =
            HostCommand::CreateAnimationTemplate(AnimationSpec::preset(AnimationPreset::KickFlash));
        assert!(create.edits_show());
        // The default show has starter templates; a new one takes the
        // lowest free id.
        let id = h.desk.with_show(|show| {
            (0..=u8::MAX)
                .find(|id| !show.0.animation_templates.contains_key(id))
                .unwrap()
        });
        assert_eq!(
            h.engine.round_trip(&h.remote, create.clone()),
            CommandOutcome::Ok {
                created: Some(CreatedId::AnimationTemplate(id))
            }
        );

        let mut spec = AnimationSpec::preset(AnimationPreset::HueDrift);
        spec.name = "Edited".to_string();
        let update =
            |id: u8, spec: AnimationSpec| HostCommand::UpdateAnimationTemplate { id, spec };
        assert_eq!(h.engine.round_trip(&h.remote, update(id, spec.clone())), OK);
        assert_eq!(
            h.engine.round_trip(&h.remote, update(id + 1, spec)),
            refused("Animation not found")
        );
        assert_eq!(
            h.desk
                .with_show(|show| show.0.animation_templates[&id].spec.name.clone()),
            "Edited"
        );

        assert_eq!(
            h.engine
                .round_trip(&h.remote, HostCommand::DeleteAnimationTemplate(id)),
            OK
        );
        assert!(h
            .desk
            .with_show(|show| !show.0.animation_templates.contains_key(&id)));

        h.engine.edit_show(|state| {
            for id in 0..=u8::MAX {
                let spec = AnimationSpec::preset(AnimationPreset::KickFlash);
                state
                    .0
                    .animation_templates
                    .insert(id, AnimationTemplate { spec });
            }
        });
        assert_eq!(
            h.engine.round_trip(&h.remote, create),
            refused("Animation limit reached")
        );
    }
}
