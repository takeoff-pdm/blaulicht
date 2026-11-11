//! Analyzes a DJ's setlist into track maps for the setlist plugin.
//!
//! ```text
//! cargo run --release -p blaulicht-setlist-analyzer -- \
//!     --out ~/shows/club/setlist  ~/Music/set.m3u8
//! ```
//!
//! Inputs are audio files (mp3, flac, wav, ogg), directories of them (sorted
//! by name) or `.m3u`/`.m3u8` playlists, in playing order. Writes one track
//! map per track plus `setlist.json`, the file the plugin loads, and prints
//! what it found so the grid and sections can be checked before the show.

mod decode;
mod grid;
mod sections;

use std::{
    fs,
    path::{Path, PathBuf},
};

use anyhow::{bail, Context};
use blaulicht_setlist::{
    map::FORMAT_VERSION, Section, SectionKind, Setlist, SetlistEntry, TrackMap, BEATS_PER_BAR,
};
use clap::Parser;

const AUDIO_EXTENSIONS: [&str; 5] = ["mp3", "flac", "wav", "ogg", "oga"];
/// Sample rate used when a file's rate does not work for the mel front end.
const FALLBACK_RATE: u32 = 44_100;

#[derive(Parser)]
#[command(about = "Analyze a setlist into track maps for the blaulicht setlist plugin")]
struct Args {
    /// Audio files, directories or m3u playlists, in playing order.
    #[arg(required = true)]
    inputs: Vec<PathBuf>,
    /// Directory for the track maps and setlist.json.
    #[arg(long, short)]
    out: PathBuf,
    #[arg(long, default_value_t = 100.0)]
    min_bpm: f64,
    #[arg(long, default_value_t = 200.0)]
    max_bpm: f64,
    /// File-name substrings (one per line, `#` comments) of tracks whose grid
    /// landed on the off-beat: their grid moves by half a beat.
    #[arg(long)]
    phase_hints: Option<PathBuf>,
    /// `SUBSTRING=BEAT`: the first downbeat of tracks whose path contains
    /// SUBSTRING is grid beat BEAT (0..3), for when the printed bar 1 is off.
    /// Repeatable.
    #[arg(long, value_name = "SUBSTRING=BEAT")]
    downbeat: Vec<String>,
    /// Also print the per-bar features.
    #[arg(long, short)]
    verbose: bool,
}

fn main() -> anyhow::Result<()> {
    let args = Args::parse();
    let mut files = Vec::new();
    for input in &args.inputs {
        collect(input, &mut files)?;
    }
    if files.is_empty() {
        bail!("no audio files in the inputs");
    }
    let hints: Vec<String> = match &args.phase_hints {
        Some(path) => fs::read_to_string(path)
            .with_context(|| format!("reading {}", path.display()))?
            .lines()
            .map(str::trim)
            .filter(|l| !l.is_empty() && !l.starts_with('#'))
            .map(String::from)
            .collect(),
        None => Vec::new(),
    };
    fs::create_dir_all(&args.out).with_context(|| format!("creating {}", args.out.display()))?;

    let mut setlist = Setlist {
        format: FORMAT_VERSION,
        tracks: Vec::new(),
    };
    let mut failed = 0;
    for (i, path) in files.iter().enumerate() {
        println!("[{}/{}] {}", i + 1, files.len(), path.display());
        let shift = hints
            .iter()
            .any(|h| path.to_string_lossy().contains(h.as_str()));
        match analyze(path, &args, shift) {
            Ok(map) => {
                let name = format!("{:02}-{}.json", i + 1, slug(&map.title));
                fs::write(args.out.join(&name), serde_json::to_string(&map)?)?;
                println!("  -> {name}");
                setlist.tracks.push(SetlistEntry {
                    title: map.title.clone(),
                    map: name,
                });
            }
            Err(err) => {
                println!("  FAILED: {err:#}");
                failed += 1;
            }
        }
    }
    let index = args.out.join("setlist.json");
    fs::write(&index, serde_json::to_string_pretty(&setlist)?)?;
    println!(
        "\n{} tracks in {}{}",
        setlist.tracks.len(),
        index.display(),
        if failed > 0 {
            format!(", {failed} failed")
        } else {
            String::new()
        }
    );
    Ok(())
}

fn analyze(path: &Path, args: &Args, shift_half_beat: bool) -> anyhow::Result<TrackMap> {
    let decoded = decode::decode(path)?;
    let (samples, rate) = if decoded.sample_rate % 100 == 0 && decoded.sample_rate >= 22_100 {
        (decoded.samples, decoded.sample_rate)
    } else {
        (
            decode::resample(&decoded.samples, decoded.sample_rate, FALLBACK_RATE),
            FALLBACK_RATE,
        )
    };
    let duration_ms = samples.len() as f64 * 1000.0 / rate as f64;

    let act = grid::activations(&samples, rate)?;
    let mut grid = grid::fit_grid(&act, args.min_bpm, args.max_bpm)
        .context("too short or no beat found")?;
    if shift_half_beat {
        grid.shift_half_beat();
    }
    let beats = ((duration_ms - grid.first_beat_ms) / grid.period_ms()).ceil().max(0.0) as u32;
    let features = sections::beat_features(&act, &grid, beats as usize);
    let scores = sections::downbeat_scores(&features);
    let (mut downbeat, downbeat_margin) =
        sections::pick_downbeat(&scores, sections::music_start(&features));
    let name = path.to_string_lossy();
    let forced = args
        .downbeat
        .iter()
        .filter_map(|hint| hint.rsplit_once('='))
        .find(|(substring, _)| name.contains(substring))
        .and_then(|(_, beat)| beat.trim().parse::<u32>().ok());
    if let Some(beat) = forced {
        downbeat = beat % BEATS_PER_BAR;
    }
    let bars = sections::bars(&features, downbeat);
    let sections = sections::sections(&bars, downbeat, beats);

    let file = path
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default();
    let stem = path
        .file_stem()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default();
    let title = match (&decoded.artist, &decoded.title) {
        (Some(artist), Some(title)) => format!("{artist} - {title}"),
        (None, Some(title)) => title.clone(),
        _ => stem,
    };
    let map = TrackMap {
        format: FORMAT_VERSION,
        title,
        file,
        artist: decoded.artist,
        tag_title: decoded.title,
        duration_ms,
        bpm: grid.bpm,
        first_beat_ms: grid.first_beat_ms,
        downbeat,
        downbeat_margin,
        bars: bars.features(),
        sections,
    };

    println!(
        "  {:.2} BPM, {}, {} bars, grid fit on {} beats ±{:.1} ms{}",
        map.bpm,
        clock(duration_ms),
        map.bars.energy.len(),
        grid.fitted_beats,
        grid.residual_ms,
        if shift_half_beat { ", moved half a beat" } else { "" }
    );
    println!(
        "  downbeat on grid beat {} ({})",
        map.downbeat,
        if forced.is_some() {
            "set by --downbeat".to_string()
        } else if downbeat_margin < 1.1 {
            format!("margin {downbeat_margin:.2}, unsure: check bar 1")
        } else {
            format!("margin {downbeat_margin:.2}")
        }
    );
    for section in &map.sections {
        print_section(&map, section);
    }
    if args.verbose {
        let b = &map.bars;
        for bar in 0..b.energy.len() {
            let bar_ms = map.ms_at_beat(map.bar_start(bar as i64) as f64);
            println!(
                "    bar {:>3} {}  kick {:>3} energy {:>3} low {:>3} high {:>3}",
                bar + 1,
                clock(bar_ms),
                b.kick[bar],
                b.energy[bar],
                b.low[bar],
                b.high[bar]
            );
        }
    }
    Ok(map)
}

fn print_section(map: &TrackMap, s: &Section) {
    let bars = (s.end - s.start) as f64 / BEATS_PER_BAR as f64;
    let bar = map.bar_at(s.start as f64).max(0.0).round() as i64 + 1;
    let mark = if s.kind == SectionKind::Drop { "  <<" } else { "" };
    println!(
        "    {}  bar {:>3}  {:<9} {:>5.1} bars{mark}",
        clock(map.ms_at_beat(s.start as f64)),
        bar,
        s.kind.name(),
        bars
    );
}

/// `m:ss.s`
fn clock(ms: f64) -> String {
    let s = ms.max(0.0) / 1000.0;
    format!("{}:{:04.1}", (s / 60.0).floor() as u32, s % 60.0)
}

fn slug(title: &str) -> String {
    let mut out = String::new();
    for c in title.chars().flat_map(char::to_lowercase) {
        if c.is_ascii_alphanumeric() {
            out.push(c);
        } else if !out.ends_with('-') && !out.is_empty() {
            out.push('-');
        }
    }
    let out: String = out.trim_end_matches('-').chars().take(60).collect();
    if out.is_empty() {
        "track".into()
    } else {
        out
    }
}

fn is_audio(path: &Path) -> bool {
    path.extension()
        .and_then(|e| e.to_str())
        .is_some_and(|e| AUDIO_EXTENSIONS.contains(&e.to_ascii_lowercase().as_str()))
}

fn collect(input: &Path, files: &mut Vec<PathBuf>) -> anyhow::Result<()> {
    if input.is_dir() {
        let mut found: Vec<PathBuf> = fs::read_dir(input)
            .with_context(|| format!("reading {}", input.display()))?
            .filter_map(|e| e.ok().map(|e| e.path()))
            .filter(|p| p.is_file() && is_audio(p))
            .collect();
        found.sort();
        files.extend(found);
        return Ok(());
    }
    let ext = input
        .extension()
        .and_then(|e| e.to_str())
        .map(str::to_ascii_lowercase);
    if matches!(ext.as_deref(), Some("m3u" | "m3u8")) {
        let text =
            fs::read_to_string(input).with_context(|| format!("reading {}", input.display()))?;
        let base = input.parent().unwrap_or(Path::new("."));
        for line in text.lines().map(str::trim) {
            if line.is_empty() || line.starts_with('#') {
                continue;
            }
            let line = line.strip_prefix("file://").unwrap_or(line);
            let path = Path::new(line);
            files.push(if path.is_absolute() {
                path.to_path_buf()
            } else {
                base.join(path)
            });
        }
        return Ok(());
    }
    if !input.exists() {
        bail!("{} does not exist", input.display());
    }
    files.push(input.to_path_buf());
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn slugs_are_file_safe() {
        assert_eq!(slug("DJ Heartstring - In Der Nacht"), "dj-heartstring-in-der-nacht");
        assert_eq!(slug("KIM SWIM ／ Back2U (Extended)"), "kim-swim-back2u-extended");
        assert_eq!(slug("¿?"), "track");
    }

    #[test]
    fn clock_formats_minutes_and_tenths() {
        assert_eq!(clock(61_234.0), "1:01.2");
        assert_eq!(clock(5_000.0), "0:05.0");
    }
}
