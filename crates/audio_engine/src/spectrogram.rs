use crate::{AudioBucket, CollectorOutput, SignalDebugData};
use blaulicht_shared::CollectedAudioSnapshot;
use egui::{Color32, ColorImage, IntoAtoms};
use std::{collections::VecDeque, time::Duration};

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
            while self.columns.len() >= self.max_columns {
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

    pub fn current_output(&self) -> CollectorOutput {
        self.columns
            .iter()
            .last()
            .unwrap_or(&CollectorOutput::default())
            .clone()
    }
}

pub struct SpectrogramDisplayOptions {
    // pub include_bass_markers: bool,
    pub include_beat_markers: bool,
}

fn downsample(columns_data: &[CollectorOutput], pixels_per_column: f32) -> Vec<CollectorOutput> {
    // Downsample if we have more columns than pixels

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
                snapshot: chunks[0].snapshot.clone(),
                debug_data: chunks[0].debug_data.clone(),
            };

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

            averaged
        })
        .collect();

    col
}

#[allow(clippy::too_many_arguments)]
fn draw_column(
    image_buffer: &mut ColorImage,
    width: usize,
    height_outer: usize,
    height: usize,
    pad_top: usize,
    pad_btm: usize,
    bucket_height: usize,
    bin_count: usize,
    x_start: usize,
    x_end: usize,
    column: &CollectorOutput,
    options: &SpectrogramDisplayOptions,
) {
    for bin_index in 0..bin_count {
        let y_max = pad_top + ((bin_count - bin_index) * bucket_height);
        let y_min = y_max - bucket_height;

        let bucket_color = spectrogram_color(column.current_audio_colunn[bin_index].volume);
        for y in y_min..y_max {
            let row_start = y * width + x_start;
            let row_end = row_start + (x_end - x_start);
            image_buffer.pixels[row_start..row_end].fill(bucket_color);
        }
    }

    if options.include_beat_markers {
        draw_markers(
            image_buffer,
            width,
            height_outer,
            height,
            pad_btm,
            x_start,
            x_end,
            column.snapshot.beat_trigger,
            column.snapshot.actual_onset_peak,
        );
    }
}

#[allow(clippy::too_many_arguments)]
fn draw_averaged_columns(
    image_buffer: &mut ColorImage,
    width: usize,
    height_outer: usize,
    height: usize,
    pad_top: usize,
    pad_btm: usize,
    bucket_height: usize,
    bin_count: usize,
    x_start: usize,
    x_end: usize,
    columns: &std::collections::VecDeque<CollectorOutput>,
    column_start: usize,
    column_end: usize,
    options: &SpectrogramDisplayOptions,
) {
    let column_count = column_end - column_start;

    for bin_index in 0..bin_count {
        let y_max = pad_top + ((bin_count - bin_index) * bucket_height);
        let y_min = y_max - bucket_height;

        let volume_sum: u32 = (column_start..column_end)
            .map(|column_index| columns[column_index].current_audio_colunn[bin_index].volume as u32)
            .sum();
        let bucket_color = spectrogram_color((volume_sum / column_count as u32) as u8);

        for y in y_min..y_max {
            let row_start = y * width + x_start;
            let row_end = row_start + (x_end - x_start);
            image_buffer.pixels[row_start..row_end].fill(bucket_color);
        }
    }

    if options.include_beat_markers {
        let beat_trigger = (column_start..column_end)
            .any(|column_index| columns[column_index].snapshot.beat_trigger);
        let actual_onset_peak = (column_start..column_end)
            .any(|column_index| columns[column_index].snapshot.actual_onset_peak);
        draw_markers(
            image_buffer,
            width,
            height_outer,
            height,
            pad_btm,
            x_start,
            x_end,
            beat_trigger,
            actual_onset_peak,
        );
    }
}

#[allow(clippy::too_many_arguments)]
fn draw_markers(
    image_buffer: &mut ColorImage,
    width: usize,
    height_outer: usize,
    height: usize,
    pad_btm: usize,
    x_start: usize,
    x_end: usize,
    beat_trigger: bool,
    actual_onset_peak: bool,
) {
    if x_start >= width {
        return;
    }

    if beat_trigger {
        for y in 0..height_outer {
            image_buffer.pixels[y * width + x_start] = Color32::RED;
        }
    }

    if actual_onset_peak {
        let dot_size = x_end - x_start;
        let y_end = (height + (pad_btm / 2)).min(height_outer);
        let y_start = y_end.saturating_sub(dot_size);
        for y in y_start..y_end {
            image_buffer.pixels[y * width + x_start] = Color32::MAGENTA;
        }
    }
}

pub fn create_spectrogram_image(
    spec: &AudioSpectrogram,
    width: usize,
    height_outer: usize,
    options: &SpectrogramDisplayOptions,
    image_buffer: &mut ColorImage,
) {
    // let image = egui::ColorImage::new([width, height_outer], pixels);

    if image_buffer.size != [width, height_outer] {
        println!("WARN: called image re-alloc");
        let pixels = vec![Color32::BLACK; width * height_outer];
        *image_buffer = egui::ColorImage::new([width, height_outer], pixels);
    }

    image_buffer
        .pixels
        .iter_mut()
        .for_each(|p| *p = Color32::BLACK);

    let pad_btm = 20;
    let pad_top = 5;
    let pad_left = 5;

    let Some(height) = height_outer.checked_sub(pad_btm + pad_top) else {
        return;
    };
    // let mut pixels = vec![Color32::BLACK; width * height_outer];

    let total_cols = spec.max_columns;

    let Some(avail_width) = width.checked_sub(pad_left) else {
        return;
    };

    if height == 0 || avail_width == 0 || total_cols == 0 {
        return;
    }

    if spec.columns.is_empty() {
        return;
    }

    let first_column_index = spec.columns.len().saturating_sub(total_cols);
    let visible_column_count = spec.columns.len() - first_column_index;
    let data_offset = total_cols - visible_column_count;

    let mut iter = spec.columns.iter().skip(first_column_index).peekable();
    let bin_count_per_column = if let Some(first) = iter.peek() {
        first.current_audio_colunn.len()
    } else {
        return;
    };

    if bin_count_per_column == 0 {
        return;
    }

    for column in &mut iter {
        if column.current_audio_colunn.len() != bin_count_per_column {
            return;
        }
    }

    let bucket_height = height as f32 / bin_count_per_column as f32;

    if bucket_height < 1.0 {
        return;
    }

    let bucket_height = bucket_height.floor() as usize;
    let width_usize = width;
    let bin_count_per_column_usize = bin_count_per_column;

    if total_cols <= avail_width {
        for visible_index in 0..visible_column_count {
            let logical_index = data_offset + visible_index;
            let x_start = pad_left + (logical_index * avail_width) / total_cols;
            let x_end =
                (pad_left + ((logical_index + 1) * avail_width) / total_cols).min(width_usize);

            if x_start >= x_end {
                continue;
            }

            draw_column(
                image_buffer,
                width_usize,
                height_outer,
                height,
                pad_top,
                pad_btm,
                bucket_height,
                bin_count_per_column_usize,
                x_start,
                x_end,
                &spec.columns[first_column_index + visible_index],
                options,
            );
        }
    } else {
        let data_logical_start = data_offset;
        let data_logical_end = data_offset + visible_column_count;

        for pixel_index in 0..avail_width {
            let logical_start = (pixel_index * total_cols) / avail_width;
            let logical_end = ((pixel_index + 1) * total_cols) / avail_width;

            let range_start = logical_start.max(data_logical_start);
            let range_end = logical_end.min(data_logical_end);
            if range_start >= range_end {
                continue;
            }

            let x_start = pad_left + pixel_index;
            let x_end = (x_start + 1).min(width_usize);
            if x_start >= x_end {
                continue;
            }

            let column_start = first_column_index + (range_start - data_offset);
            let column_end = first_column_index + (range_end - data_offset);
            draw_averaged_columns(
                image_buffer,
                width_usize,
                height_outer,
                height,
                pad_top,
                pad_btm,
                bucket_height,
                bin_count_per_column_usize,
                x_start,
                x_end,
                &spec.columns,
                column_start,
                column_end,
                options,
            );
        }
    }

    // let image = egui::ColorImage::new([width, height_outer], pixels);
    // // ctx.load_texture("spectrogram", image, egui::TextureOptions::NEAREST)
    // image
}

// Create texture once when data changes (not every frame!)
pub fn create_spectrogram_image_with_freqs(
    spec: &mut AudioSpectrogram,
    width: usize,
    height_outer: usize,
    _options: &SpectrogramDisplayOptions,
) -> egui::ColorImage {
    let pad_btm = 20;
    let pad_top = 5;
    let pad_left = 5;

    let height = height_outer - pad_btm - pad_top;
    let mut pixels = vec![Color32::BLACK; width * height_outer];

    let total_cols = spec.max_columns;

    let avail_width = width - pad_left;
    let pixels_per_column = avail_width as f32 / total_cols as f32;

    let chunks_cont = spec.columns.make_contiguous();
    let columns_data = downsample(chunks_cont, pixels_per_column);

    let pixels_per_column = (avail_width as f32 / columns_data.len() as f32).floor() as usize;

    // println!("pixels per column: {pixels_per_column}");

    // if pixels_per_column * columns_data.len() > avail_width {
    //     panic!("pixels p. column too large");
    // }

    let mut iter = columns_data.iter().peekable();
    let bin_count_per_column = if let Some(first) = iter.peek() {
        first.current_audio_colunn.len()
    } else {
        let image = egui::ColorImage::new([width, height_outer], pixels);
        return image;
    };

    for column in &mut iter {
        if column.current_audio_colunn.len() != bin_count_per_column {
            let image = egui::ColorImage::new([width, height_outer], pixels);
            return image;
        }
    }

    let bucket_height = height as f32 / bin_count_per_column as f32;

    if bucket_height < 1.0 {
        panic!("too small");
    }

    let _bucket_height = bucket_height.floor() as usize;

    let freq_min = 0;
    let freq_max = 20000;

    let freq_step = 1000;

    let freq_steps = (freq_max - freq_min) / freq_step;

    // Lines for 1khz columns.
    let mut lines_by_freqs = vec![
        // List of inner columns for freqs that match this frequency range.
        vec![];
        freq_steps
    ];

    let mut subcolumn_height = 0;

    let step_height_max = (height as f32 / freq_steps as f32).ceil() as usize;

    // TODO: change implementation: very broken right now
    // Does not respect multiple lines per freq-line
    for freq_step_idx in 0..freq_steps {
        let bound_high = ((freq_steps - freq_step_idx) * freq_step) as u64;
        let bound_low = bound_high - freq_step as u64;

        for column in columns_data.iter() {
            // NOTE: assume even distribution of frequencies.
            // let buckets_per_subline = column.current_audio_colunn.len()  / freq_steps;

            let mut sub_column = vec![];
            for bucket in column.current_audio_colunn.iter() {
                if bucket.freq_bound_lower >= bound_low && bucket.freq_bound_upper <= bound_high {
                    // decide whether to create a new subline bucket
                    if sub_column.len() < step_height_max {
                        sub_column.push(bucket.clone());
                    } else {
                        // println!("Warn: subbucket overflow: freq = ({bound_low} -- {bound_high})");
                    }
                }

                // TODO: actually sort this in?
            }

            if sub_column.len() > 0 {
                if subcolumn_height == 0 {
                    subcolumn_height = sub_column.len();
                }

                // if sub_column.len() < subcolumn_height {
                //     kVk!("WARN: sub column mismatch: expected {} got {}", sub_column.len(), subcolumn_height);
                // }

                if sub_column.len() > subcolumn_height {
                    subcolumn_height = sub_column.len();
                }
            }

            lines_by_freqs[freq_step_idx].push(sub_column);
        }
    }

    // println!("Freq STEPS: {freq_steps}");
    let mut step_height = (height as f32 / freq_steps as f32).ceil() as usize;

    if step_height > subcolumn_height {
        step_height = subcolumn_height;
        // println!("WARN: subcolumn_height != step_height => {step_height} vs {step_height_max}");
    }

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

    // Draw pass.
    for (row_index, freq_row) in lines_by_freqs.iter().enumerate() {
        let y_min_start_line = pad_top + (row_index * step_height);
        let y_max_end_line = y_min_start_line + step_height;

        for (column_index, sub_column) in freq_row.iter().enumerate() {
            let x_start = pad_left + (column_index * pixels_per_column);
            let x_end = x_start + pixels_per_column;

            let sub_column_elements = sub_column.len();

            if sub_column.len() > 0 {
                let subcolumn_height = y_max_end_line - y_min_start_line;
                let subcolumn_bucket_height = subcolumn_height / sub_column_elements;
                // println!("sub: {} total_col_height: {bin_count_per_column}: sub height: {subcolumn_height} sub bucket height: {subcolumn_bucket_height}", sub_column.len() );

                for (bucket_idx, bucket) in sub_column.iter().enumerate() {
                    let bucket_color = spectrogram_color(bucket.volume);
                    let y_min = y_min_start_line + (bucket_idx * subcolumn_bucket_height);
                    let y_max = y_min + subcolumn_bucket_height;

                    for y in y_min..y_max {
                        for x in x_start..x_end {
                            pixels[y * width + x] = bucket_color;
                        }
                    }
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::AudioBucket;

    fn spectrogram_with_onset_column() -> AudioSpectrogram {
        let mut spec = AudioSpectrogram::new(128, 2, Duration::from_millis(20));
        let mut output = CollectorOutput::default();
        output.snapshot.actual_onset_peak = true;
        output.current_audio_colunn = vec![
            AudioBucket {
                volume: 64,
                ..AudioBucket::default()
            };
            2
        ];
        spec.push_data(output);
        spec
    }

    fn spectrogram_with_columns(max_columns: usize, columns: usize) -> AudioSpectrogram {
        let mut spec = AudioSpectrogram::new(max_columns, 2, Duration::from_millis(20));
        for _ in 0..columns {
            let mut output = CollectorOutput::default();
            output.current_audio_colunn = vec![
                AudioBucket {
                    volume: 64,
                    ..AudioBucket::default()
                };
                2
            ];
            spec.push_data(output);
        }
        spec
    }

    #[test]
    fn spectrogram_image_handles_wide_single_onset_column() {
        let spec = spectrogram_with_onset_column();
        let mut image = ColorImage::new([1, 1], vec![Color32::BLACK]);

        create_spectrogram_image(
            &spec,
            723,
            160,
            &SpectrogramDisplayOptions {
                include_beat_markers: true,
            },
            &mut image,
        );

        assert_eq!(image.size, [723, 160]);
    }

    #[test]
    fn spectrogram_image_handles_tiny_allocations() {
        let spec = spectrogram_with_onset_column();

        for (width, height) in [(0, 0), (4, 160), (10, 24), (10, 25)] {
            let mut image = ColorImage::new([1, 1], vec![Color32::BLACK]);

            create_spectrogram_image(
                &spec,
                width,
                height,
                &SpectrogramDisplayOptions {
                    include_beat_markers: true,
                },
                &mut image,
            );

            assert_eq!(image.size, [width, height]);
        }
    }

    #[test]
    fn spectrogram_image_spreads_downsampled_columns_to_right_edge() {
        let spec = spectrogram_with_columns(1000, 900);
        let mut image = ColorImage::new([1, 1], vec![Color32::BLACK]);

        create_spectrogram_image(
            &spec,
            723,
            160,
            &SpectrogramDisplayOptions {
                include_beat_markers: false,
            },
            &mut image,
        );

        let right_edge_data_pixel = image.pixels[10 * image.width() + image.width() - 1];
        assert_ne!(right_edge_data_pixel, Color32::BLACK);
    }

    #[test]
    fn spectrogram_image_right_aligns_partial_window() {
        let spec = spectrogram_with_columns(10, 1);
        let mut image = ColorImage::new([1, 1], vec![Color32::BLACK]);

        create_spectrogram_image(
            &spec,
            25,
            60,
            &SpectrogramDisplayOptions {
                include_beat_markers: false,
            },
            &mut image,
        );

        let left_data_pixel = image.pixels[10 * image.width() + 5];
        let right_edge_data_pixel = image.pixels[10 * image.width() + image.width() - 1];
        assert_eq!(left_data_pixel, Color32::BLACK);
        assert_ne!(right_edge_data_pixel, Color32::BLACK);
    }
}
