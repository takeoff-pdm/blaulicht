use std::{
    env, thread,
    time::{Duration, Instant},
};

use blaulicht_audio_engine::{
    file::AudioSourceSoundfile, spectrogram::create_spectrogram_image, AudioSpectrogram,
    CollectorOutputSpec, SignalCollector, SpectrogramDisplayOptions, VAR,
};
use log::info;

fn render_spec(spec: AudioSpectrogram, count: usize) {
    let start = Instant::now();

    let dim = (3024, 700);
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

    imgbuf
        .save_with_format(
            format!("/home/mik/blaulicht-spec/{count}.bmp"),
            image::ImageFormat::Bmp,
        )
        .unwrap();

    info!("[BACKGROUND] Tick took: {:?}", start.elapsed(),);
}

fn main() {
    let input_song = env::args().nth(1).unwrap();
    let offsets: usize = env::args().nth(2).unwrap().parse().unwrap();
    let chunk_sizes: usize = env::args().nth(3).unwrap().parse().unwrap();

    let output = [CollectorOutputSpec {
        bins_p_column: Some(128),
    }];

    let audio_source_ = AudioSourceSoundfile::new(&input_song).unwrap();
    let length_millis = audio_source_.duration();

    let spec_period = 16;
    let chunks = length_millis as f32 / offsets as f32;

    let mut threads = vec![];

    for chunk in 0..chunks as usize {
        let song = input_song.clone();
        let handle = thread::spawn(move || {
            let mut last_spec_time = 0;

            let audio_source = AudioSourceSoundfile::new(&song).unwrap();

            // let length_millis = audio_source.duration();

            let mut collector = SignalCollector::new(
                blaulicht_audio_engine::SignalCollectorParams {
                    gate: None,
                    boost: None,
                },
                output,
                blaulicht_audio_engine::CollectorScratchParameters {
                    volume_frames: 1200,
                    long_historic_frames: 1200,
                    rolling_frames: 1200,
                    bass_frames: 1200,
                    bass_peak_frames: 1200,
                },
                Box::new(audio_source),
                0,
            )
            .unwrap();

            let mut spectrogram =
                AudioSpectrogram::new(600, 128, Duration::from_millis(spec_period as u64));

            let start = offsets * chunk;
            let end = start + offsets;

            for i in start..end {
                collector.tick(i).unwrap();

                if (i - last_spec_time) > spec_period as usize {
                    last_spec_time = i;
                    let out = collector.tick_output::<0>();
                    spectrogram.push_data(out);
                }
            }

            println!("Processed chunk [{chunk}] {start}ms - {end}ms");
            render_spec(spectrogram, chunk);
        });

        threads.push(handle);
    }

    for t in threads {
        t.join();
    }
}
