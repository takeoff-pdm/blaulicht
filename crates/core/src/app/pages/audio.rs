use crate::app::components::{ButtonSize, HFader};
use crate::app::{components, BlaulichtApp};
use crate::msg::FromFrontend;
use crate::{config, utils};
use blaulicht_audio_engine::{AudioSpectrogram, CollectorOutput, SpectrogramDisplayOptions};

#[cfg(feature = "audio")]
use cpal::traits::DeviceTrait;

use egui::{vec2, Color32, FontId, Frame, Margin, RichText, Widget};
use std::mem;
use std::path::PathBuf;
use std::str::FromStr;

// TODO: include snapshot in graphs

const SPECTROGRAM_PAD_BOTTOM: usize = 20;
const SPECTROGRAM_PAD_TOP: usize = 5;
const SPECTROGRAM_PAD_LEFT: usize = 5;

impl BlaulichtApp {
    fn spectrogram_color(intensity: u8) -> Color32 {
        let t = (intensity as f32 / 255.0).powf(0.5);

        let (r, g, b) = if t < 0.2 {
            let local_t = t / 0.2;
            (local_t * 0.5, 0.0, local_t)
        } else if t < 0.4 {
            let local_t = (t - 0.2) / 0.2;
            (0.5 - local_t * 0.5, 0.0, 1.0)
        } else if t < 0.6 {
            let local_t = (t - 0.4) / 0.2;
            (0.0, local_t, 1.0)
        } else if t < 0.8 {
            let local_t = (t - 0.6) / 0.2;
            (local_t, 1.0, 1.0 - local_t)
        } else {
            let local_t = (t - 0.8) / 0.2;
            (1.0, 1.0 - local_t * 0.5, local_t * 0.3)
        };

        Color32::from_rgb((r * 255.0) as u8, (g * 255.0) as u8, (b * 255.0) as u8)
    }

    fn reset_spectrogram_ring_cache(
        &mut self,
        spec: &AudioSpectrogram,
        width: usize,
        height: usize,
    ) {
        components::create_spectrogram_image(
            spec,
            width,
            height,
            &SpectrogramDisplayOptions {
                include_beat_markers: true,
            },
            &mut self.spectrogram_image_buffer,
        );

        self.spectro_scroll_px_offset = 0.0;
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

        self.spectrogram_image_buffer.size != [width, height]
            || self.last_spectrogram_max_columns.get() != spec.max_columns
            || self.last_spectrogram_bucket_data_len.get() != current_bucket_count
            || spec.columns.len() < self.last_spectrogram_columns.get()
            || current_time < self.last_spectrogram_snapshot_time.get()
    }

    fn shift_spectrogram_ring_left(&mut self, pixels: usize) {
        let [width, height] = self.spectrogram_image_buffer.size;
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
            row[width - pixels..width].fill(Color32::BLACK);
        }
    }

    fn draw_spectrogram_ring_column(
        &mut self,
        column: &CollectorOutput,
        x_start: usize,
        x_end: usize,
    ) {
        let [width, height_outer] = self.spectrogram_image_buffer.size;
        let Some(height) = height_outer.checked_sub(SPECTROGRAM_PAD_BOTTOM + SPECTROGRAM_PAD_TOP)
        else {
            return;
        };

        let bin_count = column.current_audio_colunn.len();
        if bin_count == 0 || x_start >= x_end || x_end > width {
            return;
        }

        let bucket_height = height as f32 / bin_count as f32;
        if bucket_height < 1.0 {
            return;
        }
        let bucket_height = bucket_height.floor() as usize;

        for x in x_start..x_end {
            for y in 0..height_outer {
                self.spectrogram_image_buffer.pixels[y * width + x] = Color32::BLACK;
            }
        }

        for bin_index in 0..bin_count {
            let y_max = SPECTROGRAM_PAD_TOP + ((bin_count - bin_index) * bucket_height);
            let y_min = y_max - bucket_height;
            let color = Self::spectrogram_color(column.current_audio_colunn[bin_index].volume);

            for y in y_min..y_max {
                let row_start = y * width + x_start;
                let row_end = row_start + (x_end - x_start);
                self.spectrogram_image_buffer.pixels[row_start..row_end].fill(color);
            }
        }

        if column.snapshot.beat_trigger {
            for y in 0..height_outer {
                self.spectrogram_image_buffer.pixels[y * width + x_start] = Color32::RED;
            }
        }

        if column.snapshot.actual_onset_peak {
            let dot_size = x_end - x_start;
            let y_end = (height + (SPECTROGRAM_PAD_BOTTOM / 2)).min(height_outer);
            let y_start = y_end.saturating_sub(dot_size);
            for y in y_start..y_end {
                self.spectrogram_image_buffer.pixels[y * width + x_start] = Color32::MAGENTA;
            }
        }
    }

    fn advance_spectrogram_ring(&mut self, spec: &AudioSpectrogram) -> bool {
        let last_time = self.last_spectrogram_snapshot_time.get();
        let [width, _] = self.spectrogram_image_buffer.size;
        let data_width = width.saturating_sub(SPECTROGRAM_PAD_LEFT);
        if data_width == 0 || spec.max_columns == 0 {
            return false;
        }

        let pixels_per_column = data_width as f32 / spec.max_columns as f32;
        let mut changed = false;

        for column in spec.columns.iter().filter(|c| c.snapshot.time > last_time) {
            self.spectro_scroll_px_offset += pixels_per_column;
            let pixels_to_advance = self.spectro_scroll_px_offset.floor() as usize;
            if pixels_to_advance > 0 {
                self.spectro_scroll_px_offset -= pixels_to_advance as f32;
                let write_width = pixels_to_advance.min(data_width);
                self.shift_spectrogram_ring_left(write_width);
                self.draw_spectrogram_ring_column(column, width - write_width, width);
                changed = true;
            }

            self.last_spectrogram_snapshot_time
                .set(column.snapshot.time);
        }

        self.last_spectrogram_columns.set(spec.columns.len());
        changed
    }

    fn update_spectrogram_ring_texture(
        &mut self,
        ctx: &egui::Context,
        spec: &AudioSpectrogram,
        width: usize,
        height: usize,
    ) {
        let changed = if self.spectrogram_ring_needs_reset(spec, width, height) {
            self.reset_spectrogram_ring_cache(spec, width, height);
            true
        } else {
            self.advance_spectrogram_ring(spec)
        };

        if self.spectrogram_texture_handle.is_none() {
            let texture = ctx.load_texture(
                "spectrogram",
                self.spectrogram_image_buffer.clone(),
                egui::TextureOptions::NEAREST,
            );
            self.spectrogram_texture_handle = Some(texture);
        } else if changed {
            if let Some(handle) = &mut self.spectrogram_texture_handle {
                handle.set(
                    self.spectrogram_image_buffer.clone(),
                    egui::TextureOptions::NEAREST,
                );
            }
        }
    }

    fn render_choose_audio_device_popup(
        &mut self,
        ctx: &egui::Context,
        selected_device: &mut Option<String>,
    ) {
        if !self.set_audio_device_popup_open {
            return;
        }

        const NONE_LABEL: &str = "None";

        let mut options = self.available_audio_devices.clone();
        debug_assert!(!options.contains(&NONE_LABEL.to_string()));
        options.push(NONE_LABEL.to_string());

        let (new_device, changed) = components::selection_dialog(
            ctx,
            options,
            match selected_device.clone() {
                Some(val) => val,
                None => NONE_LABEL.to_string(),
            },
            &mut self.set_audio_device_popup_open,
            "Select Audio Device".to_string(),
        );

        if changed {
            *selected_device = match new_device.as_str() {
                NONE_LABEL => None,
                other => Some(other.to_string()),
            };
        }
    }

    pub fn audio_ui(&mut self, ui: &mut egui::Ui, ctx: &egui::Context) {
        if self.audio_info_dialog_open {
            let debug_data = {
                let spec = self.data.state.audio_spectrogram.read().unwrap();
                spec.columns
                    .back()
                    .map(|col| col.debug_data.clone())
                    .unwrap_or_default()
            };
            let (mut auto_weight, mut bass_low_value, mut bass_high_value) = {
                let params = self.data.state.audio_params.read().unwrap();
                (
                    params.auto_weight,
                    params.bass_freq_low,
                    params.bass_freq_high,
                )
            };

            components::Dialog::new("Info".to_string(), vec2(540.0, 360.0))
                .moveable()
                .show(ctx, |ui| {
                    ui.horizontal(|ui| {
                        ui.label("Auto Weight");
                        if components::Switch::new(&mut auto_weight).ui(ui).changed() {
                            let mut params = self.data.state.audio_params.write().unwrap();
                            params.auto_weight = auto_weight;
                            params.changed = true;
                        }
                    });

                    ui.add_space(6.0);

                    ui.horizontal(|ui| {
                        let bass_low_before = bass_low_value;
                        let bass_high_before = bass_high_value;

                        ui.label(RichText::new("Bass Low (Hz)").size(ButtonSize::Medium.dim().1));
                        self.bass_low_numberpad.ui(ui, &mut bass_low_value);

                        ui.add_space(40.0);

                        ui.label(RichText::new("Bass High (Hz)").size(ButtonSize::Medium.dim().1));
                        self.bass_high_numberpad.ui(ui, &mut bass_high_value);

                        if bass_low_value > bass_high_value {
                            mem::swap(&mut bass_low_value, &mut bass_high_value);
                        }

                        if bass_low_value != bass_low_before || bass_high_value != bass_high_before
                        {
                            let mut params = self.data.state.audio_params.write().unwrap();
                            params.bass_freq_low = bass_low_value;
                            params.bass_freq_high = bass_high_value;
                            params.changed = true;
                        }
                    });

                    ui.add_space(8.0);

                    for (idx, band) in self.band_energy_graphs.iter().enumerate() {
                        let graph_height = 100.0;
                        let metrics_width = 140.0;
                        let graph_width = (ui.available_width() - metrics_width - 12.0).max(140.0);

                        ui.horizontal(|ui| {
                            let (response, painter) = ui.allocate_painter(
                                egui::vec2(graph_width, graph_height),
                                egui::Sense::hover(),
                            );
                            band.draw(painter, response.rect);

                            ui.add_space(8.0);

                            ui.vertical(|ui| {
                                ui.set_width(metrics_width);
                                ui.label(format!(
                                    "Peakiness: {:.2}",
                                    debug_data.band_onset_peakiness[idx]
                                ));
                                ui.label(format!(
                                    "Periodicity: {:.2}",
                                    debug_data.band_onset_periodicity[idx]
                                ));
                                ui.label(format!(
                                    "Trans. Stre: {:.2}",
                                    debug_data.band_transient_strength[idx]
                                ));
                                ui.label(format!("Weight: {:.2}", debug_data.band_weights[idx]));
                            });
                        });
                    }
                });
        }

        ui.horizontal(|ui| {
            let total_width = ui.available_width();
            let graph_panel_width = total_width / 3.0;

            ui.allocate_ui_with_layout(
                egui::vec2(ui.available_width(), ui.available_height()),
                egui::Layout::top_down(egui::Align::LEFT),
                |ui| {
                    let mut selected_device =
                        self.data.state.audio.read().unwrap().device_name.clone();

                    let selected_device_label =
                        selected_device.clone().unwrap_or_else(|| "N/A".to_string());

                    let before = selected_device.clone();

                    ui.horizontal_centered(|ui| {
                        ui.set_min_height(ButtonSize::Medium.dim().0.y);

                        if components::button(ui, false, "Change Device", ButtonSize::Medium) {
                            self.set_audio_device_popup_open = true;
                        }

                        ui.separator();

                        ui.label(RichText::new("Current Input:").size(ButtonSize::Medium.dim().1));
                        ui.label(
                            RichText::new(selected_device_label)
                                .size(ButtonSize::Medium.dim().1)
                                .color(Color32::LIGHT_RED),
                        );

                        ui.separator();

                        if components::button(ui, false, "Info", ButtonSize::Medium) {
                            self.audio_info_dialog_open = !self.audio_info_dialog_open;
                        }
                    });

                    self.render_choose_audio_device_popup(ctx, &mut selected_device);

                    if selected_device != before {
                        let new_dev = selected_device.map(|d| utils::device_from_name(d).unwrap());
                        self.data
                            .from_frontend_sender
                            .send(FromFrontend::SelectInputDevice(new_dev.clone()))
                            .unwrap();

                        let mut config_mut = self.data.config.lock().unwrap();

                        config_mut.default_audio_device = new_dev.map(|d| d.name().unwrap());

                        let path = PathBuf::from_str(&self.data.config_path).unwrap();
                        config::write_config(path, config_mut.clone()).unwrap();
                    }

                    // --- Live Spectrogram (show last 60s, no scrolling) ---
                    ui.add_space(6.0);
                    let spec_height = 160.0; // compact height
                    let spec_width = ui.available_width();

                    let spec = {
                        let spec = self.data.state.audio_spectrogram.read().unwrap();
                        spec.clone()
                    };
                    let params = self.data.state.audio_params.read().unwrap();
                    let mut volume_value = params.volume as f32;
                    let mut gate_value = params.gate as f32;
                    let mut boost_value = params.boost.unwrap_or(0) as f32;
                    let mut auto_calibrate = params.auto_calibrate;
                    // let mut filterbank_value = params.filterbank;
                    // let mut use_dp_tracking = params.use_dynamic_programming_beat;
                    drop(params);

                    {
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
                            self.update_spectrogram_ring_texture(
                                ctx,
                                &spec,
                                spec_width as usize,
                                spec_height as usize,
                            );
                            ui.image(self.spectrogram_texture_handle.as_ref().unwrap());
                        }
                    }

                    {
                        ui.horizontal(|ui| {
                            ui.add_space(5.0);

                            if ui
                                .add(
                                    HFader::new(&mut volume_value, 1.0..=200.0)
                                        .with_label("Volume"),
                                )
                                .changed()
                            {
                                let mut params = self.data.state.audio_params.write().unwrap();
                                params.volume = volume_value as u8;
                                params.changed = true;
                            }

                            ui.add_space(50.0);

                            if ui
                                .add(HFader::new(&mut gate_value, 0.0..=100.0).with_label("Gate"))
                                .changed()
                            {
                                let mut params = self.data.state.audio_params.write().unwrap();
                                params.gate = gate_value as u8;
                                params.changed = true;
                            }

                            ui.add_space(50.0);

                            if ui
                                .add(HFader::new(&mut boost_value, 0.0..=255.0).with_label("Boost"))
                                .changed()
                            {
                                let mut params = self.data.state.audio_params.write().unwrap();
                                let v = boost_value as u8;
                                params.boost = match v {
                                    0 => None,
                                    v => Some(v),
                                };
                                params.changed = true;
                            }

                            ui.add_space(50.0);

                            if components::Switch::new(&mut auto_calibrate)
                                .ui(ui)
                                .changed()
                            {
                                let mut params = self.data.state.audio_params.write().unwrap();
                                params.auto_calibrate = auto_calibrate;
                                params.changed = true;
                            }
                        });
                    }

                    // Set larger graph height
                    let graph_height = 105.0;
                    let graph_width = graph_panel_width - 5.0;
                    let padding = 0.0;

                    debug_assert!(graph_width > 0.0);

                    ui.separator();

                    ui.horizontal(|ui| {
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

                            // TODO: add another graph here.
                            // ui.add_space(padding);
                            // let (response_bass_avg_short, painter_bass_avg_short) = ui
                            //     .allocate_painter(
                            //         egui::vec2(graph_width, graph_height),
                            //         egui::Sense::hover(),
                            //     );
                            // self.bass_avg_short_graph
                            //     .draw(painter_bass_avg_short, response_bass_avg_short.rect);
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

                                            let color = if self.collector_snapshot.beat_trigger {
                                                Color32::LIGHT_GREEN
                                            } else {
                                                Color32::BLACK
                                            };

                                            let center = bpm_rect.rect.center();
                                            let radius =
                                                bpm_rect.rect.width().min(bpm_rect.rect.height())
                                                    * 0.5;

                                            painter_bpm_indicator
                                                .circle_filled(center, radius, color);
                                        }
                                    });

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
                                            let (beat_rect, beat_painter) = ui.allocate_painter(
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
                    });
                },
            );
        });
    }
}
