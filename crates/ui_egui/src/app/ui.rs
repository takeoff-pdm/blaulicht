use crate::app::view_state::ScreenId;
use crate::app::{
    components::{self, ButtonSize, Dialog},
    theme, BlaulichtApp, ExternalScreen, GuardedLifecycleAction, PopupSpec,
};
use crate::client::AppClient;
use blaulicht_engine::client::EngineClient;
use blaulicht_engine::config::ShowfileUiState;
use blaulicht_engine::hub::{CommandOutcome, HostCommand};
use blaulicht_engine::persistence::SaveStatus;

// #[cfg(feature = "audio")]
// use cpal::traits::DeviceTrait;

use egui::Context;
use std::time::Duration;
use web_time::Instant;

const BEAT_MARKER_TRIGGER_DEBOUNCE_FRACTION: f32 = 0.90;

impl Drop for BlaulichtApp {
    fn drop(&mut self) {
        // let mut consumers = self.data.to_frontend_consumers.lock().unwrap();
        // let removed = consumers.remove(UI_RECV_KEY);
        // debug_assert!(removed.is_some());
    }
}

impl BlaulichtApp {
    /// Called once before the first frame.
    ///
    /// `config_dir` is where the file dialogs start when there is no
    /// showfile and no `showfile_home`.
    pub fn new(
        cc: &eframe::CreationContext<'_>,
        client: AppClient,
        initial_popup: Option<PopupSpec>,
        desktop_mode: bool,
        showfile_home: Option<std::path::PathBuf>,
        config_dir: Option<std::path::PathBuf>,
        external_screens: Vec<(usize, usize)>,
    ) -> Self {
        let mut app = Self::new_default(client, desktop_mode, showfile_home, config_dir);

        cc.egui_ctx.set_pixels_per_point(1.0);

        app.animation_time = 0.0;

        if let Some(p) = initial_popup {
            app.show_popup(p);
        }

        let mut fonts = egui::FontDefinitions::default();
        egui_phosphor::add_to_fonts(&mut fonts, egui_phosphor::Variant::Regular);
        // `egui_phosphor` only registers its glyphs in the proportional family, so an icon
        // painted with a monospace `FontId` (the icon-only buttons on the Art-Net and Screens
        // pages) fell back to tofu. Register it as a monospace fallback as well.
        if let Some(font_keys) = fonts.families.get_mut(&egui::FontFamily::Monospace) {
            font_keys.insert(1, "phosphor".into());
        }
        blaulicht_assets::add_to_fonts(&mut fonts);
        egui_extras::install_image_loaders(&cc.egui_ctx);

        cc.egui_ctx.set_fonts(fonts);

        // Create external window if specified in cli args.
        // TODO: add clap CLI parsing.
        app.external_screens = external_screens
            .iter()
            .map(|(dim_x, dim_y)| ExternalScreen::new(egui::vec2(*dim_x as f32, *dim_y as f32)))
            .collect();

        // The startup showfile's screen layouts arrive with the engine's
        // `ShowLoaded` (already queued on the client), in the first frame.
        app.publish_ui_mirror();

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
        self.publish_ui_mirror();
        // Give an open numberpad first priority. After rendering, any remaining
        // delta adjusts the recent fixture property instead.
        crate::app::components::publish_relative_adjustment(
            ctx,
            std::mem::take(&mut self.pending_numberpad_delta),
        );

        // The engine ran its shutdown hook (`HostCommand::Quit` / `Restart`,
        // from any client): close, so `main` can exit.
        if std::mem::take(&mut self.exit_requested) {
            self.allow_close_once = true;
            ctx.send_viewport_cmd(egui::ViewportCommand::Close);
        }

        if ctx.input(|input| input.viewport().close_requested()) {
            if self.allow_close_once {
                self.allow_close_once = false;
            } else if self.save_status.has_unsaved_changes() {
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
        self.client.with_spectrogram(|audio_data| {
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
            // Peak activation over every column since the last frame, so a
            // beat that lands between two UI frames still shows up.
            let mut model_peak: Option<(f32, f32)> = None;
            for column in audio_data
                .columns
                .iter()
                .filter(|c| c.snapshot.time > self.last_beat_model_snapshot_time)
            {
                if let Some(model) = &column.debug_data.beat_tracker.model {
                    let (beat, downbeat) = model_peak.unwrap_or_default();
                    model_peak = Some((
                        beat.max(model.beat_peak),
                        downbeat.max(model.downbeat_peak),
                    ));
                }
            }
            if let Some(last) = audio_data.columns.back() {
                self.last_beat_model_snapshot_time = last.snapshot.time;
            }
            if let Some((beat, downbeat)) = model_peak {
                self.beat_model_graphs[0].update((beat * 100.0).round() as i32);
                self.beat_model_graphs[1].update((downbeat * 100.0).round() as i32);
            }
        });

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
        self.finish_jog_adjustment(ctx);
        self.publish_ui_mirror();
        self.publish_frontend_section();
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
                blaulicht_engine::config::PageRenderMode::Default,
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

    /// Saves the show (the Save button): "Saved Showfile" once the engine
    /// wrote it, or why it couldn't.
    pub(crate) fn request_showfile_save(&mut self) {
        self.send_command(HostCommand::SaveShowfile, |app, _| {
            app.show_popup(PopupSpec::with_duration(
                Duration::from_secs(2),
                "Saved Showfile".to_string(),
            ));
        });
    }

    /// The engine loaded another show: drop view state that holds ids of the
    /// previous one, and take the screen layouts the showfile brought.
    pub(crate) fn apply_show_loaded(
        &mut self,
        generation: u64,
        frontend_section: Option<ShowfileUiState>,
    ) {
        if generation != self.show_generation {
            self.reset_show_view_state();
        }
        if let Some(section) = frontend_section {
            self.apply_showfile_ui_state(section);
        }
        self.show_generation = generation;
        // Publish ours for the new show even if it looks the same.
        self.published_frontend_section = None;
    }

    /// Drops view state that holds ids of the previous show: selections and
    /// dialog targets, the scene graph canvas, the visualizer's selection and
    /// undo. Sessions are repaired by the engine.
    fn reset_show_view_state(&mut self) {
        self.visualizer_ui_state.reset_stage_view();
        self.selected_fixture_group = None;
        self.setup_fixture_id = 0;
        self.delete_fixture = None;
        self.add_fixture_group = None;
        self.scene_dialog_target = 0;
        self.view_ui_state = Default::default();
        self.view_perf_ui_state = Default::default();
        self.animation_ui_state = Default::default();
        self.scene_graph_ui_state = Default::default();
        self.palette_ui_state = Default::default();
        self.setup_groups_ui = Default::default();
        self.setup_scenes_ui = Default::default();
        self.setup_palettes_ui = Default::default();
        self.programmer_ui_state.active_palettes_cache.clear();
        self.programmer_ui_state
            .scene_overview_animation_selection_edit = None;
    }

    /// Publishes the screen layouts (the showfile's `ui` section) when they
    /// changed, once the engine announced a show to publish them for.
    fn publish_frontend_section(&mut self) {
        if self.show_generation == 0 {
            return;
        }
        let section = self.showfile_ui_state();
        if self.published_frontend_section.as_ref() == Some(&section) {
            return;
        }
        self.published_frontend_section = Some(section.clone());
        self.client.send(HostCommand::SetFrontendSection {
            generation: self.show_generation,
            section,
        });
    }

    fn render_save_status(&self, ctx: &Context) {
        let (label, color) = match &self.save_status {
            SaveStatus::NoShowfile | SaveStatus::Clean => return,
            SaveStatus::Dirty => ("Unsaved changes", egui::Color32::YELLOW),
            SaveStatus::Saving => ("Saving...", egui::Color32::LIGHT_BLUE),
            SaveStatus::Failed(_) => ("Save failed", egui::Color32::LIGHT_RED),
        };
        const DOT_RADIUS: f32 = 4.0;
        egui::Area::new(egui::Id::new("showfile_save_status"))
            .anchor(egui::Align2::RIGHT_TOP, egui::vec2(-6.0, 6.0))
            .order(egui::Order::Tooltip)
            .show(ctx, |ui| {
                let (rect, response) = ui
                    .allocate_exact_size(egui::Vec2::splat(DOT_RADIUS * 2.0), egui::Sense::hover());
                ui.painter().circle_filled(rect.center(), DOT_RADIUS, color);
                response.on_hover_text(match &self.save_status {
                    SaveStatus::Failed(error) => format!("{label}: {error}"),
                    _ => label.to_owned(),
                });
            });
    }

    pub(crate) fn request_guarded_lifecycle(&mut self, action: GuardedLifecycleAction) {
        if self.save_status.has_unsaved_changes() {
            self.pending_lifecycle_action = Some(action);
        } else {
            // Clean as far as the desk knows: the engine's final save
            // catches an edit made since its last check.
            self.send_lifecycle(action, true);
        }
    }

    /// Asks the engine to run its shutdown hook and exit (saving first if
    /// `save`). The window closes on `SystemMessage::Exiting`; if the save
    /// fails, the refusal shows and the guard dialog comes back.
    fn send_lifecycle(&mut self, action: GuardedLifecycleAction, save: bool) {
        let command = match action {
            GuardedLifecycleAction::Quit => HostCommand::Quit { save },
            GuardedLifecycleAction::Restart => HostCommand::Restart { save },
        };
        self.reply_handlers
            .send_then(&self.client, command, move |app, outcome| {
                if let CommandOutcome::Refused(why) = outcome {
                    app.show_refusal(why.clone());
                    app.pending_lifecycle_action = Some(action);
                }
            });
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
                        self.send_lifecycle(action, true);
                    }
                    if components::button(ui, false, "Discard", ButtonSize::Medium) {
                        self.pending_lifecycle_action = None;
                        self.send_lifecycle(action, false);
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
}
