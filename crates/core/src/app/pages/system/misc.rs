use crate::{
    app::{
        components::{self, ButtonSize, Dialog},
        pages::system::SystemTab,
        BlaulichtApp, GuardedLifecycleAction, PopupSpec,
    },
    audio::defs::AudioThreadControlSignal,
    msg::FromFrontend,
    state::{PluginOpenState, ScreenId, NUM_DMX_UNIVERSES},
};
use blaulicht_shared::{ControlEvent, ControlEventMessage, EventOriginator, MainUiEvent};
use egui::{Color32, Context, FontId, RichText, ThemePreference};
use std::{
    mem,
    path::Path,
    process::Command,
    time::Duration,
};

impl BlaulichtApp {
    fn render_confirm_shutdown_dialog(&mut self, ctx: &Context) {
        if !self.system_ui_state.confirm_shutdown_open {
            return;
        }

        let response = Dialog::new("Confirm Shutdown".to_string(), egui::vec2(200.0, 100.0))
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

                ui.horizontal_wrapped(|ui| {
                    if components::button(ui, false, "Confirm", ButtonSize::Large) {
                        match Command::new("/usr/bin/shutdown.sh").status() {
                            Ok(status) if status.success() => {
                                tracing::info!("Shutdown command executed successfully.");
                            }
                            Ok(status) => {
                                tracing::error!("Shutdown command failed: {status}");
                            }
                            Err(err) => {
                                tracing::error!("Failed to execute shutdown command: {err}");
                            }
                        }

                        self.system_ui_state.confirm_shutdown_open = false;
                    }

                    if components::button(ui, true, "Cancel", ButtonSize::Large) {
                        self.system_ui_state.confirm_shutdown_open = false;
                    }
                });
            });
        if response.cancel_requested {
            self.system_ui_state.confirm_shutdown_open = false;
        }
    }

    pub fn system_ui(
        &mut self,
        ui: &mut egui::Ui,
        ctx: &Context,
        screen_id: ScreenId,
        render_context: crate::app::page::PageRenderContext,
    ) {
        self.render_confirm_shutdown_dialog(ctx);
        self.render_showfile_dialog(ctx);

        for universe in 0..NUM_DMX_UNIVERSES {
            self.render_dmx_dialog(ctx, universe);
        }
        self.render_artnet_dialog(ctx);
        self.render_midi_dialog(ctx);
        self.render_serial_dialog(ctx);
        self.render_screens_dialog(ctx);
        self.render_plugin_popup(ctx, screen_id);

        let available_size = ui.available_size().max(egui::vec2(1.0, 1.0));
        let (page_rect, _) = ui.allocate_exact_size(available_size, egui::Sense::hover());
        let tab_height = system_tab_bar_height(page_rect.width()).min(page_rect.height());
        let tab_rect = egui::Rect::from_min_max(
            egui::pos2(page_rect.min.x, page_rect.max.y - tab_height),
            page_rect.max,
        );
        let content_rect = egui::Rect::from_min_max(
            page_rect.min,
            egui::pos2(page_rect.max.x, tab_rect.min.y),
        );
        let mut tab_ui = ui.new_child(
            egui::UiBuilder::new()
                .max_rect(tab_rect)
                .layout(egui::Layout::top_down(egui::Align::Min)),
        );
        tab_ui.set_clip_rect(tab_rect);
        tab_ui.separator();
        self.render_system_tab_bar(&mut tab_ui, render_context);

        let mut content_ui = ui.new_child(
            egui::UiBuilder::new()
                .max_rect(content_rect)
                .layout(egui::Layout::top_down(egui::Align::Min)),
        );
        content_ui.set_clip_rect(content_rect);
        let ui = &mut content_ui;

        if self.system_ui_state.active_tab != SystemTab::General {
            self.render_system_tab_content(ui, screen_id);
            return;
        }

        let button_size = ButtonSize::Medium.with_width(110.0);
        const HEALTH_COLUMN_WIDTH: f32 = 136.0;
        let narrow = render_context.is_dynamic()
            && render_context.width_class == crate::app::page::PageWidthClass::Narrow;
        let outer_layout = if narrow {
            egui::Layout::top_down(egui::Align::Min)
        } else {
            egui::Layout::left_to_right(egui::Align::Min)
        };

        ui.with_layout(outer_layout, |ui| {
            let default_item_spacing = ui.spacing().item_spacing;
            ui.spacing_mut().item_spacing.x = 0.0;
            let main_column_width = if narrow {
                ui.available_width()
            } else {
                (ui.available_width() - HEALTH_COLUMN_WIDTH - 1.0).max(0.0)
            };

            ui.vertical(|ui| {
                ui.spacing_mut().item_spacing = default_item_spacing;
                ui.set_width(main_column_width);
                ui.horizontal_wrapped(|ui| {
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

                    if let Some(saved_at) = self.last_save_time {
                        let ago = saved_at.elapsed().as_secs();
                        let label = if ago < 5 {
                            "just now".to_string()
                        } else if ago < 60 {
                            format!("{ago}s ago")
                        } else {
                            format!("{}m ago", ago / 60)
                        };
                        ui.label(
                            egui::RichText::new(format!("(saved {label})"))
                                .color(Color32::from_gray(120))
                                .font(FontId::monospace(12.0)),
                        );
                    }
                });

                ui.add_space(3.0);

                self.render_showfile_buttons(ui, button_size);

                ui.add_space(15.0);
                ui.separator();
                ui.add_space(15.0);

                ui.horizontal_wrapped(|ui| {
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
                        self.request_guarded_lifecycle(GuardedLifecycleAction::Quit, ctx);
                    }

                    if components::button(ui, false, "Restart", button_size) {
                        self.request_guarded_lifecycle(GuardedLifecycleAction::Restart, ctx);
                    }

                    if components::button(ui, false, "Shutdown", button_size) {
                        self.system_ui_state.confirm_shutdown_open = true;
                    }

                    if components::button(ui, self.system_ui_state.debug_open, "Debug", button_size)
                    {
                        self.system_ui_state.debug_open = !self.system_ui_state.debug_open;
                    }

                    if components::button(ui, false, "Add Screen", button_size) {
                        self.add_external_screen();
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

                        if components::action_button(
                            ui,
                            !disabled,
                            "Reload",
                            button_size,
                            Some("The engine is already reloading or unavailable"),
                        ) {
                            if self
                                .data
                                .from_frontend_sender
                                .send(FromFrontend::Reload)
                                .is_err()
                            {
                                tracing::warn!("Audio control channel is closed");
                            }

                            self.show_popup(PopupSpec::with_duration(
                                Duration::from_secs(2),
                                "Reload in progress...".to_string(),
                            ));
                        }
                    }
                });
            });

            if narrow {
                ui.separator();
            } else {
                let (separator_rect, _) = ui.allocate_exact_size(
                    egui::vec2(1.0, ui.available_height()),
                    egui::Sense::hover(),
                );
                ui.painter().vline(
                    separator_rect.center().x,
                    separator_rect.y_range(),
                    ui.visuals().widgets.noninteractive.bg_stroke,
                );
            }

            ui.allocate_ui_with_layout(
                if narrow {
                    egui::vec2(ui.available_width(), ui.available_height())
                } else {
                    egui::vec2(HEALTH_COLUMN_WIDTH, ui.available_height())
                },
                egui::Layout::top_down(egui::Align::Min),
                |ui| {
                    self.render_health_indicators(ui);
                },
            );
        });
    }

    fn render_system_tab_bar(
        &mut self,
        ui: &mut egui::Ui,
        _render_context: crate::app::page::PageRenderContext,
    ) {
        const GAP: f32 = 2.0;
        const MIN_TAB_WIDTH: f32 = 86.0;
        let available = ui.available_width().max(MIN_TAB_WIDTH);
        let columns = ((available + GAP) / (MIN_TAB_WIDTH + GAP))
            .floor()
            .max(1.0)
            .min(SystemTab::ALL.len() as f32) as usize;
        let tab_width = ((available - GAP * columns.saturating_sub(1) as f32) / columns as f32)
            .max(MIN_TAB_WIDTH);

        ui.horizontal_wrapped(|ui| {
            ui.spacing_mut().item_spacing = egui::vec2(GAP, GAP);
            for tab in SystemTab::ALL {
                if components::Button::new(
                    tab.label(),
                    ButtonSize::Medium.with_width(tab_width),
                )
                .ui(ui, self.system_ui_state.active_tab == tab)
                {
                    self.system_ui_state.active_tab = tab;
                }
            }
        });
    }

    fn render_system_tab_content(&mut self, ui: &mut egui::Ui, _screen_id: ScreenId) {
        let tab = self.system_ui_state.active_tab;
        ui.heading(tab.label());
        ui.add_space(8.0);

        egui::ScrollArea::vertical().show(ui, |ui| match tab {
            SystemTab::General => {}
            SystemTab::Dmx => {
                let health = self.data.state.health_data.read().unwrap();
                for (universe, state) in health.dmx_universes_healthy.iter().enumerate() {
                    ui.group(|ui| {
                        ui.strong(format!("Universe {universe}"));
                        ui.label(format!("Port: {}", state.port));
                        match &state.state {
                            crate::state::DmxHealthState::Healthy => {
                                ui.colored_label(Color32::LIGHT_GREEN, "ONLINE");
                            }
                            crate::state::DmxHealthState::Error(error) => {
                                ui.colored_label(Color32::LIGHT_RED, error);
                            }
                        }
                    });
                    ui.add_space(6.0);
                }
            }
            SystemTab::ArtNet => {
                let health = self.data.state.health_data.read().unwrap();
                let online = health.artnet_health_state;
                drop(health);
                ui.colored_label(
                    if online { Color32::LIGHT_GREEN } else { Color32::LIGHT_RED },
                    if online { "ONLINE" } else { "OFFLINE" },
                );
                let receivers = self.data.state.artnet_output.read().unwrap().receivers.clone();
                if receivers.is_empty() {
                    ui.label("No Art-Net receivers configured.");
                }
                for receiver in receivers {
                    ui.label(format!(
                        "{}  {}{}",
                        if receiver.enabled { "ON " } else { "OFF" },
                        receiver.address,
                        receiver
                            .owner_plugin_id
                            .map(|id| format!("  (plugin {id})"))
                            .unwrap_or_default()
                    ));
                }
                ui.add_space(8.0);
                if components::button(ui, false, "Manage Art-Net", ButtonSize::Medium) {
                    self.system_ui_state.artnet_dialog_open = true;
                }
            }
            SystemTab::Plugins => {
                let plugins = self.data.state.plugins.read().unwrap();
                if plugins.is_empty() {
                    ui.label("No plugins configured.");
                }
                for (id, plugin) in plugins.iter() {
                    let status = if plugin.has_errored() {
                        "ERROR"
                    } else if plugin.is_enabled() {
                        "ENABLED"
                    } else {
                        "DISABLED"
                    };
                    ui.label(format!("#{id}  {}  {status}", plugin.path));
                }
                drop(plugins);
                ui.add_space(8.0);
                if components::button(ui, false, "Manage Plugins", ButtonSize::Medium) {
                    self.system_ui_state.plugin_dialog_open = true;
                }
            }
            SystemTab::Midi => {
                let health = self.data.state.health_data.read().unwrap();
                if health.midi_health.available_devices.is_empty() {
                    ui.label("No MIDI inputs detected.");
                }
                for device in &health.midi_health.available_devices {
                    let status = health
                        .midi_health
                        .devices
                        .get(device)
                        .map(|state| format!("{state:?}"))
                        .unwrap_or_else(|| "AVAILABLE".to_string());
                    ui.label(format!("{device}  {status}"));
                }
            }
            SystemTab::Serial => {
                let health = self.data.state.health_data.read().unwrap();
                if health.serial_health.devices.is_empty() {
                    ui.label("No serial devices opened by Blaulicht.");
                }
                let mut devices: Vec<_> = health.serial_health.devices.iter().collect();
                devices.sort_by_key(|(name, _)| *name);
                for (name, state) in devices {
                    ui.label(format!("{name}  {state:?}"));
                }
            }
            SystemTab::Screens => {
                if components::button(ui, false, "Add Screen", ButtonSize::Medium) {
                    self.add_external_screen();
                }
                ui.add_space(8.0);
                if self.external_screens.is_empty() {
                    ui.label("No external screens.");
                }
                let mut remove = None;
                for (index, screen) in self.external_screens.iter().enumerate() {
                    ui.horizontal(|ui| {
                        ui.label(format!(
                            "#{index}  {:.0} x {:.0}",
                            screen.dimensions.x, screen.dimensions.y
                        ));
                        if screen.owner_plugin_id.is_none() && ui.small_button("Remove").clicked() {
                            remove = Some(index);
                        }
                    });
                }
                if let Some(index) = remove {
                    self.remove_external_screen(index);
                }
            }
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

fn system_tab_bar_height(width: f32) -> f32 {
    const GAP: f32 = 2.0;
    const MIN_TAB_WIDTH: f32 = 86.0;
    let columns = (((width.max(MIN_TAB_WIDTH) + GAP) / (MIN_TAB_WIDTH + GAP)).floor() as usize)
        .clamp(1, SystemTab::ALL.len());
    let rows = SystemTab::ALL.len().div_ceil(columns);
    1.0 + rows as f32 * ButtonSize::Medium.dim().0.y
        + rows.saturating_sub(1) as f32 * GAP
}

#[cfg(test)]
mod system_layout_tests {
    use super::system_tab_bar_height;

    #[test]
    fn system_tab_bar_reserves_one_or_more_content_sized_rows() {
        let wide = system_tab_bar_height(800.0);
        let narrow = system_tab_bar_height(280.0);

        assert!(wide > 20.0 && wide < 60.0);
        assert!(narrow > wide);
        assert!(narrow < 160.0);
    }
}
