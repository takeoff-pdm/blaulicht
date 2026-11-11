//! The track map format written by the analyzer and read by the plugin.

use serde::{Deserialize, Serialize};

/// Bump when a field changes meaning; readers reject other versions.
pub const FORMAT_VERSION: u32 = 1;
/// Only 4/4 is supported: club music, and what Pro DJ Link reports.
pub const BEATS_PER_BAR: u32 = 4;

/// What a section of a track is, for picking its look.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum SectionKind {
    /// Opening part before the first breakdown or drop, with or without kick.
    Intro,
    /// Kick without the full energy of a drop.
    Groove,
    /// No kick: the quiet part between drops.
    Breakdown,
    /// The bars right before a drop.
    Build,
    /// Kick and full energy after a quieter part.
    Drop,
    /// Closing part after the last drop.
    Outro,
}

impl SectionKind {
    pub const ALL: [Self; 6] = [
        Self::Intro,
        Self::Groove,
        Self::Breakdown,
        Self::Build,
        Self::Drop,
        Self::Outro,
    ];
    pub const COUNT: usize = Self::ALL.len();

    pub fn index(self) -> usize {
        self as usize
    }

    pub fn name(self) -> &'static str {
        match self {
            Self::Intro => "Intro",
            Self::Groove => "Groove",
            Self::Breakdown => "Breakdown",
            Self::Build => "Build",
            Self::Drop => "Drop",
            Self::Outro => "Outro",
        }
    }
}

/// A section from `start` (inclusive) to `end` (exclusive), in grid beats.
/// Boundaries other than the first start and the last end sit on downbeats.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
pub struct Section {
    pub kind: SectionKind,
    pub start: u32,
    pub end: u32,
}

/// Per-bar features, each normalized to 0..=255 within the track. Index `n`
/// is bar `n`, which starts at beat `downbeat + 4 n`.
#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq, Eq)]
pub struct BarFeatures {
    /// Overall loudness.
    pub energy: Vec<u8>,
    /// Bass and kick band level.
    pub low: Vec<u8>,
    /// Hats, cymbals and risers.
    pub high: Vec<u8>,
    /// Kick onsets on the beats.
    pub kick: Vec<u8>,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct TrackMap {
    pub format: u32,
    /// Display name: "artist - title" from the tags, else the file name.
    pub title: String,
    /// The analyzed file's name without its directory.
    pub file: String,
    #[serde(default)]
    pub artist: Option<String>,
    #[serde(default)]
    pub tag_title: Option<String>,
    pub duration_ms: f64,
    /// Tempo of the constant beat grid.
    pub bpm: f64,
    /// Time of grid beat 0, within one beat period of the track start.
    pub first_beat_ms: f64,
    /// Grid beat of the first downbeat (`0..4`).
    pub downbeat: u32,
    /// How clearly the downbeat phase won over the runner-up (best / second).
    #[serde(default)]
    pub downbeat_margin: f32,
    pub bars: BarFeatures,
    /// Contiguous and sorted; the first starts at beat 0.
    pub sections: Vec<Section>,
}

impl TrackMap {
    pub fn from_json(json: &str) -> Result<Self, String> {
        let map: Self = serde_json::from_str(json).map_err(|e| e.to_string())?;
        if map.format != FORMAT_VERSION {
            return Err(format!(
                "track map format {}, expected {FORMAT_VERSION}; re-run the analyzer",
                map.format
            ));
        }
        if !(map.bpm > 0.0) {
            return Err("track map has no tempo".into());
        }
        Ok(map)
    }

    pub fn period_ms(&self) -> f64 {
        60_000.0 / self.bpm
    }

    /// Grid beat (fractional) at a time in the track.
    pub fn beat_at_ms(&self, ms: f64) -> f64 {
        (ms - self.first_beat_ms) / self.period_ms()
    }

    pub fn ms_at_beat(&self, beat: f64) -> f64 {
        self.first_beat_ms + beat * self.period_ms()
    }

    /// Grid beats before the end of the track.
    pub fn beat_count(&self) -> u32 {
        self.beat_at_ms(self.duration_ms).max(0.0).ceil() as u32
    }

    /// First beat of bar `bar` (bar 0 starts at the first downbeat).
    pub fn bar_start(&self, bar: i64) -> i64 {
        self.downbeat as i64 + bar * BEATS_PER_BAR as i64
    }

    /// Bar of a beat; negative before the first downbeat.
    pub fn bar_at(&self, beat: f64) -> f64 {
        (beat - self.downbeat as f64) / BEATS_PER_BAR as f64
    }

    /// Nearest downbeat to `beat`.
    pub fn nearest_downbeat(&self, beat: f64) -> i64 {
        self.bar_start(self.bar_at(beat).round() as i64)
    }

    pub fn section_at(&self, beat: f64) -> Option<&Section> {
        self.sections
            .iter()
            .find(|s| (s.start as f64) <= beat && beat < s.end as f64)
    }

    /// Index of the first section starting after `beat`.
    pub fn next_section(&self, beat: f64) -> Option<&Section> {
        self.sections.iter().find(|s| s.start as f64 > beat)
    }

    pub fn next_drop(&self, beat: f64) -> Option<&Section> {
        self.sections
            .iter()
            .find(|s| s.kind == SectionKind::Drop && s.start as f64 > beat)
    }

    /// Lower-case letters and digits only, for matching names from other
    /// sources (Pro DJ Link metadata, a file name, typed text).
    pub fn normalize(name: &str) -> String {
        name.chars()
            .filter(|c| c.is_alphanumeric())
            .flat_map(char::to_lowercase)
            .collect()
    }

    /// How well `name` refers to this track: 2 when it equals the title, tag
    /// title or file stem (normalized), 1 when one contains the other, else 0.
    pub fn match_score(&self, name: &str) -> u8 {
        let wanted = Self::normalize(name);
        if wanted.len() < 3 {
            return 0;
        }
        let stem = self.file.rsplit_once('.').map_or(self.file.as_str(), |(s, _)| s);
        let mut names = vec![Self::normalize(&self.title), Self::normalize(stem)];
        if let Some(title) = &self.tag_title {
            names.push(Self::normalize(title));
        }
        let names: Vec<&String> = names.iter().filter(|n| n.len() >= 3).collect();
        if names.iter().any(|n| **n == wanted) {
            2
        } else if names
            .iter()
            .any(|n| n.contains(&wanted) || wanted.contains(n.as_str()))
        {
            1
        } else {
            0
        }
    }

    pub fn matches(&self, name: &str) -> bool {
        self.match_score(name) > 0
    }
}

/// The best match for `name` among `maps`, earlier tracks winning ties.
pub fn find_track(maps: &[TrackMap], name: &str) -> Option<usize> {
    let (index, score) = maps
        .iter()
        .enumerate()
        .map(|(i, m)| (i, m.match_score(name)))
        .rev()
        .max_by_key(|(_, score)| *score)?;
    (score > 0).then_some(index)
}

/// Index written next to the track maps: the setlist in playing order.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct Setlist {
    pub format: u32,
    pub tracks: Vec<SetlistEntry>,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct SetlistEntry {
    pub title: String,
    /// Track map file, relative to the setlist file's directory.
    pub map: String,
}

impl Setlist {
    pub fn from_json(json: &str) -> Result<Self, String> {
        let setlist: Self = serde_json::from_str(json).map_err(|e| e.to_string())?;
        if setlist.format != FORMAT_VERSION {
            return Err(format!(
                "setlist format {}, expected {FORMAT_VERSION}; re-run the analyzer",
                setlist.format
            ));
        }
        Ok(setlist)
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;

    /// 128 BPM, first beat at 100 ms, downbeat on beat 1, sections in bars.
    pub fn test_map(sections: &[(SectionKind, u32)]) -> TrackMap {
        let downbeat = 1;
        let mut out = Vec::new();
        let mut bar = 0;
        for (i, (kind, bars)) in sections.iter().enumerate() {
            let start = if i == 0 { 0 } else { downbeat + bar * 4 };
            bar += bars;
            out.push(Section {
                kind: *kind,
                start,
                end: downbeat + bar * 4,
            });
        }
        let beats = downbeat + bar * 4;
        TrackMap {
            format: FORMAT_VERSION,
            title: "Test Artist - Test Track".into(),
            file: "01 Test Track (Extended Mix).mp3".into(),
            artist: Some("Test Artist".into()),
            tag_title: Some("Test Track".into()),
            duration_ms: 100.0 + beats as f64 * 60_000.0 / 128.0,
            bpm: 128.0,
            first_beat_ms: 100.0,
            downbeat,
            downbeat_margin: 2.0,
            bars: BarFeatures::default(),
            sections: out,
        }
    }

    #[test]
    fn grid_round_trips() {
        let map = test_map(&[(SectionKind::Intro, 16)]);
        assert_eq!(map.beat_at_ms(100.0), 0.0);
        assert!((map.ms_at_beat(map.beat_at_ms(12_345.0)) - 12_345.0).abs() < 1e-9);
        assert_eq!(map.bar_start(2), 9);
        assert_eq!(map.bar_at(9.0), 2.0);
        assert_eq!(map.nearest_downbeat(10.9), 9);
        assert_eq!(map.nearest_downbeat(11.1), 13);
        assert_eq!(map.beat_count(), 65);
    }

    #[test]
    fn json_round_trips_and_checks_the_version() {
        let map = test_map(&[(SectionKind::Intro, 8), (SectionKind::Drop, 8)]);
        let json = serde_json::to_string(&map).unwrap();
        assert_eq!(TrackMap::from_json(&json).unwrap(), map);
        let old = json.replace("\"format\":1", "\"format\":0");
        assert!(TrackMap::from_json(&old).unwrap_err().contains("re-run"));
    }

    #[test]
    fn sections_are_found_by_beat() {
        let map = test_map(&[
            (SectionKind::Intro, 8),
            (SectionKind::Breakdown, 8),
            (SectionKind::Drop, 8),
        ]);
        assert_eq!(map.section_at(0.0).unwrap().kind, SectionKind::Intro);
        assert_eq!(map.section_at(33.0).unwrap().kind, SectionKind::Breakdown);
        assert_eq!(map.next_drop(0.0).unwrap().start, 65);
        assert!(map.next_drop(65.0).is_none());
        assert_eq!(map.next_section(33.0).unwrap().kind, SectionKind::Drop);
    }

    #[test]
    fn names_match_loosely() {
        let map = test_map(&[(SectionKind::Intro, 8)]);
        assert!(map.matches("Test Track"));
        assert!(map.matches("test artist - test track"));
        assert!(map.matches("01 Test Track (Extended Mix)"));
        assert!(map.matches("TEST TRACK (extended mix)"));
        assert!(!map.matches("Other Track"));
        assert!(!map.matches("te"));
        assert_eq!(map.match_score("test track"), 2);
        assert_eq!(map.match_score("Test Track (Extended Mix)"), 1);
    }

    #[test]
    fn the_best_match_wins_and_ties_go_to_the_earlier_track() {
        let mut a = test_map(&[(SectionKind::Intro, 8)]);
        a.title = "Artist - Song (Extended Mix)".into();
        a.tag_title = Some("Song (Extended Mix)".into());
        let mut b = a.clone();
        b.title = "Artist - Song".into();
        b.tag_title = Some("Song".into());
        b.file = "Song.mp3".into();
        let maps = vec![a, b];
        assert_eq!(find_track(&maps, "Song"), Some(1));
        assert_eq!(find_track(&maps, "Song (Extended Mix)"), Some(0));
        assert_eq!(find_track(&maps, "artist"), Some(0));
        assert_eq!(find_track(&maps, "Nothing Like It"), None);
    }
}
