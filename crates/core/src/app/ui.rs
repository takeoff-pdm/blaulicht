use crate::app::{components, theme, BlaulichtApp, ExternalScreen, PopupSpec};
use crate::config;
use crate::state::AppStateWrapper;
use crate::state::ScreenId;

// #[cfg(feature = "audio")]
// use cpal::traits::DeviceTrait;

use egui::{Context, Frame};
use std::hash::{Hash, Hasher};
use std::time::{Duration, Instant};

const BEAT_MARKER_TRIGGER_DEBOUNCE_FRACTION: f32 = 0.90;

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
        desktop_mode: bool,
        showfile_home: Option<std::path::PathBuf>,
        external_screens: Vec<(usize, usize)>,
    ) -> Self {
        let mut app = Self::new_default(state, desktop_mode, showfile_home);

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

        // Create external window if specified in cli args.
        // TODO: add clap CLI parsing.
        app.external_screens = external_screens
            .iter()
            .map(|(dim_x, dim_y)| ExternalScreen::new(egui::vec2(*dim_x as f32, *dim_y as f32)))
            .collect();

        let startup_ui_state = {
            let config_guard = app.data.config.lock().unwrap();
            config_guard
                .last_open_showfile
                .clone()
                .and_then(|path| config::read_showfile_ui_state(path).ok().flatten())
        };

        if let Some(ui_state) = startup_ui_state {
            app.apply_showfile_ui_state(ui_state);
        }

        app.sync_external_screen_infos();

        app
    }
}

impl eframe::App for BlaulichtApp {
    /// Called each time the UI needs repainting, which may be many times per second.
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        let ctx = ui.ctx();
        // Continuous rendering - always request repaints
        // TODO: try to get actual screen FPS from X11 / wayland
        ctx.request_repaint_after(std::time::Duration::from_millis(16)); // ~60 FPS

        self.handle_events();
        self.tick_autosave();

        if ctx.style().visuals.dark_mode {
            theme::set_theme(ctx, theme::BLUE);
        }

        if self.render_init_popup(ctx) {
            ctx.request_repaint();
            return;
        }

        self.render_popup(ctx);

        self.debug_dialog(ctx);

        // Update all graphs with current data
        {
            let audio_data = self.data.state.audio_spectrogram.read().unwrap();
            let audio_output = audio_data.current_output();
            let audio_snapshot = audio_output.snapshot;

            self.volume_graph.update(audio_snapshot.volume as i32);
            // self.bass_derivative_graph
            //     .update(audio_snapshot.bass_slope as i32);
            self.beat_volume_graph
                .update(audio_snapshot.beat_volume as i32);
            self.bass_graph.update(audio_snapshot.bass as i32);
            self.bass_avg_graph.update(audio_snapshot.bass_avg as i32);
            // self.bass_avg_short_graph
            //     .update(audio_snapshot.bass_avg_short as i32);
            if audio_snapshot.beat_trigger
                && audio_snapshot.time != self.last_beat_marker_snapshot_time
            {
                let now = Instant::now();
                let beat_interval =
                    Duration::from_millis(audio_snapshot.time_between_beats_millis.max(1) as u64);
                let elapsed_since_anchor =
                    now.saturating_duration_since(self.beat_marker_anchor_instant);
                let debounce_interval = std::cmp::min(self.beat_marker_interval, beat_interval);
                let accept_trigger = !self.beat_marker_has_beat
                    || elapsed_since_anchor
                        >= debounce_interval.mul_f32(BEAT_MARKER_TRIGGER_DEBOUNCE_FRACTION);

                if accept_trigger {
                    if self.beat_marker_has_beat {
                        let elapsed_beats = (elapsed_since_anchor.as_secs_f32()
                            / self.beat_marker_interval.as_secs_f32())
                        .round()
                        .max(1.0) as usize;
                        self.beat_marker_index = (self.beat_marker_index + elapsed_beats) % 4;
                    } else {
                        self.beat_marker_index = 0;
                    }

                    self.beat_marker_has_beat = true;
                    self.beat_marker_anchor_instant = now;
                    self.beat_marker_interval = beat_interval;
                }

                self.last_beat_marker_snapshot_time = audio_snapshot.time;
            }
            self.collector_snapshot = audio_snapshot;

            for (i, value) in audio_output.debug_data.band_energies.iter().enumerate() {
                self.band_energy_graphs[i].update(*value as i32);
            }
        }

        // Loop & plugin tick speed over time.
        self.loop_speed_graph
            .update(self.tick_speeds.loop_total.as_millis() as i32);
        self.plugin_speed_graph
            .update(self.tick_speeds.plugins.as_micros() as i32);

        match self.desktop_mode {
            true => {
                let mut screen = self.main_screen_desktop_mode.clone();
                self.draw_external_screen_contents(ctx, ScreenId::MAIN, &mut screen);
                // TODO: this is completely broken.
                self.main_screen_desktop_mode = screen;
            }
            false => {
                self.render_main_screen(ctx);
            }
        }
    }
}

impl BlaulichtApp {
    fn render_main_screen(&mut self, ctx: &Context) {
        // Navbar
        self.show_navbar(ctx);

        let page = self.navbar.page();

        // TODO: experimental -> add back later.
        egui::TopBottomPanel::bottom("horizontal_nav")
            .resizable(false)
            .frame(Frame::NONE)
            .show_separator_line(true)
            .show(ctx, |ui| {
                components::horizontal_nav_for_tab(ui, page);
            });

        egui::CentralPanel::default().show(ctx, |ui| {
            // // Update animation time for continuous rendering
            self.frame_count += 1; // TODO: when does this overflow?
            self.animation_time += 0.016; // 16ms ~= 60fps

            self.page_content_based_on_tab(page, ui, ctx, ScreenId::MAIN);
        });

        // Render per-plugin UI windows.
        self.render_plugin_ui(ctx, ScreenId::MAIN);

        for i in 0..self.external_screens.len() {
            self.drive_external_screen(ctx, i);
        }
    }

    pub fn logs_ui(&mut self, ui: &mut egui::Ui, ctx: &Context) {
        self.log_window.draw(ctx, ui);
    }

    fn tick_autosave(&mut self) {
        const AUTOSAVE_INTERVAL: Duration = Duration::from_secs(30);

        if self.last_autosave_check.elapsed() < AUTOSAVE_INTERVAL {
            return;
        }
        self.last_autosave_check = Instant::now();

        let path = {
            let config = self.data.config.lock().unwrap();
            match config.last_open_showfile.clone() {
                Some(p) => p,
                None => return,
            }
        };

        let showfile = self.build_showfile();
        let serialized = match serde_json::to_string_pretty(&showfile) {
            Ok(s) => s,
            Err(_) => return,
        };

        let mut hasher = std::hash::DefaultHasher::new();
        serialized.hash(&mut hasher);
        let hash = hasher.finish();

        if hash == self.last_autosave_hash {
            return;
        }
        self.last_autosave_hash = hash;
        self.last_save_time = Some(Instant::now());

        std::thread::spawn(move || {
            let _ = std::fs::write(&path, &serialized);
        });
    }

    fn build_showfile(&self) -> config::CoreShowfile {
        use blaulicht_shared::{SaveEngineState, ShowfileArtNetReceiver, ShowfileArtNetState};

        let engine_snapshot = { self.data.state.dmx_engine.read().unwrap().0.clone() };
        let plugin_state = { self.data.state.plugin_state_storage.lock().unwrap().clone() };
        let artnet_state = {
            let artnet_output = self.data.state.artnet_output.read().unwrap();
            ShowfileArtNetState {
                receivers: artnet_output
                    .receivers
                    .iter()
                    .filter(|receiver| receiver.owner_plugin_id.is_none())
                    .map(|receiver| ShowfileArtNetReceiver {
                        address: receiver.address,
                        enabled: receiver.enabled,
                    })
                    .collect(),
            }
        };

        config::CoreShowfile {
            engine: SaveEngineState::from(engine_snapshot),
            artnet: artnet_state,
            plugin_state,
            ui: Some(self.showfile_ui_state()),
        }
    }
}
