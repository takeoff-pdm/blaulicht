//! The engine's persistence service: the showfile, `config.toml`, dirty
//! tracking, autosave and the process lifecycle (quit and restart).
//!
//! - **Dirty tracking.** The engine marks the show edited
//!   ([`PersistenceLink::mark_edited`]) whenever it applies a command that
//!   changes what the showfile saves, whichever client sent it
//!   (`DmxEngine::dispatch`). Every [`DIRTY_CHECK_INTERVAL`] the service also
//!   compares a hash of the show with the one last loaded or saved: that
//!   catches changes no command reports (plugins' showfile state), and clears
//!   the mark when an edit was undone.
//! - **Commands.** Save, save as, load, close, quit and restart arrive as
//!   `HostCommand`s; the engine forwards them here ([`PersistenceLink::forward`])
//!   and the service answers them itself, once done.
//! - **No IO under the engine lock.** The service runs on its own thread. It
//!   builds the showfile under short read locks, then serializes and writes
//!   it holding none; a loaded file is read and parsed before the engine lock
//!   is taken to put it in.
//! - **Status.** It publishes [`SystemMessage::ShowfileStatus`] whenever the
//!   status changes, and [`SystemMessage::ShowLoaded`] whenever another show
//!   is loaded (both are replayed to late subscribers).
//! - **Config.** It writes `last_open_showfile` and the global plugin state;
//!   the supervisor writes `default_audio_device` through the same
//!   [`SharedConfig`], whose lock is a leaf.
//! - **Frontend section.** The desk frontend publishes its screen layouts
//!   (`HostCommand::SetFrontendSection`); the service saves them as the
//!   showfile's `ui` section and hands them back on load.

use std::{
    collections::BTreeMap,
    hash::{Hash, Hasher},
    path::{Path, PathBuf},
    sync::{
        atomic::{AtomicU64, Ordering},
        Arc, Mutex, PoisonError,
    },
    thread::JoinHandle,
    time::{Duration, Instant, SystemTime},
};

use blaulicht_shared::{
    ControlEvent, SaveEngineState, ShowfileArtNetReceiver, ShowfileArtNetState,
};
use crossbeam_channel::{Receiver, RecvTimeoutError, Sender};

use crate::{
    config::{self, CoreShowfile, ParsedShowfile, SharedConfig, ShowfileUiState},
    hub::{
        session_scope, ClientId, CommandOutcome, CommandResult, CommandSeq, HostCommand,
        SessionScope, SystemHub,
    },
    msg::SystemMessage,
    state::AppState,
};

/// How often the service compares the show with the last saved one.
pub const DIRTY_CHECK_INTERVAL: Duration = Duration::from_secs(2);
/// How often an edited show is saved by itself.
pub const AUTOSAVE_INTERVAL: Duration = Duration::from_secs(30);
/// How long the service thread waits for a request before it checks its
/// timers.
const POLL_INTERVAL: Duration = Duration::from_millis(200);

/// Whether the show has unsaved changes.
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum SaveStatus {
    /// No showfile is open: there is nothing to save to.
    NoShowfile,
    Clean,
    Dirty,
    Saving,
    /// The last save failed (with this error); the show is still unsaved.
    Failed(String),
}

impl SaveStatus {
    /// `true` while quitting or replacing the show would lose changes.
    pub fn has_unsaved_changes(&self) -> bool {
        matches!(
            self,
            SaveStatus::Dirty | SaveStatus::Saving | SaveStatus::Failed(_)
        )
    }
}

/// What [`SystemMessage::ShowfileStatus`] reports.
#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct ShowfileStatus {
    /// The open showfile.
    pub path: Option<PathBuf>,
    pub save: SaveStatus,
    /// When the show was last written to `path`. On the wire as UNIX time in
    /// milliseconds (`null` if never).
    #[serde(with = "crate::server::protocol::serde_as::unix_millis")]
    pub saved_at: Option<SystemTime>,
}

/// How the process ends after the shutdown hook ran.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ExitRequest {
    Quit,
    /// Exit with [`ExitRequest::RESTART_CODE`], so the launcher starts the
    /// process again.
    Restart,
}

impl ExitRequest {
    pub const RESTART_CODE: i32 = 42;

    pub fn code(self) -> i32 {
        match self {
            ExitRequest::Quit => 0,
            ExitRequest::Restart => Self::RESTART_CODE,
        }
    }
}

/// A showfile operation, from a client's host command.
#[derive(Debug)]
enum ShowfileCommand {
    Save,
    SaveAs(PathBuf),
    Load(PathBuf),
    Close,
    Exit { exit: ExitRequest, save: bool },
}

enum Request {
    Command {
        client: ClientId,
        seq: CommandSeq,
        command: ShowfileCommand,
    },
    FrontendSection {
        generation: u64,
        section: ShowfileUiState,
    },
    GlobalPluginState {
        key: String,
        value: String,
    },
    /// The frontend closed; `main` waits for the service to finish.
    Stop,
}

/// The engine's side of the persistence service (`AppState::persistence`):
/// mark the show edited, forward commands, read the exit request. Works
/// without a running service; requests then wait for one.
pub struct PersistenceLink {
    edits: AtomicU64,
    sender: Sender<Request>,
    receiver: Receiver<Request>,
    exit: Mutex<Option<ExitRequest>>,
}

impl Default for PersistenceLink {
    fn default() -> Self {
        let (sender, receiver) = crossbeam_channel::unbounded();
        Self {
            edits: AtomicU64::new(0),
            sender,
            receiver,
            exit: Mutex::new(None),
        }
    }
}

impl PersistenceLink {
    /// The show changed in a way the showfile saves.
    pub fn mark_edited(&self) {
        self.edits.fetch_add(1, Ordering::Relaxed);
    }

    /// How often the show was marked edited.
    pub fn edits(&self) -> u64 {
        self.edits.load(Ordering::Relaxed)
    }

    /// Hands a showfile or lifecycle command to the service. `Ok(None)`: the
    /// service answers it later; `Ok(Some(outcome))`: answered now;
    /// `Err(command)`: not the service's.
    pub(crate) fn forward(
        &self,
        client: ClientId,
        seq: CommandSeq,
        command: HostCommand,
    ) -> Result<Option<CommandOutcome>, HostCommand> {
        let command = match command {
            HostCommand::SetFrontendSection {
                generation,
                section,
            } => {
                self.send(Request::FrontendSection {
                    generation,
                    section,
                });
                return Ok(Some(CommandOutcome::Ok { created: None }));
            }
            HostCommand::SaveShowfile => ShowfileCommand::Save,
            HostCommand::SaveShowfileAs(path) => ShowfileCommand::SaveAs(path),
            HostCommand::LoadShowfile(path) => ShowfileCommand::Load(path),
            HostCommand::CloseShowfile => ShowfileCommand::Close,
            HostCommand::Quit { save } => ShowfileCommand::Exit {
                exit: ExitRequest::Quit,
                save,
            },
            HostCommand::Restart { save } => ShowfileCommand::Exit {
                exit: ExitRequest::Restart,
                save,
            },
            command => return Err(command),
        };
        self.send(Request::Command {
            client,
            seq,
            command,
        });
        Ok(None)
    }

    /// A plugin saved its global state (`key` is its storage key, `value`
    /// the encoded state): the service writes it into `config.toml`.
    pub fn save_global_plugin_state(&self, key: String, value: String) {
        self.send(Request::GlobalPluginState { key, value });
    }

    /// How the process should end, once `HostCommand::Quit` or `Restart`
    /// ran the shutdown hook.
    pub fn exit_request(&self) -> Option<ExitRequest> {
        *self.exit.lock().unwrap_or_else(PoisonError::into_inner)
    }

    fn send(&self, request: Request) {
        // Cannot fail: the link holds a receiver.
        let _ = self.sender.send(request);
    }
}

/// `true` if `event` changes what the showfile saves. `own_session` is
/// `false` for events from an operator session other than `DESK`: those
/// change only that session if they are [`SessionScope::Session`] (its
/// selection or focus; the showfile saves `DESK`'s), but their control
/// events edit scenes like everyone else's.
pub fn control_event_marks_showfile_dirty(event: &ControlEvent, own_session: bool) -> bool {
    match event {
        ControlEvent::Transaction(events) => events
            .iter()
            .any(|event| control_event_marks_showfile_dirty(event, own_session)),
        _ if !own_session && session_scope(event) == SessionScope::Session => false,
        ControlEvent::SelectGroup(_)
        | ControlEvent::DeSelectGroup(_)
        | ControlEvent::SelectFolder(_)
        | ControlEvent::LimitSelectionToFixtureInCurrentGroup(_)
        | ControlEvent::UnLimitSelectionToFixtureInCurrentGroup(_)
        | ControlEvent::RemoveSelection
        | ControlEvent::RemoveAllSelection
        | ControlEvent::PushSelection
        | ControlEvent::PopSelection
        | ControlEvent::MainUi(_)
        | ControlEvent::PluginUi(_, _)
        | ControlEvent::SetLiveMode(_)
        | ControlEvent::ToggleLiveMode
        | ControlEvent::MiscEvent { .. } => false,
        _ => true,
    }
}

/// The persistence service. Create it with [`PersistenceService::new`],
/// open the startup showfile ([`PersistenceService::open_startup`]), then
/// [`PersistenceService::spawn`] its thread.
pub struct PersistenceService {
    app: Arc<AppState>,
    hub: SystemHub,
    config: SharedConfig,
    requests: Receiver<Request>,
    /// Counts loaded shows ([`SystemMessage::ShowLoaded`]).
    generation: u64,
    /// Hash of the show as last loaded or saved; `None` without a showfile.
    baseline: Option<u64>,
    status: SaveStatus,
    saved_at: Option<SystemTime>,
    /// `PersistenceLink::edits` the status has caught up with.
    seen_edits: u64,
    /// `PersistenceLink::edits` when the show was loaded.
    edits_at_load: u64,
    /// The desk frontend's section, saved as the showfile's `ui`.
    frontend: Option<ShowfileUiState>,
    /// The generation the frontend last published its section for.
    frontend_generation: u64,
    /// Global plugin state not yet written to the config.
    pending_plugin_state: BTreeMap<String, String>,
    last_check: Instant,
    last_autosave: Instant,
    published: Option<ShowfileStatus>,
}

impl PersistenceService {
    pub fn new(app: Arc<AppState>, hub: SystemHub, config: SharedConfig) -> Self {
        let requests = app.persistence.receiver.clone();
        let now = Instant::now();
        Self {
            app,
            hub,
            config,
            requests,
            generation: 0,
            baseline: None,
            status: SaveStatus::NoShowfile,
            saved_at: None,
            seen_edits: 0,
            edits_at_load: 0,
            frontend: None,
            frontend_generation: 0,
            pending_plugin_state: BTreeMap::new(),
            last_check: now,
            last_autosave: now,
            published: None,
        }
    }

    /// Loads the showfile the config names at startup, or starts with an
    /// empty show. On failure the show is empty and has no showfile; the
    /// config keeps its entry, so the next start tries again.
    pub fn open_startup(&mut self, showfile: Option<PathBuf>) -> Result<(), String> {
        let result = match showfile {
            None => {
                self.install(ParsedShowfile::empty(), None);
                Ok(())
            }
            Some(path) => match config::parse_showfile(&path) {
                Ok(parsed) => {
                    self.install(parsed, Some(path.clone()));
                    tracing::info!(
                        "Loaded showfile {}",
                        config::absolute_showfile_path(&path).display()
                    );
                    Ok(())
                }
                Err(error) => {
                    tracing::error!("Failed to read showfile {}: {error:#}", path.display());
                    self.install(ParsedShowfile::empty(), None);
                    Err(format!("{error:#}"))
                }
            },
        };
        self.publish_status();
        result
    }

    #[cfg(test)]
    pub(crate) fn status(&self) -> &SaveStatus {
        &self.status
    }

    /// Runs the service on its own thread until `PersistenceThread::stop`.
    pub fn spawn(mut self) -> std::io::Result<PersistenceThread> {
        let app = Arc::clone(&self.app);
        let handle = std::thread::Builder::new()
            .name("persistence".to_string())
            .spawn(move || loop {
                let first = match self.requests.recv_timeout(POLL_INTERVAL) {
                    Ok(request) => Some(request),
                    Err(RecvTimeoutError::Timeout) => None,
                    // The link holds a sender; this doesn't happen.
                    Err(RecvTimeoutError::Disconnected) => break,
                };
                if first.is_some_and(|request| !self.handle(request)) {
                    break;
                }
                if !self.step(Instant::now()) {
                    break;
                }
            })?;
        Ok(PersistenceThread { handle, app })
    }

    /// Handles every queued request, then the timers (dirty check, autosave),
    /// and publishes the status if it changed. `false` once stopped.
    pub(crate) fn step(&mut self, now: Instant) -> bool {
        while let Ok(request) = self.requests.try_recv() {
            if !self.handle(request) {
                return false;
            }
        }
        self.flush_plugin_state();
        self.poll(now);
        self.publish_status();
        true
    }

    /// `false` for [`Request::Stop`].
    fn handle(&mut self, request: Request) -> bool {
        match request {
            Request::Command {
                client,
                seq,
                command,
            } => {
                let outcome = match self.run(command) {
                    Ok(()) => CommandOutcome::Ok { created: None },
                    Err(why) => CommandOutcome::Refused(why),
                };
                self.publish_status();
                self.hub.publish_to(
                    client,
                    SystemMessage::CommandResult(CommandResult {
                        client,
                        seq,
                        outcome,
                    }),
                );
            }
            Request::FrontendSection {
                generation,
                section,
            } => self.frontend_section(generation, section),
            Request::GlobalPluginState { key, value } => {
                self.pending_plugin_state.insert(key, value);
            }
            Request::Stop => {
                self.flush_plugin_state();
                // Closing the window without Quit still ends the show.
                if self.app.persistence.exit_request().is_none() {
                    self.app.set_grand_master_percent(100);
                }
                return false;
            }
        }
        true
    }

    fn run(&mut self, command: ShowfileCommand) -> Result<(), String> {
        match command {
            ShowfileCommand::Save => self.save(),
            ShowfileCommand::SaveAs(path) => self.save_as(path),
            ShowfileCommand::Load(path) => self.load(path),
            ShowfileCommand::Close => {
                self.install(ParsedShowfile::empty(), None);
                self.write_last_showfile(None);
                tracing::info!("Closed showfile");
                Ok(())
            }
            ShowfileCommand::Exit { exit, save } => self.exit(exit, save),
        }
    }

    //
    // Commands.
    //

    fn save(&mut self) -> Result<(), String> {
        let Some(path) = self.showfile() else {
            self.status = SaveStatus::NoShowfile;
            return Err("No showfile is open".to_string());
        };
        self.write_show(&path, false)
    }

    fn save_as(&mut self, path: PathBuf) -> Result<(), String> {
        let (stage, previous) = {
            let engine = self.read_engine();
            (
                engine.stage().clone(),
                engine.showfile().map(Path::to_path_buf),
            )
        };
        let relocated = match previous {
            Some(previous) => {
                crate::stage_assets::copy_assets_for_save_as(&stage, &previous, &path)
                    .map_err(|error| format!("Failed to copy stage assets: {error:#}"))?
            }
            None => stage,
        };
        self.app
            .dmx_engine
            .write()
            .unwrap_or_else(PoisonError::into_inner)
            .relocate_stage(&relocated, path.clone());
        self.write_show(&path, true)?;
        self.write_last_showfile(Some(path));
        Ok(())
    }

    fn load(&mut self, path: PathBuf) -> Result<(), String> {
        let parsed = config::parse_showfile(&path).map_err(|error| {
            tracing::error!("Failed to read showfile {}: {error:#}", path.display());
            format!(
                "Could not load {}: {error:#}",
                config::showfile_display_name(&path)
            )
        })?;
        self.install(parsed, Some(path.clone()));
        self.write_last_showfile(Some(path.clone()));
        tracing::info!(
            "Loaded showfile {}",
            config::absolute_showfile_path(&path).display()
        );
        Ok(())
    }

    /// The shutdown hook: save (if asked and a showfile is open), reset the
    /// grand master, then tell `main` how to exit.
    fn exit(&mut self, exit: ExitRequest, save: bool) -> Result<(), String> {
        if save {
            if let Some(path) = self.showfile() {
                self.write_show(&path, false)?;
            }
        }
        self.app.set_grand_master_percent(100);
        *self
            .app
            .persistence
            .exit
            .lock()
            .unwrap_or_else(PoisonError::into_inner) = Some(exit);
        tracing::info!("Shutdown hook done ({exit:?})");
        self.hub.publish(SystemMessage::Exiting {
            restart: exit == ExitRequest::Restart,
        });
        Ok(())
    }

    fn frontend_section(&mut self, generation: u64, section: ShowfileUiState) {
        if generation != self.generation {
            return;
        }
        if self.frontend_generation != generation {
            // The frontend's form of the section it was handed on load: the
            // show as loaded, not an edit.
            self.frontend_generation = generation;
            self.frontend = Some(section);
            let untouched = self.app.persistence.edits() == self.edits_at_load
                && self.status == SaveStatus::Clean;
            if untouched {
                self.baseline = self.snapshot_hash().ok();
            }
        } else if self.frontend.as_ref() != Some(&section) {
            self.frontend = Some(section);
            self.app.persistence.mark_edited();
        }
    }

    //
    // Showfile.
    //

    /// Puts `parsed` into the engine as a new show with `file` as its
    /// showfile, and announces it.
    fn install(&mut self, parsed: ParsedShowfile, file: Option<PathBuf>) {
        self.app.set_grand_master_percent(100);
        let has_file = file.is_some();
        let section = {
            // Lock order: engine, then Art-Net (as the Art-Net commands).
            let mut engine = self
                .app
                .dmx_engine
                .write()
                .unwrap_or_else(PoisonError::into_inner);
            let mut artnet = self
                .app
                .artnet_output
                .write()
                .unwrap_or_else(PoisonError::into_inner);
            config::install_showfile(
                parsed,
                file,
                &mut engine,
                &mut artnet,
                &self.app.plugin_state_storage,
            )
        };
        self.generation += 1;
        self.frontend.clone_from(&section);
        self.frontend_generation = 0;
        self.edits_at_load = self.app.persistence.edits();
        self.seen_edits = self.edits_at_load;
        self.saved_at = None;
        self.baseline = if has_file {
            self.snapshot_hash().ok()
        } else {
            None
        };
        self.status = if has_file {
            SaveStatus::Clean
        } else {
            SaveStatus::NoShowfile
        };
        self.hub.publish(SystemMessage::ShowLoaded {
            generation: self.generation,
            frontend_section: section,
        });
    }

    /// Writes the show to `path`. Skipped if it is unchanged since the last
    /// load or save, unless `force` (Save As).
    fn write_show(&mut self, path: &Path, force: bool) -> Result<(), String> {
        self.status = SaveStatus::Saving;
        self.publish_status();
        let edits = self.app.persistence.edits();
        let (serialized, hash) = match serialize(&self.build()) {
            Ok(result) => result,
            Err(error) => {
                self.status = SaveStatus::Failed(error.clone());
                return Err(format!("Save failed: {error}"));
            }
        };
        if !force && self.baseline == Some(hash) {
            self.status = SaveStatus::Clean;
            self.seen_edits = edits;
            return Ok(());
        }
        if let Err(error) = config::write_atomic(path, serialized.as_bytes()) {
            let error = format!("{error:#}");
            tracing::error!("Failed to save showfile: {error}");
            self.status = SaveStatus::Failed(error.clone());
            return Err(format!("Save failed: {error}"));
        }
        tracing::info!(
            "Saved showfile {}",
            config::absolute_showfile_path(path).display()
        );
        self.baseline = Some(hash);
        self.saved_at = Some(SystemTime::now());
        self.status = SaveStatus::Clean;
        // An edit made while the file was written shows up in `poll`.
        self.seen_edits = edits;
        Ok(())
    }

    /// The showfile as it would be saved now. Takes each lock on its own,
    /// briefly.
    fn build(&self) -> CoreShowfile {
        let (engine, stage) = {
            let engine = self.read_engine();
            (engine.0.clone(), engine.stage().clone())
        };
        let plugin_state = self
            .app
            .plugin_state_storage
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .clone();
        let artnet = ShowfileArtNetState {
            receivers: self
                .app
                .artnet_output
                .read()
                .unwrap_or_else(PoisonError::into_inner)
                .receivers
                .iter()
                .filter(|receiver| receiver.owner_plugin_id.is_none())
                .map(|receiver| ShowfileArtNetReceiver {
                    address: receiver.address,
                    enabled: receiver.enabled,
                    universes: (!receiver.sends_all_universes())
                        .then_some((receiver.first_universe, receiver.last_universe)),
                })
                .collect(),
        };
        let mut engine = SaveEngineState::from(engine);
        clear_volatile_engine_state(&mut engine);
        CoreShowfile {
            format_version: config::current_showfile_version(),
            engine,
            artnet,
            plugin_state,
            stage,
            ui: self.frontend.clone(),
        }
    }

    fn snapshot_hash(&self) -> Result<u64, String> {
        serialize(&self.build()).map(|(_, hash)| hash)
    }

    fn showfile(&self) -> Option<PathBuf> {
        self.read_engine().showfile().map(Path::to_path_buf)
    }

    fn read_engine(&self) -> std::sync::RwLockReadGuard<'_, crate::dmx::EngineState> {
        self.app
            .dmx_engine
            .read()
            .unwrap_or_else(PoisonError::into_inner)
    }

    //
    // Timers.
    //

    fn poll(&mut self, now: Instant) {
        let edits = self.app.persistence.edits();
        if edits != self.seen_edits {
            self.seen_edits = edits;
            if self.baseline.is_some() && !matches!(self.status, SaveStatus::Failed(_)) {
                self.status = SaveStatus::Dirty;
            }
        }
        if now.saturating_duration_since(self.last_check) >= DIRTY_CHECK_INTERVAL {
            self.last_check = now;
            self.check_dirty();
        }
        if now.saturating_duration_since(self.last_autosave) >= AUTOSAVE_INTERVAL {
            self.last_autosave = now;
            if matches!(self.status, SaveStatus::Dirty | SaveStatus::Failed(_)) {
                // A failure is in the status (and the log).
                let _ = self.save();
            }
        }
    }

    /// Compares the show with the last load or save.
    fn check_dirty(&mut self) {
        let Some(baseline) = self.baseline else {
            self.status = SaveStatus::NoShowfile;
            return;
        };
        let edits = self.app.persistence.edits();
        match self.snapshot_hash() {
            // An edit during the check is handled by the next one.
            Ok(_) if self.app.persistence.edits() != edits => {}
            Ok(hash) if hash == baseline => {
                self.status = SaveStatus::Clean;
                self.seen_edits = edits;
            }
            Ok(_) => {
                if !matches!(self.status, SaveStatus::Failed(_)) {
                    self.status = SaveStatus::Dirty;
                }
                self.seen_edits = edits;
            }
            Err(error) => tracing::warn!("Failed to check showfile dirty state: {error}"),
        }
    }

    //
    // Config and status.
    //

    fn write_last_showfile(&self, path: Option<PathBuf>) {
        if let Err(error) = self
            .config
            .update(|config| config.last_open_showfile = path)
        {
            tracing::error!("Failed to persist the open showfile in the config: {error:#}");
        }
    }

    fn flush_plugin_state(&mut self) {
        if self.pending_plugin_state.is_empty() {
            return;
        }
        let pending = std::mem::take(&mut self.pending_plugin_state);
        if let Err(error) = self
            .config
            .update(|config| config.plugin_state.extend(pending))
        {
            tracing::error!("Failed to write config after global state save: {error:#}");
        }
    }

    fn publish_status(&mut self) {
        let status = ShowfileStatus {
            path: self.showfile(),
            save: self.status.clone(),
            saved_at: self.saved_at,
        };
        if self.published.as_ref() != Some(&status) {
            self.published = Some(status.clone());
            self.hub.publish(SystemMessage::ShowfileStatus(status));
        }
    }
}

/// The running service thread, held by `main`.
pub struct PersistenceThread {
    handle: JoinHandle<()>,
    app: Arc<AppState>,
}

impl PersistenceThread {
    /// Stops the service once its queued requests are done (resetting the
    /// grand master if no `Quit`/`Restart` ran the shutdown hook), and
    /// returns how the process should exit.
    pub fn stop(self) -> Option<ExitRequest> {
        self.app.persistence.send(Request::Stop);
        if self.handle.join().is_err() {
            tracing::error!("The persistence thread panicked");
        }
        self.app.persistence.exit_request()
    }
}

fn serialize(showfile: &CoreShowfile) -> Result<(String, u64), String> {
    let serialized = serde_json::to_string_pretty(showfile).map_err(|error| error.to_string())?;
    let mut hasher = std::hash::DefaultHasher::new();
    serialized.hash(&mut hasher);
    Ok((serialized, hasher.finish()))
}

/// Animation timers advance on every engine tick; keeping them out of the
/// snapshot prevents a running animation from flagging the showfile dirty.
fn clear_volatile_engine_state(engine: &mut SaveEngineState) {
    for scene in &mut engine.scenes {
        for selection in &mut scene.value_mut().sink.active_animations {
            for animation in selection.value_mut() {
                for timer in &mut animation.value_mut().fixture_timers {
                    *timer.value_mut() = Default::default();
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use std::{
        path::{Path, PathBuf},
        time::{Duration, Instant},
    };

    use blaulicht_shared::{
        folder::{FolderItem, FolderKind},
        ControlEvent, ControlEventMessage, EventOriginator,
    };
    use tempdir::TempDir;

    use super::{
        control_event_marks_showfile_dirty, ExitRequest, PersistenceService, SaveStatus,
        AUTOSAVE_INTERVAL, DIRTY_CHECK_INTERVAL,
    };
    use crate::{
        client::{
            testing::{small_rig, TestEngine, TwoClients},
            EngineClient, LocalClient,
        },
        config::{Config, CoreShowfile, ShowfileDockNode, ShowfileExternalScreen, ShowfileUiState},
        hub::{ClientId, CommandOutcome, HostCommand, ModelBytes},
        msg::SystemMessage,
    };

    const OK: CommandOutcome = CommandOutcome::Ok { created: None };

    fn web(event: ControlEvent) -> ControlEventMessage {
        ControlEventMessage::new(EventOriginator::Web, event)
    }

    /// Sends a showfile command from `client`: the engine forwards it on a
    /// tick, the service (stepped at `now`) answers it.
    fn command(
        engine: &mut TestEngine,
        service: &mut PersistenceService,
        client: &LocalClient,
        now: Instant,
        command: HostCommand,
    ) -> CommandOutcome {
        let seq = client.send(command);
        let deadline = Instant::now() + Duration::from_secs(5);
        loop {
            assert!(Instant::now() < deadline, "no answer to {seq}");
            engine.tick();
            service.step(now);
            if let Some(result) = client.take_results().into_iter().find(|r| r.seq == seq) {
                return result.outcome;
            }
            std::thread::sleep(Duration::from_millis(1));
        }
    }

    /// A service for `engine` that announced its (new, empty) startup show,
    /// which is then seeded with [`small_rig`] again.
    fn start_service(engine: &TestEngine, config: PathBuf) -> PersistenceService {
        let mut service = engine.persistence(config);
        assert_eq!(service.open_startup(None), Ok(()));
        engine.edit_show(small_rig);
        service
    }

    fn read_showfile(path: &Path) -> CoreShowfile {
        serde_json::from_str(&std::fs::read_to_string(path).unwrap()).unwrap()
    }

    fn read_config(path: &Path) -> Config {
        toml::from_str(&std::fs::read_to_string(path).unwrap()).unwrap()
    }

    fn rename_group(group: u8, name: &str) -> HostCommand {
        HostCommand::RenameGroup {
            group,
            name: name.to_string(),
        }
    }

    fn group_name(showfile: &CoreShowfile, group: u8) -> String {
        let engine: blaulicht_shared::EngineState = showfile.engine.clone().try_into().unwrap();
        engine.groups[&group].name.clone()
    }

    fn scene_name(showfile: &CoreShowfile, scene: u8) -> String {
        let engine: blaulicht_shared::EngineState = showfile.engine.clone().try_into().unwrap();
        engine.scenes[&scene].name.clone()
    }

    fn system_messages(client: &LocalClient) -> Vec<SystemMessage> {
        client.take_results();
        std::iter::from_fn(|| client.try_recv_system()).collect()
    }

    fn section(width: f32) -> ShowfileUiState {
        ShowfileUiState {
            main_screen_desktop_mode: None,
            external_screens: vec![ShowfileExternalScreen {
                width,
                height: 1080.0,
                x: None,
                y: None,
                owner_plugin_id: None,
                layout: ShowfileDockNode::Leaf {
                    tabs: Vec::new(),
                    active: 0,
                },
            }],
        }
    }

    /// The acceptance test: edits from the desk and from a remote client
    /// both mark the show edited, and one autosave writes both.
    #[test]
    fn two_clients_edits_mark_the_show_dirty_and_one_autosave_writes_both() {
        let mut h = TwoClients::start(small_rig);
        let directory = TempDir::new("blaulicht-autosave").unwrap();
        let showfile = directory.path().join("show.json");
        let mut service = start_service(&h.engine, directory.path().join("config.toml"));
        let now = Instant::now();
        assert_eq!(*service.status(), SaveStatus::NoShowfile);

        assert_eq!(
            command(
                &mut h.engine,
                &mut service,
                &h.desk,
                now,
                HostCommand::SaveShowfileAs(showfile.clone())
            ),
            OK
        );
        assert_eq!(*service.status(), SaveStatus::Clean);
        assert_eq!(h.remote.showfile_path(), Some(showfile.clone()));
        let edits = h.engine.app().persistence.edits();

        // The desk's edit.
        assert_eq!(
            h.engine.round_trip(&h.desk, rename_group(0, "Desk edit")),
            OK
        );
        assert_eq!(h.engine.app().persistence.edits(), edits + 1);
        service.step(now);
        assert_eq!(*service.status(), SaveStatus::Dirty);

        // The remote's edits: a host command and a control event.
        assert_eq!(
            h.engine.round_trip(
                &h.remote,
                HostCommand::RenameScene {
                    scene: 1,
                    name: "Remote edit".to_string()
                }
            ),
            OK
        );
        assert_eq!(h.engine.app().persistence.edits(), edits + 2);
        assert_eq!(
            h.engine
                .round_trip(&h.remote, web(ControlEvent::SelectGroup(2))),
            OK
        );
        // Selecting changes only the remote's session, which isn't saved.
        assert_eq!(h.engine.app().persistence.edits(), edits + 2);
        assert_eq!(
            h.engine
                .round_trip(&h.remote, web(ControlEvent::SetAlpha(99))),
            OK
        );
        assert_eq!(h.engine.app().persistence.edits(), edits + 3);
        // A refused command changes nothing.
        assert!(matches!(
            h.engine.round_trip(&h.remote, rename_group(9, "Nope")),
            CommandOutcome::Refused(_)
        ));
        assert_eq!(h.engine.app().persistence.edits(), edits + 3);
        assert_eq!(group_name(&read_showfile(&showfile), 0), "G0");

        // One autosave writes both.
        service.step(now + AUTOSAVE_INTERVAL);
        assert_eq!(*service.status(), SaveStatus::Clean);
        let saved = read_showfile(&showfile);
        assert_eq!(group_name(&saved, 0), "Desk edit");
        assert_eq!(scene_name(&saved, 1), "Remote edit");

        // Plugins' showfile state changes no command reports: the dirty
        // check finds it, and the next autosave writes it.
        h.engine
            .app()
            .plugin_state_storage
            .lock()
            .unwrap()
            .insert("plugin_3".to_string(), "{\"x\":1}".to_string());
        let later = now + AUTOSAVE_INTERVAL + DIRTY_CHECK_INTERVAL;
        service.step(later);
        assert_eq!(*service.status(), SaveStatus::Dirty);
        service.step(later + AUTOSAVE_INTERVAL);
        assert_eq!(
            read_showfile(&showfile).plugin_state["plugin_3"],
            "{\"x\":1}"
        );

        // An edit undone before the check is clean again.
        assert_eq!(h.engine.round_trip(&h.desk, rename_group(0, "Other")), OK);
        assert_eq!(
            h.engine.round_trip(&h.desk, rename_group(0, "Desk edit")),
            OK
        );
        let later = later + AUTOSAVE_INTERVAL;
        service.step(later);
        assert_eq!(*service.status(), SaveStatus::Dirty);
        service.step(later + DIRTY_CHECK_INTERVAL);
        assert_eq!(*service.status(), SaveStatus::Clean);
    }

    #[test]
    fn saving_without_a_showfile_is_refused_and_save_reports_when_done() {
        let mut h = TwoClients::start(small_rig);
        let directory = TempDir::new("blaulicht-save").unwrap();
        let showfile = directory.path().join("show.json");
        let mut service = start_service(&h.engine, directory.path().join("config.toml"));
        let now = Instant::now();
        assert_eq!(h.engine.round_trip(&h.desk, rename_group(0, "Edit")), OK);
        service.step(now);
        // Without a showfile there is nothing to save to.
        assert_eq!(*service.status(), SaveStatus::NoShowfile);
        assert_eq!(
            command(
                &mut h.engine,
                &mut service,
                &h.remote,
                now,
                HostCommand::SaveShowfile
            ),
            CommandOutcome::Refused("No showfile is open".to_string())
        );

        assert_eq!(
            command(
                &mut h.engine,
                &mut service,
                &h.remote,
                now,
                HostCommand::SaveShowfileAs(showfile.clone())
            ),
            OK
        );
        assert_eq!(group_name(&read_showfile(&showfile), 0), "Edit");
        assert_eq!(
            read_config(&directory.path().join("config.toml")).last_open_showfile,
            Some(showfile.clone())
        );

        // A manual save writes at once; the status is published.
        assert_eq!(h.engine.round_trip(&h.desk, rename_group(0, "Saved")), OK);
        assert_eq!(
            command(
                &mut h.engine,
                &mut service,
                &h.desk,
                now,
                HostCommand::SaveShowfile
            ),
            OK
        );
        assert_eq!(group_name(&read_showfile(&showfile), 0), "Saved");
        let statuses: Vec<_> = system_messages(&h.desk)
            .into_iter()
            .filter_map(|message| match message {
                SystemMessage::ShowfileStatus(status) => Some(status),
                _ => None,
            })
            .collect();
        let last = statuses.last().expect("a status");
        assert_eq!(last.save, SaveStatus::Clean);
        assert_eq!(last.path, Some(showfile.clone()));
        assert!(last.saved_at.is_some());

        // A failing write is refused, and the status says so.
        let blocker = directory.path().join("blocker");
        std::fs::write(&blocker, b"").unwrap();
        assert!(matches!(
            command(
                &mut h.engine,
                &mut service,
                &h.desk,
                now,
                HostCommand::SaveShowfileAs(blocker.join("show.json"))
            ),
            CommandOutcome::Refused(why) if why.starts_with("Save failed")
        ));
        assert!(matches!(service.status(), SaveStatus::Failed(_)));
    }

    #[test]
    fn loading_and_closing_replace_the_show_and_announce_it() {
        let mut h = TwoClients::start(small_rig);
        let directory = TempDir::new("blaulicht-load").unwrap();
        let first = directory.path().join("first.json");
        let config = directory.path().join("config.toml");
        let mut service = start_service(&h.engine, config.clone());
        let now = Instant::now();
        assert_eq!(h.engine.round_trip(&h.desk, rename_group(0, "First")), OK);
        assert_eq!(
            command(
                &mut h.engine,
                &mut service,
                &h.desk,
                now,
                HostCommand::SaveShowfileAs(first.clone())
            ),
            OK
        );
        h.engine.app().set_grand_master_percent(40);
        assert_eq!(h.engine.round_trip(&h.desk, rename_group(0, "Unsaved")), OK);
        assert_eq!(
            h.engine
                .round_trip(&h.remote, web(ControlEvent::SelectGroup(1))),
            OK
        );
        system_messages(&h.remote);

        // A file that can't be read changes nothing.
        let missing = directory.path().join("missing.json");
        assert!(matches!(
            command(
                &mut h.engine,
                &mut service,
                &h.remote,
                now,
                HostCommand::LoadShowfile(missing)
            ),
            CommandOutcome::Refused(why) if why.starts_with("Could not load missing.json")
        ));
        assert_eq!(
            h.desk.with_show(|show| show.0.groups[&0].name.clone()),
            "Unsaved"
        );
        assert_eq!(h.desk.showfile_path(), Some(first.clone()));

        // Loading: the file's show, a new generation, sessions reset, the
        // grand master reset, and the config remembers the file.
        assert_eq!(
            command(
                &mut h.engine,
                &mut service,
                &h.remote,
                now,
                HostCommand::LoadShowfile(first.clone())
            ),
            OK
        );
        assert_eq!(
            h.desk.with_show(|show| show.0.groups[&0].name.clone()),
            "First"
        );
        assert_eq!(h.engine.app().grand_master_percent(), 100);
        assert!(h
            .remote
            .with_session(|_, me| me.selection.group_ids.is_empty())
            .unwrap());
        assert_eq!(*service.status(), SaveStatus::Clean);
        assert_eq!(read_config(&config).last_open_showfile, Some(first.clone()));
        let generations: Vec<u64> = system_messages(&h.remote)
            .into_iter()
            .filter_map(|message| match message {
                SystemMessage::ShowLoaded { generation, .. } => Some(generation),
                _ => None,
            })
            .collect();
        assert_eq!(generations, vec![2]);

        // Closing: an empty show without a showfile.
        assert_eq!(
            command(
                &mut h.engine,
                &mut service,
                &h.desk,
                now,
                HostCommand::CloseShowfile
            ),
            OK
        );
        assert_eq!(h.desk.showfile_path(), None);
        assert!(h.desk.with_show(|show| show.0.groups.is_empty()));
        assert_eq!(*service.status(), SaveStatus::NoShowfile);
        assert_eq!(read_config(&config).last_open_showfile, None);
        assert!(system_messages(&h.desk).iter().any(|message| matches!(
            message,
            SystemMessage::ShowLoaded {
                generation: 3,
                frontend_section: None
            }
        )));
    }

    #[test]
    fn the_frontend_section_is_saved_and_handed_back_on_load() {
        let mut h = TwoClients::start(small_rig);
        let directory = TempDir::new("blaulicht-frontend-section").unwrap();
        let showfile = directory.path().join("show.json");
        let mut service = start_service(&h.engine, directory.path().join("config.toml"));
        let now = Instant::now();
        assert_eq!(
            command(
                &mut h.engine,
                &mut service,
                &h.desk,
                now,
                HostCommand::SaveShowfileAs(showfile.clone())
            ),
            OK
        );
        let set = |generation, width| HostCommand::SetFrontendSection {
            generation,
            section: section(width),
        };

        // The first section for a show is the show as loaded, not an edit.
        let edits = h.engine.app().persistence.edits();
        assert_eq!(h.engine.round_trip(&h.desk, set(1, 1920.0)), OK);
        service.step(now);
        assert_eq!(*service.status(), SaveStatus::Clean);
        assert_eq!(h.engine.app().persistence.edits(), edits);
        // A section for another show is ignored.
        assert_eq!(h.engine.round_trip(&h.desk, set(7, 640.0)), OK);
        service.step(now);
        assert_eq!(*service.status(), SaveStatus::Clean);
        // A changed section is an edit.
        assert_eq!(h.engine.round_trip(&h.desk, set(1, 1280.0)), OK);
        service.step(now);
        assert_eq!(*service.status(), SaveStatus::Dirty);
        assert_eq!(
            command(
                &mut h.engine,
                &mut service,
                &h.desk,
                now,
                HostCommand::SaveShowfile
            ),
            OK
        );
        assert_eq!(read_showfile(&showfile).ui, Some(section(1280.0)));

        system_messages(&h.desk);
        assert_eq!(
            command(
                &mut h.engine,
                &mut service,
                &h.remote,
                now,
                HostCommand::LoadShowfile(showfile.clone())
            ),
            OK
        );
        assert!(system_messages(&h.desk).into_iter().any(|message| matches!(
            message,
            SystemMessage::ShowLoaded { generation: 2, frontend_section: Some(s) } if s == section(1280.0)
        )));

        // A late subscriber gets the latest show and status replayed.
        let late = h.engine.hub().subscribe(ClientId::Remote(99));
        let replayed: Vec<_> = std::iter::from_fn(|| late.try_recv()).collect();
        assert!(replayed
            .iter()
            .any(|message| matches!(message, SystemMessage::ShowLoaded { generation: 2, .. })));
        assert!(replayed.iter().any(|message| matches!(
            message,
            SystemMessage::ShowfileStatus(status) if status.path.as_deref() == Some(showfile.as_path())
        )));
    }

    #[test]
    fn save_as_copies_model_assets_next_to_the_new_file() {
        let mut h = TwoClients::start(small_rig);
        let directory = TempDir::new("blaulicht-save-as-assets").unwrap();
        let first = directory.path().join("first.json");
        let second = directory.path().join("second.json");
        let mut service = start_service(&h.engine, directory.path().join("config.toml"));
        let now = Instant::now();
        assert_eq!(
            command(
                &mut h.engine,
                &mut service,
                &h.desk,
                now,
                HostCommand::SaveShowfileAs(first.clone())
            ),
            OK
        );
        let bytes =
            std::fs::read(Path::new(env!("CARGO_MANIFEST_DIR")).join("../../test-assets/Box.glb"))
                .unwrap();
        let import = HostCommand::ImportStageModel {
            file_name: "Box.glb".to_string(),
            bytes: ModelBytes(bytes.clone().into()),
        };
        assert!(matches!(
            h.engine.round_trip(&h.desk, import),
            CommandOutcome::Ok { .. }
        ));

        assert_eq!(
            command(
                &mut h.engine,
                &mut service,
                &h.desk,
                now,
                HostCommand::SaveShowfileAs(second.clone())
            ),
            OK
        );
        let asset = h
            .desk
            .with_show(|show| show.stage().assets.values().next().cloned())
            .unwrap();
        let stored: PathBuf = crate::stage_assets::resolve_asset(&second, &asset).unwrap();
        assert!(stored.starts_with(directory.path().join("second.assets")));
        assert_eq!(std::fs::read(stored).unwrap(), bytes);
        assert_eq!(read_showfile(&second).stage.assets.len(), 1);
        assert_eq!(h.desk.showfile_path(), Some(second));
    }

    #[test]
    fn global_plugin_state_reaches_the_config_without_a_frontend() {
        let engine = TestEngine::start(small_rig);
        let directory = TempDir::new("blaulicht-global-state").unwrap();
        let config = directory.path().join("config.toml");
        let mut service = start_service(&engine, config.clone());
        engine
            .app()
            .persistence
            .save_global_plugin_state("plugin_2".to_string(), "{\"a\":1}".to_string());
        engine
            .app()
            .persistence
            .save_global_plugin_state("plugin_2".to_string(), "{\"a\":2}".to_string());
        service.step(Instant::now());
        assert_eq!(read_config(&config).plugin_state["plugin_2"], "{\"a\":2}");
    }

    #[test]
    fn quit_and_restart_run_the_shutdown_hook() {
        let mut h = TwoClients::start(small_rig);
        let directory = TempDir::new("blaulicht-quit").unwrap();
        let showfile = directory.path().join("show.json");
        let mut service = start_service(&h.engine, directory.path().join("config.toml"));
        let now = Instant::now();
        assert_eq!(
            command(
                &mut h.engine,
                &mut service,
                &h.desk,
                now,
                HostCommand::SaveShowfileAs(showfile.clone())
            ),
            OK
        );

        // Restart without saving: the edit stays unsaved, exit code 42.
        assert_eq!(h.engine.round_trip(&h.desk, rename_group(0, "Edit")), OK);
        h.engine.app().set_grand_master_percent(0);
        system_messages(&h.remote);
        assert_eq!(
            command(
                &mut h.engine,
                &mut service,
                &h.desk,
                now,
                HostCommand::Restart { save: false }
            ),
            OK
        );
        assert_eq!(group_name(&read_showfile(&showfile), 0), "G0");
        assert_eq!(h.engine.app().grand_master_percent(), 100);
        assert_eq!(
            h.engine.app().persistence.exit_request(),
            Some(ExitRequest::Restart)
        );
        assert_eq!(ExitRequest::Restart.code(), 42);
        assert!(system_messages(&h.remote)
            .iter()
            .any(|message| matches!(message, SystemMessage::Exiting { restart: true })));

        // Quit with a final save.
        assert_eq!(
            command(
                &mut h.engine,
                &mut service,
                &h.remote,
                now,
                HostCommand::Quit { save: true }
            ),
            OK
        );
        assert_eq!(group_name(&read_showfile(&showfile), 0), "Edit");
        assert_eq!(
            h.engine.app().persistence.exit_request(),
            Some(ExitRequest::Quit)
        );
    }

    #[test]
    fn a_quit_whose_save_fails_is_refused() {
        let mut h = TwoClients::start(small_rig);
        let directory = TempDir::new("blaulicht-quit-fails").unwrap();
        let blocker = directory.path().join("blocker");
        std::fs::write(&blocker, b"").unwrap();
        let mut service = start_service(&h.engine, directory.path().join("config.toml"));
        let now = Instant::now();
        // A showfile that can't be written.
        h.engine
            .edit_show(|show| show.load_stage(Default::default(), Some(blocker.join("show.json"))));
        assert_eq!(h.engine.round_trip(&h.desk, rename_group(0, "Edit")), OK);
        h.engine.app().set_grand_master_percent(0);
        assert!(matches!(
            command(
                &mut h.engine,
                &mut service,
                &h.desk,
                now,
                HostCommand::Quit { save: true }
            ),
            CommandOutcome::Refused(why) if why.starts_with("Save failed")
        ));
        assert_eq!(h.engine.app().persistence.exit_request(), None);
        assert_eq!(h.engine.app().grand_master_percent(), 0);
    }

    #[test]
    fn a_startup_showfile_that_cant_be_read_leaves_an_empty_show() {
        let engine = TestEngine::start(small_rig);
        let directory = TempDir::new("blaulicht-startup").unwrap();
        let broken = directory.path().join("broken.json");
        std::fs::write(&broken, b"{ not json").unwrap();
        let mut service = engine.persistence(directory.path().join("config.toml"));
        assert!(service.open_startup(Some(broken)).is_err());
        assert_eq!(*service.status(), SaveStatus::NoShowfile);
        let desk = engine.connect_desk();
        assert_eq!(desk.showfile_path(), None);
        assert!(desk.with_show(|show| show.0.groups.is_empty()));
    }

    #[test]
    fn selecting_a_folder_is_not_show_data_but_editing_folders_is() {
        assert!(!control_event_marks_showfile_dirty(
            &ControlEvent::SelectFolder(0),
            true
        ));
        // The Programmer's folder deselect and the SelectFolder rollback.
        assert!(!control_event_marks_showfile_dirty(
            &ControlEvent::Transaction(vec![
                ControlEvent::DeSelectGroup(0),
                ControlEvent::LimitSelectionToFixtureInCurrentGroup(1),
            ]),
            true
        ));
        for event in [
            ControlEvent::CreateFolder {
                id: 0,
                kind: FolderKind::Groups,
                parent: None,
                name: "Stage".to_string(),
            },
            ControlEvent::RenameFolder(0, "Front".to_string()),
            ControlEvent::DeleteFolder(0),
            ControlEvent::MoveToFolder {
                item: FolderItem::Group(1),
                folder: Some(0),
            },
            ControlEvent::ReorderFolder(0, 2),
        ] {
            assert!(
                control_event_marks_showfile_dirty(&event, true),
                "{event:?}"
            );
        }
    }

    /// Another operator's focus changes only their session (the showfile
    /// saves the desk's), but their control events edit the show.
    #[test]
    fn other_sessions_mark_the_show_dirty_only_when_they_edit_it() {
        for event in [
            ControlEvent::SetSceneFocus(1),
            ControlEvent::SetFocusedSceneGraph(Some(0)),
        ] {
            assert!(
                control_event_marks_showfile_dirty(&event, true),
                "{event:?}"
            );
            assert!(
                !control_event_marks_showfile_dirty(&event, false),
                "{event:?}"
            );
        }
        for event in [
            ControlEvent::SetColor((1, 2, 3)),
            ControlEvent::SetChannelOverride(0, 1, 255),
            ControlEvent::Transaction(vec![
                ControlEvent::SetSceneFocus(1),
                ControlEvent::SetAlpha(9),
            ]),
        ] {
            assert!(
                control_event_marks_showfile_dirty(&event, false),
                "{event:?}"
            );
        }
        assert!(!control_event_marks_showfile_dirty(
            &ControlEvent::Transaction(vec![
                ControlEvent::SetSceneFocus(1),
                ControlEvent::SelectGroup(0),
            ]),
            false
        ));
    }
}
