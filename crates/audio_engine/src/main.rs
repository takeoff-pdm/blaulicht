use std::{
    fs,
    num::NonZeroUsize,
    path::{Path, PathBuf},
    time::Duration,
};

use blaulicht_audio_engine::{
    create_spectrogram_image, file::AudioSourceSoundfile, AudioSpectrogram, CollectorOutputSpec,
    LoopTempoEstimator, SignalCollector, SpectrogramDisplayOptions,
};
use clap::{Parser, Subcommand, ValueEnum};
use egui::{ColorImage, Rgba};
use kdam::{tqdm, BarExt};

const FREQ_BUFFER_SIZE: usize = 2048;
const SPECTROGRAM_COLUMN_BIN_COUNT: usize = 128;

#[derive(Parser, Debug)]
#[command(author, version, about, long_about = None)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand, Debug)]
enum Command {
    /// Estimate BPM for each audio file and print a per-window breakdown.
    Bpm(BpmArgs),
    /// Render spectrogram BMP chunks for a single file.
    Spectrogram(SpectrogramArgs),
}

#[derive(Debug, Parser)]
struct BpmArgs {
    /// Audio file or directory containing audio files.
    #[arg(long)]
    input: PathBuf,
    /// Analysis window size in milliseconds.
    #[arg(long, default_value_t = 30_000)]
    window_ms: usize,
    /// Step size between analysis windows in milliseconds.
    #[arg(long, default_value_t = 30_000)]
    hop_ms: usize,
    /// Output format.
    #[arg(long, value_enum, default_value_t = BpmFormat::Text)]
    format: BpmFormat,
}

#[derive(Clone, Copy, Debug, ValueEnum)]
enum BpmFormat {
    Text,
    Csv,
}

#[derive(Debug, Parser)]
struct SpectrogramArgs {
    #[arg(long, value_name = "MILLIS")]
    offset_size: NonZeroUsize,
    #[arg(long, value_name = "MILLIS")]
    chunk_size: NonZeroUsize,
    #[arg(long, value_name = "PATH")]
    song_path: PathBuf,
    #[arg(long, value_name = "DIR")]
    output_base: PathBuf,
    #[arg(long, value_name = "COUNT")]
    worker_threads: NonZeroUsize,
}

#[derive(Debug)]
struct WindowTempo {
    start_ms: usize,
    end_ms: usize,
    loop_bpm: Option<f64>,
    loop_candidate_bpm: Option<f64>,
    loop_score: Option<f64>,
}

fn main() -> anyhow::Result<()> {
    match Cli::parse().command {
        Command::Bpm(args) => run_bpm(args),
        Command::Spectrogram(args) => run_spectrogram(args),
    }
}

fn run_bpm(args: BpmArgs) -> anyhow::Result<()> {
    if args.window_ms == 0 || args.hop_ms == 0 {
        anyhow::bail!("--window-ms and --hop-ms must be greater than zero");
    }

    let mut files = Vec::new();
    collect_audio_files(&args.input, &mut files)?;
    files.sort();
    if files.is_empty() {
        anyhow::bail!("no audio files found under {}", args.input.display());
    }

    if matches!(args.format, BpmFormat::Csv) {
        println!("file,start_ms,end_ms,loop_bpm,loop_candidate_bpm,loop_score");
    }

    for file in files {
        match analyze_bpm_file(&file, args.window_ms, args.hop_ms) {
            Ok(windows) => print_bpm_windows(&file, &windows, args.format),
            Err(err) => eprintln!("FAILED {}: {err:#}", file.display()),
        }
    }

    Ok(())
}

fn collect_audio_files(path: &Path, files: &mut Vec<PathBuf>) -> anyhow::Result<()> {
    if path.is_file() {
        if is_audio_path(path) {
            files.push(path.to_path_buf());
        }
        return Ok(());
    }

    for entry in fs::read_dir(path)? {
        let entry = entry?;
        let path = entry.path();
        if path.is_dir() {
            collect_audio_files(&path, files)?;
        } else if path.is_file() && is_audio_path(&path) {
            files.push(path);
        }
    }
    Ok(())
}

fn is_audio_path(path: &Path) -> bool {
    path.extension()
        .and_then(|extension| extension.to_str())
        .map(|extension| {
            matches!(
                extension.to_ascii_lowercase().as_str(),
                "mp3" | "flac" | "wav" | "ogg" | "aiff" | "aif"
            )
        })
        .unwrap_or(false)
}

fn analyze_bpm_file(
    path: &Path,
    window_ms: usize,
    hop_ms: usize,
) -> anyhow::Result<Vec<WindowTempo>> {
    let path_string = path.to_string_lossy();
    let source = AudioSourceSoundfile::new(&path_string, FREQ_BUFFER_SIZE)
        .map_err(|err| anyhow::anyhow!("decode {}: {err}", path.display()))?;
    let duration_ms = source.duration();
    if duration_ms == 0 {
        anyhow::bail!("empty audio file");
    }

    let samples = source.samples().to_vec();
    let sample_rate = source.sample_rate();

    let mut windows = Vec::new();
    let estimator = LoopTempoEstimator::default();

    let mut start_ms = 0usize;
    while start_ms < duration_ms {
        let end_ms = start_ms.saturating_add(window_ms).min(duration_ms);
        let loop_tempo = estimator.analyze(
            window_samples(&samples, sample_rate, start_ms, end_ms),
            sample_rate,
        );
        windows.push(WindowTempo {
            start_ms,
            end_ms,
            loop_bpm: loop_tempo.bpm,
            loop_candidate_bpm: loop_tempo.candidate_bpm,
            loop_score: loop_tempo.score,
        });
        if end_ms == duration_ms {
            break;
        }
        start_ms = start_ms.saturating_add(hop_ms);
    }

    Ok(windows)
}

fn window_samples(samples: &[f32], sample_rate: u32, start_ms: usize, end_ms: usize) -> &[f32] {
    let start = ms_to_sample(start_ms, sample_rate).min(samples.len());
    let end = ms_to_sample(end_ms, sample_rate).min(samples.len());
    &samples[start.min(end)..end]
}

fn ms_to_sample(ms: usize, sample_rate: u32) -> usize {
    ((ms as u128 * sample_rate as u128) / 1000) as usize
}

fn print_bpm_windows(path: &Path, windows: &[WindowTempo], format: BpmFormat) {
    match format {
        BpmFormat::Text => {
            println!("{}", path.display());
            for window in windows {
                println!(
                    "  {}-{}  loop={} candidate={} score={}",
                    format_ms(window.start_ms),
                    format_ms(window.end_ms),
                    format_optional_bpm(window.loop_bpm),
                    format_optional_bpm(window.loop_candidate_bpm),
                    format_optional_score(window.loop_score),
                );
            }
        }
        BpmFormat::Csv => {
            for window in windows {
                println!(
                    "{},{},{},{},{},{}",
                    csv_escape(&path.display().to_string()),
                    window.start_ms,
                    window.end_ms,
                    format_optional_bpm(window.loop_bpm),
                    format_optional_bpm(window.loop_candidate_bpm),
                    format_optional_score(window.loop_score),
                );
            }
        }
    }
}

fn format_ms(ms: usize) -> String {
    let total_seconds = ms / 1000;
    format!("{:02}:{:02}", total_seconds / 60, total_seconds % 60)
}

fn format_optional_bpm(value: Option<f64>) -> String {
    value
        .map(|value| format!("{value:.2}"))
        .unwrap_or_else(|| "-".to_string())
}

fn format_optional_score(value: Option<f64>) -> String {
    value
        .map(|value| format!("{value:.3}"))
        .unwrap_or_else(|| "-".to_string())
}

fn csv_escape(value: &str) -> String {
    if value.contains([',', '"', '\n']) {
        format!("\"{}\"", value.replace('"', "\"\""))
    } else {
        value.to_string()
    }
}

fn render_spec(spec: AudioSpectrogram, count: usize, song_path: &Path, output_base: &Path) {
    let dim = (700, 200);
    let mut imgbuf = image::ImageBuffer::new(dim.0, dim.1);

    let mut image = ColorImage::filled([0, 0], Rgba::BLACK.into());

    create_spectrogram_image(
        &spec,
        dim.0 as usize,
        dim.1 as usize,
        &SpectrogramDisplayOptions {
            include_beat_markers: false,
        },
        &mut image,
    );

    for (x, y, pixel) in imgbuf.enumerate_pixels_mut() {
        let source_pixel = image.pixels[y as usize * image.width() + x as usize];
        *pixel = image::Rgb([source_pixel.r(), source_pixel.g(), source_pixel.b()]);
    }

    let song_name = song_path
        .file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_else(|| "output".to_string());

    let path = output_base.join(song_name);
    let _ = fs::create_dir_all(&path);

    let path = path.join(format!("{count}.bmp"));
    let _ = fs::remove_file(&path);

    imgbuf
        .save_with_format(&path, image::ImageFormat::Bmp)
        .unwrap();
}

fn run_spectrogram(args: SpectrogramArgs) -> anyhow::Result<()> {
    let SpectrogramArgs {
        offset_size,
        chunk_size: _,
        song_path,
        output_base,
        worker_threads: _,
    } = args;

    let output = [CollectorOutputSpec {
        bins_p_column: Some(SPECTROGRAM_COLUMN_BIN_COUNT),
        raw: false,
    }];

    let song_path_str = song_path.to_string_lossy();
    let audio_source = AudioSourceSoundfile::new(&song_path_str, FREQ_BUFFER_SIZE)
        .map_err(|err| anyhow::anyhow!("decode {}: {err}", song_path.display()))?;
    let length_millis = audio_source.duration();

    let spec_period = 16;
    let offsets = offset_size.get();
    let chunks = (length_millis as f32 / offsets as f32) as usize;

    println!("conversion gets us: {chunks} chunks");

    let mut pb = tqdm!(total = chunks);

    let mut last_spec_time = 0;

    let mut collector = SignalCollector::new(
        blaulicht_audio_engine::SignalCollectorParams {
            gate: 0,
            boost: None,
            volume: 100,
            auto_calibrate: false,
            auto_weight: false,
            changed: false,
            bass_freq_low: 0,
            bass_freq_high: 250,
            bass_volume: 25,
            drop_sensitivity: 70,
            drop_bass_min: 25,
            drop_bass_avg_min: 15,
            drop_require_both: false,
            drop_sustain_ms: 280,
            drop_breakdown_hold_ms: 2500,
            breakdown_sensitivity: 100,
            breakdown_on_low_bpm: false,
            breakdown_bpm_confidence_min: 40,
            drop_use_peakiness: false,
            drop_peakiness_min: 6,
            manual_bpm: None,
        },
        output,
        blaulicht_audio_engine::CollectorScratchParameters {
            volume_frames: 1200,
            long_historic_frames: 1200,
            rolling_frames: 1200,
            bass_frames: 1200,
        },
        audio_source,
        0,
    )?;

    for chunk in 0..chunks {
        let start = chunk * offsets;
        let end = start + offsets;

        let mut spectrogram = AudioSpectrogram::new(
            600,
            SPECTROGRAM_COLUMN_BIN_COUNT,
            Duration::from_millis(spec_period as u64),
        );

        for i in start..end {
            collector.tick(i as u64)?;

            if (i - last_spec_time) > spec_period {
                last_spec_time = i;
                let out = collector.tick_output::<0>();
                spectrogram.push_data(out);
            }
        }

        render_spec(spectrogram.clone(), chunk, &song_path, &output_base);
        pb.update(1).unwrap();
    }

    Ok(())
}
