//! Following the decks: which track is playing where, from deck messages.
//!
//! Anything that knows the decks can send these, one JSON object per UDP
//! datagram to `127.0.0.1:9120` (the setlist plugin binds loopback only):
//! the Pro DJ Link plugin (track metadata and CDJ-3000 absolute position),
//! a bridge for other DJ software, or `scripts/setlist_deck_sim.py` for tests.
//!
//! ```json
//! {"deck": 2, "track": "Artist - Title", "position_ms": 61234.5,
//!  "pitch": 1.023, "playing": true, "on_air": true}
//! ```
//!
//! - `track` is matched against the setlist (title, tag title or file name;
//!   see [`TrackMap::match_score`]); `index` (0-based setlist position) can
//!   be sent instead.
//! - `position_ms` is the position in the track's own time (not slowed down
//!   or sped up by the pitch), so it maps onto the analyzer's grid no matter
//!   how the sender's beat grid was made.
//! - `pitch` is the playback speed factor; without it, `bpm` (effective
//!   tempo) over the map's tempo is used; without both, 1.0.
//! - `playing` and `on_air` default to true. Send at least every 500 ms
//!   (every beat is plenty); a deck silent for 1.5 s is dropped.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::map::{find_track, SectionKind, TrackMap};

pub const DEFAULT_PORT: u16 = 9120;
const STALE_MS: u64 = 1_500;

fn yes() -> bool {
    true
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct DeckMessage {
    pub deck: u8,
    #[serde(default)]
    pub track: String,
    #[serde(default)]
    pub index: Option<usize>,
    pub position_ms: f64,
    #[serde(default)]
    pub pitch: Option<f64>,
    #[serde(default)]
    pub bpm: Option<f64>,
    #[serde(default = "yes")]
    pub playing: bool,
    #[serde(default = "yes")]
    pub on_air: bool,
}

impl DeckMessage {
    pub fn parse(bytes: &[u8]) -> Result<Self, String> {
        serde_json::from_slice(bytes).map_err(|e| e.to_string())
    }

    /// Playback speed relative to the track's own tempo.
    pub fn speed(&self, map: &TrackMap) -> f64 {
        self.pitch
            .or_else(|| self.bpm.map(|bpm| bpm / map.bpm))
            .filter(|s| s.is_finite() && *s > 0.0)
            .unwrap_or(1.0)
    }
}

#[derive(Clone, Debug)]
pub struct DeckState {
    pub msg: DeckMessage,
    pub received_ms: u64,
    /// Setlist index of the track, if it is in the setlist.
    pub track: Option<usize>,
    /// When the deck last became audible (playing and on air).
    pub live_since_ms: Option<u64>,
}

impl DeckState {
    pub fn live(&self) -> bool {
        self.msg.playing && self.msg.on_air
    }

    /// Grid beat now, extrapolated from the last message.
    pub fn beat(&self, now_ms: u64, map: &TrackMap) -> f64 {
        let mut ms = self.msg.position_ms;
        if self.msg.playing {
            ms += now_ms.saturating_sub(self.received_ms) as f64 * self.msg.speed(map);
        }
        map.beat_at_ms(ms)
    }

    /// Tempo the track is played at.
    pub fn bpm(&self, map: &TrackMap) -> f64 {
        map.bpm * self.msg.speed(map)
    }
}

#[derive(Debug, Default)]
pub struct Decks {
    decks: BTreeMap<u8, DeckState>,
    active: Option<u8>,
}

impl Decks {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn ingest(&mut self, msg: DeckMessage, now_ms: u64, maps: &[TrackMap]) {
        let track = match msg.index {
            Some(i) => (i < maps.len()).then_some(i),
            None => find_track(maps, &msg.track),
        };
        let was_live = self.decks.get(&msg.deck).and_then(|d| {
            (d.live() && d.track == track).then_some(d.live_since_ms).flatten()
        });
        let live = msg.playing && msg.on_air;
        self.decks.insert(
            msg.deck,
            DeckState {
                msg,
                received_ms: now_ms,
                track,
                live_since_ms: if live { was_live.or(Some(now_ms)) } else { None },
            },
        );
    }

    /// Drops silent decks and picks the deck the lights follow. Returns it.
    ///
    /// Among live decks with a known track, a deck in its intro beats one in
    /// its outro, and one past its intro beats both (the DJ mixes the next
    /// track in over the outro). Otherwise the current deck stays, and a new
    /// pick goes to the deck that became live last.
    pub fn update(&mut self, now_ms: u64, maps: &[TrackMap]) -> Option<u8> {
        self.decks
            .retain(|_, d| now_ms.saturating_sub(d.received_ms) <= STALE_MS);
        let rank = |d: &DeckState| -> Option<(u8, bool, u64)> {
            let map = maps.get(d.track?)?;
            if !d.live() {
                return None;
            }
            let section = match map.section_at(d.beat(now_ms, map)).map(|s| s.kind) {
                Some(SectionKind::Outro) | None => 0,
                Some(SectionKind::Intro) => 1,
                Some(_) => 2,
            };
            let current = Some(d.msg.deck) == self.active;
            Some((section, current, d.live_since_ms.unwrap_or(0)))
        };
        self.active = self
            .decks
            .values()
            .filter_map(|d| rank(d).map(|r| (r, d.msg.deck)))
            .max_by_key(|(r, _)| *r)
            .map(|(_, deck)| deck);
        self.active
    }

    pub fn active(&self) -> Option<&DeckState> {
        self.decks.get(&self.active?)
    }

    pub fn all(&self) -> impl Iterator<Item = &DeckState> {
        self.decks.values()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::map::tests::test_map;
    use SectionKind::*;

    fn maps() -> Vec<TrackMap> {
        // Intro 0..33, Drop 33..97, Outro 97..129.
        let mut a = test_map(&[(Intro, 8), (Drop, 16), (Outro, 8)]);
        a.title = "First - One".into();
        a.tag_title = Some("One".into());
        a.file = "one.mp3".into();
        let mut b = a.clone();
        b.title = "Second - Two".into();
        b.tag_title = Some("Two".into());
        b.file = "two.mp3".into();
        vec![a, b]
    }

    fn msg(deck: u8, track: &str, beat: f64, live: bool) -> DeckMessage {
        let map = &maps()[0];
        DeckMessage {
            deck,
            track: track.into(),
            index: None,
            position_ms: map.ms_at_beat(beat),
            pitch: None,
            bpm: None,
            playing: live,
            on_air: true,
        }
    }

    #[test]
    fn parses_minimal_and_full_messages() {
        let m = DeckMessage::parse(br#"{"deck":1,"track":"One","position_ms":1000}"#).unwrap();
        assert!(m.playing && m.on_air);
        assert_eq!(m.speed(&maps()[0]), 1.0);
        let m = DeckMessage::parse(
            br#"{"deck":2,"index":1,"position_ms":5.5,"bpm":131.2,"playing":false,"on_air":false}"#,
        )
        .unwrap();
        assert!((m.speed(&maps()[0]) - 131.2 / 128.0).abs() < 1e-9);
        assert!(DeckMessage::parse(b"{\"deck\":1}").is_err());
    }

    #[test]
    fn extrapolates_with_the_pitch() {
        let maps = maps();
        let mut decks = Decks::new();
        let mut m = msg(1, "One", 10.0, true);
        m.pitch = Some(1.05);
        decks.ingest(m, 1_000, &maps);
        assert_eq!(decks.update(1_000, &maps), Some(1));
        let d = decks.active().unwrap();
        // One second later: 1.05 s of track time.
        let expected = 10.0 + 1.05 * 128.0 / 60.0;
        assert!((d.beat(2_000, &maps[0]) - expected).abs() < 1e-9);
        assert!((d.bpm(&maps[0]) - 134.4).abs() < 1e-9);
    }

    #[test]
    fn follows_the_incoming_track_once_the_outgoing_one_ends() {
        let maps = maps();
        let mut decks = Decks::new();
        // Deck 1 plays One's drop; deck 2 starts Two's intro.
        decks.ingest(msg(1, "One", 60.0, true), 0, &maps);
        decks.ingest(msg(2, "Two", 0.0, true), 0, &maps);
        assert_eq!(decks.update(0, &maps), Some(1));
        // One reaches its outro while Two is still in its intro: Two.
        decks.ingest(msg(1, "One", 100.0, true), 100, &maps);
        decks.ingest(msg(2, "Two", 8.0, true), 100, &maps);
        assert_eq!(decks.update(100, &maps), Some(2));
        // Deck 1 stops: still Two.
        decks.ingest(msg(1, "One", 110.0, false), 200, &maps);
        assert_eq!(decks.update(200, &maps), Some(2));
        assert_eq!(decks.active().unwrap().track, Some(1));
    }

    #[test]
    fn keeps_the_current_deck_on_a_tie() {
        let maps = maps();
        let mut decks = Decks::new();
        decks.ingest(msg(1, "One", 60.0, true), 0, &maps);
        assert_eq!(decks.update(0, &maps), Some(1));
        // Deck 2 goes live later, also in a drop: deck 1 stays.
        decks.ingest(msg(2, "Two", 60.0, true), 50, &maps);
        assert_eq!(decks.update(50, &maps), Some(1));
    }

    #[test]
    fn unknown_tracks_and_silent_decks_are_ignored() {
        let maps = maps();
        let mut decks = Decks::new();
        decks.ingest(msg(1, "Not In The Set", 60.0, true), 0, &maps);
        assert_eq!(decks.update(0, &maps), None);
        decks.ingest(msg(2, "Two", 60.0, true), 0, &maps);
        assert_eq!(decks.update(0, &maps), Some(2));
        assert_eq!(decks.update(STALE_MS + 1, &maps), None);
        assert_eq!(decks.all().count(), 0);
    }
}
