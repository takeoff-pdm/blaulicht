//! Setlist: follows a pre-analyzed setlist and fires cues ahead of the music.
//!
//! The analyzer (`crates/setlist_analyzer`) writes a track map per track and
//! a `setlist.json`; this plugin loads them, follows which track plays where,
//! and runs the cue plan of `blaulicht-setlist`: every section switches to
//! its look (an exclusive overlay stack), builds ramp up, the beat before a
//! drop goes dark and the drop hits on its downbeat.
//!
//! Two ways to follow:
//! - **Beat clock**: the operator picks the track and syncs it (button on bar
//!   1, "Drop!" on a drop, or a click on the timeline); the engine's beats
//!   keep it in time. With "audio sync" on in the Pro DJ Link plugin, those
//!   beats come from the CDJs.
//! - **Decks**: deck messages on `127.0.0.1:9120` say which track plays at
//!   which position (see `blaulicht_setlist::deck`), so track changes,
//!   seeks and loops are followed on their own.

use blaulicht_plugin_framework as bpf;
use blaulicht_plugin_framework::{ui, Plugin, UdpPort};
use blaulicht_setlist::{
    clock::ClockInput, deck::DEFAULT_PORT, plan, runner::Fired, Action, BeatClock, CueKind,
    DeckMessage, Decks, Runner, SectionKind, Setlist, ShowSettings, TrackMap, BEATS_PER_BAR,
};
use blaulicht_shared::{
    AnimationSpeedModifier, ControlEvent, EngineState, LogLevel, PluginUiEvent, TickInput,
};
use serde::{Deserialize, Serialize};

const TXT_PATH: u8 = 1;
const BTN_LOAD: u8 = 2;
const SW_DRIVE: u8 = 3;
const CMB_SOURCE: u8 = 4;
const CMB_TRACK: u8 = 5;
const BTN_PREV: u8 = 6;
const BTN_NEXT: u8 = 7;
const BTN_SYNC_START: u8 = 8;
const BTN_SYNC_DROP: u8 = 9;
const BTN_BAR_BACK: u8 = 10;
const BTN_BEAT_BACK: u8 = 11;
const BTN_BEAT_FWD: u8 = 12;
const BTN_BAR_FWD: u8 = 13;
const BTN_STOP: u8 = 14;
const CHK_BAR_PHASE: u8 = 15;
const PAINTER_TIMELINE: u8 = 16;
const TABS: u8 = 17;
const TAB_FOLLOW: u8 = 18;
const TAB_LOOKS: u8 = 19;
const TAB_TRACKS: u8 = 20;
const FRAME_MAIN: u8 = 21;
/// `CMB_LOOK + SectionKind::index()`.
const CMB_LOOK: u8 = 30;
const SLD_BLACKOUT: u8 = 40;
const CMB_BLACKOUT_SCENE: u8 = 41;
const CMB_HIT: u8 = 42;
const SLD_HIT_BEATS: u8 = 43;
const CHK_RAMP_ALPHA: u8 = 44;
const SLD_RAMP_FROM: u8 = 45;
const CHK_RAMP_SPEED: u8 = 46;
const SLD_LEAD: u8 = 47;
/// `BTN_TRACK + setlist index`, for the first [`MAX_TRACK_BUTTONS`] tracks.
const BTN_TRACK: u8 = 100;
const MAX_TRACK_BUTTONS: usize = 100;

const TIMELINE_WIDTH: i32 = 680;
const TIMELINE_HEIGHT: i32 = 46;
const MAX_LOG_LINES: usize = 6;
const DMX_SYNC_MS: u64 = 1_000;
const PANEL_WIDTH: i32 = 700;
/// UI refresh while no frontend shows the panel.
const RENDER_CLOSED_MS: u64 = 1_000;

#[derive(Serialize, Deserialize, Clone, Copy, PartialEq, Eq, Default, Debug)]
enum Source {
    #[default]
    Clock,
    Decks,
}

#[derive(Serialize, Deserialize, Default)]
#[serde(default)]
struct SaveState {
    setlist_path: String,
    settings: ShowSettings,
    source: Source,
    /// Send cues to the engine; off, the plugin only follows.
    drive: bool,
    follow_bar_phase: bool,
    current: usize,
}

struct SetlistPlugin {
    state: SaveState,
    dirty: bool,
    path_text: String,
    loaded: bool,
    maps: Vec<TrackMap>,
    load_errors: Vec<String>,
    current: Option<usize>,
    runner: Option<Runner>,
    clock: BeatClock,
    decks: Decks,
    udp: Option<UdpPort>,
    udp_error: String,
    deck_error: String,
    /// Grid beat now, if following.
    position: Option<f64>,
    bpm: f64,
    log: Vec<String>,
    now: u64,
    last_clock: u32,
    dmx: EngineState,
    last_dmx_sync: Option<u64>,
    last_render: Option<u64>,
}

impl SetlistPlugin {
    fn new() -> Self {
        Self {
            state: SaveState::default(),
            dirty: false,
            path_text: String::new(),
            loaded: false,
            maps: Vec::new(),
            load_errors: Vec::new(),
            current: None,
            runner: None,
            clock: BeatClock::new(),
            decks: Decks::new(),
            udp: None,
            udp_error: String::new(),
            deck_error: String::new(),
            position: None,
            bpm: 0.0,
            log: Vec::new(),
            now: 0,
            last_clock: 0,
            dmx: EngineState::default(),
            last_dmx_sync: None,
            last_render: None,
        }
    }

    fn save(&self) {
        if let Ok(json) = serde_json::to_string(&self.state) {
            bpf::save_plugin_state(bpf::PluginStateLocation::Showfile, &json);
        }
    }

    fn log_msg(&mut self, msg: String) {
        if self.log.len() >= MAX_LOG_LINES {
            self.log.remove(0);
        }
        self.log.push(msg);
    }

    fn map(&self) -> Option<&TrackMap> {
        self.maps.get(self.current?)
    }

    fn bind_udp(&mut self) {
        if self.udp.is_some() {
            return;
        }
        match UdpPort::open_loopback(DEFAULT_PORT) {
            Ok(port) => {
                self.udp = Some(port);
                self.udp_error.clear();
            }
            Err(e) => self.udp_error = format!("cannot listen on 127.0.0.1:{DEFAULT_PORT}: {e}"),
        }
    }

    fn load_setlist(&mut self, out: &mut Vec<Action>) {
        self.finish(out);
        self.maps.clear();
        self.load_errors.clear();
        self.current = None;
        self.runner = None;
        let path = self.state.setlist_path.trim().to_string();
        if path.is_empty() {
            return;
        }
        let setlist = match read_file(&path).and_then(|t| Setlist::from_json(&t)) {
            Ok(s) => s,
            Err(e) => {
                self.load_errors.push(e);
                return;
            }
        };
        let dir = path.rsplit_once('/').map_or(".", |(d, _)| d).to_string();
        for entry in &setlist.tracks {
            match read_file(&format!("{dir}/{}", entry.map)).and_then(|t| TrackMap::from_json(&t)) {
                Ok(map) => self.maps.push(map),
                Err(e) => self.load_errors.push(format!("{}: {e}", entry.title)),
            }
        }
        bpf::bl_log(
            &format!("setlist: {} tracks from {path}", self.maps.len()),
            LogLevel::Info,
        );
        if !self.maps.is_empty() {
            let index = self.state.current.min(self.maps.len() - 1);
            self.switch_track(index, out);
        }
    }

    /// Follow track `index` from now on.
    fn switch_track(&mut self, index: usize, out: &mut Vec<Action>) {
        if index >= self.maps.len() {
            return;
        }
        self.finish(out);
        self.current = Some(index);
        if self.state.current != index {
            self.state.current = index;
            self.dirty = true;
        }
        let map = &self.maps[index];
        self.runner = Some(Runner::new(plan(map, &self.state.settings)));
        self.clock.load(map);
        let title = map.title.clone();
        self.log_msg(format!("Track {}: {title}", index + 1));
    }

    /// New settings: a new plan, caught up to the position on the next tick.
    fn replan(&mut self, out: &mut Vec<Action>) {
        self.dirty = true;
        let Some(index) = self.current else { return };
        self.finish(out);
        self.runner = Some(Runner::new(plan(&self.maps[index], &self.state.settings)));
    }

    /// Put ramped scenes back (only if cues were being sent).
    fn finish(&mut self, out: &mut Vec<Action>) {
        if let Some(runner) = self.runner.as_mut() {
            if self.state.drive {
                runner.finish(out);
            } else {
                runner.reset();
            }
        }
    }

    /// Scene names and live mode, for the UI only: decoding the whole engine
    /// state is the most expensive thing this plugin does, so following never
    /// waits for it.
    fn sync_dmx(&mut self) {
        if self
            .last_dmx_sync
            .is_none_or(|t| self.now.saturating_sub(t) >= DMX_SYNC_MS)
        {
            self.last_dmx_sync = Some(self.now);
            self.dmx = bpf::get_dmx();
        }
    }

    /// `None` ("keep") and the user scenes.
    fn scene_options(&self) -> Vec<(Option<u8>, String)> {
        let mut out = vec![(None, "-".to_string())];
        for (id, scene) in self.dmx.user_scenes() {
            out.push((Some(*id), truncate(&format!("{id}: {}", scene.name), 22)));
        }
        out
    }

    /// The scene behind a combo selection, from the options last drawn.
    fn scene_at(&mut self, selected: u8) -> Option<Option<u8>> {
        if self.last_dmx_sync.is_none() {
            // An event from a frontend this plugin never drew for (tests,
            // inspector): fetch the scenes once.
            self.sync_dmx();
        }
        self.scene_options().get(selected as usize).map(|(s, _)| *s)
    }

    fn handle_ui(&mut self, event: &PluginUiEvent, out: &mut Vec<Action>) {
        match event {
            PluginUiEvent::Text { id: TXT_PATH, text } => self.path_text = text.clone(),
            PluginUiEvent::Button { id } => self.handle_button(*id, out),
            PluginUiEvent::Switch { id: SW_DRIVE, value } => {
                if !*value {
                    self.finish(out);
                }
                self.state.drive = *value;
                if let Some(r) = self.runner.as_mut() {
                    r.reset();
                }
                self.dirty = true;
            }
            PluginUiEvent::Checkbox { id, checked } => {
                let s = &mut self.state.settings;
                match *id {
                    CHK_BAR_PHASE => self.state.follow_bar_phase = *checked,
                    CHK_RAMP_ALPHA => s.ramp_alpha = *checked,
                    CHK_RAMP_SPEED => s.ramp_speed = *checked,
                    _ => return,
                }
                self.replan(out);
            }
            PluginUiEvent::Slider { id, value } => {
                let s = &mut self.state.settings;
                match *id {
                    SLD_BLACKOUT => s.blackout_half_beats = *value,
                    SLD_HIT_BEATS => s.hit_beats = *value,
                    SLD_RAMP_FROM => s.ramp_from_percent = (*value).min(100),
                    SLD_LEAD => s.lead_ms = *value as u16,
                    _ => return,
                }
                self.replan(out);
            }
            PluginUiEvent::ComboBox { id, selected } => match *id {
                CMB_SOURCE => {
                    self.finish(out);
                    self.state.source = if *selected == 1 { Source::Decks } else { Source::Clock };
                    if self.state.source == Source::Decks {
                        self.bind_udp();
                    }
                    self.dirty = true;
                }
                CMB_TRACK => self.switch_track(*selected as usize, out),
                id if (CMB_LOOK..CMB_LOOK + SectionKind::COUNT as u8).contains(&id) => {
                    if let Some(scene) = self.scene_at(*selected) {
                        let kind = SectionKind::ALL[(id - CMB_LOOK) as usize];
                        self.state.settings.set_look(kind, scene);
                        self.replan(out);
                    }
                }
                CMB_BLACKOUT_SCENE => {
                    if let Some(scene) = self.scene_at(*selected) {
                        self.state.settings.blackout_scene = scene;
                        self.replan(out);
                    }
                }
                CMB_HIT => {
                    if let Some(scene) = self.scene_at(*selected) {
                        self.state.settings.hit_scene = scene;
                        self.replan(out);
                    }
                }
                _ => {}
            },
            PluginUiEvent::CanvasClick { id: PAINTER_TIMELINE, x, .. } => {
                if self.state.source != Source::Clock {
                    return;
                }
                let Some(map) = self.map() else { return };
                let beat = (*x as f64 / TIMELINE_WIDTH as f64) * map.beat_count() as f64;
                let target = map.nearest_downbeat(beat).max(0) as f64;
                let bar = bar_number(map, target);
                self.clock.sync(self.now, target);
                self.log_msg(format!("Synced to bar {bar}"));
            }
            _ => {}
        }
    }

    fn handle_button(&mut self, id: u8, out: &mut Vec<Action>) {
        match id {
            BTN_LOAD => {
                self.state.setlist_path = self.path_text.trim().to_string();
                self.state.current = 0;
                self.dirty = true;
                self.load_setlist(out);
            }
            BTN_PREV => {
                if let Some(i) = self.current.and_then(|i| i.checked_sub(1)) {
                    self.switch_track(i, out);
                }
            }
            BTN_NEXT => {
                if let Some(i) = self.current {
                    self.switch_track(i + 1, out);
                }
            }
            BTN_SYNC_START => {
                if let Some(map) = self.map() {
                    let downbeat = map.downbeat as f64;
                    self.clock.sync(self.now, downbeat);
                    self.log_msg("Synced to bar 1".into());
                }
            }
            BTN_SYNC_DROP => {
                let Some(map) = self.map() else { return };
                let here = if self.clock.running() { self.clock.position() } else { 0.0 };
                let drop = map
                    .sections
                    .iter()
                    .filter(|s| s.kind == SectionKind::Drop)
                    .min_by(|a, b| {
                        (a.start as f64 - here).abs().total_cmp(&(b.start as f64 - here).abs())
                    })
                    .map(|s| s.start as f64);
                if let Some(drop) = drop {
                    let bar = bar_number(map, drop);
                    self.clock.sync(self.now, drop);
                    self.log_msg(format!("Synced to the drop at bar {bar}"));
                }
            }
            BTN_BAR_BACK => self.clock.nudge(-(BEATS_PER_BAR as f64)),
            BTN_BEAT_BACK => self.clock.nudge(-1.0),
            BTN_BEAT_FWD => self.clock.nudge(1.0),
            BTN_BAR_FWD => self.clock.nudge(BEATS_PER_BAR as f64),
            BTN_STOP => self.clock.stop(),
            id if id >= BTN_TRACK && ((id - BTN_TRACK) as usize) < self.maps.len() => {
                self.switch_track((id - BTN_TRACK) as usize, out)
            }
            _ => {}
        }
    }

    fn follow(&mut self, input: &TickInput, out: &mut Vec<Action>) {
        match self.state.source {
            Source::Decks => {
                if let Some(port) = &self.udp {
                    for datagram in port.poll() {
                        match DeckMessage::parse(&datagram.body) {
                            Ok(msg) => {
                                self.deck_error.clear();
                                self.decks.ingest(msg, self.now, &self.maps);
                            }
                            Err(e) => self.deck_error = truncate(&e, 80),
                        }
                    }
                }
                self.decks.update(self.now, &self.maps);
                let active = self
                    .decks
                    .active()
                    .and_then(|d| d.track.map(|t| (t, d.clone())));
                match active {
                    Some((track, deck)) => {
                        if self.current != Some(track) {
                            self.switch_track(track, out);
                        }
                        let map = &self.maps[track];
                        self.position = Some(deck.beat(self.now, map));
                        self.bpm = deck.bpm(map);
                    }
                    None => self.position = None,
                }
            }
            Source::Clock => {
                let Some(index) = self.current else {
                    self.position = None;
                    return;
                };
                let audio = &input.audio_data;
                let clock_input = ClockInput {
                    now_ms: self.now,
                    beat_event_id: audio.beat_event_id,
                    bpm: audio.bpm,
                    bpm_confidence: audio.bpm_confidence,
                    beat_in_bar: audio.beat_in_bar,
                };
                self.clock.follow_bar_phase = self.state.follow_bar_phase;
                self.clock.tick(&clock_input, &self.maps[index]);
                self.bpm = self.clock.bpm();
                self.position = self.clock.running().then(|| self.clock.position());
            }
        }

        let Some(runner) = self.runner.as_mut() else { return };
        match (self.position, self.state.drive) {
            (Some(position), true) => {
                let lead = self.state.settings.lead_ms as f64 * self.bpm / 60_000.0;
                let fired = runner.advance(position + lead, out);
                let Some(map) = self.maps.get(self.current.unwrap_or(0)) else { return };
                let bar = bar_number(map, position + lead);
                let lines: Vec<String> = fired
                    .iter()
                    .map(|f| match f {
                        Fired::Cue(kind) => format!("Bar {bar}: {}", kind.name()),
                        Fired::CaughtUp => format!("Bar {bar}: caught up"),
                    })
                    .collect();
                for line in lines {
                    self.log_msg(line);
                }
            }
            // Not following: catch up when it comes back.
            _ => runner.reset(),
        }
    }

    fn send(&self, actions: Vec<Action>) {
        for action in actions {
            bpf::send_event(match action {
                Action::Look(scenes) => ControlEvent::SetOverlays(scenes),
                Action::Alpha { scene, percent } => {
                    ControlEvent::SetSceneMasterAlpha(scene, percent)
                }
                Action::Speed { scene, index } => ControlEvent::SetSceneMasterSpeed(
                    scene,
                    AnimationSpeedModifier::from_index(index as usize),
                ),
            });
        }
    }

    // UI ------------------------------------------------------------------

    fn render(&self) {
        ui::begin();
        ui::set_max_width(PANEL_WIDTH);
        ui::begin_frame_styled(FRAME_MAIN, "Setlist", 4, 6, 2, 2);

        ui::begin_horizontal();
        ui::text_edit("setlist.json", TXT_PATH, &self.path_text);
        ui::button("Load", BTN_LOAD);
        ui::end_horizontal();
        if self.maps.is_empty() && self.load_errors.is_empty() {
            ui::label("Run setlist-analyze, then load the setlist.json it wrote.");
        }
        for error in self.load_errors.iter().take(4) {
            ui::label(&format!("Error: {error}"));
        }

        ui::begin_tabs(TABS);
        ui::begin_tab(TABS, TAB_FOLLOW, "Follow");
        self.render_follow();
        ui::end_tab();
        ui::begin_tab(TABS, TAB_LOOKS, "Looks");
        self.render_looks();
        ui::end_tab();
        ui::begin_tab(TABS, TAB_TRACKS, "Tracks");
        self.render_tracks();
        ui::end_tab();
        ui::end_tabs();

        ui::end_frame();
    }

    fn render_follow(&self) {
        ui::begin_horizontal();
        ui::switch("Drive lights", SW_DRIVE, self.state.drive);
        let sources = vec!["Beat clock".to_string(), format!("Decks (UDP {DEFAULT_PORT})")];
        ui::combo_box("Follow", CMB_SOURCE, &sources, (self.state.source == Source::Decks) as u8);
        ui::end_horizontal();
        if self.dmx.live_mode {
            ui::label("Live mode is on: overlays (and so these cues) are not shown.");
        }

        if !self.maps.is_empty() {
            let titles: Vec<String> = self
                .maps
                .iter()
                .enumerate()
                .map(|(i, m)| truncate(&format!("{}. {}", i + 1, m.title), 40))
                .collect();
            ui::begin_horizontal();
            ui::button("<", BTN_PREV);
            ui::combo_box("Track", CMB_TRACK, &titles, self.current.unwrap_or(0) as u8);
            ui::button(">", BTN_NEXT);
            ui::end_horizontal();
        }

        let Some(map) = self.map() else { return };
        let status = match self.position {
            Some(pos) => {
                let section = map.section_at(pos).map_or("-", |s| s.kind.name());
                format!(
                    "Bar {:>5.1} / {}   {:<9}  {:.1} BPM",
                    map.bar_at(pos) + 1.0,
                    map.bars.energy.len(),
                    section,
                    self.bpm
                )
            }
            None => format!("Stopped   {:.1} BPM track", map.bpm),
        };
        ui::label_styled(&status, 18, true);
        if let (Some(pos), Some(runner)) = (self.position, &self.runner) {
            let lead = self.state.settings.lead_ms as f64 * self.bpm / 60_000.0;
            if let Some((beat, kind)) = runner.upcoming(pos + lead) {
                let bars = (beat - pos) / BEATS_PER_BAR as f64;
                let secs = (beat - pos) * 60.0 / self.bpm.max(1.0);
                let emphasis = matches!(
                    kind,
                    CueKind::Blackout | CueKind::Hit | CueKind::Section(SectionKind::Drop)
                );
                ui::label_styled(
                    &format!("Next: {} in {bars:.1} bars ({secs:.1} s)", kind.name()),
                    if emphasis { 16 } else { 13 },
                    true,
                );
            }
        }
        self.render_timeline(map);

        match self.state.source {
            Source::Clock => {
                ui::begin_horizontal();
                ui::button("Sync bar 1", BTN_SYNC_START);
                ui::button("Drop!", BTN_SYNC_DROP);
                ui::button("-bar", BTN_BAR_BACK);
                ui::button("-beat", BTN_BEAT_BACK);
                ui::button("+beat", BTN_BEAT_FWD);
                ui::button("+bar", BTN_BAR_FWD);
                ui::button_styled("Stop", BTN_STOP, self.clock.running());
                ui::end_horizontal();
                ui::checkbox(
                    "Take bar phase from the tempo source (Pro DJ Link)",
                    CHK_BAR_PHASE,
                    self.state.follow_bar_phase,
                );
                ui::label(
                    "Press Sync on the track's first downbeat, Drop! on a drop, or click the \
                     timeline.",
                );
                if self.clock.running() && self.clock.matched() == 0 {
                    ui::label("No engine beats yet: free-running.");
                }
            }
            Source::Decks => {
                if !self.udp_error.is_empty() {
                    ui::label(&self.udp_error);
                }
                let mut any = false;
                for deck in self.decks.all() {
                    any = true;
                    let track = deck
                        .track
                        .and_then(|t| self.maps.get(t))
                        .map_or_else(
                            || format!("{} (not in setlist)", deck.msg.track),
                            |m| m.title.clone(),
                        );
                    let active = self.decks.active().is_some_and(|a| a.msg.deck == deck.msg.deck);
                    ui::label(&format!(
                        "{} Deck {}: {}{}",
                        if active { ">" } else { " " },
                        deck.msg.deck,
                        truncate(&track, 50),
                        if deck.live() { "" } else { " (not live)" }
                    ));
                }
                if !any {
                    ui::label(&format!("Waiting for deck messages on 127.0.0.1:{DEFAULT_PORT}."));
                }
                if !self.deck_error.is_empty() {
                    ui::label(&format!("Bad deck message: {}", self.deck_error));
                }
            }
        }

        for line in &self.log {
            ui::label_styled(line, 11, true);
        }
    }

    fn render_timeline(&self, map: &TrackMap) {
        let total = map.beat_count().max(1) as f64;
        let x = |beat: f64| ((beat / total) * TIMELINE_WIDTH as f64).round() as i32;
        ui::painter_begin(PAINTER_TIMELINE, TIMELINE_WIDTH, TIMELINE_HEIGHT);
        ui::painter_rect(0, 0, TIMELINE_WIDTH, TIMELINE_HEIGHT, 20, 20, 24, 255);
        // Energy per bar as a faint curve behind the sections.
        for (bar, energy) in map.bars.energy.iter().enumerate() {
            let start = map.bar_start(bar as i64) as f64;
            let (x0, x1) = (x(start), x(start + BEATS_PER_BAR as f64));
            let h = (*energy as i32 * (TIMELINE_HEIGHT - 16)) / 255;
            ui::painter_rect(x0, TIMELINE_HEIGHT - h, (x1 - x0).max(1), h, 70, 70, 80, 255);
        }
        for s in &map.sections {
            let (r, g, b) = section_color(s.kind);
            let (x0, x1) = (x(s.start as f64), x(s.end as f64));
            ui::painter_rect(x0, 0, (x1 - x0).max(1), 12, r, g, b, 255);
            if s.kind == SectionKind::Drop {
                ui::painter_line(x0, 0, x0, TIMELINE_HEIGHT, 255, 80, 80, 255, 2);
            }
        }
        if let Some(pos) = self.position {
            let px = x(pos);
            ui::painter_line(px, 0, px, TIMELINE_HEIGHT, 255, 255, 255, 255, 2);
        }
        ui::painter_end();
    }

    fn render_looks(&self) {
        let options = self.scene_options();
        let labels: Vec<String> = options.iter().map(|(_, l)| l.clone()).collect();
        let index = |scene: Option<u8>| {
            options.iter().position(|(s, _)| *s == scene).unwrap_or(0) as u8
        };
        let s = &self.state.settings;
        ui::label("Scene per section (- keeps the previous look):");
        for kind in SectionKind::ALL {
            ui::combo_box(kind.name(), CMB_LOOK + kind.index() as u8, &labels, index(s.look(kind)));
        }
        ui::separator();
        ui::label("Before each drop:");
        ui::slider("Blackout (half beats)", SLD_BLACKOUT, 0, 8, s.blackout_half_beats);
        ui::combo_box(
            "Show during blackout (dimmers at 0, movers aimed)",
            CMB_BLACKOUT_SCENE,
            &labels,
            index(s.blackout_scene),
        );
        ui::begin_horizontal();
        ui::checkbox("Fade build in", CHK_RAMP_ALPHA, s.ramp_alpha);
        ui::slider("from %", SLD_RAMP_FROM, 0, 100, s.ramp_from_percent);
        ui::checkbox("Speed build up x1 -> x4", CHK_RAMP_SPEED, s.ramp_speed);
        ui::end_horizontal();
        ui::separator();
        ui::label("On the drop:");
        ui::combo_box("Hit scene on top", CMB_HIT, &labels, index(s.hit_scene));
        ui::slider("Hit length (beats)", SLD_HIT_BEATS, 0, 8, s.hit_beats);
        ui::separator();
        ui::slider("Fire early (ms)", SLD_LEAD, 0, 250, s.lead_ms.min(250) as u8);
    }

    fn render_tracks(&self) {
        if self.maps.is_empty() {
            ui::label("No setlist loaded.");
            return;
        }
        for (i, map) in self.maps.iter().enumerate().take(MAX_TRACK_BUTTONS) {
            let drops = map.sections.iter().filter(|s| s.kind == SectionKind::Drop).count();
            ui::begin_horizontal();
            ui::button_styled(
                &format!("{:>2}", i + 1),
                BTN_TRACK + i as u8,
                self.current != Some(i),
            );
            ui::label(&format!(
                "{}  {:.1} BPM  {} bars  {} drops{}",
                truncate(&map.title, 44),
                map.bpm,
                map.bars.energy.len(),
                drops,
                if map.downbeat_margin < 1.1 { "  (check bar 1)" } else { "" }
            ));
            ui::end_horizontal();
        }
    }
}

impl Plugin for SetlistPlugin {
    fn initialize(&mut self, input: TickInput) {
        self.last_clock = input.clock;
        if let Some(json) = bpf::load_plugin_state(bpf::PluginStateLocation::Showfile) {
            match serde_json::from_str::<SaveState>(&json) {
                Ok(state) => self.state = state,
                Err(e) => bpf::bl_log(&format!("setlist: state not loaded: {e}"), LogLevel::Warn),
            }
        }
        self.path_text = self.state.setlist_path.clone();
        if self.state.source == Source::Decks {
            self.bind_udp();
        }
    }

    fn run(&mut self, input: TickInput) {
        let delta = input.clock.wrapping_sub(self.last_clock);
        self.last_clock = input.clock;
        self.now += delta as u64;

        let mut out = Vec::new();
        if !self.loaded {
            // Engine state and the shell are only there once running.
            self.loaded = true;
            self.load_setlist(&mut out);
        }
        for ev in &input.events.events {
            if let ControlEvent::PluginUi(event, plugin_id) = ev.body() {
                if plugin_id == input.id {
                    self.handle_ui(&event, &mut out);
                }
            }
        }
        self.follow(&input, &mut out);
        self.send(out);
        // Drawing costs more than following; nobody needs it at tick rate
        // while the panel is closed.
        if ui::is_open() {
            self.sync_dmx();
            self.render();
        } else if self.last_render.is_none_or(|t| self.now.saturating_sub(t) >= RENDER_CLOSED_MS) {
            self.last_render = Some(self.now);
            self.render();
        }
        if self.dirty {
            self.save();
            self.dirty = false;
        }
    }
}

/// Native unit tests must not export a second `main` symbol.
#[cfg(not(test))]
#[no_mangle]
extern "C" fn main() {
    bpf::hook_plugin(Box::new(SetlistPlugin::new()));
}

/// 1-based bar number of a beat, as DJs count.
fn bar_number(map: &TrackMap, beat: f64) -> i64 {
    map.bar_at(beat).floor() as i64 + 1
}

fn section_color(kind: SectionKind) -> (u8, u8, u8) {
    match kind {
        SectionKind::Intro => (90, 110, 140),
        SectionKind::Groove => (60, 120, 200),
        SectionKind::Breakdown => (130, 80, 180),
        SectionKind::Build => (230, 150, 40),
        SectionKind::Drop => (220, 50, 60),
        SectionKind::Outro => (90, 90, 100),
    }
}

fn truncate(text: &str, max: usize) -> String {
    if text.chars().count() <= max {
        return text.to_string();
    }
    let mut out: String = text.chars().take(max.saturating_sub(3)).collect();
    out.push_str("...");
    out
}

/// Single-quotes a path for bash, keeping a leading `~/` working.
fn shell_quote(path: &str) -> String {
    let (prefix, rest) = match path.strip_prefix("~/") {
        Some(rest) => ("\"$HOME\"/", rest),
        None => ("", path),
    };
    format!("{prefix}'{}'", rest.replace('\'', r"'\''"))
}

/// Reads a text file through the host shell (plugins have no file API).
/// The host returns at most 64 KiB; a track map is a few KiB.
fn read_file(path: &str) -> Result<String, String> {
    let text = bpf::system(&format!("cat {} 2>/dev/null", shell_quote(path)));
    if text.trim().is_empty() {
        Err(format!("cannot read {path}"))
    } else {
        Ok(text)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn quotes_paths_for_the_shell() {
        assert_eq!(shell_quote("/a b/c.json"), "'/a b/c.json'");
        assert_eq!(shell_quote("~/sets/it's.json"), r#""$HOME"/'sets/it'\''s.json'"#);
    }

    #[test]
    fn saved_state_round_trips_and_tolerates_missing_fields() {
        let json = r#"{"setlist_path":"/x/setlist.json"}"#;
        let state: SaveState = serde_json::from_str(json).unwrap();
        assert_eq!(state.setlist_path, "/x/setlist.json");
        assert_eq!(state.source, Source::Clock);
        assert!(!state.drive);
        assert_eq!(state.settings, ShowSettings::default());
    }
}
