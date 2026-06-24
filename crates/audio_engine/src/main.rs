use std::{
    collections::HashMap,
    fs,
    num::NonZeroUsize,
    path::{Path, PathBuf},
    str::FromStr,
    sync::Arc,
    thread,
    time::{Duration, Instant},
};

use blaulicht_audio_engine::{
    audio_source, file::AudioSourceSoundfile, noise::AudioSourceNoise,
    spectrogram::create_spectrogram_image, AudioSpectrogram, CollectorOutputSpec, SignalCollector,
    SpectrogramDisplayOptions,
};
use clap::Parser;
use egui::{mutex::Mutex, ColorImage};
use kdam::{tqdm, BarExt};

const FREQ_BUFFER_SIZE: usize = 2048;

#[derive(Parser, Debug)]
#[command(author, version, about, long_about = None)]
struct Cli {
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

fn render_spec(spec: AudioSpectrogram, count: usize, song_path: &Path, output_base: &Path) {
    let _start = Instant::now();
    let dim = (700, 200);
    let mut imgbuf = image::ImageBuffer::new(dim.0, dim.1);

    let mut image = ColorImage::default();

    create_spectrogram_image(
        &spec,
        dim.0 as usize,
        dim.1 as usize,
        &SpectrogramDisplayOptions {
            // include_bass_markers: false,
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
    // println!("PATH: {path:?}");

    let a = fs::remove_file(&path);

    imgbuf
        .save_with_format(&path, image::ImageFormat::Bmp)
        .unwrap();

    // info!("[BACKGROUND] Tick took: {:?}", start.elapsed(),);
}

fn main() {
    // let Cli {
    //     offset_size,
    //     chunk_size,
    //     song_path,
    //     output_base,
    //     worker_threads,
    // } = Cli::parse();

    // let offsets = offset_size.get();
    // let chunk_sizes = chunk_size.get();
    // let thread_limit = worker_threads.get();
    //

    let spectrogram_column_bin_count = 128;
    let output = [CollectorOutputSpec {
        bins_p_column: Some(spectrogram_column_bin_count),
        raw: false,
    }];

    let song_path = PathBuf::from_str("FOO").unwrap();
    let output_base = PathBuf::from_str("./OUTPUT").unwrap();

    let song_path_str = "/home/mik/Documents/mit-CBCAST-kommst-zu-inst-BERGHAIN.wav";

    // TODO: what is a good value for this?
    let audio_source = AudioSourceSoundfile::new(&song_path_str, FREQ_BUFFER_SIZE).unwrap();
    // let length = 20000;
    // let audio_source = AudioSourceNoise::new(41000, length, 2000);
    let length_millis = audio_source.duration();

    let spec_period = 16;
    let offsets = 10000;
    let chunks = (length_millis as f32 / offsets as f32) as usize;

    println!("conversion gets us: {chunks} chunks");

    let mut pb = tqdm!(total = chunks as usize);

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
        },
        output,
        blaulicht_audio_engine::CollectorScratchParameters {
            volume_frames: 1200,
            rolling_frames: 1200,
            bass_frames: 1200,
            bass_peak_frames: 1200,
        },
        audio_source,
        0,
    )
    .unwrap();

    // println!("Thread: created collector");

    for chunk in 0..chunks {
        let start = chunk * offsets;
        let end = start + offsets;

        let mut spectrogram = AudioSpectrogram::new(
            600,
            spectrogram_column_bin_count,
            Duration::from_millis(spec_period as u64),
        );

        for i in start..end {
            // println!("run: {i}/{end}");

            collector.tick(i as u64).unwrap();

            if (i - last_spec_time) > spec_period as usize {
                last_spec_time = i;
                let out = collector.tick_output::<0>();
                spectrogram.push_data(out);
            }
        }

        render_spec(spectrogram.clone(), chunk, &song_path, &output_base);
        pb.update(1).unwrap();
    }
}
