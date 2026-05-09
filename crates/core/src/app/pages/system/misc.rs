use crate::{
    app::{
        components::{self, ButtonSize, Dialog},
        external_screen, BlaulichtApp, ExternalScreen, PopupSpec, pages::system,
    },
    audio::defs::AudioThreadControlSignal,
    msg::FromFrontend,
    state::{NUM_DMX_UNIVERSES, PluginOpenState, ScreenId},
};
use blaulicht_shared::{ControlEvent, ControlEventMessage, EventOriginator, MainUiEvent};
use egui::{Color32, Context, FontId, RichText, ThemePreference};
use std::{mem, path::Path, process::{self, Command}, time::Duration};

impl BlaulichtApp {
    fn render_confirm_shutdown_dialog(&mut self, ctx: &Context) {
        if !self.system_ui_state.confirm_shutdown_open {
            return;
        }

        Dialog::new("Confirm Shutdown".to_string(), egui::vec2(200.0, 100.0))
            .with_backdrop()
            .show(ctx, |ui| {
                let blink = ((self.animation_time * 4.0) as i32) % 2 == 0;

                ui.heading(
                    RichText::new("Confirm Shutdown")
                        .color(if blink {
                            Color32::RED
                        } else {
                            ui.visuals().text_color()
                        })
                        .strong(),
                );

                ui.add_space(12.0);

                ui.horizontal(|ui| {
                    if components::button(ui, false, "Confirm", ButtonSize::Large) {
                        let status = Command::new("/usr/bin/shutdown.sh")
                            .status()
                            .expect("Failed to execute shutdown command");

                        if status.success() {
                            tracing::info!("Shutdown command executed successfully.");
                        } else {
                            tracing::error!("Shutdown command failed!");
                        }

                        self.system_ui_state.confirm_shutdown_open = false;
                    }

                    if components::button(ui, true, "Cancel", ButtonSize::Large) {
                        self.system_ui_state.confirm_shutdown_open = false;
                    }
                });
            });
    }

    fn attach_external_screen(&mut self) {
        self.external_screens.push(ExternalScreen::default());
    }

    pub fn system_ui(&mut self, ui: &mut egui::Ui, ctx: &Context, screen_id: ScreenId) {
        self.render_confirm_shutdown_dialog(ctx);
        self.render_showfile_dialog(ctx);

        for universe in 0..NUM_DMX_UNIVERSES {
            self.render_dmx_dialog(ctx, universe);
        }
        self.render_artnet_dialog(ctx);
        self.render_midi_dialog(ctx);
        self.render_serial_dialog(ctx);
        self.render_plugin_popup(ctx, screen_id);

        let button_size = ButtonSize::Medium.with_width(110.0);
        const HEALTH_COLUMN_WIDTH: f32 = 136.0;

        ui.horizontal(|ui| {
            let default_item_spacing = ui.spacing().item_spacing;
            ui.spacing_mut().item_spacing.x = 0.0;
            let main_column_width = (ui.available_width() - HEALTH_COLUMN_WIDTH - 1.0).max(0.0);

            ui.vertical(|ui| {
                ui.spacing_mut().item_spacing = default_item_spacing;
                ui.set_width(main_column_width);
                ui.horizontal(|ui| {
                    let showfile = self
                        .data
                        .config
                        .lock()
                        .unwrap()
                        .last_open_showfile
                        .as_ref()
                        .map(|s| s.to_string_lossy().to_string())
                        .unwrap_or_else(|| "N/A".to_string());

                    ui.label("showfile:");

                    ui.label(
                        egui::RichText::new(showfile)
                            .strong()
                            .font(FontId::monospace(14.0)),
                    );
                });

                ui.add_space(3.0);

                self.render_showfile_buttons(ui, button_size);

                ui.add_space(15.0);
                ui.separator();
                ui.add_space(15.0);

                ui.horizontal(|ui| {
                    if components::button(
                        ui,
                        !ui.ctx().style().visuals.dark_mode,
                        "LIGHT",
                        button_size,
                    ) {
                        ui.ctx().set_theme(ThemePreference::Light);
                    }

                    if components::button(
                        ui,
                        ui.ctx().style().visuals.dark_mode,
                        "DARK",
                        button_size,
                    ) {
                        ui.ctx().set_theme(ThemePreference::Dark);
                    }
                });

                ui.add_space(15.0);
                ui.separator();
                ui.add_space(15.0);

                ui.horizontal(|ui| {
                    if components::button(ui, false, "Quit", button_size) {
                        // TODO: add protections against unsaved changes.
                        // TODO: use command quit function, both for quitting and restarting
                        ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                    }

                    if components::button(ui, false, "Restart", button_size) {
                        // TODO: should originate from deeper inside the code?
                        // TODO: add protections against unsaved changes.
                        // TODO: can we use the viewport command maybe?
                        process::exit(42);
                    }

                    if components::button(ui, false, "Shutdown", button_size) {
                        self.system_ui_state.confirm_shutdown_open = true;
                    }

                    if components::button(ui, self.system_ui_state.debug_open, "Debug", button_size)
                    {
                        self.system_ui_state.debug_open = !self.system_ui_state.debug_open;
                    }

                    if components::button(ui, false, "Add Screen", button_size) {
                        self.attach_external_screen();
                    }
                });

                // --- Loop Speed & Tick Speed Graphs ---
                ui.vertical(|ui| {
                    self.render_system_speed_graph(ui);

                    ui.add_space(8.0);

                    // --- Reload button ---
                    {
                        let signal = *self.data.state.mainloop_state.read().unwrap();

                        ui.label(format!("LOOP {:?}", signal));

                        let disabled = match signal {
                            AudioThreadControlSignal::CONTINUE => false,
                            AudioThreadControlSignal::ABORT
                            | AudioThreadControlSignal::ABORTED
                            | AudioThreadControlSignal::CRASHED
                            | AudioThreadControlSignal::RELOAD => true,
                        };

                        if components::button(ui, !disabled, "Reload", button_size) && !disabled {
                            self.data
                                .from_frontend_sender
                                .send(FromFrontend::Reload)
                                .unwrap();

                            self.show_popup(PopupSpec::with_duration(
                                Duration::from_secs(2),
                                "Reload in progress...".to_string(),
                            ));
                        }
                    }
                });
            });

            let (separator_rect, _) = ui
                .allocate_exact_size(egui::vec2(1.0, ui.available_height()), egui::Sense::hover());
            ui.painter().vline(
                separator_rect.center().x,
                separator_rect.y_range(),
                ui.visuals().widgets.noninteractive.bg_stroke,
            );

            ui.allocate_ui_with_layout(
                egui::vec2(HEALTH_COLUMN_WIDTH, ui.available_height()),
                egui::Layout::top_down(egui::Align::Min),
                |ui| {
                    self.render_health_indicators(ui);
                },
            );
        });
    }

    fn render_plugin_popup(&mut self, ctx: &Context, screen_id: ScreenId) {
        if !self.system_ui_state.plugin_dialog_open {
            return;
        }

        Dialog::new("Plugins".to_string(), egui::vec2(500.0, 400.0))
            .with_backdrop()
            .show(ctx, |ui| {
                // --- Plugin Overview ---
                ui.label("Plugins");

                ui.add_space(4.0);

                ui.set_height(200.0);
                ui.set_min_height(200.0);

                egui::ScrollArea::vertical().show(ui, |ui| {
                    {
                        let plugins = self.data.state.plugins.read().unwrap();
                        let current_visibility =
                            self.data.state.plugin_ui_visibility.read().unwrap().clone();

                        for (i, (plugin_id, plugin)) in plugins.iter().enumerate() {
                            let box_size = egui::vec2(ui.available_width(), 42.0);
                            ui.allocate_ui_with_layout(
                                box_size,
                                egui::Layout::top_down(egui::Align::Center),
                                |ui| {
                                    let (rect, _response) =
                                        ui.allocate_exact_size(box_size, egui::Sense::empty());
                                    let painter = ui.painter();

                                    // State color and blinking logic
                                    let mut show_border = true;
                                    let border_color =
                                        match (plugin.has_errored(), plugin.is_enabled()) {
                                            // Alive and healthy.
                                            (false, true) => egui::Color32::from_rgb(0, 200, 0),
                                            // Dead, crashed.
                                            (true, true) => {
                                                let blink =
                                                    ((self.animation_time * 8.0) as i32) % 2 == 0;
                                                show_border = blink;
                                                egui::Color32::from_rgb(200, 0, 0)
                                            }
                                            // Disabled.
                                            (_, false) => {
                                                let blink =
                                                    ((self.animation_time * 2.0) as i32) % 2 == 0;
                                                show_border = blink;
                                                egui::Color32::from_rgb(200, 200, 0)
                                            }
                                        };

                                    // Draw the main box
                                    painter.rect_filled(rect, 0.0, egui::Color32::from_gray(30));

                                    // Draw the left border if needed
                                    let border_width = 6.0;
                                    if show_border {
                                        let border_rect = egui::Rect::from_min_max(
                                            rect.left_top(),
                                            rect.left_bottom() + egui::vec2(border_width, 0.0),
                                        );
                                        painter.rect_filled(border_rect, 0.0, border_color);
                                    }

                                    // Plugin name
                                    let path_str = plugin.path.to_string().to_string();
                                    let path = Path::new(&path_str);
                                    let basename = path.file_stem().unwrap().to_string_lossy();
                                    // let basename = path.file_name().unwrap().to_string_lossy();
                                    let name = format!("P:{basename} ({})", i + 1);

                                    let text_padding = 5.0;

                                    painter.text(
                                        rect.left_center()
                                            + egui::vec2(border_width + text_padding, 0.0),
                                        egui::Align2::LEFT_CENTER,
                                        name,
                                        egui::FontId::monospace(12.0),
                                        if plugin.has_errored() {
                                            Color32::WHITE
                                        } else {
                                            egui::Color32::from_gray(90)
                                        },
                                    );
                                },
                            );
                            ui.horizontal(|ui| {
                                // TODO: write a helper function for accessing this screen-specific
                                // state.
                                let visibility = *current_visibility
                                    .get(plugin_id)
                                    .unwrap_or(&PluginOpenState::CLOSED);

                                let label = if visibility.open {
                                    "Hide UI"
                                } else {
                                    "Show UI"
                                };
                                if ui.small_button(label).clicked() {
                                    let mut map =
                                        self.data.state.plugin_ui_visibility.write().unwrap();

                                    // PATCH: ensure that the window is only open on one screen.

                                    let entry =
                                        map.entry(*plugin_id).or_insert(PluginOpenState::CLOSED);

                                    match visibility.open {
                                        true => {
                                            entry.open = false;
                                        }
                                        false => {
                                            entry.open = true;
                                            entry.screen_id = screen_id;
                                        }
                                    };

                                    // Notify plugins.
                                    self.data
                                        .event_bus_connection
                                        .send(ControlEventMessage::new(
                                            EventOriginator::Web,
                                            ControlEvent::MainUi(MainUiEvent::SetPluginUIOpen {
                                                plugin_id: *plugin_id,
                                                open: entry.open,
                                            }),
                                        ));
                                }
                            });
                            ui.add_space(8.0);
                        }

                        ui.separator();

                        mem::drop(plugins)
                    }
                });

                if components::button(ui, false, "Close", ButtonSize::Medium) {
                    self.system_ui_state.plugin_dialog_open = false;
                }
            });
    }
}
