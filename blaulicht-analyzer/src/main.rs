use std::{
    cmp, fs,
    path::{Path, PathBuf},
};

use anyhow::{Context, Result, anyhow};
use audioviz::spectrum::Frequency;
use blaulicht_audio_engine::file::AudioSourceSoundfile;

const BIN_COUNT: usize = 64;
const BASE_STEP_MS: usize = 200;
const MAX_FRAMES: usize = 60;
const SHADE_MAP: [char; 10] = [' ', '.', ':', '-', '=', '+', '*', '#', '%', '@'];

fn main() -> Result<()> {
    run()
}

fn run() -> Result<()> {
    let cwd = std::env::current_dir().context("failed to determine current directory")?;
    let audio_files = collect_audio_files(&cwd)?;

    if audio_files.is_empty() {
        println!("No audio files found in {}.", cwd.display());
        return Ok(());
    }

    println!(
        "Found {} audio file(s) in {}.",
        audio_files.len(),
        cwd.display()
    );

    for path in audio_files {
        visualize_file(&path)?;
    }

    Ok(())
}

fn collect_audio_files(dir: &Path) -> Result<Vec<PathBuf>> {
    let entries =
        fs::read_dir(dir).with_context(|| format!("failed to read directory {}", dir.display()))?;

    let mut files = Vec::new();
    for entry in entries {
        let entry = entry?;
        let path = entry.path();

        if !path.is_file() {
            continue;
        }

        let extension = path
            .extension()
            .and_then(|s| s.to_str())
            .map(|s| s.to_ascii_lowercase());

        if let Some(ext) = extension {
            if is_supported_extension(&ext) {
                files.push(path);
            }
        }
    }

    files.sort();
    Ok(files)
}

fn is_supported_extension(ext: &str) -> bool {
    matches!(
        ext,
        "mp3" | "wav" | "flac" | "ogg" | "m4a" | "aac" | "aiff" | "aif" | "opus" | "caf" | "mka"
    )
}

fn visualize_file(path: &Path) -> Result<()> {
    let display_name = path
        .file_name()
        .and_then(|name| name.to_str())
        .map(|name| name.to_owned())
        .unwrap_or_else(|| path.to_string_lossy().to_string());

    let path_string = path.to_string_lossy();
    let source = AudioSourceSoundfile::new(&path_string)
        .map_err(|err| anyhow!("failed to decode {}: {err}", path.display()))?;

    let duration = source.duration();
    if duration == 0 {
        println!("\n{display_name}: empty audio stream");
        return Ok(());
    }

    let sample_rate = source.sample_rate();
    let estimated_frames = cmp::max(1, duration / BASE_STEP_MS);
    let frame_count = cmp::min(estimated_frames, MAX_FRAMES.max(1));
    let frame_step_ms = if estimated_frames <= MAX_FRAMES {
        BASE_STEP_MS
    } else {
        cmp::max(1, duration / frame_count)
    };

    println!(
        "\n{} ({} ms, {} Hz, step {} ms)",
        display_name, duration, sample_rate, frame_step_ms
    );
    println!("{:>6}     {}", "time", "spectrum");

    for frame_idx in 0..frame_count {
        let time_ms = frame_idx * frame_step_ms;
        if time_ms >= duration {
            break;
        }

        let freqs = source.get_frequencies_at_time(time_ms);

        let line = if freqs.is_empty() {
            " ".repeat(BIN_COUNT)
        } else {
            let bins = bin_frequencies(&freqs, BIN_COUNT);
            render_bins(&bins)
        };

        println!("{:>6} ms |{}", time_ms, line);
    }

    Ok(())
}

fn bin_frequencies(freqs: &[Frequency], bin_count: usize) -> Vec<f32> {
    if bin_count == 0 {
        return Vec::new();
    }

    let mut bins = vec![0.0f32; bin_count];
    let mut counts = vec![0usize; bin_count];

    for freq in freqs.iter() {
        if !freq.volume.is_finite() || freq.volume <= 0.0 {
            continue;
        }

        let mut idx = (freq.position * bin_count as f32).floor() as usize;
        if idx >= bin_count {
            idx = bin_count - 1;
        }

        bins[idx] += freq.volume;
        counts[idx] += 1;
    }

    for (value, count) in bins.iter_mut().zip(counts.iter()) {
        if *count > 0 {
            *value /= *count as f32;
        }
    }

    let max_value = bins
        .iter()
        .copied()
        .fold(0.0f32, |acc, v| if v > acc { v } else { acc });

    if max_value > 0.0 {
        for value in bins.iter_mut() {
            *value /= max_value;
        }
    }

    bins
}

fn render_bins(bins: &[f32]) -> String {
    bins.iter()
        .map(|&value| {
            let normalized = value.clamp(0.0, 1.0);
            let scaled = (normalized * (SHADE_MAP.len() as f32 - 1.0)).floor() as usize;
            let idx = cmp::min(scaled, SHADE_MAP.len() - 1);
            SHADE_MAP[idx]
        })
        .collect()
}
