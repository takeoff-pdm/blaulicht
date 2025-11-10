use crate::app::components::ButtonSize;
use crate::app::{components, theme, AppPage, BlaulichtApp, PopupSpec};
use crate::audio::defs::AudioThreadControlSignal;
use crate::dmx::{DmxEngine, EngineState};
use crate::msg::FromFrontend;
use crate::state::{AudioSpectrogram, AudioSpectrogramColumn};
use crate::{config, utils};
use crate::{msg::SystemMessage, state::AppStateWrapper};
use blaulicht_shared::{
    CollectedAudioSnapshot, ControlEvent, ControlEventMessage, EventOriginator, LogLevel,
    PluginUiEvent,
};
use cpal::traits::DeviceTrait;
use crossbeam_channel::TryRecvError;
use egui::mutex::RwLockWriteGuard;
use egui::{
    vec2, Color32, Context, CornerRadius, FontId, Frame, Margin, Painter, Rect, RichText, Sense,
    Stroke, TextStyle, ThemePreference, Ui, Vec2,
};
use egui_file::FileDialog;
use noise::utils::Color;
use std::ffi::OsStr;
use std::fs::{self, File};
use std::io::Read;
use std::mem;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::str::FromStr;
use std::sync::RwLockReadGuard;
use std::time::{Duration, Instant};
use strum::IntoEnumIterator;

// TODO: include snapshot in graphs

impl BlaulichtApp {
    // Create texture once when data changes (not every frame!)
    fn create_spectrogram_texture(
        ctx: &egui::Context,
        spec: RwLockReadGuard<'_, AudioSpectrogram>,
        width: usize,
        height: usize,
    ) -> egui::TextureHandle {
        let mut pixels = vec![Color32::BLACK; width * height];
        let start = Instant::now();

        let total_cols = spec.max_columns;
        let current_cols = spec.columns.len();

        // How many screen pixels should each logical column get?
        let pixels_per_column = width as f32 / total_cols as f32;

        // println!(
        //     "w = {width} | current {} / max {} | pixels_per_col = {}",
        //     current_cols, total_cols, pixels_per_column
        // );

        let mut chunks_cont = spec.columns.clone();
        let columns_data = chunks_cont.make_contiguous();

        // Downsample if we have more columns than pixels
        let columns: Vec<AudioSpectrogramColumn> = if pixels_per_column >= 1.0 {
            // Stretching: each column gets multiple pixels
            columns_data.iter().cloned().collect()
        } else {
            // Compressing: multiple columns per pixel - need to average
            let cols_per_pixel = (1.0 / pixels_per_column).ceil() as usize;

            let coll: Vec<_> = columns_data
                .chunks(cols_per_pixel)
                .map(|chunks| {
                    if chunks.is_empty() {
                        return AudioSpectrogramColumn {
                            samples: vec![],
                            snapshot: CollectedAudioSnapshot::default(),
                        };
                    }
                    let bucket_count = chunks[0].samples.len();
                    let mut averaged = AudioSpectrogramColumn {
                        samples: Vec::with_capacity(bucket_count),
                        snapshot: CollectedAudioSnapshot::default(),
                    };

                    let mut bass_avg_short_AVG = 0;
                    for c in chunks {
                        bass_avg_short_AVG += averaged.snapshot.bass_avg_short as u16;
                    }

                    bass_avg_short_AVG /= chunks.len() as u16;

                    averaged.snapshot.bass_avg_short = bass_avg_short_AVG as u8;

                    for bucket_idx in 0..bucket_count {
                        let sum: u32 = chunks
                            .iter()
                            .map(|col| col.samples[bucket_idx] as u32)
                            .sum();
                        let avg = (sum / chunks.len() as u32) as u8;
                        averaged.samples.push(avg);
                    }

                    // average the snapshot

                    averaged
                })
                .collect();

            coll
        };

        // Now draw with proper scaling
        let col_width = if pixels_per_column >= 1.0 {
            pixels_per_column.floor() as usize
        } else {
            1 // One pixel per averaged column
        };

        let last_bpm_marker_time = 0;

        for (idx, col) in columns.iter().enumerate() {
            let col_start_x = idx * col_width;
            let col_end_x = ((idx + 1) * col_width).min(width);

            let bucket_height = height as f32 / col.samples.len() as f32;

            for (bidx, &bucket) in col.samples.iter().rev().enumerate() {
                let y_min = (bidx as f32 * bucket_height) as usize;
                let y_max = ((bidx + 1) as f32 * bucket_height).min(height as f32) as usize;

                let bucket_color = Self::spectrogram_color(bucket);

                for x in col_start_x..col_end_x {
                    for y in y_min..y_max {
                        if x < width && y < height {
                            pixels[y * width + x] = bucket_color;
                        }
                    }
                }

                if col.snapshot.bass_avg_short == 255 {
                    for y in 0..10 {
                        pixels[y * width + col_start_x] = Color32::MAGENTA;
                    }
                }

                // IF there should be a beat marker, insert it here.
                // if idx % 10 == 0 {
                if col.snapshot.beat_trigger {
                    for y in 0..height {
                        pixels[y * width + col_start_x] = Color32::RED;
                    }
                }
            }
        }

        let image = egui::ColorImage::new([width, height], pixels);
        ctx.load_texture("spectrogram", image, egui::TextureOptions::NEAREST)
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

    pub fn audio_ui(&mut self, ui: &mut egui::Ui, ctx: &egui::Context) {
        // Main content area with graphs panel
        ui.horizontal(|ui| {
            // Left content area (3/4 width)
            let total_width = ui.available_width();
            let graph_panel_width = total_width / 3.0;
            // let main_panel_width = total_width - graph_panel_width - 16.0; // 16px for separator

            // ui.vertical(|ui| {
            //     ui.set_width(main_panel_width);
            //     ui.heading("Main Content");
            //     ui.label("This is the main content area taking up 3/4 of the width.");
            //     ui.add_space(20.0);
            //     ui.label("You can put your main application content here.");
            // });

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

                        ui.label("Audio");

                        ui.separator();

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
                    });

                    if self.set_audio_device_popup_open {
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
                        );

                        if changed {
                            selected_device = match new_device.as_str() {
                                NONE_LABEL => None,
                                other => Some(other.to_string()),
                            };
                        }
                    }

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
                    let spec_height = 180.0; // compact height
                    let spec_width = ui.available_width();

                    let spec = self.data.state.audio_spectrogram.read().unwrap();
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
                        let texture = Self::create_spectrogram_texture(
                            ctx,
                            spec,
                            spec_width as usize,
                            spec_height as usize,
                        );
                        ui.image(&texture);
                    }

                    // Set larger graph height
                    let graph_height = 115.0;
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

                            ui.add_space(padding);
                            let (response_bass_avg_short, painter_bass_avg_short) = ui
                                .allocate_painter(
                                    egui::vec2(graph_width, graph_height),
                                    egui::Sense::hover(),
                                );
                            self.bass_avg_short_graph
                                .draw(painter_bass_avg_short, response_bass_avg_short.rect);
                        });

                        ui.vertical(|ui| {
                            let (response_bpm, painter_bpm) = ui.allocate_painter(
                                egui::vec2(graph_width, graph_height),
                                egui::Sense::hover(),
                            );
                            self.bpm_graph.draw(painter_bpm, response_bpm.rect);

                            ui.add_space(padding);
                            let (response_time_between_beats, painter_time_between_beats) = ui
                                .allocate_painter(
                                    egui::vec2(graph_width, graph_height),
                                    egui::Sense::hover(),
                                );
                            self.time_between_beats_graph
                                .draw(painter_time_between_beats, response_time_between_beats.rect);
                        });
                    });
                },
            );
        });
    }
}
