use blaulicht_audio_engine::{
    create_spectrogram_image, AudioSpectrogram, CollectorOutput, SpectrogramDisplayOptions,
};
use criterion::{criterion_group, criterion_main, Criterion};
use egui::ColorImage;
use std::time::Duration;

fn criterion_benchmark(c: &mut Criterion) {
    let mut spec = AudioSpectrogram::new(1000, 128, Duration::from_millis(1000 / 60));

    for _ in 0..500 {
        spec.push_data(CollectorOutput::default());
    }

    let mut image = ColorImage::default();

    c.bench_function("Render spectrogram once", |b| {
        let width = 1800;
        let height_outer = 200;

        b.iter(|| {
            create_spectrogram_image(
                &spec,
                width,
                height_outer,
                &SpectrogramDisplayOptions {
                    include_beat_markers: true,
                },
                &mut image,
            );
        })
    });
}

criterion_group!(benches, criterion_benchmark);
criterion_main!(benches);
