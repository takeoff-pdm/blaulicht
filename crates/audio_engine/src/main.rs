use std::{
    collections::HashMap,
    fs,
    num::NonZeroUsize,
    path::{Path, PathBuf},
    sync::Arc,
    thread,
    time::{Duration, Instant},
};

use blaulicht_audio_engine::{
    file::AudioSourceSoundfile, spectrogram::create_spectrogram_image, AudioSpectrogram,
    CollectorOutputSpec, SignalCollector, SpectrogramDisplayOptions,
};
use clap::Parser;
use egui::mutex::Mutex;
use kdam::{tqdm, BarExt};
use log::info;

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

    let dim = (600, 150);
    let mut imgbuf = image::ImageBuffer::new(dim.0, dim.1);

    let image = create_spectrogram_image(
        &spec,
        dim.0 as usize,
        dim.1 as usize,
        &SpectrogramDisplayOptions {
            include_bass_markers: false,
            include_beat_markers: false,
        },
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
    println!("PATH: {path:?}");

    imgbuf
        .save_with_format(&path, image::ImageFormat::Bmp)
        .unwrap();

    // info!("[BACKGROUND] Tick took: {:?}", start.elapsed(),);
}

fn main() {
    let Cli {
        offset_size,
        chunk_size,
        song_path,
        output_base,
        worker_threads,
    } = Cli::parse();

    let offsets = offset_size.get();
    let chunk_sizes = chunk_size.get();
    let thread_limit = worker_threads.get();

    let output = [CollectorOutputSpec {
        bins_p_column: Some(128),
    }];

    let song_path_str = song_path.to_string_lossy().into_owned();

    let audio_source = AudioSourceSoundfile::new(&song_path_str).unwrap();
    let length_millis = audio_source.duration();

    let spec_period = 16;
    let chunks = length_millis as f32 / offsets as f32;

    let threads: Arc<Mutex<HashMap<usize, _>>> = Arc::new(Mutex::new(HashMap::new()));

    let mut pb = tqdm!(total = chunks as usize);

    let base_audio_source = audio_source.clone();
    let threads_for_spawner = Arc::clone(&threads);
    let song_path_for_spawner = song_path.clone();
    let output_base_for_spawner = output_base.clone();
    thread::spawn(move || {
        for chunk in 0..chunks as usize {
            loop {
                {
                    let t = threads_for_spawner.lock();
                    if t.len() < thread_limit {
                        break;
                    }
                }

                thread::sleep(Duration::from_millis(100));
            }

            let audio_source = base_audio_source.clone();
            let song_path = song_path_for_spawner.clone();
            let output_base = output_base_for_spawner.clone();
            let handle = thread::spawn(move || {
                let mut last_spec_time = 0;

                let mut collector = SignalCollector::new(
                    blaulicht_audio_engine::SignalCollectorParams {
                        gate: 0,
                        boost: None,
                        volume: 100,
                        auto_calibrate: false,
                        changed: false,
                    },
                    output,
                    blaulicht_audio_engine::CollectorScratchParameters {
                        volume_frames: 1200,
                        long_historic_frames: 1200,
                        rolling_frames: 1200,
                        bass_frames: 1200,
                        bass_peak_frames: 1200,
                    },
                    audio_source,
                    0,
                )
                .unwrap();

                let mut spectrogram =
                    AudioSpectrogram::new(600, 128, Duration::from_millis(spec_period as u64));

                let start = offsets * chunk;
                let end = start + chunk_sizes;

                for i in start..end {
                    collector.tick(i).unwrap();

                    if (i - last_spec_time) > spec_period as usize {
                        last_spec_time = i;
                        let out = collector.tick_output::<0>();
                        spectrogram.push_data(out);
                    }
                }

                render_spec(spectrogram, chunk, &song_path, &output_base);
            });

            {
                let mut t = threads_for_spawner.lock();
                t.insert(chunk, handle);
            }
        }
    });

    thread::sleep(Duration::from_millis(100));

    loop {
        let mut t = threads.lock();
        if t.is_empty() {
            break;
        }

        let mut to_be_removed = vec![];

        for (chunk, handle) in t.iter() {
            if handle.is_finished() {
                to_be_removed.push(*chunk);
            }
        }

        for r in to_be_removed {
            if let Some(handle) = t.remove(&r) {
                drop(handle);
                let _ = pb.update(1);
                // println!("Thread {r} finished.")
            }
        }
    }

    eprintln!("");
}
