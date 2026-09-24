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
use std::{mem, path::Path, process::Command, time::Duration};

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

        let available_size = ui.available_size().max(egui::vec2(1.0, 1.0));
        let (page_rect, _) = ui.allocate_exact_size(available_size, egui::Sense::hover());
        let tab_height = system_tab_bar_height(page_rect.width()).min(page_rect.height());
        let tab_rect = egui::Rect::from_min_max(
            egui::pos2(page_rect.min.x, page_rect.max.y - tab_height),
            page_rect.max,
        );
        let content_rect =
            egui::Rect::from_min_max(page_rect.min, egui::pos2(page_rect.max.x, tab_rect.min.y));
        let mut tab_ui = ui.new_child(
            egui::UiBuilder::new()
                .max_rect(tab_rect)
                .layout(egui::Layout::top_down(egui::Align::Min)),
        );
        tab_ui.set_clip_rect(tab_rect);
        // Draw the divider manually so its height matches what
        // `system_tab_bar_height` reserves (a plain `separator()` adds style
        // dependent padding and would push the buttons out of the clip rect).
        tab_ui.spacing_mut().item_spacing.y = SYSTEM_TAB_GAP;
        let (divider_rect, _) = tab_ui.allocate_exact_size(
            egui::vec2(tab_ui.available_width(), SYSTEM_TAB_DIVIDER_HEIGHT),
            egui::Sense::hover(),
        );
        tab_ui.painter().hline(
            divider_rect.x_range(),
            divider_rect.center().y,
            tab_ui.visuals().widgets.noninteractive.bg_stroke,
        );
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
        let available = ui.available_width();
        let columns = system_tab_bar_columns(available);
        let tab_width = ((available - SYSTEM_TAB_GAP * columns.saturating_sub(1) as f32)
            / columns as f32)
            .max(SYSTEM_TAB_MIN_WIDTH);

        ui.horizontal_wrapped(|ui| {
            ui.spacing_mut().item_spacing = egui::vec2(SYSTEM_TAB_GAP, SYSTEM_TAB_GAP);
            for tab in SystemTab::ALL {
                if components::Button::new(tab.label(), ButtonSize::Medium.with_width(tab_width))
                    .ui(ui, self.system_ui_state.active_tab == tab)
                {
                    self.system_ui_state.active_tab = tab;
                }
            }
        });
    }

    fn render_system_tab_content(&mut self, ui: &mut egui::Ui, screen_id: ScreenId) {
        let tab = self.system_ui_state.active_tab;
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
                SystemTab::General => {}
                SystemTab::Dmx => {
                    let health = self.data.state.health_data.read().unwrap();
                    render_dmx_universe_rows(ui, &health.dmx_universes_healthy);
                }
                SystemTab::ArtNet => {
                    self.render_artnet_management(ui);
                }
                SystemTab::Plugins => {
                    self.render_plugin_management(ui, screen_id);
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
            });
    }

    fn render_plugin_management(&mut self, ui: &mut egui::Ui, screen_id: ScreenId) {
        // The caller (`render_system_tab_content`) already wraps the tab in a
        // page-height scroll area, so the list is not scrolled separately here.
        let plugins = self.data.state.plugins.read().unwrap();
        let current_visibility = self.data.state.plugin_ui_visibility.read().unwrap().clone();

        for (i, (plugin_id, plugin)) in plugins.iter().enumerate() {
            let box_size = egui::vec2(ui.available_width(), 42.0);
            ui.allocate_ui_with_layout(
                box_size,
                egui::Layout::top_down(egui::Align::Center),
                |ui| {
                    let (rect, _response) = ui.allocate_exact_size(box_size, egui::Sense::empty());
                    let painter = ui.painter();

                    // State color and blinking logic
                    let mut show_border = true;
                    let border_color = match (plugin.has_errored(), plugin.is_enabled()) {
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
                    let path_str = plugin.path.to_string().to_string();
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
                    let mut map = self.data.state.plugin_ui_visibility.write().unwrap();

                    // PATCH: ensure that the window is only open on one screen.

                    let entry = map.entry(*plugin_id).or_insert(PluginOpenState::CLOSED);

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
}

fn render_dmx_universe_rows(ui: &mut egui::Ui, states: &[crate::state::DmxHealth]) {
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

const SYSTEM_TAB_GAP: f32 = 2.0;
const SYSTEM_TAB_MIN_WIDTH: f32 = 86.0;
const SYSTEM_TAB_DIVIDER_HEIGHT: f32 = 1.0;

/// Number of tab buttons that fit into one row at the given width.
fn system_tab_bar_columns(width: f32) -> usize {
    (((width.max(SYSTEM_TAB_MIN_WIDTH) + SYSTEM_TAB_GAP) / (SYSTEM_TAB_MIN_WIDTH + SYSTEM_TAB_GAP))
        .floor() as usize)
        .clamp(1, SystemTab::ALL.len())
}

/// Exact height of the bottom tab bar: divider, gap below it, and the wrapped
/// button rows. Must stay in sync with how `system_ui` draws the bar.
fn system_tab_bar_height(width: f32) -> f32 {
    let rows = SystemTab::ALL.len().div_ceil(system_tab_bar_columns(width));
    SYSTEM_TAB_DIVIDER_HEIGHT
        + SYSTEM_TAB_GAP
        + rows as f32 * ButtonSize::Medium.dim().0.y
        + rows.saturating_sub(1) as f32 * SYSTEM_TAB_GAP
}

#[cfg(test)]
mod system_layout_tests {
    use super::{render_dmx_universe_rows, system_tab_bar_height};
    use crate::state::{DmxHealth, NUM_DMX_UNIVERSES};

    #[test]
    fn system_tab_bar_reserves_one_or_more_content_sized_rows() {
        let wide = system_tab_bar_height(800.0);
        let narrow = system_tab_bar_height(280.0);

        assert!(wide > 20.0 && wide < 60.0);
        assert!(narrow > wide);
        assert!(narrow < 160.0);
    }

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
