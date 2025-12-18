use std::{collections::VecDeque, sync::RwLockReadGuard, time::Duration};

use blaulicht_shared::CollectedAudioSnapshot;
use egui::Color32;

use crate::CollectorOutput;

// use crate::{::collector::CollectorOutput, state::AudioSpectrogram};

/// Rolling buffer of recent spectra for a live spectrogram.
#[derive(Clone)]
pub struct AudioSpectrogram {
    /// Most-recent-last columns; each column is `bin_count` tall with u8 intensities 0..=255.
    /// Contains bins. A bin is just a averaged part of the frequency space.
    pub columns: VecDeque<CollectorOutput>,
    /// Maximum number of time columns to keep.
    pub max_columns: usize,
    /// Number of frequency bins per column.
    pub bin_count: usize,
    // Time to wait between ticks of the spectrogram.
    pub tick_period: Duration,
}

impl AudioSpectrogram {
    pub fn new(max_columns: usize, bin_count: usize, tick_period: Duration) -> Self {
        Self {
            columns: VecDeque::with_capacity(max_columns),
            max_columns,
            bin_count,
            tick_period,
        }
    }

    // pub fn audio_snapshot(&mut self, snapshot: CollectedAudioSnapshot) {
    //     self.snapshot = snapshot;
    // }

    pub fn push_data(&mut self, mut data: CollectorOutput) {
        // Column
        {
            // Ensure correct height; pad or truncate as needed.
            if data.current_audio_colunn.len() != self.bin_count {
                // panic!("Had to resize  {} vs. {}", col.len(), self.bin_count);
                // This can happen due to rounding issues.
                data.current_audio_colunn.resize(self.bin_count, 0);
            }
            // println!("{} vs {}", self.columns.len(), self.max_columns);
            if self.columns.len() >= self.max_columns {
                self.columns.pop_front();
                // println!("too many columns, reducing...");
            }
            self.columns.push_back(data);
        }
    }

    pub fn current_snapshot(&self) -> CollectedAudioSnapshot {
        self.columns
            .iter()
            .last()
            .unwrap_or(&CollectorOutput::default())
            .snapshot
    }
}

pub struct SpectrogramDisplayOptions {
    pub include_bass_markers: bool,
    pub include_beat_markers: bool,
}

// Create texture once when data changes (not every frame!)
pub fn create_spectrogram_image(
    spec: &AudioSpectrogram,
    width: usize,
    height_outer: usize,
    options: &SpectrogramDisplayOptions,
) -> egui::ColorImage {
    let pad_btm = 25;
    let height = height_outer - pad_btm;
    let mut pixels = vec![Color32::BLACK; width * height_outer];

    let total_cols = spec.max_columns;

    let pixels_per_column = width as f32 / total_cols as f32;

    let mut chunks_cont = spec.columns.clone();
    let columns_data = chunks_cont.make_contiguous();

    // Downsample if we have more columns than pixels
    let columns: Vec<CollectorOutput> = if pixels_per_column >= 1.0 {
        // Stretching: each column gets multiple pixels
        columns_data.iter().cloned().collect()
    } else {
        // Compressing: multiple columns per pixel - need to average
        let cols_per_pixel = (1.0 / pixels_per_column).ceil() as usize;

        let col: Vec<_> = columns_data
            .chunks(cols_per_pixel)
            .map(|chunks| {
                if chunks.is_empty() {
                    return CollectorOutput::default();
                }
                let bucket_count = chunks[0].current_audio_colunn.len();
                let mut averaged = CollectorOutput {
                    current_audio_colunn: Vec::with_capacity(bucket_count),
                    snapshot: CollectedAudioSnapshot::default(),
                };

                let mut bass_avg_short_avg = 0;
                for _ in 0..chunks.len() {
                    bass_avg_short_avg += averaged.snapshot.bass_avg_short as u16;
                }

                bass_avg_short_avg /= chunks.len() as u16;

                averaged.snapshot.bass_avg_short = bass_avg_short_avg as u8;

                for bucket_idx in 0..bucket_count {
                    let sum: u32 = chunks
                        .iter()
                        .map(|col| col.current_audio_colunn[bucket_idx] as u32)
                        .sum();
                    let avg = (sum / chunks.len() as u32) as u8;
                    averaged.current_audio_colunn.push(avg);
                }

                // average the snapshot

                averaged
            })
            .collect();

        col
    };

    // Now draw with proper scaling
    let col_width = if pixels_per_column >= 1.0 {
        pixels_per_column.floor() as usize
    } else {
        1 // One pixel per averaged column
    };

    for (idx, col) in columns.iter().enumerate() {
        let col_start_x = idx * col_width;
        let col_end_x = ((idx + 1) * col_width).min(width);

        let bucket_height = height as f32 / col.current_audio_colunn.len() as f32;

        for (bidx, &bucket) in col.current_audio_colunn.iter().rev().enumerate() {
            let y_min = (bidx as f32 * bucket_height) as usize;
            let y_max = ((bidx + 1) as f32 * bucket_height).min(height as f32) as usize;

            let bucket_color = spectrogram_color(bucket);

            // Draw buckets.
            for x in col_start_x..col_end_x {
                for y in y_min..y_max {
                    if x < width && y < height {
                        pixels[y * width + x] = bucket_color;
                    }
                }
            }

            // Draw bass marker.
            {
                if options.include_bass_markers {
                    if col.snapshot.bass_avg_short == 255 {
                        let dot_size = 5;
                        for y in (height + (pad_btm / 2) - dot_size)..height + (pad_btm / 2) {
                            pixels[y * width + col_start_x] = Color32::GREEN;
                        }
                    }
                }
            }

            // Draw beat marker.
            {
                if options.include_beat_markers {
                    if col.snapshot.beat_trigger {
                        for y in 0..height_outer {
                            pixels[y * width + col_start_x] = Color32::RED;
                        }
                    }
                }
            }
        }
    }

    let image = egui::ColorImage::new([width, height_outer], pixels);
    // ctx.load_texture("spectrogram", image, egui::TextureOptions::NEAREST)
    image
}

// Classic spectrogram: black -> purple -> blue -> cyan -> green -> yellow -> red -> white
fn spectrogram_color(intensity: u8) -> Color32 {
    let t = (intensity as f32 / 255.0).powf(0.5); // Gamma correction

    let (r, g, b) = if t < 0.2 {
        // Black to Purple
        let local_t = t / 0.2;
        (local_t * 0.5, 0.0, local_t)
    } else if t < 0.4 {
        // Purple to Blue
        let local_t = (t - 0.2) / 0.2;
        (0.5 - local_t * 0.5, 0.0, 1.0)
    } else if t < 0.6 {
        // Blue to Cyan/Green
        let local_t = (t - 0.4) / 0.2;
        (0.0, local_t, 1.0)
    } else if t < 0.8 {
        // Cyan to Yellow
        let local_t = (t - 0.6) / 0.2;
        (local_t, 1.0, 1.0 - local_t)
    } else {
        // Yellow to Red to White
        let local_t = (t - 0.8) / 0.2;
        (1.0, 1.0 - local_t * 0.5, local_t * 0.3)
    };

    Color32::from_rgb((r * 255.0) as u8, (g * 255.0) as u8, (b * 255.0) as u8)
}
