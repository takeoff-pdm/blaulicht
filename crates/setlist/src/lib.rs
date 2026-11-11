//! Pre-analyzed setlists for the light engine.
//!
//! The offline analyzer (`crates/setlist_analyzer`) turns each track of a
//! DJ's setlist into a [`TrackMap`]: the beat grid, the bars and the song
//! sections (intro, breakdown, build, drop, ...). The setlist plugin
//! (`crates/plugins/setlist`) follows where the DJ is in that track and fires
//! cues ahead of the music, which a reactive engine cannot do: a ramp that
//! peaks on the drop, a blackout on the beat before it, and the drop look on
//! its downbeat.
//!
//! Everything here is pure and host-testable; the plugin only adds I/O, UI
//! and the mapping of [`Action`]s to control events.
//!
//! Positions are in grid beats (`f64`): beat `b` of a map is at
//! `first_beat_ms + b * 60000 / bpm` in the track. Timing in beats rather
//! than milliseconds means a track the DJ plays at +4 % just runs faster.

pub mod clock;
pub mod deck;
pub mod map;
pub mod plan;
pub mod runner;

pub use clock::BeatClock;
pub use deck::{DeckMessage, Decks};
pub use map::{BarFeatures, Section, SectionKind, Setlist, SetlistEntry, TrackMap, BEATS_PER_BAR};
pub use plan::{plan, Action, Cue, CueKind, Plan, Ramp, ShowSettings};
pub use runner::Runner;
