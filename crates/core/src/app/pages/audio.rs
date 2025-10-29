use crate::app::components::ButtonSize;
use crate::app::{components, theme, AppPage, BlaulichtApp, PopupSpec};
use crate::audio::defs::AudioThreadControlSignal;
use crate::dmx::{DmxEngine, EngineState};
use crate::msg::FromFrontend;
use crate::{config, utils};
use crate::{msg::SystemMessage, state::AppStateWrapper};
use blaulicht_shared::{
    ControlEvent, ControlEventMessage, EventOriginator, LogLevel, PluginUiEvent,
};
use cpal::traits::DeviceTrait;
use crossbeam_channel::TryRecvError;
use egui::mutex::RwLockWriteGuard;
use egui::{
    Color32, Context, CornerRadius, FontId, Frame, Margin, Painter, Rect, RichText, Sense, Stroke,
    ThemePreference, Ui, Vec2,
};
use egui_file::FileDialog;
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

impl BlaulichtApp {
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
                    let spectro_height = 180.0; // compact height
                    let spec_width = ui.available_width();
                    let (spec_resp, spec_painter) = ui.allocate_painter(
                        egui::vec2(spec_width, spectro_height),
                        egui::Sense::hover(),
                    );
                    let spec_rect = spec_resp.rect;
                    // Background
                    spec_painter.rect_filled(spec_rect, 0.0, Color32::from_rgb(10, 10, 10));

                    let spec = self.data.state.audio_spectrogram.read().unwrap();
                    if spec.columns.is_empty() {
                        spec_painter.text(
                            spec_rect.center_top() + egui::vec2(0.0, 6.0),
                            egui::Align2::CENTER_TOP,
                            "Waiting for audio…",
                            egui::FontId::proportional(12.0),
                            Color32::GRAY,
                        );
                    } else {
                        let bins = spec.bin_count.max(1);
                        let cols = spec.columns.len();
                        let refresh_hz = {
                            self.data
                                .config
                                .lock()
                                .unwrap()
                                .spectrogram_refresh_hz
                                .max(1) as usize
                        };
                        let want_cols = refresh_hz * 60; // last 60 seconds
                        let draw_cols = cols.min(want_cols).max(1);
                        let col_w = (spec_rect.width() / draw_cols as f32).max(1.0);
                        let row_h = (spec_rect.height() / bins as f32).max(1.0);

                        let start_idx = cols.saturating_sub(draw_cols);
                        for (j, col) in spec.columns.iter().skip(start_idx).enumerate() {
                            let x0 = spec_rect.left() + j as f32 * col_w;
                            let x1 = x0 + col_w;
                            let mut y = spec_rect.bottom();
                            for v in col.iter().take(bins) {
                                let y1 = y;
                                let y0 = (y1 - row_h).max(spec_rect.top());
                                let intensity = *v as f32 / 255.0;
                                let color = if intensity <= 0.25 {
                                    // dark blue -> cyan
                                    let t = (intensity / 0.25).clamp(0.0, 1.0);
                                    let r: u8 = 0;
                                    let g = (180.0 * t) as u8;
                                    let b = (70.0 + 185.0 * t) as u8;
                                    Color32::from_rgb(r, g, b)
                                } else if intensity <= 0.5 {
                                    // cyan -> green
                                    let t = ((intensity - 0.25) / 0.25).clamp(0.0, 1.0);
                                    let r: u8 = 0;
                                    let g = (180.0 + 75.0 * t) as u8;
                                    let b = (255.0 * (1.0 - t)) as u8;
                                    Color32::from_rgb(r, g, b)
                                } else if intensity <= 0.75 {
                                    // green -> yellow
                                    let t = ((intensity - 0.5) / 0.25).clamp(0.0, 1.0);
                                    let r = (255.0 * t) as u8;
                                    let g = 255;
                                    let b = 0;
                                    Color32::from_rgb(r, g, b)
                                } else {
                                    // yellow -> red/white
                                    let t = ((intensity - 0.75) / 0.25).clamp(0.0, 1.0);
                                    let r = 255;
                                    let g = (255.0 * (1.0 - 0.6 * t)) as u8;
                                    let b = (64.0 * (1.0 - t)) as u8;
                                    Color32::from_rgb(r, g, b)
                                };
                                let rct = egui::Rect::from_min_max(
                                    egui::pos2(x0, y0),
                                    egui::pos2(x1, y1),
                                );
                                spec_painter.rect_filled(rct, 0.0, color);
                                y = y0;
                                if y <= spec_rect.top() {
                                    break;
                                }
                            }
                        }

                        // Border
                        spec_painter.rect_stroke(
                            spec_rect,
                            0.0,
                            Stroke::new(1.0, Color32::from_gray(50)),
                            egui::StrokeKind::Middle,
                        );
                    }

                    // Set larger graph height
                    let graph_height = 125.0;
                    let graph_width = graph_panel_width - 5.0;
                    let padding = 10.0;

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
                            let (response_beat_volume, painter_beat_volume) = ui.allocate_painter(
                                egui::vec2(graph_width, graph_height),
                                egui::Sense::hover(),
                            );
                            self.beat_volume_graph
                                .draw(painter_beat_volume, response_beat_volume.rect);

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
                        })
                    });
                },
            );
        });
    }
}
