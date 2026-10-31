use crate::app::components::{ButtonSize, HFader};
use crate::app::{components, BlaulichtApp};
use blaulicht_audio_engine::{
    bin_rows, create_spectrogram_image, draw_markers, spectrogram_color, AudioSpectrogram,
    BeatDetection, BeatFrontEnd, BpmDetectStatus, CollectorOutput, SignalDebugData,
    SpectrogramDisplayOptions, SpectrogramImage,
};
use blaulicht_engine::client::EngineClient;
use blaulicht_engine::hub::{AudioParam, HostCommand};
use blaulicht_engine::msg::DummyInput;
use blaulicht_shared::AudioSourceStatus;

use egui::{vec2, Color32, FontId, Frame, Margin, RichText, TextureHandle, Widget};
use std::cell::Cell;

// TODO: include snapshot in graphs

const SPECTROGRAM_PAD_BOTTOM: usize = 20;
const SPECTROGRAM_PAD_TOP: usize = 5;
const SPECTROGRAM_PAD_LEFT: usize = 5;

const INFO_HEADER_COLOR: Color32 = Color32::from_rgb(120, 170, 255);
const INFO_OK_COLOR: Color32 = Color32::from_rgb(120, 220, 140);
const INFO_WARN_COLOR: Color32 = Color32::from_rgb(240, 200, 100);
const INFO_BAD_COLOR: Color32 = Color32::from_rgb(240, 130, 130);

fn info_header(ui: &mut egui::Ui, text: &str) {
    ui.label(RichText::new(text).strong().color(INFO_HEADER_COLOR));
}

fn info_hint(ui: &mut egui::Ui, text: &str) {
    ui.label(RichText::new(text).small().weak());
}

/// One compact line of `key value` pairs, keys dimmed, values in the normal
/// text color; wraps when the column is narrow.
fn info_line(ui: &mut egui::Ui, pairs: &[(&str, String)]) {
    let font = egui::TextStyle::Small.resolve(ui.style());
    let weak = ui.visuals().weak_text_color();
    let strong = ui.visuals().text_color();
    let mut job = egui::text::LayoutJob::default();
    for (i, (key, value)) in pairs.iter().enumerate() {
        let format = |color| egui::TextFormat {
            font_id: font.clone(),
            color,
            ..Default::default()
        };
        let sep = if i == 0 { "" } else { "   " };
        job.append(&format!("{sep}{key} "), 0.0, format(weak));
        job.append(value, 0.0, format(strong));
    }
    ui.label(job);
}

/// `key: VALUE [note]` with the value colored by its health.
fn info_status(ui: &mut egui::Ui, key: &str, value: &str, color: Color32, note: Option<&str>) {
    ui.horizontal_wrapped(|ui| {
        ui.label(RichText::new(format!("{key}:")).small().weak());
        ui.label(RichText::new(value).strong().color(color));
        if let Some(note) = note {
            ui.label(RichText::new(note).small().color(color));
        }
    });
}

// SPECPERF-TEMP begin
mod specperf {
    use std::sync::Mutex;
    use std::time::Instant;

    pub struct Stats {
        pub window_start: Option<Instant>,
        pub last_frame: Option<Instant>,
        pub frames: u32,
        pub frame_gap_max_ms: f32,
        pub cols: u32,
        pub last_col_time: u64,
        pub col_dt_min: u64,
        pub col_dt_max: u64,
        pub col_dt_over_30: u32,
        pub frames_0_cols: u32,
        pub frames_2plus_cols: u32,
        pub max_cols_per_frame: u32,
        pub shifts: u32,
        pub px: u32,
        pub last_shift: Option<Instant>,
        pub shift_gap_max_ms: f32,
        pub uploads: u32,
        pub upload_us_total: u128,
        pub upload_us_max: u128,
        pub update_us_max: u128,
    }

    pub static STATS: Mutex<Stats> = Mutex::new(Stats {
        window_start: None,
        last_frame: None,
        frames: 0,
        frame_gap_max_ms: 0.0,
        cols: 0,
        last_col_time: 0,
        col_dt_min: u64::MAX,
        col_dt_max: 0,
        col_dt_over_30: 0,
        frames_0_cols: 0,
        frames_2plus_cols: 0,
        max_cols_per_frame: 0,
        shifts: 0,
        px: 0,
        last_shift: None,
        shift_gap_max_ms: 0.0,
        uploads: 0,
        upload_us_total: 0,
        upload_us_max: 0,
        update_us_max: 0,
    });
}
// SPECPERF-TEMP end

/// The Audio page's spectrogram texture, scrolled smoothly by appending the
/// columns the engine added since the last frame.
pub struct SpectrogramRing {
    spectro_scroll_px_offset: f32,
    /// Per-bin volume sums of the columns that have not yet advanced the
    /// ring by a whole pixel; averaged into the next drawn pixel column.
    spectro_pending_sum: Vec<u32>,
    spectro_pending_count: u32,
    spectro_pending_beat: bool,
    spectro_pending_onset: bool,
    spectrogram_image_buffer: SpectrogramImage,
    spectrogram_texture_handle: Option<TextureHandle>,
    last_spectrogram_columns: Cell<usize>,
    last_spectrogram_bucket_data_len: Cell<usize>,
    last_spectrogram_max_columns: Cell<usize>,
    last_spectrogram_snapshot_time: Cell<u64>,
}

impl Default for SpectrogramRing {
    fn default() -> Self {
        Self {
            spectro_scroll_px_offset: 0.0,
            spectro_pending_sum: Vec::new(),
            spectro_pending_count: 0,
            spectro_pending_beat: false,
            spectro_pending_onset: false,
            spectrogram_image_buffer: SpectrogramImage::default(),
            spectrogram_texture_handle: None,
            last_spectrogram_columns: Cell::new(0),
            last_spectrogram_bucket_data_len: Cell::new(0),
            last_spectrogram_max_columns: Cell::new(0),
            last_spectrogram_snapshot_time: Cell::new(0),
        }
    }
}

impl SpectrogramRing {
    fn reset_spectrogram_ring_cache(
        &mut self,
        spec: &AudioSpectrogram,
        width: usize,
        height: usize,
    ) {
        create_spectrogram_image(
            spec,
            width,
            height,
            &SpectrogramDisplayOptions {
                include_beat_markers: true,
            },
            &mut self.spectrogram_image_buffer,
        );

        self.spectro_scroll_px_offset = 0.0;
        self.clear_spectrogram_pending();
        self.last_spectrogram_columns.set(spec.columns.len());
        self.last_spectrogram_max_columns.set(spec.max_columns);
        self.last_spectrogram_bucket_data_len.set(
            spec.columns
                .back()
                .map(|c| c.current_audio_colunn.len())
                .unwrap_or(0),
        );
        self.last_spectrogram_snapshot_time
            .set(spec.columns.back().map(|c| c.snapshot.time).unwrap_or(0));
    }

    fn spectrogram_ring_needs_reset(
        &self,
        spec: &AudioSpectrogram,
        width: usize,
        height: usize,
    ) -> bool {
        let current_bucket_count = spec
            .columns
            .back()
            .map(|c| c.current_audio_colunn.len())
            .unwrap_or(0);
        let current_time = spec.columns.back().map(|c| c.snapshot.time).unwrap_or(0);

        self.spectrogram_image_buffer.size() != [width, height]
            || self.last_spectrogram_max_columns.get() != spec.max_columns
            || self.last_spectrogram_bucket_data_len.get() != current_bucket_count
            || spec.columns.len() < self.last_spectrogram_columns.get()
            || current_time < self.last_spectrogram_snapshot_time.get()
    }

    fn shift_spectrogram_ring_left(&mut self, pixels: usize) {
        let [width, height] = self.spectrogram_image_buffer.size();
        let data_width = width.saturating_sub(SPECTROGRAM_PAD_LEFT);
        let pixels = pixels.min(data_width);

        if pixels == 0 {
            return;
        }

        for y in 0..height {
            let row_start = y * width;
            let row_end = row_start + width;
            let row = &mut self.spectrogram_image_buffer.pixels[row_start..row_end];

            if pixels < data_width {
                row.copy_within(SPECTROGRAM_PAD_LEFT + pixels..width, SPECTROGRAM_PAD_LEFT);
            }
            row[width - pixels..width].fill(SpectrogramImage::BLACK);
        }
    }

    fn clear_spectrogram_pending(&mut self) {
        self.spectro_pending_sum.clear();
        self.spectro_pending_count = 0;
        self.spectro_pending_beat = false;
        self.spectro_pending_onset = false;
    }

    /// Fold one collector column into the pending accumulator. Several
    /// columns usually share a single pixel column (a 120 s window at 60 Hz
    /// is ~10 columns per pixel), so the pixel shows their average instead of
    /// whichever column happened to cross the pixel boundary. Beat and onset
    /// markers are kept if any of the folded columns carried them.
    fn accumulate_spectrogram_column(&mut self, column: &CollectorOutput) {
        let bins = &column.current_audio_colunn;
        if self.spectro_pending_sum.len() != bins.len() {
            self.spectro_pending_sum.clear();
            self.spectro_pending_sum.resize(bins.len(), 0);
            self.spectro_pending_count = 0;
        }
        for (sum, bin) in self.spectro_pending_sum.iter_mut().zip(bins) {
            *sum += bin.volume as u32;
        }
        self.spectro_pending_count += 1;
        self.spectro_pending_beat |= column.snapshot.beat_trigger;
        self.spectro_pending_onset |= column.snapshot.actual_onset_peak;
    }

    /// Draw the averaged pending columns into `x_start..x_end` and clear them.
    fn flush_spectrogram_pending(&mut self, x_start: usize, x_end: usize) {
        let [width, height_outer] = self.spectrogram_image_buffer.size();
        let Some(height) = height_outer.checked_sub(SPECTROGRAM_PAD_BOTTOM + SPECTROGRAM_PAD_TOP)
        else {
            self.clear_spectrogram_pending();
            return;
        };

        let bin_count = self.spectro_pending_sum.len();
        let count = self.spectro_pending_count;
        if bin_count == 0 || count == 0 || x_start >= x_end || x_end > width || height < bin_count {
            self.clear_spectrogram_pending();
            return;
        }

        for x in x_start..x_end {
            for y in 0..height_outer {
                self.spectrogram_image_buffer.pixels[y * width + x] = SpectrogramImage::BLACK;
            }
        }

        for bin_index in 0..bin_count {
            let (y_min, y_max) = bin_rows(SPECTROGRAM_PAD_TOP, height, bin_count, bin_index);
            let volume = (self.spectro_pending_sum[bin_index] / count) as u8;
            let color = spectrogram_color(volume);

            for y in y_min..y_max {
                let row_start = y * width + x_start;
                let row_end = row_start + (x_end - x_start);
                self.spectrogram_image_buffer.pixels[row_start..row_end].fill(color);
            }
        }

        draw_markers(
            &mut self.spectrogram_image_buffer,
            width,
            height_outer,
            height,
            SPECTROGRAM_PAD_BOTTOM,
            x_start,
            x_end,
            self.spectro_pending_beat,
            self.spectro_pending_onset,
        );

        self.clear_spectrogram_pending();
    }

    fn advance_spectrogram_ring(&mut self, spec: &AudioSpectrogram) -> bool {
        let last_time = self.last_spectrogram_snapshot_time.get();
        let [width, _] = self.spectrogram_image_buffer.size();
        let data_width = width.saturating_sub(SPECTROGRAM_PAD_LEFT);
        if data_width == 0 || spec.max_columns == 0 {
            return false;
        }

        let pixels_per_column = data_width as f32 / spec.max_columns as f32;
        let mut changed = false;

        let mut specperf_cols = 0u32; // SPECPERF-TEMP
        for column in spec.columns.iter().filter(|c| c.snapshot.time > last_time) {
            // SPECPERF-TEMP begin
            {
                specperf_cols += 1;
                let mut st = specperf::STATS.lock().unwrap();
                if st.last_col_time != 0 {
                    let dt = column.snapshot.time.saturating_sub(st.last_col_time);
                    st.col_dt_min = st.col_dt_min.min(dt);
                    st.col_dt_max = st.col_dt_max.max(dt);
                    if dt > 30 {
                        st.col_dt_over_30 += 1;
                    }
                }
                st.last_col_time = column.snapshot.time;
                st.cols += 1;
            }
            // SPECPERF-TEMP end
            self.accumulate_spectrogram_column(column);
            self.spectro_scroll_px_offset += pixels_per_column;
            let pixels_to_advance = self.spectro_scroll_px_offset.floor() as usize;
            if pixels_to_advance > 0 {
                self.spectro_scroll_px_offset -= pixels_to_advance as f32;
                let write_width = pixels_to_advance.min(data_width);
                // SPECPERF-TEMP begin
                {
                    let mut st = specperf::STATS.lock().unwrap();
                    let now = std::time::Instant::now();
                    if let Some(prev) = st.last_shift {
                        let gap = now.duration_since(prev).as_secs_f32() * 1000.0;
                        st.shift_gap_max_ms = st.shift_gap_max_ms.max(gap);
                    }
                    st.last_shift = Some(now);
                    st.shifts += 1;
                    st.px += write_width as u32;
                }
                // SPECPERF-TEMP end
                self.shift_spectrogram_ring_left(write_width);
                self.flush_spectrogram_pending(width - write_width, width);
                changed = true;
            }

            self.last_spectrogram_snapshot_time
                .set(column.snapshot.time);
        }

        self.last_spectrogram_columns.set(spec.columns.len());
        // SPECPERF-TEMP begin
        {
            let mut st = specperf::STATS.lock().unwrap();
            if specperf_cols == 0 {
                st.frames_0_cols += 1;
            } else if specperf_cols >= 2 {
                st.frames_2plus_cols += 1;
            }
            st.max_cols_per_frame = st.max_cols_per_frame.max(specperf_cols);
        }
        // SPECPERF-TEMP end
        changed
    }

    fn update_spectrogram_ring_texture(
        &mut self,
        ctx: &egui::Context,
        spec: &AudioSpectrogram,
        width: usize,
        height: usize,
    ) {
        let specperf_t0 = std::time::Instant::now(); // SPECPERF-TEMP
        let changed = if self.spectrogram_ring_needs_reset(spec, width, height) {
            self.reset_spectrogram_ring_cache(spec, width, height);
            true
        } else {
            self.advance_spectrogram_ring(spec)
        };

        if self.spectrogram_texture_handle.is_none() {
            let texture = ctx.load_texture(
                "spectrogram",
                components::spectrogram_color_image(&self.spectrogram_image_buffer),
                egui::TextureOptions::NEAREST,
            );
            self.spectrogram_texture_handle = Some(texture);
        } else if changed {
            if let Some(handle) = &mut self.spectrogram_texture_handle {
                let specperf_up = std::time::Instant::now(); // SPECPERF-TEMP
                handle.set(
                    components::spectrogram_color_image(&self.spectrogram_image_buffer),
                    egui::TextureOptions::NEAREST,
                );
                // SPECPERF-TEMP begin
                let us = specperf_up.elapsed().as_micros();
                let mut st = specperf::STATS.lock().unwrap();
                st.uploads += 1;
                st.upload_us_total += us;
                st.upload_us_max = st.upload_us_max.max(us);
                // SPECPERF-TEMP end
            }
        }
        // SPECPERF-TEMP begin
        {
            let now = std::time::Instant::now();
            let mut st = specperf::STATS.lock().unwrap();
            st.update_us_max = st.update_us_max.max(specperf_t0.elapsed().as_micros());
            if let Some(prev) = st.last_frame {
                let gap = now.duration_since(prev).as_secs_f32() * 1000.0;
                st.frame_gap_max_ms = st.frame_gap_max_ms.max(gap);
            }
            st.last_frame = Some(now);
            st.frames += 1;
            let start = *st.window_start.get_or_insert(now);
            let secs = now.duration_since(start).as_secs_f32();
            if secs >= 2.0 {
                tracing::info!(
                    "SPECPERF secs={secs:.2} ppp={:.2} tex={}x{} max_columns={} cols_per_px={:.2} | ui_fps={:.1} frame_gap_max={:.1}ms update_max={}us | cols/s={:.1} col_dt=[{}..{}]ms col_dt>30ms={} frames_0cols={} frames_2+cols={} max_cols/frame={} | shifts/s={:.1} px/s={:.1} shift_gap_max={:.1}ms | uploads={} upload_avg={}us upload_max={}us",
                    ctx.pixels_per_point(),
                    width,
                    height,
                    spec.max_columns,
                    spec.max_columns as f32 / width.saturating_sub(SPECTROGRAM_PAD_LEFT).max(1) as f32,
                    st.frames as f32 / secs,
                    st.frame_gap_max_ms,
                    st.update_us_max,
                    st.cols as f32 / secs,
                    if st.col_dt_min == u64::MAX { 0 } else { st.col_dt_min },
                    st.col_dt_max,
                    st.col_dt_over_30,
                    st.frames_0_cols,
                    st.frames_2plus_cols,
                    st.max_cols_per_frame,
                    st.shifts as f32 / secs,
                    st.px as f32 / secs,
                    st.shift_gap_max_ms,
                    st.uploads,
                    st.upload_us_total / st.uploads.max(1) as u128,
                    st.upload_us_max,
                );
                let last_col_time = st.last_col_time;
                let last_shift = st.last_shift;
                *st = specperf::Stats {
                    window_start: Some(now),
                    last_frame: Some(now),
                    frames: 0,
                    frame_gap_max_ms: 0.0,
                    cols: 0,
                    last_col_time,
                    col_dt_min: u64::MAX,
                    col_dt_max: 0,
                    col_dt_over_30: 0,
                    frames_0_cols: 0,
                    frames_2plus_cols: 0,
                    max_cols_per_frame: 0,
                    shifts: 0,
                    px: 0,
                    last_shift,
                    shift_gap_max_ms: 0.0,
                    uploads: 0,
                    upload_us_total: 0,
                    upload_us_max: 0,
                    update_us_max: 0,
                };
            }
        }
        // SPECPERF-TEMP end
    }
}

impl BlaulichtApp {
    fn set_audio_param(&mut self, param: AudioParam) {
        self.send_command(HostCommand::SetAudioParam(param), |_, _| {});
    }

    fn render_choose_audio_device_popup(
        &mut self,
        ctx: &egui::Context,
        selected_device: &mut Option<String>,
    ) {
        if !self.set_audio_device_popup_open {
            return;
        }

        let mut options = self.available_audio_devices.clone();
        options.extend(DummyInput::ALL.map(|dummy| dummy.name().to_owned()));

        let (new_device, changed) = components::selection_dialog(
            ctx,
            options,
            selected_device
                .clone()
                .unwrap_or_else(|| DummyInput::Silence.name().to_owned()),
            &mut self.set_audio_device_popup_open,
            "Select Audio Device".to_string(),
        );

        if changed {
            *selected_device = Some(new_device);
        }
    }

    /// Fullscreen, read-only audio analysis info (frequency bands, signal and
    /// section state, beat detection, the PCM tracker and the learned beat
    /// model) in three columns so everything fits on the 800x480 panel. The
    /// only control is the manual BPM override.
    fn render_audio_info_dialog(&mut self, ctx: &egui::Context) {
        let debug_data = self.client.with_spectrogram(|spec| {
            spec.columns
                .back()
                .map(|col| col.debug_data.clone())
                .unwrap_or_default()
        });
        let bpm_confidence_pct = (self.collector_snapshot.bpm_confidence * 100.0) as i32;

        let screen = ctx.content_rect();
        // The dialog frame adds 6/12 px inner margin and a 1 px stroke per side.
        let size = (screen.size() - vec2(14.0, 26.0)).max(vec2(200.0, 200.0));
        let mut close = false;
        let response = components::Dialog::new("Audio Info".to_string(), size)
            .with_backdrop()
            .fixed_pos(screen.min.to_vec2())
            .show(ctx, |ui| {
                ui.with_layout(egui::Layout::top_down(egui::Align::Min), |ui| {
                    ui.horizontal(|ui| {
                        ui.label(
                            RichText::new("AUDIO ANALYSIS")
                                .strong()
                                .size(ButtonSize::Medium.dim().1 + 2.0)
                                .color(INFO_HEADER_COLOR),
                        );
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            if components::button(ui, false, "Close", ButtonSize::Medium) {
                                close = true;
                            }
                        });
                    });
                    ui.separator();

                    let gap = 10.0;
                    let height = ui.available_height();
                    let total = (ui.available_width() - 2.0 * gap).max(300.0);
                    let widths = [total / 3.0; 3];

                    ui.horizontal_top(|ui| {
                        ui.spacing_mut().item_spacing.x = gap;
                        for (idx, width) in widths.into_iter().enumerate() {
                            ui.allocate_ui_with_layout(
                                vec2(width, height),
                                egui::Layout::top_down(egui::Align::Min),
                                |ui| {
                                    ui.set_width(width);
                                    egui::ScrollArea::vertical()
                                        .id_salt(("audio_info_column", idx))
                                        .max_height(height)
                                        .auto_shrink([false, false])
                                        .show(ui, |ui| {
                                            ui.spacing_mut().item_spacing.y = 3.0;
                                            match idx {
                                                0 => self.info_bands_column(ui, &debug_data),
                                                1 => self.info_signal_column(
                                                    ui,
                                                    &debug_data,
                                                    bpm_confidence_pct,
                                                ),
                                                _ => self.info_tracker_column(ui, &debug_data),
                                            }
                                        });
                                },
                            );
                        }
                    });
                });
            });
        if close || response.cancel_requested {
            self.audio_info_dialog_open = false;
        }
    }

    fn info_bands_column(&mut self, ui: &mut egui::Ui, debug_data: &SignalDebugData) {
        info_header(ui, "FREQUENCY BANDS");
        let auto_weight = self.client.with_audio_params(|params| params.auto_weight);
        info_line(
            ui,
            &[
                (
                    "bass range",
                    format!(
                        "{:.0}-{:.0} Hz",
                        debug_data.bass_range.start, debug_data.bass_range.end
                    ),
                ),
                (
                    "weights",
                    if auto_weight { "auto" } else { "fixed" }.to_string(),
                ),
            ],
        );
        ui.add_space(2.0);

        // Share the column between the three graphs, each with one metrics
        // line underneath.
        let metrics_height = 16.0;
        let graph_height =
            ((ui.available_height() - 3.0 * (metrics_height + 6.0)) / 3.0).clamp(40.0, 110.0);
        for (idx, band) in self.band_energy_graphs.iter().enumerate() {
            let (response, painter) = ui.allocate_painter(
                vec2(ui.available_width(), graph_height),
                egui::Sense::hover(),
            );
            band.draw(painter, response.rect);
            info_line(
                ui,
                &[
                    ("peak", format!("{:.2}", debug_data.band_onset_peakiness[idx])),
                    ("period", format!("{:.2}", debug_data.band_onset_periodicity[idx])),
                    ("trans", format!("{:.2}", debug_data.band_transient_strength[idx])),
                    ("weight", format!("{:.2}", debug_data.band_weights[idx])),
                ],
            );
            ui.add_space(3.0);
        }
    }

    fn info_signal_column(
        &mut self,
        ui: &mut egui::Ui,
        debug_data: &SignalDebugData,
        bpm_confidence_pct: i32,
    ) {
        let snapshot = &self.collector_snapshot;

        info_header(ui, "SIGNAL");
        let source_color = match snapshot.source_status {
            AudioSourceStatus::Active => INFO_OK_COLOR,
            AudioSourceStatus::NoFrame => INFO_WARN_COLOR,
            _ => INFO_BAD_COLOR,
        };
        info_status(
            ui,
            "Source",
            &format!("{:?}", snapshot.source_status),
            source_color,
            Some(&format!("frame age {} ms", snapshot.frame_age_ms)),
        );
        let (section_label, section_color) = components::section_label(snapshot.section_state);
        // Drop the state glyph: the proportional font has no glyph for it.
        let section_label = section_label
            .split_once(' ')
            .map_or(section_label, |(_, label)| label);
        info_status(ui, "Section", section_label, section_color, None);
        info_line(
            ui,
            &[
                ("volume", snapshot.volume.to_string()),
                ("beat vol", snapshot.beat_volume.to_string()),
            ],
        );
        info_line(
            ui,
            &[
                ("bass", snapshot.bass.to_string()),
                ("avg", snapshot.bass_avg.to_string()),
                ("avg short", snapshot.bass_avg_short.to_string()),
            ],
        );
        info_line(
            ui,
            &[
                (
                    "beat in bar",
                    snapshot
                        .beat_in_bar
                        .map_or("-".to_string(), |beat| beat.to_string()),
                ),
                (
                    "tempo source",
                    snapshot.tempo_source.map_or("analyzer".to_string(), |(owner, device)| {
                        format!("external {owner}/{device}")
                    }),
                ),
            ],
        );

        ui.add_space(6.0);
        info_header(ui, "BEAT DETECTION");

        let (mut manual_bpm_enabled, mut manual_bpm_value) =
            self.client.with_audio_params(|params| {
                (
                    params.manual_bpm.is_some(),
                    params.manual_bpm.unwrap_or(120.0),
                )
            });
        let mut manual_bpm = None;
        ui.horizontal(|ui| {
            if components::Switch::new(&mut manual_bpm_enabled)
                .ui(ui)
                .on_hover_text("Bypasses tempo detection and pins the beat scheduler.")
                .changed()
            {
                manual_bpm = Some(manual_bpm_enabled.then_some(manual_bpm_value));
            }
            ui.label("Manual BPM");
            if manual_bpm_enabled {
                let before = manual_bpm_value;
                self.manual_bpm_numberpad.ui(ui, &mut manual_bpm_value);
                if (manual_bpm_value - before).abs() > f32::EPSILON {
                    manual_bpm = Some(Some(manual_bpm_value));
                }
            }
        });
        if let Some(bpm) = manual_bpm {
            self.set_audio_param(AudioParam::ManualBpm(bpm));
        }

        let (status_label, status_detail, status_color) = match debug_data.bpm_status {
            BpmDetectStatus::Detecting => (
                "Detecting",
                "Tempo is being estimated from onsets.".to_string(),
                INFO_OK_COLOR,
            ),
            BpmDetectStatus::NoEnergy => (
                "No energy",
                "All bands below the noise floor, no audio to analyze.".to_string(),
                INFO_BAD_COLOR,
            ),
            BpmDetectStatus::Warmup { have, need } => (
                "Warming up",
                format!(
                    "Filling onset history: {have}/{need} (~{:.1}s left).",
                    ((need.saturating_sub(have)) as f32
                        * blaulicht_audio_engine::ONSET_SAMPLE_PERIOD_MS as f32)
                        / 1000.0
                ),
                INFO_WARN_COLOR,
            ),
            BpmDetectStatus::FlatOnset => (
                "Flat onset",
                "Bass without transients (sustained tone), nothing periodic to lock onto."
                    .to_string(),
                INFO_WARN_COLOR,
            ),
            BpmDetectStatus::WeakPeriodicity {
                strength,
                threshold,
            } => (
                "Weak periodicity",
                format!(
                    "Periodicity {strength:.3} < {threshold:.3}, beat too irregular; \
                     holding previous estimate."
                ),
                INFO_WARN_COLOR,
            ),
        };
        info_status(ui, "Status", status_label, status_color, None);
        info_hint(ui, &status_detail);
        info_line(
            ui,
            &[
                (
                    "latched",
                    if debug_data.bpm_estimate > 0.0 {
                        format!("{:.1} BPM", debug_data.bpm_estimate)
                    } else {
                        "none yet".to_string()
                    },
                ),
                ("conf", format!("{bpm_confidence_pct}%")),
            ],
        );

        // --- Loop tempo worker ---
        ui.add_space(6.0);
        info_header(ui, "LOOP TEMPO WORKER");
        let loop_data = debug_data.loop_tempo;
        let loop_color = match loop_data.status {
            blaulicht_audio_engine::LoopTempoStatus::Accepted => INFO_OK_COLOR,
            blaulicht_audio_engine::LoopTempoStatus::Rejected
            | blaulicht_audio_engine::LoopTempoStatus::WorkerStopped => INFO_BAD_COLOR,
            blaulicht_audio_engine::LoopTempoStatus::WarmingUp
            | blaulicht_audio_engine::LoopTempoStatus::Analyzing => INFO_WARN_COLOR,
        };
        info_status(
            ui,
            "Status",
            &format!("{:?}", loop_data.status),
            loop_color,
            loop_data.fused.then_some("fused"),
        );
        let bpm_or_dash = |bpm: f32| {
            if bpm > 0.0 {
                format!("{bpm:.2}")
            } else {
                "-".to_string()
            }
        };
        info_line(
            ui,
            &[
                ("candidate", bpm_or_dash(loop_data.candidate_bpm)),
                ("accepted", bpm_or_dash(loop_data.accepted_bpm)),
            ],
        );
        info_line(
            ui,
            &[
                (
                    "score",
                    format!("{:.3}/{:.3}", loop_data.score, loop_data.threshold),
                ),
                ("buffer", format!("{:.1}s", loop_data.buffered_seconds)),
                ("age", format!("{}ms", loop_data.age_ms)),
            ],
        );
    }

    fn info_tracker_column(&mut self, ui: &mut egui::Ui, debug_data: &SignalDebugData) {
        let tracker = &debug_data.beat_tracker;
        info_header(ui, "BEAT TRACKER");
        let (front_end_label, front_end_color) = match tracker.front_end {
            BeatFrontEnd::Idle => ("idle", INFO_WARN_COLOR),
            BeatFrontEnd::SpectralFlux => ("spectral flux", INFO_OK_COLOR),
            BeatFrontEnd::Model => ("model", INFO_OK_COLOR),
        };
        let mode_mismatch =
            tracker.mode == BeatDetection::Model && tracker.front_end == BeatFrontEnd::SpectralFlux;
        let config_note = if mode_mismatch {
            "model failed to load".to_string()
        } else {
            format!("config: {:?}", tracker.mode).to_lowercase()
        };
        info_status(
            ui,
            "Front end",
            front_end_label,
            if mode_mismatch {
                INFO_BAD_COLOR
            } else {
                front_end_color
            },
            Some(&config_note),
        );
        if tracker.mode == BeatDetection::Legacy {
            info_hint(ui, "Legacy mode: the spectrum estimator runs instead.");
        } else {
            info_line(
                ui,
                &[
                    (
                        "tempo",
                        if tracker.tempo_bpm > 0.0 {
                            format!("{:.2} BPM", tracker.tempo_bpm)
                        } else {
                            "-".to_string()
                        },
                    ),
                    ("conf", format!("{:.0}%", tracker.tempo_confidence * 100.0)),
                ],
            );
            info_line(
                ui,
                &[
                    (
                        "history",
                        format!("{}/{}", tracker.history_frames, tracker.history_needed),
                    ),
                    (
                        "manual",
                        if tracker.manual_bpm { "yes" } else { "no" }.to_string(),
                    ),
                ],
            );
            info_line(
                ui,
                &[(
                    "challenger",
                    if tracker.challenger_bpm > 0.0 {
                        format!(
                            "{:.1} BPM, held {:.1}/{:.1}s",
                            tracker.challenger_bpm,
                            tracker.challenger_held_ms / 1000.0,
                            tracker.switch_hold_ms / 1000.0
                        )
                    } else {
                        "-".to_string()
                    },
                )],
            );
            info_line(
                ui,
                &[(
                    "phase",
                    if tracker.phase_locked {
                        format!(
                            "locked, rival {}/{} beats",
                            tracker.phase_challenger_beats, tracker.phase_jump_beats
                        )
                    } else {
                        "unlocked".to_string()
                    },
                )],
            );
            info_line(
                ui,
                &[
                    ("next beat", format!("{:+.0} ms", tracker.next_beat_in_ms)),
                    ("clock off", format!("{:.0} ms", tracker.clock_offset_ms)),
                ],
            );
        }

        let Some(model) = &tracker.model else {
            return;
        };
        ui.add_space(6.0);
        info_header(ui, "BEAT MODEL");
        info_line(
            ui,
            &[
                (
                    "beat",
                    format!("{:.2} / thr {:.2}", model.beat, model.beat_threshold),
                ),
                (
                    "down",
                    format!("{:.2} / thr {:.2}", model.downbeat, model.downbeat_threshold),
                ),
            ],
        );
        info_line(
            ui,
            &[
                ("beats", model.beats.to_string()),
                ("downbeats", model.downbeats.to_string()),
                ("frames", model.frames.to_string()),
            ],
        );
        info_line(
            ui,
            &[
                (
                    "net",
                    format!(
                        "{}ch x {} blocks, rf {} fr",
                        model.channels, model.blocks, model.receptive_field_frames
                    ),
                ),
                ("lead", format!("{:.0} ms", model.lead_ms)),
            ],
        );
        ui.add_space(2.0);
        // The activation graphs take what is left of the column.
        let graph_height = ((ui.available_height() - 6.0) / 2.0).clamp(40.0, 110.0);
        for graph in &self.beat_model_graphs {
            let (response, painter) = ui.allocate_painter(
                vec2(ui.available_width(), graph_height),
                egui::Sense::hover(),
            );
            graph.draw(painter, response.rect);
        }
    }

    pub fn audio_ui(
        &mut self,
        ui: &mut egui::Ui,
        ctx: &egui::Context,
        render_context: crate::app::page::PageRenderContext,
    ) {
        if self.audio_info_dialog_open {
            self.render_audio_info_dialog(ctx);
        }

        ui.horizontal(|ui| {
            let total_width = ui.available_width();
            let graph_panel_width = total_width / 3.0;

            ui.allocate_ui_with_layout(
                egui::vec2(ui.available_width(), ui.available_height()),
                egui::Layout::top_down(egui::Align::LEFT),
                |ui| {
                    let mut selected_device = self.selected_audio_input.clone();

                    let selected_device_label =
                        selected_device.clone().unwrap_or_else(|| "N/A".to_string());

                    let before = selected_device.clone();

                    ui.with_layout(
                        egui::Layout::left_to_right(egui::Align::Center).with_main_wrap(
                            render_context.is_dynamic()
                                && render_context.width_class
                                    == crate::app::page::PageWidthClass::Narrow,
                        ),
                        |ui| {
                            ui.set_min_height(ButtonSize::Medium.dim().0.y);

                            ui.label(
                                RichText::new("Current Input:").size(ButtonSize::Medium.dim().1),
                            );
                            ui.label(
                                RichText::new(selected_device_label)
                                    .size(ButtonSize::Medium.dim().1)
                                    .color(Color32::LIGHT_RED),
                            );

                            if components::button(ui, false, "Change Device", ButtonSize::Medium) {
                                self.set_audio_device_popup_open = true;
                            }

                            ui.separator();

                            if components::button(ui, false, "Info", ButtonSize::Medium) {
                                self.audio_info_dialog_open = !self.audio_info_dialog_open;
                            }
                        },
                    );

                    self.render_choose_audio_device_popup(ctx, &mut selected_device);

                    if selected_device != before {
                        // The engine resolves the name, restarts on it and
                        // publishes `AudioSelected`; a vanished device is
                        // refused (popup).
                        self.send_command(
                            HostCommand::SelectAudioInput {
                                name: selected_device,
                            },
                            |_, _| {},
                        );
                    }

                    // --- Live Spectrogram (show last 60s, no scrolling) ---
                    ui.add_space(6.0);
                    let spec_height = if render_context.is_dynamic() {
                        (render_context.size.y * 0.25).clamp(120.0, 320.0)
                    } else {
                        160.0
                    };
                    let spec_width = ui.available_width();

                    let (mut volume_value, mut gate_value, mut boost_value, mut auto_calibrate) =
                        self.client.with_audio_params(|params| {
                            (
                                params.volume as f32,
                                params.gate as f32,
                                params.boost.unwrap_or(0) as f32,
                                params.auto_calibrate,
                            )
                        });

                    // Draw from the engine's spectrogram in place: a full
                    // 120 s window is thousands of 128-bin columns.
                    let ring = &mut self.spectrogram_ring;
                    let has_columns = self.client.with_spectrogram(|spec| {
                        if spec.columns.is_empty() {
                            return false;
                        }
                        ring.update_spectrogram_ring_texture(
                            ctx,
                            spec,
                            spec_width as usize,
                            spec_height as usize,
                        );
                        true
                    });
                    if !has_columns {
                        let (spec_resp, spec_painter) = ui.allocate_painter(
                            egui::vec2(spec_width, spec_height),
                            egui::Sense::hover(),
                        );
                        let spec_rect = spec_resp.rect;
                        spec_painter.rect_filled(spec_rect, 0.0, Color32::from_rgb(10, 10, 10));
                        spec_painter.text(
                            spec_rect.center_top() + egui::vec2(0.0, 6.0),
                            egui::Align2::CENTER_TOP,
                            "Waiting for audio…",
                            egui::FontId::proportional(12.0),
                            Color32::GRAY,
                        );
                    } else if let Some(texture) = ring.spectrogram_texture_handle.as_ref() {
                        ui.image(texture);
                    }

                    {
                        ui.horizontal_wrapped(|ui| {
                            ui.add_space(5.0);

                            if ui
                                .add(
                                    HFader::new(&mut volume_value, 1.0..=200.0)
                                        .with_label("Volume"),
                                )
                                .changed()
                            {
                                self.set_audio_param(AudioParam::Volume(volume_value as u8));
                            }

                            ui.add_space(8.0);

                            if ui
                                .add(HFader::new(&mut gate_value, 0.0..=100.0).with_label("Gate"))
                                .changed()
                            {
                                self.set_audio_param(AudioParam::Gate(gate_value as u8));
                            }

                            ui.add_space(8.0);

                            if ui
                                .add(HFader::new(&mut boost_value, 0.0..=255.0).with_label("Boost"))
                                .changed()
                            {
                                let boost = boost_value as u8;
                                self.set_audio_param(AudioParam::Boost(
                                    (boost > 0).then_some(boost),
                                ));
                            }

                            ui.add_space(8.0);

                            if ui.checkbox(&mut auto_calibrate, "Auto-calibrate").changed() {
                                self.set_audio_param(AudioParam::AutoCalibrate(auto_calibrate));
                            }
                        });
                    }

                    // Set larger graph height
                    let graph_height = 105.0;
                    let graph_width = graph_panel_width - 5.0;
                    let padding = 0.0;

                    debug_assert!(graph_width > 0.0);

                    ui.separator();

                    ui.with_layout(
                        egui::Layout::left_to_right(egui::Align::Min).with_main_wrap(
                            render_context.is_dynamic()
                                && render_context.width_class
                                    == crate::app::page::PageWidthClass::Narrow,
                        ),
                        |ui| {
                            ui.vertical(|ui| {
                                let (response, painter) = ui.allocate_painter(
                                    egui::vec2(graph_width, graph_height),
                                    egui::Sense::hover(),
                                );
                                self.volume_graph.draw(painter, response.rect);

                                ui.add_space(padding);
                                let (response_bass, painter_bass) = ui.allocate_painter(
                                    egui::vec2(graph_width, graph_height),
                                    egui::Sense::hover(),
                                );
                                self.bass_graph.draw(painter_bass, response_bass.rect);
                            });

                            ui.vertical(|ui| {
                                let (response_bass_avg, painter_bass_avg) = ui.allocate_painter(
                                    egui::vec2(graph_width, graph_height),
                                    egui::Sense::hover(),
                                );
                                self.bass_avg_graph
                                    .draw(painter_bass_avg, response_bass_avg.rect);

                                let (response_bass_avg_short, painter_bass_avg_short) = ui
                                    .allocate_painter(
                                        egui::vec2(graph_width, graph_height),
                                        egui::Sense::hover(),
                                    );
                                self.bass_avg_short_graph
                                    .draw(painter_bass_avg_short, response_bass_avg_short.rect);
                            });

                            ui.vertical(|ui| {
                                let bg_color = egui::Color32::from_rgb(20, 20, 25);

                                const BPM_FONT_SIZE: f32 = 30.0;

                                Frame::NONE
                                    .fill(bg_color)
                                    .outer_margin(Margin {
                                        left: 0,
                                        right: 0,
                                        top: 15,
                                        bottom: 0,
                                    })
                                    .inner_margin(Margin::same(8))
                                    .show(ui, |ui| {
                                        ui.set_height(graph_height);
                                        ui.set_width(graph_width);

                                        ui.horizontal(|ui| {
                                            ui.heading(
                                                RichText::new(format!(
                                                    "{: >3.1} BPM",
                                                    self.collector_snapshot.bpm
                                                ))
                                                .strong()
                                                .color(Color32::LIGHT_GREEN)
                                                .font(FontId::monospace(BPM_FONT_SIZE)),
                                            );

                                            ui.add_space(20.0);

                                            {
                                                let (bpm_rect, painter_bpm_indicator) = ui
                                                    .allocate_painter(
                                                        egui::vec2(20.0, 20.0),
                                                        egui::Sense::empty(),
                                                    );

                                                let color = if self.collector_snapshot.beat_trigger
                                                {
                                                    Color32::LIGHT_GREEN
                                                } else {
                                                    Color32::BLACK
                                                };

                                                let center = bpm_rect.rect.center();
                                                let radius = bpm_rect
                                                    .rect
                                                    .width()
                                                    .min(bpm_rect.rect.height())
                                                    * 0.5;

                                                painter_bpm_indicator
                                                    .circle_filled(center, radius, color);
                                            }
                                        });

                                        // Currently-detected audio section, shown below the BPM.
                                        let (section_text, section_color) =
                                            components::section_label(
                                                self.collector_snapshot.section_state,
                                            );
                                        ui.label(
                                            RichText::new(section_text)
                                                .strong()
                                                .color(section_color)
                                                .font(FontId::monospace(20.0)),
                                        );

                                        ui.add_space(4.0);

                                        ui.horizontal(|ui| {
                                            ui.add_space(2.0);
                                            let beat_interval_secs =
                                                self.beat_marker_interval.as_secs_f32();
                                            let active_beat_marker = if self.beat_marker_has_beat
                                                && beat_interval_secs > 0.0
                                            {
                                                let elapsed_beats = (self
                                                    .beat_marker_anchor_instant
                                                    .elapsed()
                                                    .as_secs_f32()
                                                    / beat_interval_secs)
                                                    .floor()
                                                    as usize;
                                                (self.beat_marker_index + elapsed_beats) % 4
                                            } else {
                                                self.beat_marker_index
                                            };

                                            for beat_idx in 0..4 {
                                                let (beat_rect, beat_painter) = ui
                                                    .allocate_painter(
                                                        egui::vec2(26.0, 10.0),
                                                        egui::Sense::empty(),
                                                    );
                                                let active = self.beat_marker_has_beat
                                                    && active_beat_marker == beat_idx;
                                                let fill = if active {
                                                    Color32::LIGHT_GREEN
                                                } else {
                                                    Color32::from_rgb(45, 50, 55)
                                                };
                                                let stroke = if active {
                                                    egui::Stroke::new(1.0, Color32::WHITE)
                                                } else {
                                                    egui::Stroke::new(1.0, Color32::from_gray(85))
                                                };

                                                beat_painter.rect_filled(beat_rect.rect, 2.0, fill);
                                                beat_painter.rect_stroke(
                                                    beat_rect.rect,
                                                    2.0,
                                                    stroke,
                                                    egui::StrokeKind::Inside,
                                                );
                                            }
                                        });

                                        ui.add_space(3.0);

                                        ui.heading(
                                            RichText::new(format!(
                                                "{: >3} MS",
                                                self.collector_snapshot.time_between_beats_millis
                                            ))
                                            .color(Color32::GRAY)
                                            .strong()
                                            .font(FontId::monospace(BPM_FONT_SIZE)),
                                        );
                                    });
                                // ui.set_height(graph_height);
                                // ui.set_width(graph_width);
                                //
                                // let rect = ui.max_rect();
                                // let painter = ui.painter();
                                //
                                // painter.rect_filled(rect, 0.0, bg_color);
                            });
                        },
                    );
                },
            );
        });
    }
}
