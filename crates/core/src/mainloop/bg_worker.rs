use std::{
    env,
    fs,
    path::PathBuf,
    sync::Arc,
    time::{Duration, Instant},
};

use blaulicht_audio_engine::SpectrogramDisplayOptions;
use log::{info, warn};

use crate::state::AppState;

// use crate::{app::components::SpectrogramDisplayOptions, state::AppState};

const BG_WORKER_TICK_DURATION: Duration = Duration::from_millis(33); // 60fps

pub fn spawn_bg_worker(app_state: Arc<AppState>) {
    info!("[BACKGROUND] Started worker");

    let mut count = 0u64;
    let mut output_dir = match env::var_os("BLAULICHT_SPECTROGRAM_DIR") {
        Some(path) => {
            let dir = PathBuf::from(path);
            match fs::create_dir_all(&dir) {
                Ok(_) => {
                    info!(
                        "[BACKGROUND] Writing spectrogram snapshots to {}",
                        dir.display()
                    );
                    Some(dir)
                }
                Err(err) => {
                    warn!(
                        "[BACKGROUND] Unable to create spectrogram output directory {}: {err}",
                        dir.display()
                    );
                    None
                }
            }
        }
        None => {
            info!("[BACKGROUND] Spectrogram output directory not set; skipping disk writes.");
            None
        }
    };

    let dim = (480, 100);
    let mut imgbuf = image::ImageBuffer::new(dim.0, dim.1);

    // panic!("HESSEN");

    loop {
        let start = Instant::now();
        //
        // Save spectrogram
        //
        let spec = app_state.audio_spectrogram.read().unwrap();
        let image = crate::app::components::create_spectrogram_image(
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

        if let Some(dir) = &output_dir {
            let path = dir.join(format!("{count}.bmp"));
            if let Err(err) = imgbuf.save_with_format(&path, image::ImageFormat::Bmp) {
                warn!(
                    "[BACKGROUND] Failed to write spectrogram frame {}: {err}",
                    path.display()
                );
                // Disable further writes if the error is persistent (e.g. permission denied)
                output_dir = None;
            }
        }

        count += 1;

        if start.elapsed() > BG_WORKER_TICK_DURATION {
            warn!(
                "[BACKGROUND] Tick took: {:?}, but tick period is {:?}",
                start.elapsed(),
                BG_WORKER_TICK_DURATION
            );
        }

        spin_sleep::sleep(BG_WORKER_TICK_DURATION - start.elapsed()); // Account for diff
    }
}
