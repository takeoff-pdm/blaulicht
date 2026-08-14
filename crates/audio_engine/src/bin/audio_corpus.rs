use std::{
    collections::BTreeMap,
    fs,
    path::{Path, PathBuf},
    time::Duration,
};

use blaulicht_audio_engine::{
    create_spectrogram_image, file::AudioSourceSoundfile, AudioSpectrogram, BpmDetectStatus,
    CollectorOutputSpec, LoopTempoEstimator, SignalCollector, SpectrogramDisplayOptions,
};
use clap::Parser;
use egui::{Color32, ColorImage};

const FREQ_BUFFER_SIZE: usize = 2048;
const BIN_COUNT: usize = 128;
const FRAME_STEP_MS: usize = 10;
const SPECTROGRAM_COLUMNS: usize = 600;
const SPECTROGRAM_PERIOD_MS: usize = 20;

#[derive(Debug, Parser)]
#[command(about = "Run the audio analysis engine against a directory of audio files")]
struct Args {
    /// Directory containing audio files. Files are processed recursively.
    #[arg(long)]
    input: PathBuf,
    /// Directory for generated spectrogram PNGs.
    #[arg(long)]
    output: PathBuf,
    /// Stop after processing this many files, useful for a quick smoke test.
    #[arg(long)]
    limit: Option<usize>,
}

fn collect_files(path: &Path, files: &mut Vec<PathBuf>) -> anyhow::Result<()> {
    if path.is_file() {
        files.push(path.to_path_buf());
        return Ok(());
    }

    for entry in fs::read_dir(path)? {
        let entry = entry?;
        let entry_path = entry.path();
        if entry_path.is_dir() {
            collect_files(&entry_path, files)?;
        } else if entry_path.is_file() {
            files.push(entry_path);
        }
    }
    Ok(())
}

fn render_spectrogram(spec: &AudioSpectrogram, path: &Path) -> anyhow::Result<()> {
    let width = 1200usize;
    let height = 360usize;
    let mut image = ColorImage::new([width, height], vec![Color32::BLACK; width * height]);
    create_spectrogram_image(
        spec,
        width,
        height,
        &SpectrogramDisplayOptions {
            include_beat_markers: true,
        },
        &mut image,
    );

    let mut output = image::RgbImage::new(width as u32, height as u32);
    for (x, y, pixel) in output.enumerate_pixels_mut() {
        let source = image.pixels[y as usize * width + x as usize];
        *pixel = image::Rgb([source.r(), source.g(), source.b()]);
    }
    output.save(path)?;
    Ok(())
}

fn process_file(path: &Path, output_dir: &Path) -> anyhow::Result<()> {
    let path_string = path.to_string_lossy();
    let source = AudioSourceSoundfile::new(&path_string, FREQ_BUFFER_SIZE)
        .map_err(|err| anyhow::anyhow!("decode {}: {err}", path.display()))?;
    let duration_ms = source.duration();
    let loop_tempo = LoopTempoEstimator::default().analyze(source.samples(), source.sample_rate());

    let mut collector = SignalCollector::new(
        Default::default(),
        [
            CollectorOutputSpec {
                bins_p_column: Some(BIN_COUNT),
                raw: false,
            },
            CollectorOutputSpec {
                bins_p_column: Some(BIN_COUNT),
                raw: true,
            },
        ],
        Default::default(),
        source,
        0,
    )?;
    let mut spectrogram = AudioSpectrogram::new(
        SPECTROGRAM_COLUMNS,
        BIN_COUNT,
        Duration::from_millis(SPECTROGRAM_PERIOD_MS as u64),
    );

    let stem = path
        .file_stem()
        .map(|value| value.to_string_lossy().replace(['/', '\\'], "_"))
        .unwrap_or_else(|| "audio".to_string());
    let file_output_dir = output_dir.join(&stem);
    fs::create_dir_all(&file_output_dir)?;

    let mut frames = 0usize;
    let mut beat_events = 0usize;
    let mut onset_events = 0usize;
    let mut last_spectrogram_ms = 0usize;
    let mut max_bpm = 0.0f32;
    let mut max_bass = 0u8;
    let mut max_volume = 0u8;
    let mut max_band_energy = 0.0f32;
    let mut max_onset_peakiness = 0.0f32;
    let mut max_onset_periodicity = 0.0f32;
    let mut status_counts = BTreeMap::<String, usize>::new();
    let mut max_weak_periodicity = 0.0f32;
    let mut active_frames = 0usize;
    let mut segment = 0usize;

    for now in (0..duration_ms).step_by(FRAME_STEP_MS) {
        collector
            .tick(now as u64)
            .map_err(|err| anyhow::anyhow!("analysis at {now} ms in {}: {err}", path.display()))?;
        let output = collector.tick_output::<0>();
        let snapshot = &output.snapshot;

        if !snapshot.bpm.is_finite() || !snapshot.bpm_confidence.is_finite() {
            anyhow::bail!(
                "non-finite analysis output at {now} ms in {}",
                path.display()
            );
        }
        if !output
            .debug_data
            .band_energies
            .iter()
            .chain(output.debug_data.band_onset_peakiness.iter())
            .chain(output.debug_data.band_onset_periodicity.iter())
            .all(|value| value.is_finite())
        {
            anyhow::bail!("non-finite debug output at {now} ms in {}", path.display());
        }

        frames += 1;
        beat_events += usize::from(snapshot.beat_trigger);
        onset_events += usize::from(snapshot.actual_onset_peak);
        max_bpm = max_bpm.max(snapshot.bpm);
        max_bass = max_bass.max(snapshot.bass);
        max_volume = max_volume.max(snapshot.volume);
        max_band_energy = max_band_energy.max(
            output
                .debug_data
                .band_energies
                .iter()
                .copied()
                .fold(0.0, f32::max),
        );
        max_onset_peakiness = max_onset_peakiness.max(
            output
                .debug_data
                .band_onset_peakiness
                .iter()
                .copied()
                .fold(0.0, f32::max),
        );
        max_onset_periodicity = max_onset_periodicity.max(
            output
                .debug_data
                .band_onset_periodicity
                .iter()
                .copied()
                .fold(0.0, f32::max),
        );
        let status_name = match output.debug_data.bpm_status {
            BpmDetectStatus::Detecting => "Detecting",
            BpmDetectStatus::NoEnergy => "NoEnergy",
            BpmDetectStatus::Warmup { .. } => "Warmup",
            BpmDetectStatus::FlatOnset => "FlatOnset",
            BpmDetectStatus::WeakPeriodicity { strength, .. } => {
                max_weak_periodicity = max_weak_periodicity.max(strength);
                "WeakPeriodicity"
            }
        };
        *status_counts.entry(status_name.to_string()).or_default() += 1;
        active_frames +=
            usize::from(snapshot.source_status == blaulicht_shared::AudioSourceStatus::Active);

        if now.saturating_sub(last_spectrogram_ms) >= SPECTROGRAM_PERIOD_MS {
            spectrogram.push_data(collector.tick_output::<1>());
            last_spectrogram_ms = now;
        }

        if spectrogram.columns.len() == SPECTROGRAM_COLUMNS {
            let segment_path = file_output_dir.join(format!("{segment:04}.png"));
            render_spectrogram(&spectrogram, &segment_path)?;
            segment += 1;
            spectrogram.columns.clear();
        }
    }

    if !spectrogram.columns.is_empty() {
        let segment_path = file_output_dir.join(format!("{segment:04}.png"));
        render_spectrogram(&spectrogram, &segment_path)?;
    }

    if frames == 0 {
        anyhow::bail!("no decodable frames in {}", path.display());
    }

    println!(
        "{}: duration={}ms frames={} active={} beats={} onsets={} live_max_bpm={:.1} loop_bpm={:?} loop_candidate_bpm={:?} loop_score={:?} loop_tatums={:?} loop_lag={:?} loop_rejection={:?} max_bass={} max_volume={} max_band_energy={:.3} max_peakiness={:.3} max_periodicity={:.3} max_weak_periodicity={:.5} statuses={:?} spectrograms={}",
        path.display(),
        duration_ms,
        frames,
        active_frames,
        beat_events,
        onset_events,
        max_bpm,
        loop_tempo.bpm,
        loop_tempo.candidate_bpm,
        loop_tempo.score,
        loop_tempo.tatum_count,
        loop_tempo.onset_lag,
        loop_tempo.rejection,
        max_bass,
        max_volume,
        max_band_energy,
        max_onset_peakiness,
        max_onset_periodicity,
        max_weak_periodicity,
        status_counts,
        segment + usize::from(!spectrogram.columns.is_empty()),
    );
    Ok(())
}

fn main() -> anyhow::Result<()> {
    let args = Args::parse();
    let mut files = Vec::new();
    collect_files(&args.input, &mut files)?;
    files.sort();
    if let Some(limit) = args.limit {
        files.truncate(limit);
    }
    if files.is_empty() {
        anyhow::bail!("no audio files found under {}", args.input.display());
    }

    fs::create_dir_all(&args.output)?;
    let mut failures = Vec::new();
    for file in &files {
        if let Err(err) = process_file(file, &args.output) {
            eprintln!("FAILED {}: {err:#}", file.display());
            failures.push(file.display().to_string());
        }
    }

    if failures.is_empty() {
        println!("processed {} audio file(s) successfully", files.len());
        Ok(())
    } else {
        anyhow::bail!(
            "{} audio file(s) failed: {}",
            failures.len(),
            failures.join(", ")
        )
    }
}
