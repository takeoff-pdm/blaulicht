use std::{collections::VecDeque, sync::RwLockReadGuard, time::Duration};

use audioviz::io::output;
use blaulicht_shared::CollectedAudioSnapshot;
use egui::Color32;

use crate::{AudioBucket, CollectorOutput};

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
                data.current_audio_colunn
                    .resize(self.bin_count, AudioBucket::default());
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
            .clone()
    }
}

pub struct SpectrogramDisplayOptions {
    pub include_bass_markers: bool,
    pub include_beat_markers: bool,
}

fn downsample(columns_data: &[CollectorOutput], pixels_per_column: f32) -> Vec<CollectorOutput> {
    // Downsample if we have more columns than pixels
    let columns: Vec<CollectorOutput> = if pixels_per_column >= 1.0 {
        // Stretching: each column gets multiple pixels
        columns_data.iter().cloned().collect()
    } else {
        println!("WARN: downsample() Pixel per column: {pixels_per_column} need to downsample");

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
                        .map(|col| col.current_audio_colunn[bucket_idx].volume as u32)
                        .sum();
                    let avg = (sum / chunks.len() as u32) as u8;

                    let freq_bound_upper = chunks
                        .iter()
                        .map(|col| col.current_audio_colunn[bucket_idx].freq_bound_upper)
                        .max()
                        .unwrap_or(0);

                    let freq_bound_lower = chunks
                        .iter()
                        .map(|col| col.current_audio_colunn[bucket_idx].freq_bound_lower)
                        .min()
                        .unwrap_or(0);

                    averaged.current_audio_colunn.push(crate::AudioBucket {
                        volume: avg,
                        freq_bound_upper,
                        freq_bound_lower,
                    });
                }

                // average the snapshot

                averaged
            })
            .collect();

        col
    };

    columns
}

// Create texture once when data changes (not every frame!)
pub fn create_spectrogram_image(
    spec: &AudioSpectrogram,
    width: usize,
    height_outer: usize,
    options: &SpectrogramDisplayOptions,
) -> egui::ColorImage {
    let pad_btm = 20;
    let pad_top = 5;
    let pad_left = 5;

    let height = height_outer - pad_btm - pad_top;
    let mut pixels = vec![Color32::BLACK; width * height_outer];

    let total_cols = spec.max_columns;

    let avail_width = width - pad_left;
    let pixels_per_column = (avail_width as f32 / total_cols as f32);

    let mut chunks_cont = spec.columns.clone();
    let columns_data = chunks_cont.make_contiguous();

    // NOTE: downsampling does nothing for vertical issues.
    let columns_data = downsample(columns_data, pixels_per_column);

    let pixels_per_column = (avail_width as f32 / columns_data.len() as f32).floor() as usize;

    println!("pixels per column: {pixels_per_column}");

    // if pixels_per_column * columns_data.len() > avail_width {
    //     panic!("pixels p. column too large");
    // }

    // Render line-by line.

    // Sanity check: every line has same column size.
    let mut bin_count_per_column = 0;
    for column in columns_data.iter() {
        if bin_count_per_column == 0 {
            bin_count_per_column = column.current_audio_colunn.len();
        }

        if bin_count_per_column != column.current_audio_colunn.len() {
            println!("WARN: Illegal spectrogram input 1");
            // let image = egui::ColorImage::new([width, height_outer], pixels);
            // ctx.load_texture("spectrogram", image, egui::TextureOptions::NEAREST)
            // return image;
        }
    }

    if bin_count_per_column == 0 {
        // println!("Illegal spectrogram input 2");
        let image = egui::ColorImage::new([width, height_outer], pixels);
        // ctx.load_texture("spectrogram", image, egui::TextureOptions::NEAREST)
        return image;
    }

    let bucket_height = height as f32 / bin_count_per_column as f32;

    println!("bucket_height: {bucket_height} | bin_count_per_column: {bin_count_per_column}");

    if bucket_height < 1.0 {
        panic!("too small");
    }

    let bucket_height = bucket_height.floor() as usize;

    let freq_min = 0;
    let freq_max = 20000;

    let freq_step = 1000;

    let freq_steps = (freq_max - freq_min) / freq_step;
    println!("Freq STEPS: {freq_steps}");
    let step_height = (height as f32 / freq_steps as f32).floor() as usize;

    {
        let mut is_black = true;

        for step in 0..freq_steps {
            let y_min = pad_top + (step * step_height);
            let y_max = y_min + step_height;

            let x_min = 0;
            let x_max = pad_left;

            let color = if is_black {
                Color32::LIGHT_GRAY
            } else {
                Color32::DARK_GRAY
            };

            for x in x_min..x_max {
                for y in y_min..y_max {
                    pixels[y * width + x] = color;
                }
            }

            is_black = !is_black;
        }
    }

    // TODO: change the data structure:
    let mut lines_by_freqs = vec![vec![]; freq_steps];

    // TODO: change implementation: very broken right now
    // Does not respect multiple lines per freq-line
    for freq_step_idx in 0..freq_steps {
        let bound_low = (freq_step_idx * freq_step) as u64;
        let bound_high = bound_low + freq_step as u64;

        for (_, column) in columns_data.iter().enumerate() {
            for (_, bucket) in column.current_audio_colunn.iter().enumerate() {
                if bucket.freq_bound_lower >= bound_low && bucket.freq_bound_upper <= bound_high {
                    lines_by_freqs[freq_step_idx].push(bucket);
                }
            }
        }
    }

    // Draw pass.
    for (row_index, freq_row) in lines_by_freqs.iter().enumerate() {
        let y_min_start_line = pad_top + (row_index * step_height);
        let y_max_end_line = y_min_start_line + step_height;

        for (column_index, bucket) in freq_row.iter().enumerate() {
            let x_start = pad_left + (column_index * pixels_per_column);
            let x_end = x_start + pixels_per_column;

            let bucket_color = spectrogram_color(bucket.volume);
            for y in y_min..y_max {
                for x in x_start..x_end {
                    pixels[y * width + x] = bucket_color;
                }
            }

            // for (column_index, column) in columns_data.iter().enumerate() {
            //     let bucket = column.current_audio_colunn[bin_index];
            //     if bucket.freq_bound_lower >= bound_low && bucket.freq_bound_upper <= bound_high {
            //         lines_by_freqs[freq_step_idx].push(bucket);
            //     }
            // }
        }
    }

    // for bin_index in 0..bin_count_per_column {
    //     let y_min = pad_top + (bin_index * bucket_height);
    //     let y_max = y_min + bucket_height;
    //     // pad_top + ((bucket_index + 1) as f32 * bucket_height).min(height as f32) as usize;
    //
    //     for (column_index, column) in columns_data.iter().enumerate() {
    //         let x_start = pad_left + (column_index * pixels_per_column);
    //         let x_end = x_start + pixels_per_column;
    //
    //         let bucket_color = spectrogram_color(column.current_audio_colunn[bin_index].volume);
    //         for y in y_min..y_max {
    //             for x in x_start..x_end {
    //                 pixels[y * width + x] = bucket_color;
    //             }
    //         }
    //     }
    // }

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
