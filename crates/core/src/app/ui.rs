use crate::app::{
    components::{self, ButtonSize, Dialog},
    theme, BlaulichtApp, ExternalScreen, GuardedLifecycleAction, PopupSpec, ShowfileSaveStatus,
};
use crate::config;
use crate::state::AppStateWrapper;
use crate::state::ScreenId;

// #[cfg(feature = "audio")]
// use cpal::traits::DeviceTrait;

use egui::Context;
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

        let startup_showfile_state = {
            let config_guard = app.data.config.lock().unwrap();
            config_guard
                .last_open_showfile
                .clone()
                .and_then(|path| config::read_showfile_ui_state(path).ok())
        };

        if let Some(showfile_state) = startup_showfile_state {
            app.visualizer_ui_state.load_stage(showfile_state.stage);
            if let Some(ui_state) = showfile_state.ui {
                app.apply_showfile_ui_state(ui_state);
            }
        }

        app.sync_external_screen_infos();
        app.reset_save_tracking();

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
        self.poll_save_completion(ctx);
        self.detect_showfile_dirty();
        self.tick_autosave();

        if ctx.input(|input| input.viewport().close_requested()) {
            if self.allow_close_once {
                self.allow_close_once = false;
            } else if matches!(
                self.save_status,
                ShowfileSaveStatus::Dirty
                    | ShowfileSaveStatus::Saving
                    | ShowfileSaveStatus::Failed(_)
            ) {
                ctx.send_viewport_cmd(egui::ViewportCommand::CancelClose);
                self.pending_lifecycle_action = Some(GuardedLifecycleAction::Quit);
            }
        }

        if ctx.style().visuals.dark_mode {
            theme::set_theme(ctx, theme::BLUE);
        }

        if self.render_init_popup(ctx) {
            ctx.request_repaint();
            return;
        }

        self.render_popup(ctx);
        self.render_save_status(ctx);
        self.render_lifecycle_guard(ctx);

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
            self.bass_avg_short_graph
                .update(audio_snapshot.bass_avg_short as i32);
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

        egui::CentralPanel::default().show(ctx, |ui| {
            // // Update animation time for continuous rendering
            self.frame_count += 1; // TODO: when does this overflow?
            self.animation_time += 0.016; // 16ms ~= 60fps

            let render_context = crate::app::page::PageRenderContext::new(
                crate::config::PageRenderMode::Default,
                ui.available_size(),
            );
            self.page_content_based_on_tab(page, ui, ctx, ScreenId::MAIN, render_context);
        });

        // Render per-plugin UI windows.
        self.render_plugin_ui(ctx, ScreenId::MAIN);

        let mut i = 0;
        while i < self.external_screens.len() {
            self.drive_external_screen(ctx, i);
            i += 1;
        }
    }

    pub fn logs_ui(
        &mut self,
        ui: &mut egui::Ui,
        ctx: &Context,
        _render_context: crate::app::page::PageRenderContext,
    ) {
        self.log_window.draw(ctx, ui);
    }

    fn tick_autosave(&mut self) {
        const AUTOSAVE_INTERVAL: Duration = Duration::from_secs(30);

        if self.last_autosave_check.elapsed() < AUTOSAVE_INTERVAL {
            return;
        }
        self.last_autosave_check = Instant::now();

        if matches!(
            self.save_status,
            ShowfileSaveStatus::Dirty | ShowfileSaveStatus::Failed(_)
        ) {
            self.request_showfile_save(false);
        }
    }

    pub(crate) fn mark_showfile_dirty(&mut self) {
        if self
            .data
            .config
            .lock()
            .unwrap()
            .last_open_showfile
            .is_none()
        {
            return;
        }
        if self.save_in_flight {
            self.save_pending = true;
        } else {
            self.save_status = ShowfileSaveStatus::Dirty;
        }
    }

    fn detect_showfile_dirty(&mut self) {
        const DIRTY_CHECK_INTERVAL: Duration = Duration::from_secs(2);
        if self.last_dirty_check.elapsed() < DIRTY_CHECK_INTERVAL || self.save_in_flight {
            return;
        }
        self.last_dirty_check = Instant::now();
        if self
            .data
            .config
            .lock()
            .unwrap()
            .last_open_showfile
            .is_none()
        {
            self.save_status = ShowfileSaveStatus::NoShowfile;
            return;
        }
        if let Ok((_, hash)) = self.serialized_showfile() {
            self.save_status = if hash == self.last_autosave_hash {
                ShowfileSaveStatus::Clean
            } else {
                ShowfileSaveStatus::Dirty
            };
        }
    }

    fn serialized_showfile(&self) -> Result<(String, u64), String> {
        let serialized = serde_json::to_string_pretty(&self.build_showfile())
            .map_err(|error| error.to_string())?;
        let mut hasher = std::hash::DefaultHasher::new();
        serialized.hash(&mut hasher);
        Ok((serialized, hasher.finish()))
    }

    pub(crate) fn request_showfile_save(&mut self, manual: bool) {
        let Some(path) = self.data.config.lock().unwrap().last_open_showfile.clone() else {
            self.save_status = ShowfileSaveStatus::NoShowfile;
            if manual {
                self.show_popup(PopupSpec::with_duration(
                    Duration::from_secs(2),
                    "No Showfile".to_string(),
                ));
            }
            return;
        };
        if self.save_in_flight {
            self.save_pending = true;
            return;
        }
        let (serialized, hash) = match self.serialized_showfile() {
            Ok(result) => result,
            Err(error) => {
                self.save_status = ShowfileSaveStatus::Failed(error);
                return;
            }
        };
        if hash == self.last_autosave_hash {
            self.save_status = ShowfileSaveStatus::Clean;
            return;
        }

        self.save_status = ShowfileSaveStatus::Saving;
        self.save_in_flight = true;
        let sender = self.save_completion_sender.clone();
        std::thread::spawn(move || {
            let result = config::write_atomic(&path, serialized.as_bytes())
                .map_err(|error| error.to_string());
            let _ = sender.send(crate::app::SaveCompletion {
                hash,
                manual,
                result,
            });
        });
    }

    fn poll_save_completion(&mut self, ctx: &Context) {
        while let Ok(completion) = self.save_completion_receiver.try_recv() {
            self.save_in_flight = false;
            match completion.result {
                Ok(()) => {
                    self.last_autosave_hash = completion.hash;
                    self.last_save_time = Some(Instant::now());
                    self.save_status = ShowfileSaveStatus::Clean;
                    let config = self.data.config.lock().unwrap().clone();
                    if let Err(error) = config::write_config(
                        std::path::PathBuf::from(&self.data.config_path),
                        config,
                    ) {
                        tracing::warn!("Failed to persist showfile path: {error}");
                    }
                    if completion.manual {
                        self.show_popup(PopupSpec::with_duration(
                            Duration::from_secs(2),
                            "Saved Showfile".to_string(),
                        ));
                    }
                }
                Err(error) => {
                    tracing::error!("Failed to save showfile: {error}");
                    self.save_status = ShowfileSaveStatus::Failed(error);
                    self.pending_lifecycle_action = self.lifecycle_action_after_save.take();
                }
            }
            if self.save_pending {
                self.save_pending = false;
                self.request_showfile_save(false);
                if !self.save_in_flight && matches!(self.save_status, ShowfileSaveStatus::Clean) {
                    if let Some(action) = self.lifecycle_action_after_save.take() {
                        self.execute_lifecycle_action(action, ctx);
                    }
                }
            } else if matches!(self.save_status, ShowfileSaveStatus::Clean) {
                if let Some(action) = self.lifecycle_action_after_save.take() {
                    self.execute_lifecycle_action(action, ctx);
                }
            }
        }
    }

    pub(crate) fn reset_save_tracking(&mut self) {
        self.last_autosave_hash = self
            .serialized_showfile()
            .map(|(_, hash)| hash)
            .unwrap_or(0);
        self.save_status = if self
            .data
            .config
            .lock()
            .unwrap()
            .last_open_showfile
            .is_some()
        {
            ShowfileSaveStatus::Clean
        } else {
            ShowfileSaveStatus::NoShowfile
        };
    }

    fn render_save_status(&self, ctx: &Context) {
        let (label, color) = match &self.save_status {
            ShowfileSaveStatus::NoShowfile | ShowfileSaveStatus::Clean => return,
            ShowfileSaveStatus::Dirty => ("Unsaved changes", egui::Color32::YELLOW),
            ShowfileSaveStatus::Saving => ("Saving...", egui::Color32::LIGHT_BLUE),
            ShowfileSaveStatus::Failed(_) => ("Save failed", egui::Color32::LIGHT_RED),
        };
        egui::Area::new(egui::Id::new("showfile_save_status"))
            .anchor(egui::Align2::RIGHT_TOP, egui::vec2(-12.0, 12.0))
            .order(egui::Order::Tooltip)
            .show(ctx, |ui| {
                egui::Frame::popup(ui.style()).show(ui, |ui| {
                    ui.set_min_width(140.0);
                    ui.set_max_width(320.0);
                    ui.add(
                        egui::Label::new(egui::RichText::new(label).color(color))
                            .wrap_mode(egui::TextWrapMode::Extend),
                    );
                    if let ShowfileSaveStatus::Failed(error) = &self.save_status {
                        ui.label(error);
                    }
                });
            });
    }

    pub(crate) fn request_guarded_lifecycle(
        &mut self,
        action: GuardedLifecycleAction,
        ctx: &Context,
    ) {
        if matches!(
            self.save_status,
            ShowfileSaveStatus::Dirty | ShowfileSaveStatus::Saving | ShowfileSaveStatus::Failed(_)
        ) {
            self.pending_lifecycle_action = Some(action);
        } else {
            self.execute_lifecycle_action(action, ctx);
        }
    }

    fn execute_lifecycle_action(&mut self, action: GuardedLifecycleAction, ctx: &Context) {
        match action {
            GuardedLifecycleAction::Quit => {
                self.data.state.set_grand_master_percent(100);
                self.allow_close_once = true;
                ctx.send_viewport_cmd(egui::ViewportCommand::Close);
            }
            GuardedLifecycleAction::Restart => std::process::exit(42),
        }
    }

    fn render_lifecycle_guard(&mut self, ctx: &Context) {
        let Some(action) = self.pending_lifecycle_action else {
            return;
        };
        let verb = match action {
            GuardedLifecycleAction::Quit => "quit",
            GuardedLifecycleAction::Restart => "restart",
        };
        let response = Dialog::new("Unsaved Changes".to_string(), egui::vec2(360.0, 160.0))
            .with_backdrop()
            .dismiss_on_backdrop()
            .show(ctx, |ui| {
                ui.heading("Unsaved changes");
                ui.label(format!("Save the current show before {verb}?"));
                ui.add_space(12.0);
                ui.horizontal(|ui| {
                    if components::button(ui, true, "Save", ButtonSize::Medium) {
                        self.pending_lifecycle_action = None;
                        self.lifecycle_action_after_save = Some(action);
                        self.request_showfile_save(true);
                    }
                    if components::button(ui, false, "Discard", ButtonSize::Medium) {
                        self.pending_lifecycle_action = None;
                        match action {
                            GuardedLifecycleAction::Quit => {
                                self.allow_close_once = true;
                                ctx.send_viewport_cmd(egui::ViewportCommand::Close)
                            }
                            GuardedLifecycleAction::Restart => std::process::exit(42),
                        }
                    }
                    if components::button(ui, false, "Cancel", ButtonSize::Medium) {
                        self.pending_lifecycle_action = None;
                    }
                });
            });
        if response.cancel_requested {
            self.pending_lifecycle_action = None;
        }
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
            format_version: config::current_showfile_version(),
            engine: SaveEngineState::from(engine_snapshot),
            artnet: artnet_state,
            plugin_state,
            stage: self.visualizer_ui_state.stage.clone(),
            ui: Some(self.showfile_ui_state()),
        }
    }
}
