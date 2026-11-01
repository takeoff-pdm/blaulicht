use crate::{AudioBucket, CollectorOutput};
use blaulicht_shared::CollectedAudioSnapshot;
use std::{collections::VecDeque, time::Duration};
use web_time::Instant;

/// Columns older than the window are kept this much longer, so a viewer that
/// trails the newest column (see the live audio page) still has data for its
/// left edge.
const WINDOW_SLACK_MS: u64 = 1000;

/// Rolling buffer of recent spectra for a live spectrogram.
#[derive(Clone)]
pub struct AudioSpectrogram {
    /// Most-recent-last columns; each column is `bin_count` tall with u8 intensities 0..=255.
    /// Contains bins. A bin is just a averaged part of the frequency space.
    ///
    /// Read freely, but mutate only through the methods so
    /// [`Self::column_times_ms`] stays in step.
    pub columns: VecDeque<CollectorOutput>,
    /// When each column was taken, in milliseconds on the producer's clock
    /// (see [`Self::epoch`]). Parallel to `columns` and never decreasing.
    pub column_times_ms: VecDeque<u64>,
    /// Maximum number of time columns to keep. With a [`Self::window`] set this
    /// is only a safety cap.
    pub max_columns: usize,
    /// Number of frequency bins per column.
    pub bin_count: usize,
    // Time to wait between ticks of the spectrogram.
    pub tick_period: Duration,
    /// Span of time the spectrogram shows. When set, columns are trimmed by
    /// age rather than by count, so a producer that falls behind its tick
    /// rate never stretches the window.
    pub window: Option<Duration>,
    /// The instant `column_times_ms == 0` refers to, when the columns are
    /// stamped live. Lets a viewer turn the stamps into wall-clock time.
    pub epoch: Option<Instant>,
}

impl AudioSpectrogram {
    pub fn new(max_columns: usize, bin_count: usize, tick_period: Duration) -> Self {
        Self {
            columns: VecDeque::with_capacity(max_columns),
            column_times_ms: VecDeque::with_capacity(max_columns),
            max_columns,
            bin_count,
            tick_period,
            window: None,
            epoch: None,
        }
    }

    /// Start a fresh live recording: drop every column and stamp new ones
    /// relative to `epoch`, keeping roughly `window` of history.
    pub fn restart_live(&mut self, epoch: Instant, window: Duration, max_columns: usize) {
        self.clear();
        self.epoch = Some(epoch);
        self.window = Some(window);
        self.max_columns = max_columns;
    }

    pub fn clear(&mut self) {
        self.columns.clear();
        self.column_times_ms.clear();
    }

    /// Push a column stamped one tick period after the previous one. For
    /// offline renderers that run on simulated time.
    pub fn push_data(&mut self, data: CollectorOutput) {
        let time_ms = self
            .column_times_ms
            .back()
            .map(|last| last + self.tick_period.as_millis() as u64)
            .unwrap_or(0);
        self.push_data_at(time_ms, data);
    }

    /// Push a column taken at `time_ms` (see [`Self::column_times_ms`]).
    pub fn push_data_at(&mut self, time_ms: u64, mut data: CollectorOutput) {
        if self.max_columns == 0 || self.bin_count == 0 {
            return;
        }
        // Ensure correct height; pad or truncate as needed.
        if data.current_audio_colunn.len() != self.bin_count {
            // This can happen due to rounding issues.
            data.current_audio_colunn
                .resize(self.bin_count, AudioBucket::default());
        }
        // Stamps must not go backwards: viewers binary-search them.
        let time_ms = time_ms.max(self.column_times_ms.back().copied().unwrap_or(0));

        while self.columns.len() >= self.max_columns {
            self.pop_front();
        }
        if let Some(window) = self.window {
            let keep_ms = window.as_millis() as u64 + WINDOW_SLACK_MS;
            let cutoff = time_ms.saturating_sub(keep_ms);
            while self.column_times_ms.front().is_some_and(|&t| t < cutoff) {
                self.pop_front();
            }
        }
        self.columns.push_back(data);
        self.column_times_ms.push_back(time_ms);
    }

    fn pop_front(&mut self) {
        self.columns.pop_front();
        self.column_times_ms.pop_front();
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

/// A plain row-major RGBA image (straight alpha, one `[r, g, b, a]` per pixel).
/// Every pixel the spectrogram draws is opaque. Frontends convert it into their
/// own texture type.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct SpectrogramImage {
    pub width: usize,
    pub height: usize,
    /// `width * height` pixels, row by row from the top.
    pub pixels: Vec<[u8; 4]>,
}

impl SpectrogramImage {
    pub const BLACK: [u8; 4] = [0, 0, 0, 255];
    const MAGENTA: [u8; 4] = [255, 0, 255, 255];
    const LIGHT_GRAY: [u8; 4] = [220, 220, 220, 255];
    const DARK_GRAY: [u8; 4] = [96, 96, 96, 255];

    /// A black image of the given size.
    pub fn new(width: usize, height: usize) -> Self {
        Self {
            width,
            height,
            pixels: vec![Self::BLACK; width.saturating_mul(height)],
        }
    }

    fn from_pixels(width: usize, height: usize, pixels: Vec<[u8; 4]>) -> Self {
        Self {
            width,
            height,
            pixels,
        }
    }

    pub fn size(&self) -> [usize; 2] {
        [self.width, self.height]
    }

    /// The pixels as packed RGB bytes, alpha dropped (for BMP dumps).
    pub fn rgb_bytes(&self) -> Vec<u8> {
        self.pixels
            .iter()
            .flat_map(|&[r, g, b, _]| [r, g, b])
            .collect()
    }

    /// Writes the image as a BMP file (debug dumps), so callers need no
    /// image crate of their own.
    pub fn save_bmp(&self, path: &std::path::Path) -> std::io::Result<()> {
        let buffer =
            image::RgbImage::from_raw(self.width as u32, self.height as u32, self.rgb_bytes())
                .ok_or_else(|| {
                    std::io::Error::other("spectrogram image has the wrong pixel count")
                })?;
        buffer
            .save_with_format(path, image::ImageFormat::Bmp)
            .map_err(std::io::Error::other)
    }
}

pub struct SpectrogramDisplayOptions {
    // pub include_bass_markers: bool,
    pub include_beat_markers: bool,
}

fn downsample(columns_data: &[CollectorOutput], pixels_per_column: f32) -> Vec<CollectorOutput> {
    if columns_data.is_empty() || !pixels_per_column.is_finite() || pixels_per_column <= 0.0 {
        return Vec::new();
    }
    // Downsample if we have more columns than pixels

    // Compressing: multiple columns per pixel - need to average
    let cols_per_pixel = (1.0 / pixels_per_column).ceil() as usize;

    let col: Vec<_> = columns_data
        .chunks(cols_per_pixel)
        .map(|chunks| {
            if chunks.is_empty() {
                return CollectorOutput::default();
            }
            // Columns are public and can be supplied by integrations, so do
            // not assume every column has the first column's bin count.
            let bucket_count = chunks
                .iter()
                .map(|column| column.current_audio_colunn.len())
                .min()
                .unwrap_or(0);
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

/// Height in pixels of the onset marker drawn in the bottom padding.
const ONSET_MARKER_HEIGHT: usize = 4;

/// Mix the beat marker colour over a spectrogram pixel. Cyan stays distinct
/// from the warm colour map without dominating it.
fn blend_beat_marker(under: [u8; 4]) -> [u8; 4] {
    const ALPHA: u32 = 80;
    let mix = |u: u8, m: u8| ((u as u32 * (255 - ALPHA) + m as u32 * ALPHA) / 255) as u8;
    let [r, g, b, _] = under;
    [mix(r, 90), mix(g, 210), mix(b, 255), 255]
}

/// Pixel rows `[y_min, y_max)` covered by `bin_index` when `bin_count` bins
/// share `height` rows below `pad_top`. Bin 0 is the lowest frequency and is
/// drawn at the bottom; rows are distributed proportionally so the bins fill
/// the whole area instead of leaving a remainder strip when
/// `height % bin_count != 0`.
pub fn bin_rows(
    pad_top: usize,
    height: usize,
    bin_count: usize,
    bin_index: usize,
) -> (usize, usize) {
    let from_top = bin_count - bin_index;
    let y_max = pad_top + (from_top * height) / bin_count;
    let y_min = pad_top + ((from_top - 1) * height) / bin_count;
    (y_min, y_max)
}

#[allow(clippy::too_many_arguments)]
fn draw_column(
    image_buffer: &mut SpectrogramImage,
    width: usize,
    height_outer: usize,
    height: usize,
    pad_top: usize,
    pad_btm: usize,
    bin_count: usize,
    x_start: usize,
    x_end: usize,
    column: &CollectorOutput,
    options: &SpectrogramDisplayOptions,
) {
    for bin_index in 0..bin_count {
        let (y_min, y_max) = bin_rows(pad_top, height, bin_count, bin_index);

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
    image_buffer: &mut SpectrogramImage,
    width: usize,
    height_outer: usize,
    height: usize,
    pad_top: usize,
    pad_btm: usize,
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
        let (y_min, y_max) = bin_rows(pad_top, height, bin_count, bin_index);

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

/// Overlay beat and onset markers on the pixel columns `x_start..x_end`.
///
/// A beat is a translucent cyan line blended over the spectrum so the data
/// underneath stays readable; with many collector columns per pixel a beat
/// marks a whole pixel column, and opaque lines would hide a large share of
/// the image at dense tempos. An onset is a short magenta bar in the bottom
/// padding.
#[allow(clippy::too_many_arguments)]
pub fn draw_markers(
    image_buffer: &mut SpectrogramImage,
    width: usize,
    height_outer: usize,
    height: usize,
    pad_btm: usize,
    x_start: usize,
    x_end: usize,
    beat_trigger: bool,
    actual_onset_peak: bool,
) {
    if x_start >= width || x_start >= x_end {
        return;
    }

    if beat_trigger {
        for y in 0..height_outer {
            let pixel = &mut image_buffer.pixels[y * width + x_start];
            *pixel = blend_beat_marker(*pixel);
        }
    }

    if actual_onset_peak {
        let y_end = (height + (pad_btm / 2)).min(height_outer);
        let y_start = y_end.saturating_sub(ONSET_MARKER_HEIGHT);
        for y in y_start..y_end {
            for x in x_start..x_end {
                image_buffer.pixels[y * width + x] = SpectrogramImage::MAGENTA;
            }
        }
    }
}

pub fn create_spectrogram_image(
    spec: &AudioSpectrogram,
    width: usize,
    height_outer: usize,
    options: &SpectrogramDisplayOptions,
    image_buffer: &mut SpectrogramImage,
) {
    if image_buffer.size() != [width, height_outer] {
        log::debug!("Spectrogram image re-allocated");
        *image_buffer = SpectrogramImage::new(width, height_outer);
    }

    image_buffer.pixels.fill(SpectrogramImage::BLACK);

    let pad_btm = 20;
    let pad_top = 5;
    let pad_left = 5;

    let Some(height) = height_outer.checked_sub(pad_btm + pad_top) else {
        return;
    };
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

    if height < bin_count_per_column {
        return;
    }

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
}

// Create texture once when data changes (not every frame!)
pub fn create_spectrogram_image_with_freqs(
    spec: &mut AudioSpectrogram,
    width: usize,
    height_outer: usize,
    _options: &SpectrogramDisplayOptions,
) -> SpectrogramImage {
    let pad_btm = 20;
    let pad_top = 5;
    let pad_left = 5;

    let Some(height) = height_outer.checked_sub(pad_btm + pad_top) else {
        return SpectrogramImage::new(width, height_outer);
    };
    let mut pixels = vec![SpectrogramImage::BLACK; width.saturating_mul(height_outer)];

    let total_cols = spec.max_columns;

    let Some(avail_width) = width.checked_sub(pad_left) else {
        return SpectrogramImage::from_pixels(width, height_outer, pixels);
    };
    if height == 0 || avail_width == 0 || total_cols == 0 || spec.columns.is_empty() {
        return SpectrogramImage::from_pixels(width, height_outer, pixels);
    }
    let pixels_per_column = avail_width as f32 / total_cols as f32;

    let chunks_cont = spec.columns.make_contiguous();
    let columns_data = downsample(chunks_cont, pixels_per_column);

    if columns_data.is_empty() {
        return SpectrogramImage::from_pixels(width, height_outer, pixels);
    }
    let pixels_per_column = (avail_width as f32 / columns_data.len() as f32).floor() as usize;

    // println!("pixels per column: {pixels_per_column}");

    // if pixels_per_column * columns_data.len() > avail_width {
    //     panic!("pixels p. column too large");
    // }

    let mut iter = columns_data.iter().peekable();
    let bin_count_per_column = if let Some(first) = iter.peek() {
        first.current_audio_colunn.len()
    } else {
        return SpectrogramImage::from_pixels(width, height_outer, pixels);
    };

    for column in &mut iter {
        if column.current_audio_colunn.len() != bin_count_per_column {
            return SpectrogramImage::from_pixels(width, height_outer, pixels);
        }
    }

    let bucket_height = height as f32 / bin_count_per_column as f32;

    if bucket_height < 1.0 {
        return SpectrogramImage::from_pixels(width, height_outer, pixels);
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
                SpectrogramImage::LIGHT_GRAY
            } else {
                SpectrogramImage::DARK_GRAY
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

    SpectrogramImage::from_pixels(width, height_outer, pixels)
}

/// Inferno colour map stops (matplotlib), dark to bright.
const INFERNO: [(u8, u8, u8); 11] = [
    (0, 0, 4),
    (22, 11, 57),
    (66, 10, 104),
    (106, 23, 110),
    (147, 38, 103),
    (188, 55, 84),
    (221, 81, 58),
    (243, 120, 25),
    (252, 165, 10),
    (246, 215, 70),
    (252, 255, 164),
];

/// Maps a bin intensity onto the inferno colour map. The mild gamma lifts
/// quiet detail without washing the noise floor into a saturated haze.
pub fn spectrogram_color(intensity: u8) -> [u8; 4] {
    let t = (intensity as f32 / 255.0).powf(0.7) * (INFERNO.len() - 1) as f32;
    let index = (t as usize).min(INFERNO.len() - 2);
    let frac = t - index as f32;
    let (a, b) = (INFERNO[index], INFERNO[index + 1]);
    let lerp = |a: u8, b: u8| (a as f32 + (b as f32 - a as f32) * frac).round() as u8;
    [lerp(a.0, b.0), lerp(a.1, b.1), lerp(a.2, b.2), 255]
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
    fn push_data_stamps_one_tick_period_apart() {
        let spec = spectrogram_with_columns(10, 3);
        assert_eq!(spec.column_times_ms, [0, 20, 40]);
    }

    #[test]
    fn push_data_still_trims_by_count_without_a_window() {
        let spec = spectrogram_with_columns(4, 9);
        assert_eq!(spec.columns.len(), 4);
        assert_eq!(spec.column_times_ms, [100, 120, 140, 160]);
    }

    #[test]
    fn live_window_trims_by_age_not_count() {
        let mut spec = AudioSpectrogram::new(40, 2, Duration::from_millis(20));
        spec.restart_live(Instant::now(), Duration::from_secs(2), 10_000);
        // A slow producer: one column every 100 ms for 10 s.
        for i in 0..=100u64 {
            spec.push_data_at(i * 100, CollectorOutput::default());
        }
        let newest = *spec.column_times_ms.back().unwrap();
        let oldest = *spec.column_times_ms.front().unwrap();
        assert_eq!(newest, 10_000);
        // Window plus slack, regardless of how many columns that is.
        assert_eq!(newest - oldest, 2_000 + WINDOW_SLACK_MS);
        assert_eq!(spec.columns.len(), spec.column_times_ms.len());
    }

    #[test]
    fn stamps_never_go_backwards() {
        let mut spec = AudioSpectrogram::new(40, 2, Duration::from_millis(20));
        spec.push_data_at(500, CollectorOutput::default());
        spec.push_data_at(400, CollectorOutput::default());
        assert_eq!(spec.column_times_ms, [500, 500]);
    }

    #[test]
    fn restart_live_drops_old_columns() {
        let mut spec = spectrogram_with_columns(10, 5);
        spec.restart_live(Instant::now(), Duration::from_secs(1), 100);
        assert!(spec.columns.is_empty() && spec.column_times_ms.is_empty());
        assert_eq!(spec.max_columns, 100);
    }

    #[test]
    fn beat_marker_keeps_the_spectrum_visible() {
        let over_black = blend_beat_marker(SpectrogramImage::BLACK);
        let over_orange = blend_beat_marker([243, 120, 25, 255]);
        assert!(over_black[2] > over_black[0] && over_black[2] < 128);
        // The colour underneath still shows through.
        assert!(over_orange[0] > over_black[0] + 100);
        assert!(over_orange[1] > over_black[1]);
    }

    #[test]
    fn spectrogram_color_spans_the_colour_map() {
        assert_eq!(spectrogram_color(0), [0, 0, 4, 255]);
        assert_eq!(spectrogram_color(255), [252, 255, 164, 255]);
        let luminance = |[r, g, b, _]: [u8; 4]| r as u32 * 3 + g as u32 * 6 + b as u32;
        for i in 0..255u8 {
            // Allow for per-channel rounding.
            assert!(luminance(spectrogram_color(i)) <= luminance(spectrogram_color(i + 1)) + 4);
        }
    }

    #[test]
    fn bin_rows_fill_the_whole_area_without_gaps_or_overlap() {
        let (pad_top, height, bins) = (5, 135, 128);
        let mut covered = 0;
        let mut expected_next = pad_top + height;
        for bin_index in 0..bins {
            let (y_min, y_max) = bin_rows(pad_top, height, bins, bin_index);
            assert_eq!(y_max, expected_next, "bin {bin_index} leaves a gap");
            assert!(y_max > y_min, "bin {bin_index} is empty");
            covered += y_max - y_min;
            expected_next = y_min;
        }
        assert_eq!(expected_next, pad_top);
        assert_eq!(covered, height);
    }

    #[test]
    fn spectrogram_image_handles_wide_single_onset_column() {
        let spec = spectrogram_with_onset_column();
        let mut image = SpectrogramImage::new(1, 1);

        create_spectrogram_image(
            &spec,
            723,
            160,
            &SpectrogramDisplayOptions {
                include_beat_markers: true,
            },
            &mut image,
        );

        assert_eq!(image.size(), [723, 160]);
    }

    #[test]
    fn spectrogram_image_handles_tiny_allocations() {
        let spec = spectrogram_with_onset_column();

        for (width, height) in [(0, 0), (4, 160), (10, 24), (10, 25)] {
            let mut image = SpectrogramImage::new(1, 1);

            create_spectrogram_image(
                &spec,
                width,
                height,
                &SpectrogramDisplayOptions {
                    include_beat_markers: true,
                },
                &mut image,
            );

            assert_eq!(image.size(), [width, height]);
        }
    }

    #[test]
    fn spectrogram_image_spreads_downsampled_columns_to_right_edge() {
        let spec = spectrogram_with_columns(1000, 900);
        let mut image = SpectrogramImage::new(1, 1);

        create_spectrogram_image(
            &spec,
            723,
            160,
            &SpectrogramDisplayOptions {
                include_beat_markers: false,
            },
            &mut image,
        );

        let right_edge_data_pixel = image.pixels[10 * image.width + image.width - 1];
        assert_ne!(right_edge_data_pixel, SpectrogramImage::BLACK);
    }

    #[test]
    fn spectrogram_image_right_aligns_partial_window() {
        let spec = spectrogram_with_columns(10, 1);
        let mut image = SpectrogramImage::new(1, 1);

        create_spectrogram_image(
            &spec,
            25,
            60,
            &SpectrogramDisplayOptions {
                include_beat_markers: false,
            },
            &mut image,
        );

        let left_data_pixel = image.pixels[10 * image.width + 5];
        let right_edge_data_pixel = image.pixels[10 * image.width + image.width - 1];
        assert_eq!(left_data_pixel, SpectrogramImage::BLACK);
        assert_ne!(right_edge_data_pixel, SpectrogramImage::BLACK);
    }
}
