use crate::app::components::{ButtonSize, HFader};
use crate::app::{components, BlaulichtApp};
use crate::audio::defs::AudioThreadControlSignal;
use crate::dmx::{DmxEngine, EngineState};
use crate::msg::FromFrontend;
use crate::{config, utils};
use crate::{msg::SystemMessage, state::AppStateWrapper};
use blaulicht_audio_engine::SpectrogramDisplayOptions;
use blaulicht_shared::{
    CollectedAudioSnapshot, ControlEvent, ControlEventMessage, EventOriginator, LogLevel,
    PluginUiEvent,
};
use cpal::traits::DeviceTrait;
use crossbeam_channel::TryRecvError;
use egui::mutex::RwLockWriteGuard;
use egui::{
    vec2, Checkbox, Color32, ComboBox, Context, CornerRadius, FontId, Frame, Margin, Painter, Rect,
    RichText, Sense, Stroke, TextStyle, ThemePreference, Ui, Vec2,
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
                    let params = self.data.state.audio_params.read().unwrap();
                    let mut gate_value = params.gate.unwrap_or(0) as f32;
                    let mut boost_value = params.boost.unwrap_or(0) as f32;
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
                            let image = components::create_spectrogram_image(
                                spec,
                                spec_width as usize,
                                spec_height as usize,
                                &SpectrogramDisplayOptions {
                                    include_bass_markers: true,
                                    include_beat_markers: true,
                                },
                            );

                            {}

                            let texture = ctx.load_texture(
                                "spectrogram",
                                image,
                                egui::TextureOptions::NEAREST,
                            );

                            ui.image(&texture);
                        }
                    }

                    {
                        if ui
                            .add(HFader::new(&mut gate_value, 0.0..=100.0).with_label("Gate"))
                            .changed()
                        {
                            let mut params = self.data.state.audio_params.write().unwrap();
                            let v = gate_value as u8;
                            params.gate = match v {
                                0 => None,
                                v => Some(v),
                            };
                        }

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
                        }
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
