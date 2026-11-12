use crate::app::components::{ButtonSize, HFader};
use crate::app::{components, BlaulichtApp};
use blaulicht_audio_engine::{
    bin_rows, draw_markers, spectrogram_color, AudioSpectrogram, BeatDetection, BeatFrontEnd,
    BpmDetectStatus, SignalDebugData, SpectrogramImage,
};
use blaulicht_engine::client::EngineClient;
use blaulicht_engine::hub::{AudioParam, HostCommand};
use blaulicht_engine::msg::DummyInput;
use blaulicht_shared::AudioSourceStatus;

use egui::{
    vec2, Color32, ColorImage, FontId, Frame, Margin, RichText, TextureHandle, Vec2, Widget,
};
use std::collections::VecDeque;
use std::ops::Range;
use web_time::Instant;

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

/// How far the drawn head trails the engine clock, so the newest column has
/// normally arrived before its pixels are drawn (two 60 Hz ticks plus loop
/// jitter).
const SPECTROGRAM_LATENCY_MS: f64 = 40.0;
/// How far the head may run past the newest column, and how long one column
/// may be stretched over a gap, before the image shows nothing instead of
/// stale data.
const SPECTROGRAM_HOLD_MS: f64 = 100.0;

/// What paints one pixel column of the scrolling spectrogram.
#[derive(Debug, PartialEq)]
enum PixelSource {
    /// The average of these spectrogram columns, all taken within the pixel.
    Average(Range<usize>),
    /// No column was taken within the pixel: stretch this earlier one over it.
    Hold(usize),
    /// No recent data.
    Empty,
}

/// Which columns paint pixel `pixel`. Pixels sit on an absolute grid in
/// engine time: pixel `k` covers `[k, k + 1) / px_per_ms` milliseconds, so the
/// image scrolls with the clock no matter how irregularly columns arrive.
fn pixel_source(times: &VecDeque<u64>, pixel: i64, px_per_ms: f64) -> PixelSource {
    let t0 = pixel as f64 / px_per_ms;
    let t1 = (pixel + 1) as f64 / px_per_ms;
    let start = times.partition_point(|&t| (t as f64) < t0);
    let end = times.partition_point(|&t| (t as f64) < t1);
    if start < end {
        PixelSource::Average(start..end)
    } else if start > 0 && t0 - times[start - 1] as f64 <= SPECTROGRAM_HOLD_MS {
        PixelSource::Hold(start - 1)
    } else {
        PixelSource::Empty
    }
}

/// The rightmost pixel whose whole span lies before the head: the engine
/// clock minus the latency, but never more than the hold time past the
/// newest column, so a stalled engine freezes the image.
fn head_pixel(engine_now_ms: f64, newest_ms: u64, px_per_ms: f64) -> i64 {
    let head_ms =
        (engine_now_ms - SPECTROGRAM_LATENCY_MS).min(newest_ms as f64 + SPECTROGRAM_HOLD_MS);
    (head_ms * px_per_ms).floor() as i64 - 1
}

/// Everything that, when it changes, invalidates the drawn texture.
#[derive(Clone, Copy, PartialEq)]
struct SpectrogramRingKey {
    /// Texture size in physical pixels.
    size: [usize; 2],
    pad_top: usize,
    pad_bottom: usize,
    window_ms: u64,
    bin_count: usize,
    epoch: Option<Instant>,
}

/// The live spectrogram as a ring-buffer texture at physical-pixel
/// resolution. Every frame the head moves with the wall clock, only the newly
/// exposed pixel columns are drawn and uploaded, and the texture is painted
/// as two quads split at the ring's wrap point.
#[derive(Default)]
pub struct SpectrogramRing {
    key: Option<SpectrogramRingKey>,
    texture: Option<TextureHandle>,
    /// Absolute pixel index (see [`pixel_source`]) of the rightmost drawn
    /// column; it sits at the right edge of the image.
    drawn: i64,
}

impl SpectrogramRing {
    pub fn show(&mut self, ui: &mut egui::Ui, spec: &AudioSpectrogram, size: Vec2) {
        let (rect, _) = ui.allocate_exact_size(size, egui::Sense::hover());
        let painter = ui.painter_at(rect);
        painter.rect_filled(rect, 0.0, Color32::BLACK);

        // Snap the texture to physical pixels so NEAREST maps texels 1:1.
        let ppp = ui.ctx().pixels_per_point();
        let left = ((rect.left() + SPECTROGRAM_PAD_LEFT as f32) * ppp).round();
        let right = (rect.right() * ppp).round();
        let top = (rect.top() * ppp).round();
        let bottom = (rect.bottom() * ppp).round();
        let width = (right - left).max(0.0) as usize;
        let height = (bottom - top).max(0.0) as usize;
        let pad_top = (SPECTROGRAM_PAD_TOP as f32 * ppp).round() as usize;
        let pad_bottom = (SPECTROGRAM_PAD_BOTTOM as f32 * ppp).round() as usize;

        let window_ms = spec
            .window
            .unwrap_or(spec.tick_period * spec.max_columns as u32)
            .as_millis() as u64;
        let Some(&newest_ms) = spec.column_times_ms.back() else {
            return;
        };
        if width == 0 || height <= pad_top + pad_bottom || window_ms == 0 {
            return;
        }

        let key = SpectrogramRingKey {
            size: [width, height],
            pad_top,
            pad_bottom,
            window_ms,
            bin_count: spec.bin_count,
            epoch: spec.epoch,
        };
        let px_per_ms = width as f64 / window_ms as f64;
        let engine_now_ms = spec
            .now_ms()
            .unwrap_or(newest_ms as f64 + SPECTROGRAM_LATENCY_MS);
        let head = head_pixel(engine_now_ms, newest_ms, px_per_ms);

        if self.key != Some(key) || self.texture.is_none() {
            self.texture = Some(ui.ctx().load_texture(
                "spectrogram",
                ColorImage::new([width, height], vec![Color32::BLACK; width * height]),
                egui::TextureOptions::NEAREST,
            ));
            self.key = Some(key);
            self.drawn = head - width as i64;
        }

        // Draw what the head exposed; after a long absence that is at most
        // one full ring.
        let first = (self.drawn + 1).max(head - width as i64 + 1);
        if first <= head {
            self.upload(first, head, spec, &key);
            self.drawn = head;
        }

        let Some(texture) = &self.texture else {
            return;
        };
        // The leftmost visible pixel is the one after `drawn`, one ring back.
        let split = (self.drawn + 1).rem_euclid(width as i64) as usize;
        let x_split = (left + (width - split) as f32) / ppp;
        let (left, right, top, bottom) = (left / ppp, right / ppp, top / ppp, bottom / ppp);
        let u_split = split as f32 / width as f32;
        painter.image(
            texture.id(),
            egui::Rect::from_min_max(egui::pos2(left, top), egui::pos2(x_split, bottom)),
            egui::Rect::from_min_max(egui::pos2(u_split, 0.0), egui::pos2(1.0, 1.0)),
            Color32::WHITE,
        );
        if split > 0 {
            painter.image(
                texture.id(),
                egui::Rect::from_min_max(egui::pos2(x_split, top), egui::pos2(right, bottom)),
                egui::Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(u_split, 1.0)),
                Color32::WHITE,
            );
        }
    }

    /// Draw pixels `first..=last` and upload them into their ring slots, in
    /// one or two runs depending on whether they wrap.
    fn upload(&mut self, first: i64, last: i64, spec: &AudioSpectrogram, key: &SpectrogramRingKey) {
        let Some(texture) = &mut self.texture else {
            return;
        };
        let [width, height] = key.size;
        let px_per_ms = width as f64 / key.window_ms as f64;

        let mut pixel = first;
        while pixel <= last {
            let slot = pixel.rem_euclid(width as i64) as usize;
            let run = ((last - pixel + 1) as usize).min(width - slot);
            let mut image = SpectrogramImage::new(run, height);
            for x in 0..run {
                let source = pixel_source(&spec.column_times_ms, pixel + x as i64, px_per_ms);
                paint_pixel_column(&mut image, x, key, spec, source);
            }
            texture.set_partial(
                [slot, 0],
                components::spectrogram_color_image(&image),
                egui::TextureOptions::NEAREST,
            );
            pixel += run as i64;
        }
    }
}

/// Paint one pixel column of `image` (already black) from `source`.
fn paint_pixel_column(
    image: &mut SpectrogramImage,
    x: usize,
    key: &SpectrogramRingKey,
    spec: &AudioSpectrogram,
    source: PixelSource,
) {
    let [width, height_outer] = image.size();
    let height = height_outer - key.pad_top - key.pad_bottom;
    let (columns, with_markers) = match source {
        PixelSource::Empty => return,
        PixelSource::Hold(index) => (index..index + 1, false),
        PixelSource::Average(range) => (range, true),
    };
    let count = columns.len() as u32;

    for bin_index in 0..key.bin_count {
        let sum: u32 = columns
            .clone()
            .filter_map(|i| spec.columns[i].current_audio_colunn.get(bin_index))
            .map(|bin| bin.volume as u32)
            .sum();
        let color = spectrogram_color((sum / count) as u8);
        let (y_min, y_max) = bin_rows(key.pad_top, height, key.bin_count, bin_index);
        for y in y_min..y_max {
            image.pixels[y * width + x] = color;
        }
    }

    // Markers only where a column was actually taken, so a stretched column
    // does not repeat its beat.
    if with_markers {
        let beat = columns
            .clone()
            .any(|i| spec.columns[i].snapshot.beat_trigger);
        let onset = columns
            .clone()
            .any(|i| spec.columns[i].snapshot.actual_onset_peak);
        draw_markers(
            image,
            width,
            height_outer,
            height,
            key.pad_bottom,
            x,
            x + 1,
            beat,
            onset,
        );
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
                                    ui.set_max_width(width);
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
        });
        // On its own row so the column keeps its width.
        if manual_bpm_enabled {
            let before = manual_bpm_value;
            self.manual_bpm_numberpad.ui(ui, &mut manual_bpm_value);
            if (manual_bpm_value - before).abs() > f32::EPSILON {
                manual_bpm = Some(Some(manual_bpm_value));
            }
        }
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
        if self.layout.is_desktop() {
            self.render_audio_desktop(ui);
            return;
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
                    self.client.with_spectrogram(|spec| {
                        if spec.columns.is_empty() {
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
                        } else {
                            ring.show(ui, spec, vec2(spec_width, spec_height));
                        }
                    });

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

/// Track width of the desktop Input faders (`pages.html#audio` `.hf`).
const DESKTOP_FADER_TRACK: f32 = 175.0;
/// Height of the desktop spectrogram (`pages.html#audio`).
const DESKTOP_SPECTROGRAM_HEIGHT: f32 = 196.0;

impl BlaulichtApp {
    /// Audio on the desktop (086, `pages.html#audio`): the current input
    /// with Change device (the one dropdown) and Info in the page bar, the
    /// spectrogram pane, and the Input pane with the horizontal Volume,
    /// Gate and Boost faders and the Auto-calibrate switch. The kiosk keeps
    /// its layout with the graphs and the BPM box.
    fn render_audio_desktop(&mut self, ui: &mut egui::Ui) {
        use super::system::desk;
        use crate::app::components::{dropdown, studio};
        use crate::app::pages::setup::duo;
        use crate::app::ui_tokens::{
            studio as tokens, Density, Weight, PAGE_BAR_HEIGHT, TARGET_SHOW, TEXT,
        };
        use egui::{pos2, Align2, Rect, Sense};

        let t = tokens(ui.ctx());
        let full = desk::page_rect(ui);
        let bar_rect = Rect::from_min_size(full.min, vec2(full.width(), PAGE_BAR_HEIGHT));
        let spec_h = 26.0 + 8.0 + DESKTOP_SPECTROGRAM_HEIGHT + 8.0;
        let spec_rect = Rect::from_min_size(
            pos2(full.left(), bar_rect.bottom() + 1.0),
            vec2(full.width(), spec_h),
        );
        let input_rect = Rect::from_min_max(pos2(full.left(), spec_rect.bottom() + 1.0), full.max);

        // The page bar.
        let mut bar = desk::pane_at(ui, bar_rect, t.pane, "audio-bar");
        let current = self.selected_audio_input.clone();
        let mut options = self.available_audio_devices.clone();
        options.extend(DummyInput::ALL.map(|dummy| dummy.name().to_owned()));
        let shown = current
            .clone()
            .unwrap_or_else(|| DummyInput::Silence.name().to_owned());
        let mut picked = None;
        let mut info = false;
        studio::page_bar(
            &mut bar,
            |ui| {
                let f = Density::of(ui.ctx()).font(Weight::Regular, TEXT);
                ui.label(egui::RichText::new("Current input").font(f).color(t.text));
                ui.add_space(6.0);
                ui.label(
                    egui::RichText::new(&shown)
                        .font(egui::FontId::monospace(TEXT + 1.0))
                        .strong()
                        .color(t.strong),
                );
                ui.add_space(6.0);
                let change = studio::mini_button(ui, "Change device", false, TARGET_SHOW);
                let dd = ui.id().with("audio-device-dd");
                if change.clicked() {
                    dropdown::toggle(ui.ctx(), dd);
                }
                let items = dropdown::items(&options);
                if let dropdown::Outcome::Picked(i) = (dropdown::Popover {
                    id: dd,
                    anchor: change.rect,
                    anchor_id: Some(change.id),
                    items: &items,
                    options: dropdown::Options {
                        width: Some(240.0),
                        ..Default::default()
                    },
                })
                .show_single(ui.ctx(), options.iter().position(|o| *o == shown))
                {
                    picked = options.get(i).cloned();
                }
                // The divider between the input and Info.
                let (r, _) = ui.allocate_exact_size(vec2(9.0, 24.0), Sense::hover());
                ui.painter().vline(
                    r.center().x.round() + 0.5,
                    r.y_range(),
                    egui::Stroke::new(1.0_f32, t.line),
                );
                info = studio::mini_button(ui, "Info", self.audio_info_dialog_open, TARGET_SHOW)
                    .clicked();
            },
            |_| {},
        );
        if info {
            self.audio_info_dialog_open = !self.audio_info_dialog_open;
        }
        if let Some(name) = picked {
            if Some(&name) != current.as_ref() {
                // The engine resolves the name, restarts on it and publishes
                // `AudioSelected`; a vanished device is refused (popup).
                self.send_command(
                    HostCommand::SelectAudioInput { name: Some(name) },
                    |_, _| {},
                );
            }
        }

        // The spectrogram.
        let mut spec = desk::pane_at(ui, spec_rect, t.pane, "audio-spectrogram");
        studio::pane_header(&mut spec, None, "Spectrogram", false, |ui| {
            let f = Density::of(ui.ctx()).font(Weight::Regular, TEXT - 1.0);
            ui.label(egui::RichText::new("Last 60 s").font(f).color(t.muted));
        });
        let area = Rect::from_min_size(
            spec.cursor().min + vec2(8.0, 8.0),
            vec2(spec_rect.width() - 16.0, DESKTOP_SPECTROGRAM_HEIGHT),
        );
        let mut area_ui = spec.new_child(
            egui::UiBuilder::new()
                .id_salt("audio-spectrogram-area")
                .max_rect(area),
        );
        let ring = &mut self.spectrogram_ring;
        self.client.with_spectrogram(|data| {
            if data.columns.is_empty() {
                area_ui.painter().rect_filled(area, 0.0, t.well);
                area_ui.painter().text(
                    area.center(),
                    Align2::CENTER_CENTER,
                    "Waiting for audio…",
                    Density::of(area_ui.ctx()).font(Weight::Regular, TEXT),
                    t.muted,
                );
            } else {
                ring.show(&mut area_ui, data, area.size());
            }
        });

        // The input faders.
        let mut input = desk::pane_at(ui, input_rect, t.pane, "audio-input");
        studio::pane_header(&mut input, None, "Input", false, |_| {});
        let (mut volume, mut gate, mut boost, mut auto_calibrate) =
            self.client.with_audio_params(|params| {
                (
                    params.volume as f32,
                    params.gate as f32,
                    params.boost.unwrap_or(0) as f32,
                    params.auto_calibrate,
                )
            });
        let mut params = Vec::new();
        duo::body(&mut input, Some(48.0), |ui| {
            // Each control in its own 32px row, placed by hand so they share
            // one centre line; x advances by what the previous one used.
            let top = ui.cursor().top();
            let right = ui.max_rect().right();
            let mut x = ui.cursor().left() + 8.0;
            let slot = |ui: &mut egui::Ui, salt: &str, x: f32, y: f32| {
                let rect = Rect::from_min_max(pos2(x, y), pos2(right.max(x), y + 32.0));
                ui.new_child(
                    egui::UiBuilder::new()
                        .id_salt(("audio-input", salt))
                        .max_rect(rect)
                        .layout(egui::Layout::left_to_right(egui::Align::Center)),
                )
            };
            let mut centre = top + 16.0;
            for (name, value, range) in [
                ("Volume", &mut volume, 1.0..=200.0),
                ("Gate", &mut gate, 0.0..=100.0),
                ("Boost", &mut boost, 0.0..=255.0),
            ] {
                let mut child = slot(ui, name, x, top);
                let r = studio::hfader(&mut child, value, range, name, DESKTOP_FADER_TRACK);
                // The track sits lower than the row's middle; the switch
                // follows it.
                centre = r.rect.center().y;
                x += child.min_rect().width() + 24.0;
                if r.changed() {
                    params.push(match name {
                        "Volume" => AudioParam::Volume(*value as u8),
                        "Gate" => AudioParam::Gate(*value as u8),
                        _ => {
                            let boost = *value as u8;
                            AudioParam::Boost((boost > 0).then_some(boost))
                        }
                    });
                }
            }
            let mut child = slot(ui, "auto-calibrate", x, centre - 16.0);
            child.spacing_mut().item_spacing.x = 8.0;
            if studio::switch(&mut child, &mut auto_calibrate).changed() {
                params.push(AudioParam::AutoCalibrate(auto_calibrate));
            }
            let f = Density::of(ui.ctx()).font(Weight::Regular, TEXT);
            child.label(egui::RichText::new("Auto-calibrate").font(f).color(t.text));
        });
        for param in params {
            self.set_audio_param(param);
        }
    }
}

#[cfg(test)]
mod spectrogram_ring_tests {
    use super::*;

    // 10 px per 100 ms, so pixel k covers [10k, 10k + 10) ms.
    const PX_PER_MS: f64 = 0.1;

    fn times(ms: &[u64]) -> VecDeque<u64> {
        ms.iter().copied().collect()
    }

    #[test]
    fn columns_within_one_pixel_are_averaged() {
        let t = times(&[0, 12, 15, 18, 30]);
        assert_eq!(pixel_source(&t, 1, PX_PER_MS), PixelSource::Average(1..4));
    }

    #[test]
    fn a_late_column_stretches_over_the_gap() {
        // 60 ms between columns: the pixels in between hold the earlier one.
        let t = times(&[0, 60]);
        assert_eq!(pixel_source(&t, 0, PX_PER_MS), PixelSource::Average(0..1));
        for pixel in 1..6 {
            assert_eq!(pixel_source(&t, pixel, PX_PER_MS), PixelSource::Hold(0));
        }
        assert_eq!(pixel_source(&t, 6, PX_PER_MS), PixelSource::Average(1..2));
    }

    #[test]
    fn a_long_dropout_shows_nothing() {
        let t = times(&[0, 1000]);
        assert_eq!(pixel_source(&t, 10, PX_PER_MS), PixelSource::Hold(0));
        assert_eq!(pixel_source(&t, 11, PX_PER_MS), PixelSource::Empty);
        assert_eq!(pixel_source(&t, 99, PX_PER_MS), PixelSource::Empty);
    }

    #[test]
    fn nothing_before_the_first_column() {
        let t = times(&[500]);
        assert_eq!(pixel_source(&t, 3, PX_PER_MS), PixelSource::Empty);
    }

    #[test]
    fn head_follows_the_clock_behind_the_latency() {
        // Clock at 1000 ms, data up to date: head at 960 ms → pixel 95.
        assert_eq!(head_pixel(1000.0, 995, PX_PER_MS), 95);
        // Advancing the clock advances the head even without new columns.
        assert_eq!(head_pixel(1050.0, 995, PX_PER_MS), 100);
    }

    #[test]
    fn head_stops_when_the_engine_stalls() {
        // Newest column at 1000 ms, clock far ahead: head held at 1100 ms.
        assert_eq!(head_pixel(5000.0, 1000, PX_PER_MS), 109);
    }
}
