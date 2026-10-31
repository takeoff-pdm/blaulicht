use crate::app::{
    components::{self, ButtonSize, Dialog},
    view_state::{PluginOpenState, ScreenId},
    BlaulichtApp, GuardedLifecycleAction, PopupSpec,
};
use blaulicht_engine::{
    audio::defs::AudioThreadControlSignal, client::EngineClient, hub::HostCommand,
    state::NUM_DMX_UNIVERSES,
};
use blaulicht_shared::{AppPage, AppSubPage};
use egui::{Color32, Context, FontId, RichText, ThemePreference};
use std::{path::Path, process::Command, time::Duration};

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
        self.render_midi_dialog(ctx);
        self.render_serial_dialog(ctx);
        self.render_screens_dialog(ctx);

        self.subpage_ui(ui, AppPage::System, |app, ui, subpage| match subpage {
            AppSubPage::System_System => app.render_system_general(ui, render_context),
            other => app.render_system_tab_content(ui, screen_id, other),
        });
    }

    /// The General tab: showfile, theme, lifecycle and health overview.
    fn render_system_general(
        &mut self,
        ui: &mut egui::Ui,
        render_context: crate::app::page::PageRenderContext,
    ) {
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
                    let showfile = self.client.showfile_path();

                    ui.label("showfile:");

                    let name = showfile
                        .as_deref()
                        .map(blaulicht_engine::config::showfile_display_name)
                        .unwrap_or_else(|| "N/A".to_string());
                    let name_label = ui.label(
                        egui::RichText::new(name)
                            .strong()
                            .font(FontId::monospace(14.0)),
                    );
                    if let Some(path) = &showfile {
                        name_label.on_hover_text(
                            blaulicht_engine::config::absolute_showfile_path(path)
                                .to_string_lossy()
                                .into_owned(),
                        );
                    }

                    if let Some(saved_at) = self.saved_at {
                        let ago = saved_at.elapsed().unwrap_or_default().as_secs();
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
                        self.request_guarded_lifecycle(GuardedLifecycleAction::Quit);
                    }

                    if components::button(ui, false, "Restart", button_size) {
                        self.request_guarded_lifecycle(GuardedLifecycleAction::Restart);
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
                        let signal = self.client.mainloop_state();

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
                            self.send_command(HostCommand::ReloadPlugins, |app, _| {
                                app.show_popup(PopupSpec::with_duration(
                                    Duration::from_secs(2),
                                    "Reload in progress...".to_string(),
                                ));
                            });
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

    fn render_system_tab_content(
        &mut self,
        ui: &mut egui::Ui,
        screen_id: ScreenId,
        tab: AppSubPage,
    ) {
        ui.heading(tab.label());
        ui.add_space(8.0);

        // Solid (non-floating) scroll bar so it stays visible instead of only
        // fading in on hover; the content is laid out beside it.
        ui.spacing_mut().scroll.floating = false;
        ui.spacing_mut().scroll.bar_width = 10.0;
        egui::ScrollArea::vertical()
            .id_salt("system-tab-content")
            .auto_shrink([false, false])
            .scroll_bar_visibility(egui::scroll_area::ScrollBarVisibility::VisibleWhenNeeded)
            .show(ui, |ui| match tab {
                AppSubPage::System_Dmx => {
                    self.client.with_health(|health| {
                        render_dmx_universe_rows(ui, &health.dmx_universes_healthy)
                    });
                }
                AppSubPage::System_ArtNet => {
                    self.render_artnet_management(ui);
                }
                AppSubPage::System_Plugins => {
                    self.render_plugin_management(ui, screen_id);
                }
                AppSubPage::System_Midi => self.client.with_health(|health| {
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
                }),
                AppSubPage::System_Serial => self.client.with_health(|health| {
                    if health.serial_health.devices.is_empty() {
                        ui.label("No serial devices opened by Blaulicht.");
                    }
                    let mut devices: Vec<_> = health.serial_health.devices.iter().collect();
                    devices.sort_by_key(|(name, _)| *name);
                    for (name, state) in devices {
                        ui.label(format!("{name}  {state:?}"));
                    }
                }),
                AppSubPage::System_Screens => {
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
                            if screen.owner_plugin_id.is_none()
                                && ui.small_button("Remove").clicked()
                            {
                                remove = Some(index);
                            }
                        });
                    }
                    if let Some(index) = remove {
                        self.remove_external_screen(index);
                    }
                }
                // General is rendered by `render_system_general`; other
                // pages' tabs never reach this page.
                _ => {}
            });
    }

    fn render_plugin_management(&mut self, ui: &mut egui::Ui, screen_id: ScreenId) {
        // The caller (`render_system_tab_content`) already wraps the tab in a
        // page-height scroll area, so the list is not scrolled separately here.
        // (id, path, errored, enabled) of each plugin, in the map's order.
        let plugins: Vec<(u8, String, bool, bool)> = self.client.with_plugins(|plugins| {
            plugins
                .iter()
                .map(|(id, plugin)| {
                    (
                        *id,
                        plugin.path.to_string(),
                        plugin.has_errored(),
                        plugin.is_enabled(),
                    )
                })
                .collect()
        });

        for (i, (plugin_id, path_str, has_errored, is_enabled)) in plugins.into_iter().enumerate() {
            let box_size = egui::vec2(ui.available_width(), 42.0);
            ui.allocate_ui_with_layout(
                box_size,
                egui::Layout::top_down(egui::Align::Center),
                |ui| {
                    let (rect, _response) = ui.allocate_exact_size(box_size, egui::Sense::empty());
                    let painter = ui.painter();

                    // State color and blinking logic
                    let mut show_border = true;
                    let border_color = match (has_errored, is_enabled) {
                        // Alive and healthy.
                        (false, true) => egui::Color32::from_rgb(0, 200, 0),
                        // Dead, crashed.
                        (true, true) => {
                            let blink = ((self.animation_time * 8.0) as i32) % 2 == 0;
                            show_border = blink;
                            egui::Color32::from_rgb(200, 0, 0)
                        }
                        // Disabled.
                        (_, false) => {
                            let blink = ((self.animation_time * 2.0) as i32) % 2 == 0;
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
                    let path = Path::new(&path_str);
                    let basename = path.file_stem().unwrap().to_string_lossy();
                    // let basename = path.file_name().unwrap().to_string_lossy();
                    let name = format!("P:{basename} ({})", i + 1);

                    let text_padding = 5.0;

                    painter.text(
                        rect.left_center() + egui::vec2(border_width + text_padding, 0.0),
                        egui::Align2::LEFT_CENTER,
                        name,
                        egui::FontId::monospace(12.0),
                        if has_errored {
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
                let visibility = self.view_state.plugin_open_state(plugin_id);

                let label = if visibility.open {
                    "Hide UI"
                } else {
                    "Show UI"
                };
                if ui.small_button(label).clicked() {
                    // PATCH: ensure that the window is only open on one screen.
                    let entry = self
                        .view_state
                        .plugin_visibility
                        .entry(plugin_id)
                        .or_insert(PluginOpenState::CLOSED);

                    match visibility.open {
                        true => {
                            entry.open = false;
                        }
                        false => {
                            entry.open = true;
                            entry.screen_id = screen_id;
                        }
                    };
                    // Plugins learn about it through the engine's mirror
                    // (`ui_is_open`), published at the end of the frame.
                }
            });
            ui.add_space(8.0);
        }

        ui.separator();
    }
}

fn render_dmx_universe_rows(ui: &mut egui::Ui, states: &[blaulicht_engine::state::DmxHealth]) {
    for (universe, state) in states.iter().enumerate() {
        ui.group(|ui| {
            ui.strong(format!("Universe {universe}"));
            // Unconfigured universes carry an empty port, which rendered as a bare "Port:".
            let port = if state.port.trim().is_empty() {
                "-"
            } else {
                state.port.trim()
            };
            ui.label(format!("Port: {port}"));
            match &state.state {
                blaulicht_engine::state::DmxHealthState::Healthy => {
                    ui.colored_label(Color32::LIGHT_GREEN, "ONLINE");
                }
                blaulicht_engine::state::DmxHealthState::Error(error) => {
                    ui.colored_label(Color32::LIGHT_RED, error);
                }
            }
        });
        ui.add_space(6.0);
    }
}

#[cfg(test)]
mod system_layout_tests {
    use super::render_dmx_universe_rows;
    use blaulicht_engine::state::{DmxHealth, NUM_DMX_UNIVERSES};

    #[test]
    fn ten_dmx_universes_scroll_vertically_without_horizontal_overflow() {
        let ctx = egui::Context::default();
        let input = egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(
                egui::Pos2::ZERO,
                egui::vec2(320.0, 240.0),
            )),
            ..Default::default()
        };
        let states: [DmxHealth; NUM_DMX_UNIVERSES] = std::array::from_fn(|universe| {
            DmxHealth::error(
                format!("/dev/dmx-{universe}"),
                "Not initialized".to_string(),
            )
        });
        let mut sizes = None;

        let _ = ctx.run(input, |ctx| {
            egui::CentralPanel::default().show(ctx, |ui| {
                let output = egui::ScrollArea::vertical().show(ui, |ui| {
                    render_dmx_universe_rows(ui, &states);
                });
                sizes = Some((output.content_size, output.inner_rect.size()));
            });
        });

        let (content, viewport) = sizes.unwrap();
        assert!(
            content.y > viewport.y,
            "the compact page should require scrolling"
        );
        assert!(
            content.x <= viewport.x,
            "DMX rows must not overflow horizontally: {content:?} vs {viewport:?}"
        );
    }
}
