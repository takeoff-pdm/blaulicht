use crate::app::components::ButtonSize;
use crate::app::{components, theme, AppPage, BlaulichtApp, PopupSpec};
use crate::{msg::SystemMessage, state::AppStateWrapper};
use blaulicht_shared::{
    ControlEvent, ControlEventMessage, EventOriginator, LogLevel, PluginUiEvent,
};

#[cfg(feature = "audio")]
use cpal::traits::DeviceTrait;

use crossbeam_channel::TryRecvError;
use egui::{vec2, Context, FontId, RichText};
use strum::IntoEnumIterator;

pub enum FileDialogOpenOrigin {
    Save,
    Load,
}

impl Drop for BlaulichtApp {
    fn drop(&mut self) {
        // let mut consumers = self.data.to_frontend_consumers.lock().unwrap();
        // let removed = consumers.remove(UI_RECV_KEY);
        // debug_assert!(removed.is_some());
    }
}

impl BlaulichtApp {
    /// Called once before the first frame.
    pub fn new(
        cc: &eframe::CreationContext<'_>,
        state: AppStateWrapper,
        initial_popup: Option<PopupSpec>,
    ) -> Self {
        let mut app = Self::new_default(state);

        cc.egui_ctx.set_pixels_per_point(1.0);

        app.animation_time = 0.0;

        if let Some(p) = initial_popup {
            app.show_popup(p);
        }

        let mut fonts = egui::FontDefinitions::default();
        egui_phosphor::add_to_fonts(&mut fonts, egui_phosphor::Variant::Regular);
        blaulicht_assets::add_to_fonts(&mut fonts);
        egui_extras::install_image_loaders(&cc.egui_ctx);

        cc.egui_ctx.set_fonts(fonts);

        app
    }
}

impl eframe::App for BlaulichtApp {
    /// Called each time the UI needs repainting, which may be many times per second.
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        if ctx.style().visuals.dark_mode {
            theme::set_theme(ctx, theme::REKORDBOX);
        }

        if self.render_init_popup(ctx) {
            ctx.request_repaint();
            return;
        }
        self.render_popup(ctx);

        if self.system_ui_state.debug_open {
            egui::Window::new("Debug")
                .title_bar(false)
                .fixed_size(vec2(200.0, 50.0))
                .show(ctx, |ui| {
                    ui.set_width(200.0);
                    ui.set_height(50.0);
                    let dt = ctx.input(|i| i.stable_dt);
                    let fps = if dt > 0.0 { 1.0 / dt } else { 0.0 };
                    ui.label(
                        RichText::new(format!("FPS: {:.1}", fps)).font(FontId::monospace(24.0)),
                    );
                });
        }

        //
        // At the beginning, process any incoming state changes.
        //

        let mut empty = 0;
        loop {
            match self.data.event_bus_connection.try_recv() {
                Some(bus) => println!("bus: {:?}", bus),
                None => {
                    empty += 1;
                }
            }

            match self.data.system_message_receiver.try_recv() {
                Ok(sys) => match sys {
                    SystemMessage::Heartbeat(_) => {
                        self.last_heartbeat_frame = self.frame_count;
                    }
                    SystemMessage::Log(log_msg, level) => {
                        self.log_window
                            .add_log(level, log_msg, "System".to_string());
                    }
                    SystemMessage::WasmLog(wasm_log_body) => {
                        self.log_window.add_log(
                            wasm_log_body.level.clone(),
                            format!("PID: {} | {}", wasm_log_body.plugin_id, wasm_log_body.msg),
                            "WASM".to_string(),
                        );
                    }
                    SystemMessage::LoopSpeed(duration) => {
                        self.loop_speed = duration.as_micros() as usize;
                    }
                    SystemMessage::TickSpeed(duration) => {
                        self.tick_speed = duration.as_micros() as usize;
                    }
                    SystemMessage::AudioSelected(device) => {
                        // self.log_window.add_log(
                        //     LogLevel::Info,
                        //     format!("Audio device selected: {}", if device.is_some() { "Yes" } else { "No" }),
                        //     "Audio".to_string(),
                        // );
                    }
                    SystemMessage::AudioDevicesView(items) => {
                        // Check if number of devices changed.
                        //
                        if self.available_audio_devices.len() != items.len() {
                            self.log_window.add_log(
                                LogLevel::Debug,
                                format!("Available audio devices updated: {} devices", items.len()),
                                "Audio".to_string(),
                            );
                            let items_str = items
                                .into_iter()
                                .map(|(_, dev)| dev.name().unwrap())
                                .collect();
                            self.available_audio_devices = items_str;
                        }
                    }
                    SystemMessage::DMX(dmx_msg) => {
                        // self.log_window.add_log(
                        //     LogLevel::Info,
                        //     format!("DMX message: {:?}", dmx_msg),
                        //     "DMX".to_string(),
                        // );
                    }
                    SystemMessage::SavePluginState {
                        plugin_name,
                        state_data,
                    } => {}
                },
                Err(TryRecvError::Empty) => {
                    empty += 1;
                }
                Err(TryRecvError::Empty) => {}
                Err(TryRecvError::Disconnected) => {
                    unreachable!("CANNOT REACH")
                }
            }

            if empty >= 3 {
                break;
            }
        }

        // Update all graphs with current data
        {
            let audio_data = self.data.state.audio_spectrogram.read().unwrap();
            let audio_snapshot = audio_data.current_snapshot();

            self.volume_graph.update(audio_snapshot.volume as i32);
            self.volume_graph.update(audio_snapshot.volume as i32);
            self.beat_volume_graph
                .update(audio_snapshot.beat_volume as i32);
            self.bass_graph.update(audio_snapshot.bass as i32);
            self.bass_avg_graph.update(audio_snapshot.bass_avg as i32);
            self.bass_avg_short_graph
                .update(audio_snapshot.bass_avg_short as i32);
            self.collector_snapshot = audio_snapshot;
        }

        // Navbar
        self.navbar_ui(ctx);

        egui::CentralPanel::default().show(ctx, |ui| {
            // Continuous rendering - always request repaints
            ctx.request_repaint_after(std::time::Duration::from_millis(16)); // ~60 FPS

            // // Update animation time for continuous rendering
            self.frame_count += 1;
            self.animation_time += 0.016; // 16ms = 0.016 seconds

            // Page content based on selected tab
            match self.current_page {
                AppPage::Logs => {
                    self.logs_ui(ui, ctx);
                }
                AppPage::System => {
                    self.system_ui(ui, ctx);
                }
                AppPage::Audio => {
                    self.audio_ui(ui, ctx);
                }
                AppPage::FixturesSetup => {
                    self.fixtures_ui_setup(ui, ctx);
                }
                AppPage::View => {
                    self.view_ui(ui, ctx);
                }
                AppPage::ViewPerformance => {
                    self.view_perf_ui(ui, ctx);
                }
                AppPage::FixturesPerformance => {
                    self.fixtures_ui(ui, ctx);
                }
                AppPage::Animations => {
                    self.animations_ui(ctx, ui);
                }
            }
        });

        // Render per-plugin UI windows (visible across pages)
        self.render_plugin_ui(ctx);
    }
}

impl BlaulichtApp {
    fn logs_ui(&mut self, ui: &mut egui::Ui, ctx: &Context) {
        self.log_window.draw(ctx, ui);
    }
}
