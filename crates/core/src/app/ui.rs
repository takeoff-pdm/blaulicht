use crate::app::components::ButtonSize;
use crate::app::{components, theme, AppPage, BlaulichtApp, PopupSpec};
use crate::audio::defs::{AudioThreadControlSignal, DMX_TICK_TIME};
use crate::dmx::{DmxEngine, EngineState};
use crate::msg::FromFrontend;
use crate::{config, plugin, utils};
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

pub enum FileDialogOpenOrigin {
    Save,
    Load,
}

//
// Actual eframe shit.
//

// fn update_button_style(ctx: &egui::Context) {
//     let mut style = (*ctx.style()).clone();
//
//     // Base look
//     style.visuals.widgets.inactive.bg_fill = egui::Color32::from_rgb(220, 220, 220); // idle
//     style.visuals.widgets.hovered.bg_fill = egui::Color32::from_rgb(180, 200, 255); // hover
//     style.visuals.widgets.active.bg_fill = egui::Color32::from_rgb(0, 120, 255); // when pressed
//
//     ctx.set_style(style);
// }

const UI_RECV_KEY: &'static str = "eframe_ui";

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
        // This is also where you can customize the look and feel of egui using
        // `cc.egui_ctx.set_visuals` and `cc.egui_ctx.set_fonts`.

        // Load previous app state (if any).
        // Note that you must enable the `persistence` feature for this to work.
        // let mut app: TemplateApp = if let Some(storage) = cc.storage {
        //     eframe::get_value(storage, eframe::APP_KEY).unwrap_or_default()
        // } else {
        //     Default::default()
        // };

        // let (sender, receiver) = crossbeam_channel::unbounded();
        {
            // let mut consumers = state.to_frontend_consumers.lock().unwrap();
            // consumers.insert(UI_RECV_KEY.to_string(), sender);
        }

        let mut app = Self::new_default(state);

        cc.egui_ctx.set_pixels_per_point(1.0);

        // Initialize animation time
        app.animation_time = 0.0;

        if let Some(p) = initial_popup {
            app.show_popup(p);
        }

        let mut fonts = egui::FontDefinitions::default();
        egui_phosphor::add_to_fonts(&mut fonts, egui_phosphor::Variant::Regular);

        cc.egui_ctx.set_fonts(fonts);

        app
    }

    pub fn show_popup(&mut self, popup: PopupSpec) {
        self.popup = Some(popup);
        self.popup_open_time = Instant::now();
    }

    fn close_popup(&mut self) {
        self.popup = None;
    }

    // `ui` is your egui Ui, `progress` is a value between 0.0 and 1.0
    fn draw_progress_bar(ui: &mut Ui, progress: f32, height: f32, text: &str) {
        // Allocate space for the progress bar
        let (rect, _response) =
            ui.allocate_exact_size(Vec2::new(ui.available_width(), height), Sense::hover());

        let painter: &Painter = ui.painter();

        // Draw the background (non-rounded)
        painter.rect_filled(rect, 0.0, Color32::from_rgb(30, 30, 30));

        // Draw the filled portion
        let fill_rect = Rect::from_min_max(
            rect.min,
            egui::pos2(rect.min.x + rect.width() * progress, rect.max.y),
        );
        painter.rect_filled(fill_rect, 0.0, Color32::from_rgb(60, 120, 200));

        // Optional: draw percentage text
        painter.text(
            rect.center(),
            egui::Align2::CENTER_CENTER,
            text,
            FontId::proportional(12.0),
            Color32::WHITE,
        );
    }

    fn render_popup(&mut self, ctx: &egui::Context) {
        if let Some(ref popup) = self.popup {
            let popup = popup.clone();

            let screen_rect = ctx.screen_rect();
            let popup_size = egui::Vec2::new(200.0, 100.0); // desired popup size

            let center_pos = egui::Pos2::new(
                screen_rect.center().x - popup_size.x / 2.0,
                screen_rect.center().y - popup_size.y / 2.0,
            );

            let elapsed = self.popup_open_time.elapsed();
            if elapsed >= popup.lifetime_duration {
                self.close_popup();
            }

            egui::Window::new(&popup.label)
                .fixed_size(popup_size)
                .collapsible(false)
                .resizable(false)
                .title_bar(false)
                .fixed_pos(center_pos)
                .frame(Frame {
                    corner_radius: CornerRadius::same(1),
                    fill: Color32::from_gray(40),
                    stroke: Stroke::new(1.0, Color32::from_gray(60)),
                    inner_margin: Margin::symmetric(6, 12),
                    ..Frame::default()
                })
                .show(ctx, |ui| {
                    ui.vertical_centered(|ui| {
                        ui.label(RichText::new(&popup.label).size(24.0));

                        if let Some(ref btn) = popup.button {
                            ui.add_space(8.0);

                            if components::button(ui, false, &btn.label, ButtonSize::Large) {
                                self.close_popup();
                            }

                            ui.add_space(8.0);
                        }

                        let progress = 1.0
                            - elapsed.as_millis() as f32
                                / popup.lifetime_duration.as_millis() as f32;

                        let text = format!(
                            "{} seconds remaining",
                            popup.lifetime_duration.as_secs() - elapsed.as_secs()
                        );

                        Self::draw_progress_bar(ui, progress, 18.0, &text);
                    })
                });
        }
    }
}

impl eframe::App for BlaulichtApp {
    /// Called each time the UI needs repainting, which may be many times per second.
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        theme::set_theme(ctx, theme::REKORDBOX);

        if self.debug_open {
            egui::Window::new("Egui Settings").show(ctx, |ui| {
                let dt = ctx.input(|i| i.stable_dt);
                let fps = if dt > 0.0 { 1.0 / dt } else { 0.0 };
                ui.label(format!("FPS: {:.1}", fps));
            });
        }

        self.render_popup(ctx);
        // for i in 0..1000 {
        //     self.data
        //         .event_bus_connection
        //         .send(ControlEventMessage::new(
        //             EventOriginator::Web,
        //             ControlEvent::SetColor((0, 0, 0)),
        //         ));
        // }
        // println!("loop");
        // ctx.request_repaint_after(std::time::Duration::from_millis(16)); // ~60 FPS
        // return;

        //
        // At the beginning, process any incoming state changes.
        //

        // let mut new_signal = false;
        let mut empty = 0;
        loop {
            match self.data.event_bus_connection.try_recv() {
                Some(bus) => println!("bus: {:?}", bus),
                None => {
                    empty += 1;
                }
            }

            // match self.data.signal_receiver.try_recv() {
            //     Ok(signal) => {
            //         // self.collector.signal(signal);
            //         // new_signal = true;
            //     }
            //     Err(TryRecvError::Empty) => {
            //         empty += 1;
            //     }
            //     Err(TryRecvError::Disconnected) => unreachable!("CANNOT REACH"),
            // }

            match self.data.system_message_receiver.try_recv() {
                Ok(sys) => match sys {
                    SystemMessage::Heartbeat(_) => {
                        self.last_heartbeat_frame = self.frame_count;
                        // self.log_window.add_log(
                        //     LogLevel::Debug,
                        //     "Heartbeat received".to_string(),
                        //     "System".to_string(),
                        // );
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
                    SystemMessage::WasmControlsLog(wasm_controls_log) => {
                        self.log_window.add_log(
                            LogLevel::Debug,
                            format!("WASM Controls: {:?}", wasm_controls_log),
                            "WASM".to_string(),
                        );
                    }
                    SystemMessage::WasmControlsSet(wasm_controls_set) => {
                        self.log_window.add_log(
                            LogLevel::Info,
                            format!("WASM Controls Set: {:?}", wasm_controls_set),
                            "WASM".to_string(),
                        );
                    }
                    SystemMessage::WasmControlsConfig(wasm_controls_config) => {
                        self.log_window.add_log(
                            LogLevel::Info,
                            format!("WASM Controls Config: {:?}", wasm_controls_config),
                            "WASM".to_string(),
                        );
                    }
                    SystemMessage::LoopSpeed(duration) => {
                        self.loop_speed = duration.as_micros() as usize;
                        // self.log_window.add_log(
                        //     LogLevel::Debug,
                        //     format!("Loop speed: {} μs", duration.as_micros()),
                        //     "Performance".to_string(),
                        // );
                    }
                    SystemMessage::TickSpeed(duration) => {
                        self.tick_speed = duration.as_micros() as usize;
                        // self.log_window.add_log(
                        //     LogLevel::Debug,
                        //     format!("Tick speed: {} μs", duration.as_micros()),
                        //     "Performance".to_string(),
                        // );
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
            let audio_data = self.data.state.audio_snapshot.read().unwrap();
            self.volume_graph.update(audio_data.volume as i32);
            self.volume_graph.update(audio_data.volume as i32);
            self.beat_volume_graph.update(audio_data.beat_volume as i32);
            self.bass_graph.update(audio_data.bass as i32);
            self.bass_avg_graph.update(audio_data.bass_avg as i32);
            self.bass_avg_short_graph
                .update(audio_data.bass_avg_short as i32);
            self.bpm_graph.update(audio_data.bpm as i32);
            self.time_between_beats_graph
                .update(audio_data.time_between_beats_millis as i32);
        }

        // Put your widgets into a `SidePanel`, `TopBottomPanel`, `CentralPanel`, `Window` or `Area`.
        // For inspiration and more examples, go to https://emilk.github.io/egui

        // egui::TopBottomPanel::top("top_panel").show(ctx, |ui| {
        // The top panel is often a good place for a menu bar:

        //     egui::MenuBar::new().ui(ui, |ui| {
        //});
        // });

        // Left sidebar
        self.left_panel_ui(ctx);

        egui::CentralPanel::default().show(ctx, |ui| {
            // The central panel the region left after adding TopPanel's and SidePanel's
            // ui.heading("eframe template");
            //
            // Continuous rendering - always request repaints
            ctx.request_repaint_after(std::time::Duration::from_millis(16)); // ~60 FPS
                                                                             //
                                                                             // // Update animation time for continuous rendering
            self.frame_count += 1;
            self.animation_time += 0.016; // 16ms = 0.016 seconds
                                          //
                                          // ui.horizontal(|ui| {
                                          //     ui.label("Write something: ");
                                          //     ui.text_edit_singleline(&mut self.label);
                                          // });
                                          //
                                          // ui.add(egui::Slider::new(&mut self.value, 0.0..=10.0).text("value"));
                                          // if ui.button("Increment").clicked() {
                                          //     self.value += 1.0;
                                          // }
                                          //
                                          // ui.separator();

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
                AppPage::Show => {
                    self.show_ui(ui, ctx);
                }
                AppPage::FixturesPerformance => {
                    self.fixtures_ui(ui, ctx);
                }
                AppPage::Animations => {
                    self.animations_ui(ctx, ui);
                }
            }

            // ui.separator();
            //
            // ui.add(egui::github_link_file!(
            //     "https://github.com/emilk/eframe_template/blob/main/",
            //     "Source code."
            // ));
            //
            // ui.with_layout(egui::Layout::bottom_up(egui::Align::LEFT), |ui| {
            //     powered_by_egui_and_eframe(ui);
            //     egui::warn_if_debug_build(ui);
            // });
        });

        // Render per-plugin UI windows (visible across pages)
        {
            let ops_map = self.data.state.plugin_ui_ops.read().unwrap().clone();
            let visibility_map = self.data.state.plugin_ui_visibility.read().unwrap().clone();
            let popped_out_map = self.data.state.plugin_ui_popped_out.read().unwrap().clone();

            for (plugin_id, visible) in visibility_map.iter() {
                let mut is_open = *visible;
                if !is_open {
                    continue;
                }

                let title = format!("Plugin UI #{plugin_id}");
                let is_popped_out = *popped_out_map.get(plugin_id).unwrap_or(&false);

                if is_popped_out {
                    let viewport_id =
                        egui::ViewportId::from_hash_of(format!("plugin_{}", plugin_id));
                    let plugin_id_copy = *plugin_id;
                    let ops_copy = ops_map.get(plugin_id).cloned();
                    let data_clone = self.data.clone();

                    ctx.show_viewport_immediate(
                        viewport_id,
                        egui::ViewportBuilder::default()
                            .with_title(title.clone())
                            .with_inner_size([800.0, 600.0])
                            .with_resizable(true),
                        move |ctx, _class| {
                            let mut pop_in_clicked = false;
                            egui::CentralPanel::default().show(ctx, |ui| {
                                ui.horizontal(|ui| {
                                    if ui.button("🗗 Pop In").clicked() {
                                        pop_in_clicked = true;
                                    }
                                });
                                ui.separator();

                                egui::ScrollArea::both()
                                    .auto_shrink([false, false])
                                    .stick_to_bottom(false)
                                    .show(ui, |ui| {
                                        if let Some(ref ops) = ops_copy {
                                            let mut idx = 0usize;
                                            render_plugin_ops(
                                                ui,
                                                ops,
                                                &mut idx,
                                                &data_clone,
                                                *plugin_id,
                                            );
                                        } else {
                                            ui.label("No UI generated by plugin yet.");
                                        }
                                    });
                            });

                            if pop_in_clicked {
                                let mut map =
                                    data_clone.state.plugin_ui_popped_out.write().unwrap();
                                if let Some(v) = map.get_mut(&plugin_id_copy) {
                                    *v = false;
                                }
                            }

                            if ctx.input(|i| i.viewport().close_requested()) {
                                let mut map =
                                    data_clone.state.plugin_ui_visibility.write().unwrap();
                                if let Some(v) = map.get_mut(&plugin_id_copy) {
                                    *v = false;
                                }
                            }
                        },
                    );
                } else {
                    let mut pop_out_clicked = false;
                    egui::Window::new(&title)
                        .open(&mut is_open)
                        .resizable(true)
                        .show(ctx, |ui| {
                            ui.horizontal(|ui| {
                                if ui.button("🗗 Pop Out").clicked() {
                                    pop_out_clicked = true;
                                }
                            });
                            ui.separator();

                            egui::ScrollArea::both()
                                .auto_shrink([false, false])
                                .stick_to_bottom(false)
                                .show(ui, |ui| {
                                    if let Some(ops) = ops_map.get(plugin_id) {
                                        let mut idx = 0usize;
                                        render_plugin_ops(
                                            ui, ops, &mut idx, &self.data, *plugin_id,
                                        );
                                    } else {
                                        ui.label("No UI generated by plugin yet.");
                                    }
                                });
                        });

                    if pop_out_clicked {
                        let mut map = self.data.state.plugin_ui_popped_out.write().unwrap();
                        if let Some(v) = map.get_mut(plugin_id) {
                            *v = true;
                        }
                    }

                    if !is_open {
                        let mut map = self.data.state.plugin_ui_visibility.write().unwrap();
                        if let Some(v) = map.get_mut(plugin_id) {
                            *v = false;
                        }
                    }
                }
            }
        }
    }
}

fn render_plugin_ops(
    ui: &mut egui::Ui,
    ops: &Vec<crate::ui_ops::WasmUiOp>,
    idx: &mut usize,
    data: &crate::state::AppStateWrapper,
    plugin_id: u8,
) {
    use crate::ui_ops::WasmUiOp as Op;
    while *idx < ops.len() {
        match &ops[*idx] {
            Op::Label(text) => {
                ui.label(text);
                *idx += 1;
            }
            Op::Separator => {
                ui.separator();
                *idx += 1;
            }
            Op::Button { label, id } => {
                if ui.button(label).clicked() {
                    let evt = ControlEvent::PluginUi(PluginUiEvent::Button { id: *id }, plugin_id);
                    data.event_bus_connection
                        .send(ControlEventMessage::new(EventOriginator::Web, evt));
                }
                *idx += 1;
            }
            Op::Checkbox { label, id, checked } => {
                let mut c = *checked;
                if ui.checkbox(&mut c, label).changed() {
                    let evt = ControlEvent::PluginUi(
                        PluginUiEvent::Checkbox {
                            id: *id,
                            checked: c,
                        },
                        plugin_id,
                    );
                    data.event_bus_connection
                        .send(ControlEventMessage::new(EventOriginator::Web, evt));
                }
                *idx += 1;
            }
            Op::Slider {
                label,
                id,
                min,
                max,
                value,
            } => {
                let mut v = *value;
                if ui
                    .add(egui::Slider::new(&mut v, (*min)..=(*max)).text(label))
                    .changed()
                {
                    let evt = ControlEvent::PluginUi(
                        PluginUiEvent::Slider { id: *id, value: v },
                        plugin_id,
                    );
                    data.event_bus_connection
                        .send(ControlEventMessage::new(EventOriginator::Web, evt));
                }
                *idx += 1;
            }
            Op::TextEdit { label, id, text } => {
                let edit_id = ui.make_persistent_id(format!("text_edit_{}", id));
                let mut s = ui.data_mut(|d| {
                    d.get_temp::<String>(edit_id)
                        .unwrap_or_else(|| text.clone())
                });

                let response = ui.text_edit_singleline(&mut s);

                ui.data_mut(|d| d.insert_temp(edit_id, s.clone()));

                if response.changed() {
                    let evt =
                        ControlEvent::PluginUi(PluginUiEvent::Text { id: *id, text: s }, plugin_id);
                    data.event_bus_connection
                        .send(ControlEventMessage::new(EventOriginator::Web, evt));
                } else if !response.has_focus() {
                    ui.label(label);
                }
                *idx += 1;
            }
            Op::TextEditMultiline { label: _, id, text } => {
                let mut s = text.clone();
                let resp = ui.add(
                    egui::TextEdit::multiline(&mut s)
                        .desired_rows(3)
                        .desired_width(300.0),
                );
                if resp.changed() {
                    let evt =
                        ControlEvent::PluginUi(PluginUiEvent::Text { id: *id, text: s }, plugin_id);
                    data.event_bus_connection
                        .send(ControlEventMessage::new(EventOriginator::Web, evt));
                }
                *idx += 1;
            }
            Op::ColorPicker { id, r, g, b, a } => {
                let mut color = egui::Color32::from_rgba_premultiplied(*r, *g, *b, *a);
                if ui.color_edit_button_srgba(&mut color).changed() {
                    let [r, g, b, a] = color.to_array();
                    let evt = ControlEvent::PluginUi(
                        PluginUiEvent::Color {
                            id: *id,
                            r,
                            g,
                            b,
                            a,
                        },
                        plugin_id,
                    );
                    data.event_bus_connection
                        .send(ControlEventMessage::new(EventOriginator::Web, evt));
                }
                *idx += 1;
            }
            Op::BeginFrame { id: _ } => {
                *idx += 1;
                ui.group(|ui| {
                    render_plugin_ops(ui, ops, idx, data, plugin_id);
                });
            }
            Op::BeginFrameStyled {
                id: _,
                title,
                pad_x,
                pad_y,
                margin_x,
                margin_y,
            } => {
                *idx += 1;
                let px = (*pad_x).max(0) as f32;
                let py = (*pad_y).max(0) as f32;
                let mx = (*margin_x).max(0) as f32;
                let my = (*margin_y).max(0) as f32;
                // Outer margins (vertical)
                if my > 0.0 {
                    ui.add_space(my);
                }
                egui::Frame::group(ui.style()).show(ui, |ui| {
                    // Inner padding via spaces
                    ui.add_space(py);
                    ui.horizontal(|ui| {
                        if px > 0.0 {
                            ui.add_space(px);
                        }
                        ui.vertical(|ui| {
                            ui.label(egui::RichText::new(title).strong());
                            ui.separator();
                            render_plugin_ops(ui, ops, idx, data, plugin_id);
                        });
                        if px > 0.0 {
                            ui.add_space(px);
                        }
                    });
                    ui.add_space(py);
                });
                if my > 0.0 {
                    ui.add_space(my);
                }
            }
            Op::EndFrame => {
                *idx += 1;
                return;
            }
            Op::BeginCollapsing {
                id: _,
                title,
                default_open,
            } => {
                *idx += 1;
                egui::CollapsingHeader::new(title)
                    .default_open(*default_open)
                    .show(ui, |ui| {
                        render_plugin_ops(ui, ops, idx, data, plugin_id);
                    });
            }
            Op::EndCollapsing => {
                *idx += 1;
                return;
            }
            Op::BeginTabs { id } => {
                // Collect tabs metadata
                *idx += 1;
                let start = *idx;
                let mut tabs: Vec<(u8, String, usize, usize)> = Vec::new();
                let mut scan = start;
                while scan < ops.len() {
                    match &ops[scan] {
                        Op::BeginTab {
                            tabs_id,
                            tab_id,
                            title,
                        } if tabs_id == id => {
                            let tab_start = scan + 1;
                            // find EndTab
                            scan += 1;
                            let mut depth = 1;
                            while scan < ops.len() && depth > 0 {
                                match &ops[scan] {
                                    Op::BeginTab {
                                        tabs_id: _,
                                        tab_id: _,
                                        title: _,
                                    } => depth += 1,
                                    Op::EndTab => depth -= 1,
                                    _ => {}
                                }
                                scan += 1;
                            }
                            let tab_end = scan - 1; // EndTab consumed in loop
                            tabs.push((*tab_id, title.clone(), tab_start, tab_end));
                        }
                        Op::EndTabs => break,
                        _ => scan += 1,
                    }
                }

                // Current selection
                let mut sel_map = data.state.plugin_ui_tabs_selected.write().unwrap();
                let current = sel_map
                    .entry((0, *id))
                    .or_insert_with(|| tabs.get(0).map(|t| t.0).unwrap_or(0));

                // Render tab header
                let mut selection_changed = false;
                ui.horizontal(|ui| {
                    for (tab_id, title, _, _) in &tabs {
                        let clicked = ui.selectable_label(*current == *tab_id, title).clicked();
                        if clicked && *current != *tab_id {
                            *current = *tab_id;
                            selection_changed = true;
                        }
                    }
                });

                ui.separator();

                // Emit selection change event
                if selection_changed {
                    let evt = ControlEvent::PluginUi(
                        PluginUiEvent::TabChanged {
                            tabs_id: *id,
                            tab_id: *current,
                        },
                        plugin_id,
                    );
                    data.event_bus_connection
                        .send(ControlEventMessage::new(EventOriginator::Web, evt));
                }

                // Render selected tab content
                if let Some((_, _, s, e)) = tabs.iter().find(|(tid, _, _, _)| tid == current) {
                    let mut inner_idx = *s;
                    while inner_idx < *e {
                        render_plugin_ops(ui, ops, &mut inner_idx, data, plugin_id);
                    }
                    *idx = *e; // position at end of inner
                }

                // Advance idx to after EndTabs
                while *idx < ops.len() {
                    if matches!(ops[*idx], Op::EndTabs) {
                        *idx += 1;
                        break;
                    }
                    *idx += 1;
                }
            }
            Op::BeginTab { .. } | Op::EndTab | Op::EndTabs => {
                // Should be handled in BeginTabs
                *idx += 1;
            }
            Op::BeginVertical => {
                *idx += 1;
                ui.vertical(|ui| {
                    render_plugin_ops(ui, ops, idx, data, plugin_id);
                });
            }
            Op::EndVertical => {
                *idx += 1;
                return;
            }
            Op::BeginHorizontal => {
                *idx += 1;
                ui.horizontal(|ui| {
                    render_plugin_ops(ui, ops, idx, data, plugin_id);
                });
            }
            Op::EndHorizontal => {
                *idx += 1;
                return;
            }
            Op::PainterBegin { id, width, height } => {
                *idx += 1;
                let available_size = ui.available_size();
                println!(
                    "[Host] PainterBegin: canvas_id={}, requested={}x{}, available={:?}",
                    id, width, height, available_size
                );

                // Use the requested dimensions if available_size is too small or zero
                let effective_available = egui::vec2(
                    available_size.x.max(*width as f32),
                    available_size.y.max(*height as f32),
                );

                let aspect_ratio = *width as f32 / *height as f32;
                let scaled_size = if effective_available.x / effective_available.y > aspect_ratio {
                    egui::vec2(effective_available.y * aspect_ratio, effective_available.y)
                } else {
                    egui::vec2(effective_available.x, effective_available.x / aspect_ratio)
                };
                println!(
                    "[Host] PainterBegin: scaled_size={:?}, rect will be allocated",
                    scaled_size
                );
                let (rect, resp) =
                    ui.allocate_exact_size(scaled_size, egui::Sense::click_and_drag());
                println!("[Host] PainterBegin: allocated rect={:?}", rect);

                let scale_x = *width as f32 / scaled_size.x;
                let scale_y = *height as f32 / scaled_size.y;

                if resp.clicked() {
                    if let Some(pos) = resp.interact_pointer_pos() {
                        let local_pos = pos - rect.min;
                        let evt = ControlEvent::PluginUi(
                            PluginUiEvent::CanvasClick {
                                id: *id,
                                x: (local_pos.x * scale_x) as i32,
                                y: (local_pos.y * scale_y) as i32,
                            },
                            plugin_id,
                        );
                        data.event_bus_connection
                            .send(ControlEventMessage::new(EventOriginator::Web, evt));
                    }
                }

                if resp.dragged() {
                    if let Some(pos) = resp.interact_pointer_pos() {
                        let local_pos = pos - rect.min;
                        let delta = resp.drag_delta();
                        let evt = ControlEvent::PluginUi(
                            PluginUiEvent::CanvasDrag {
                                id: *id,
                                x: (local_pos.x * scale_x) as i32,
                                y: (local_pos.y * scale_y) as i32,
                                dx: (delta.x * scale_x) as i32,
                                dy: (delta.y * scale_y) as i32,
                            },
                            plugin_id,
                        );
                        data.event_bus_connection
                            .send(ControlEventMessage::new(EventOriginator::Web, evt));
                    }
                }

                let multi_touch = ui.input(|i| i.multi_touch());
                if let Some(zoom_delta) = multi_touch {
                    let zoom = zoom_delta.zoom_delta;
                    if zoom != 1.0 {
                        if let Some(pos) = resp.hover_pos() {
                            let local_pos = pos - rect.min;
                            let evt = ControlEvent::PluginUi(
                                PluginUiEvent::CanvasPinch {
                                    id: *id,
                                    x: (local_pos.x * scale_x) as i32,
                                    y: (local_pos.y * scale_y) as i32,
                                    delta: zoom - 1.0,
                                },
                                plugin_id,
                            );
                            data.event_bus_connection
                                .send(ControlEventMessage::new(EventOriginator::Web, evt));
                        }
                    }
                }

                let painter = ui.painter_at(rect);
                let render_scale_x = scaled_size.x / *width as f32;
                let render_scale_y = scaled_size.y / *height as f32;

                while *idx < ops.len() {
                    match &ops[*idx] {
                        Op::PainterRect {
                            x,
                            y,
                            w,
                            h,
                            r,
                            g,
                            b,
                            a,
                        } => {
                            let color = egui::Color32::from_rgba_premultiplied(*r, *g, *b, *a);
                            let rct = egui::Rect::from_min_size(
                                rect.min
                                    + egui::vec2(
                                        *x as f32 * render_scale_x,
                                        *y as f32 * render_scale_y,
                                    ),
                                egui::vec2(*w as f32 * render_scale_x, *h as f32 * render_scale_y),
                            );
                            painter.rect_filled(rct, 0.0, color);
                            *idx += 1;
                        }
                        Op::PainterCircle {
                            x,
                            y,
                            radius,
                            r,
                            g,
                            b,
                            a,
                        } => {
                            let color = egui::Color32::from_rgba_premultiplied(*r, *g, *b, *a);
                            painter.circle_filled(
                                rect.min
                                    + egui::vec2(
                                        *x as f32 * render_scale_x,
                                        *y as f32 * render_scale_y,
                                    ),
                                *radius as f32 * render_scale_x.min(render_scale_y),
                                color,
                            );
                            *idx += 1;
                        }
                        Op::PainterLine {
                            x1,
                            y1,
                            x2,
                            y2,
                            r,
                            g,
                            b,
                            a,
                            thickness,
                        } => {
                            let color = egui::Color32::from_rgba_premultiplied(*r, *g, *b, *a);
                            painter.line_segment(
                                [
                                    rect.min
                                        + egui::vec2(
                                            *x1 as f32 * render_scale_x,
                                            *y1 as f32 * render_scale_y,
                                        ),
                                    rect.min
                                        + egui::vec2(
                                            *x2 as f32 * render_scale_x,
                                            *y2 as f32 * render_scale_y,
                                        ),
                                ],
                                egui::Stroke::new((*thickness).max(1) as f32, color),
                            );
                            *idx += 1;
                        }
                        Op::PainterText {
                            x,
                            y,
                            size,
                            r,
                            g,
                            b,
                            a,
                            text,
                        } => {
                            let color = egui::Color32::from_rgba_premultiplied(*r, *g, *b, *a);
                            painter.text(
                                rect.min
                                    + egui::vec2(
                                        *x as f32 * render_scale_x,
                                        *y as f32 * render_scale_y,
                                    ),
                                egui::Align2::LEFT_TOP,
                                text,
                                egui::FontId::proportional(
                                    (*size).max(8) as f32 * render_scale_x.min(render_scale_y),
                                ),
                                color,
                            );
                            *idx += 1;
                        }
                        Op::PainterEnd => {
                            *idx += 1;
                            break;
                        }
                        Op::PainterRectStroke {
                            x,
                            y,
                            w,
                            h,
                            r,
                            g,
                            b,
                            a,
                            thickness,
                        } => {
                            let color = egui::Color32::from_rgba_premultiplied(*r, *g, *b, *a);
                            let rct = egui::Rect::from_min_size(
                                rect.min
                                    + egui::vec2(
                                        *x as f32 * render_scale_x,
                                        *y as f32 * render_scale_y,
                                    ),
                                egui::vec2(*w as f32 * render_scale_x, *h as f32 * render_scale_y),
                            );
                            painter.rect_stroke(
                                rct,
                                0.0,
                                egui::Stroke::new((*thickness).max(1) as f32, color),
                                egui::StrokeKind::Middle,
                            );
                            *idx += 1;
                        }
                        Op::PainterCircleStroke {
                            x,
                            y,
                            radius,
                            r,
                            g,
                            b,
                            a,
                            thickness,
                        } => {
                            let color = egui::Color32::from_rgba_premultiplied(*r, *g, *b, *a);
                            painter.circle_stroke(
                                rect.min
                                    + egui::vec2(
                                        *x as f32 * render_scale_x,
                                        *y as f32 * render_scale_y,
                                    ),
                                *radius as f32 * render_scale_x.min(render_scale_y),
                                egui::Stroke::new((*thickness).max(1) as f32, color),
                            );
                            *idx += 1;
                        }
                        Op::PainterCubicBezier {
                            x1,
                            y1,
                            cx1,
                            cy1,
                            cx2,
                            cy2,
                            x2,
                            y2,
                            r,
                            g,
                            b,
                            a,
                            thickness,
                        } => {
                            let color = egui::Color32::from_rgba_premultiplied(*r, *g, *b, *a);
                            let shape = egui::epaint::CubicBezierShape {
                                points: [
                                    rect.min
                                        + egui::vec2(
                                            *x1 as f32 * render_scale_x,
                                            *y1 as f32 * render_scale_y,
                                        ),
                                    rect.min
                                        + egui::vec2(
                                            *cx1 as f32 * render_scale_x,
                                            *cy1 as f32 * render_scale_y,
                                        ),
                                    rect.min
                                        + egui::vec2(
                                            *cx2 as f32 * render_scale_x,
                                            *cy2 as f32 * render_scale_y,
                                        ),
                                    rect.min
                                        + egui::vec2(
                                            *x2 as f32 * render_scale_x,
                                            *y2 as f32 * render_scale_y,
                                        ),
                                ],
                                closed: false,
                                fill: egui::Color32::TRANSPARENT,
                                stroke: egui::epaint::PathStroke::new(
                                    (*thickness).max(1) as f32,
                                    color,
                                ),
                            };
                            painter.add(shape);
                            *idx += 1;
                        }
                        _ => {
                            // Unexpected op inside painter - stop painter
                            break;
                        }
                    }
                }
            }
            Op::PainterRect { .. }
            | Op::PainterCircle { .. }
            | Op::PainterLine { .. }
            | Op::PainterText { .. }
            | Op::PainterRectStroke { .. }
            | Op::PainterCircleStroke { .. }
            | Op::PainterCubicBezier { .. }
            | Op::PainterEnd => {
                // These should be consumed within PainterBegin; skip to avoid infinite loop
                *idx += 1;
            }
        }
    }
}

impl BlaulichtApp {
    fn logs_ui(&mut self, ui: &mut egui::Ui, ctx: &Context) {
        self.log_window.draw(ctx, ui);

        // Terminal.
        // Right sidebar
        // Bottom log panel - fixed height
        // egui::TopBottomPanel::bottom("log_panel")
        //     .default_height(300.0)
        //     .show(ctx, |ui| {
        //     });
    }

    fn save_showfile(&mut self) {
        let mut config_mut = self.data.config.lock().unwrap();

        match config_mut.last_open_showfile.clone() {
            Some(ref path) => {
                let mut dmx = self.data.state.dmx_engine.write().unwrap();

                {
                    let plugin_state = self.data.state.plugin_state_storage.lock().unwrap();
                    dmx.0.plugin_state = plugin_state.clone();
                }

                let serialized = postcard::to_allocvec(&dmx.clone()).unwrap();
                std::fs::write(&path, serialized).unwrap();

                config_mut.last_open_showfile = Some(path.clone());

                let config_path = PathBuf::from_str(&self.data.config_path).unwrap();
                config::write_config(config_path, config_mut.clone()).unwrap();

                self.data
                    .system_message_sender
                    .send(SystemMessage::Log(
                        format!("Saved showfile to {path:?}"),
                        LogLevel::Info,
                    ))
                    .unwrap();

                mem::drop(config_mut);
                mem::drop(dmx);

                self.show_popup(PopupSpec::with_duration(
                    Duration::from_secs(2),
                    "Saved Showfile".to_string(),
                ));
            }
            None => {
                self.data
                    .system_message_sender
                    .send(SystemMessage::Log(
                        "No opened showfile, not saving".to_string(),
                        LogLevel::Err,
                    ))
                    .unwrap();

                mem::drop(config_mut);

                self.show_popup(PopupSpec::with_duration(
                    Duration::from_secs(2),
                    "No Showfile".to_string(),
                ));
            }
        }
    }

    fn system_ui(&mut self, ui: &mut egui::Ui, ctx: &Context) {
        if let Some(dialog) = &mut self.open_file_dialog {
            if dialog.show(ctx).selected() {
                let mut config = self.data.config.lock().unwrap();

                if let Some(file) = dialog.path() {
                    match self.file_dialog_open_origin {
                        FileDialogOpenOrigin::Save => {
                            // let dmx = self.data.state.dmx_engine.read().unwrap();
                            // let serialized = postcard::to_allocvec(&dmx.clone()).unwrap();
                            // std::fs::write(file, serialized).unwrap();

                            config.last_open_showfile = Some(file.to_path_buf());
                            //
                            // let config_path = PathBuf::from_str(&self.data.config_path).unwrap();
                            // config::write_config(config_path, config.clone()).unwrap();
                            //
                            // self.data
                            //     .system_message_sender
                            //     .send(SystemMessage::Log(
                            //         format!("Saved showfile to {file:?}"),
                            //         LogLevel::Info,
                            //     ))
                            //     .unwrap();

                            mem::drop(config);

                            self.save_showfile();
                        }
                        FileDialogOpenOrigin::Load => {
                            config.last_open_showfile = Some(file.to_path_buf());

                            let mut f = File::open(file).expect("no file found");
                            let metadata = fs::metadata(file).expect("unable to read metadata");
                            let mut buffer = vec![0; metadata.len() as usize];
                            f.read(&mut buffer).expect("buffer overflow");

                            let decoded: blaulicht_shared::EngineState =
                                postcard::from_bytes(&buffer).unwrap();

                            {
                                let mut plugin_state =
                                    self.data.state.plugin_state_storage.lock().unwrap();
                                *plugin_state = decoded.plugin_state.clone();
                            }

                            let mut dmx = self.data.state.dmx_engine.write().unwrap();
                            // dmx.overwrite(decoded);
                            dmx.load_showfile(decoded);
                            mem::drop(dmx);

                            config.last_open_showfile = Some(file.to_path_buf());

                            let config_path = PathBuf::from_str(&self.data.config_path).unwrap();
                            config::write_config(config_path, config.clone()).unwrap();

                            self.data
                                .system_message_sender
                                .send(SystemMessage::Log(
                                    format!("Loaded showfile from {file:?}"),
                                    LogLevel::Info,
                                ))
                                .unwrap();

                            mem::drop(config);

                            self.show_popup(PopupSpec::with_duration(
                                Duration::from_secs(2),
                                "Loaded Showfile".to_string(),
                            ));
                        }
                    }
                }
            }
        }

        let button_size = ButtonSize::Large.with_width(130.0);

        ui.vertical(|ui| {
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

                ui.label("Showfile:");
                ui.label(showfile);
            });

            ui.horizontal(|ui| {
                if components::button(ui, false, "Load Showfile", button_size) {
                    // Show only files with the extension "txt".
                    let filter = Box::new({
                        let ext = Some(OsStr::new("txt"));
                        move |path: &Path| -> bool { path.extension() == ext }
                    });

                    let config = self.data.config.lock().unwrap();

                    let mut dialog = FileDialog::open_file(config.last_open_showfile.clone())
                        .show_files_filter(filter);

                    dialog.open();
                    self.open_file_dialog = Some(dialog);
                    self.file_dialog_open_origin = FileDialogOpenOrigin::Load;
                }

                if components::button(ui, false, "Save to Showfile", button_size) {
                    // Show only files with the extension "txt".
                    let filter = Box::new({
                        let ext = Some(OsStr::new("txt"));
                        move |path: &Path| -> bool { path.extension() == ext }
                    });

                    let config = self.data.config.lock().unwrap();

                    let mut dialog = FileDialog::open_file(config.last_open_showfile.clone())
                        .show_files_filter(filter);

                    dialog.open();
                    self.open_file_dialog = Some(dialog);
                    self.file_dialog_open_origin = FileDialogOpenOrigin::Save;
                }

                let (label, allowed) = match &self
                    .data
                    .config
                    .lock()
                    .unwrap()
                    .last_open_showfile
                    .is_some()
                {
                    true => ("Save Showfile", true),
                    false => ("Sace Showfile", false),
                };
                if components::button(ui, allowed, label, button_size) {
                    self.save_showfile();
                }
            });

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

                if components::button(ui, ui.ctx().style().visuals.dark_mode, "DARK", button_size) {
                    ui.ctx().set_theme(ThemePreference::Dark);
                }
            });

            ui.add_space(15.0);
            ui.separator();
            ui.add_space(15.0);

            ui.horizontal(|ui| {
                if components::button(ui, false, "Quit", button_size) {
                    ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                }

                if components::button(ui, false, "Shutdown", button_size) {
                    self.confirm_shutdown_open = true;
                }

                if components::button(ui, self.debug_open, "Debug", button_size) {
                    self.debug_open = !self.debug_open;
                }
            });

            // ui.horizontal(|ui| {
            //     if components::button(ui, false, "SETUP", button_size) {
            //         // self.confirm_shutdown_open = true;
            //         // let mut dmx_engine = self.data.state.dmx_engine.write().unwrap();
            //         // dmx_engine.start_setup();
            //     }
            // });
        });

        if self.confirm_shutdown_open {
            components::dialog(
                ctx,
                "Confirm Shutdown",
                egui::vec2(200.0, 100.0),
                false,
                |ui| {
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
                                println!("Shutdown command executed successfully.");
                            } else {
                                eprintln!("Shutdown command failed!");
                            }

                            self.confirm_shutdown_open = false;
                        }

                        if components::button(ui, true, "Cancel", ButtonSize::Large) {
                            self.confirm_shutdown_open = false;
                        }
                    });
                },
            );
        }

        ui.separator();

        egui::SidePanel::right("right_panel")
            .resizable(true)
            .default_width(250.0)
            .width_range(200.0..=400.0)
            .show(ctx, |ui| {
                // --- Plugin Overview ---
                ui.label("Plugins");

                ui.add_space(4.0);

                {
                    let plugins = self.data.state.plugins.read().unwrap();
                    let current_visibility =
                        self.data.state.plugin_ui_visibility.read().unwrap().clone();

                    for (i, (id, plugin)) in plugins.iter().enumerate() {
                        let box_size = egui::vec2(ui.available_width(), 42.0);
                        ui.allocate_ui_with_layout(
                            box_size,
                            egui::Layout::top_down(egui::Align::Center),
                            |ui| {
                                let (rect, _response) =
                                    ui.allocate_exact_size(box_size, egui::Sense::hover());
                                let painter = ui.painter();

                                // State color and blinking logic
                                let mut show_border = true;
                                let border_color = match (plugin.has_errored(), plugin.is_enabled())
                                {
                                    // Alive and healthy.
                                    (false, true) => egui::Color32::from_rgb(0, 200, 0),
                                    // Dead, crashed.
                                    (true, true) => {
                                        let blink = ((self.animation_time * 8.0) as i32) % 2 == 0;
                                        show_border = blink;
                                        egui::Color32::from_rgb(200, 0, 0)
                                    }
                                    // Disabled
                                    (_, false) => {
                                        let blink = ((self.animation_time * 2.0) as i32) % 2 == 0;
                                        show_border = blink;
                                        egui::Color32::from_rgb(200, 200, 0)
                                    }
                                };

                                // Draw the main box
                                painter.rect_filled(rect, 0.0, egui::Color32::from_gray(30));

                                // Draw the left border if needed
                                if show_border {
                                    let border_width = 6.0;
                                    let border_rect = egui::Rect::from_min_max(
                                        rect.left_top(),
                                        rect.left_bottom() + egui::vec2(border_width, 0.0),
                                    );
                                    painter.rect_filled(border_rect, 0.0, border_color);
                                }

                                // Plugin name
                                let name = format!("P:{} ({})", plugin.path, i + 1);
                                painter.text(
                                    rect.center(),
                                    egui::Align2::CENTER_CENTER,
                                    name,
                                    egui::FontId::monospace(10.0),
                                    if plugin.has_errored() {
                                        Color32::WHITE
                                    } else {
                                        egui::Color32::from_gray(90)
                                    },
                                );
                            },
                        );
                        ui.horizontal(|ui| {
                            let is_open = *current_visibility.get(id).unwrap_or(&false);
                            let label = if is_open { "Hide UI" } else { "Show UI" };
                            if ui.small_button(label).clicked() {
                                let mut map = self.data.state.plugin_ui_visibility.write().unwrap();
                                let entry = map.entry(*id).or_insert(false);
                                *entry = !*entry;
                            }
                        });
                        ui.add_space(8.0);
                    }
                    ui.separator();

                    mem::drop(plugins)
                }

                // --- Loop Speed & Tick Speed Graphs ---
                ui.horizontal(|ui| {
                    let graph_width = (ui.available_width() - 16.0) / 2.0;
                    let graph_height = 36.0;
                    // Loop Speed Graph
                    let (loop_resp, loop_painter) = ui.allocate_painter(
                        egui::vec2(graph_width, graph_height),
                        egui::Sense::hover(),
                    );
                    // Draw value and label above the bar, left-aligned
                    let loop_val_str = match self.loop_speed {
                        v if v > 1000 => format!("{:.0} ms", self.loop_speed / 1000),
                        _ => format!("{:.0} us", self.loop_speed),
                    };
                    let label_y = loop_resp.rect.top() + 2.0;
                    let label_x = loop_resp.rect.left() + 4.0;
                    loop_painter.text(
                        egui::pos2(label_x, label_y),
                        egui::Align2::LEFT_TOP,
                        &loop_val_str,
                        egui::FontId::monospace(10.0),
                        Color32::WHITE,
                    );
                    loop_painter.text(
                        egui::pos2(label_x + 60.0, label_y), // 60px offset for value
                        egui::Align2::LEFT_TOP,
                        "Loop",
                        egui::FontId::proportional(10.0),
                        Color32::WHITE,
                    );
                    // Draw loop speed as a bar (simple visualization)
                    let loop_val = self.loop_speed as f32;
                    let loop_max = DMX_TICK_TIME.as_micros() as f32;
                    let loop_bar_width = (loop_val / loop_max).min(1.0) * (graph_width - 8.0);
                    let loop_bar_rect = egui::Rect::from_min_max(
                        loop_resp.rect.left_top() + egui::vec2(4.0, 16.0),
                        loop_resp.rect.left_top()
                            + egui::vec2(4.0 + loop_bar_width, graph_height - 8.0),
                    );
                    let color = match self.loop_speed {
                        v if v as f32 > loop_max => Color32::RED,
                        _ => Color32::from_rgb(0, 200, 255),
                    };
                    loop_painter.rect_filled(loop_bar_rect, 2.0, color);

                    // Tick Speed Graph
                    let (tick_resp, tick_painter) = ui.allocate_painter(
                        egui::vec2(graph_width, graph_height),
                        egui::Sense::hover(),
                    );
                    let tick_val_str = format!("{:.0} μs", self.tick_speed);
                    let tick_label_y = tick_resp.rect.top() + 2.0;
                    let tick_label_x = tick_resp.rect.left() + 4.0;
                    tick_painter.text(
                        egui::pos2(tick_label_x, tick_label_y),
                        egui::Align2::LEFT_TOP,
                        &tick_val_str,
                        egui::FontId::monospace(10.0),
                        Color32::WHITE,
                    );
                    tick_painter.text(
                        egui::pos2(tick_label_x + 60.0, tick_label_y),
                        egui::Align2::LEFT_TOP,
                        "Tick",
                        egui::FontId::proportional(10.0),
                        Color32::WHITE,
                    );
                    let tick_val = self.tick_speed as f32;
                    let tick_max = 1000.0; // 1 MS
                    let tick_bar_width = (tick_val / tick_max).min(1.0) * (graph_width - 8.0);
                    let tick_bar_rect = egui::Rect::from_min_max(
                        tick_resp.rect.left_top() + egui::vec2(4.0, 16.0),
                        tick_resp.rect.left_top()
                            + egui::vec2(4.0 + tick_bar_width, graph_height - 8.0),
                    );
                    tick_painter.rect_filled(tick_bar_rect, 2.0, Color32::from_rgb(255, 200, 0));
                });

                ui.add_space(8.0);

                // --- Heartbeat Indicator ---
                ui.horizontal(|ui| {
                    // Heartbeat: small circle, green if recent, gray if not
                    let heartbeat_recent = self.last_heartbeat_frame + 30 > self.frame_count;
                    let color = if heartbeat_recent {
                        Color32::from_rgb(0, 255, 0)
                    } else {
                        Color32::from_gray(80)
                    };
                    ui.painter().circle_filled(
                        ui.cursor().left_top() + egui::vec2(10.0, 10.0),
                        8.0,
                        color,
                    );
                    ui.label("Heartbeat");
                });

                // --- Reload button ---
                {
                    let signal = *self.data.state.mainloop_state.read().unwrap();

                    ui.label(format!("LOOP {:?}", signal));

                    let bg_color = match signal {
                        AudioThreadControlSignal::CONTINUE => egui::Color32::from_gray(40),
                        AudioThreadControlSignal::ABORT
                        | AudioThreadControlSignal::ABORTED
                        | AudioThreadControlSignal::CRASHED => egui::Color32::DARK_RED,
                        AudioThreadControlSignal::RELOAD => egui::Color32::from_rgb(60, 120, 200),
                    };

                    let rect = ui.allocate_exact_size(egui::vec2(90.0, 60.0), egui::Sense::click());
                    let painter = ui.painter();
                    painter.rect_filled(rect.0, 0.0, bg_color);

                    painter.text(
                        rect.0.center(),
                        egui::Align2::CENTER_CENTER,
                        "RELOAD",
                        egui::FontId::proportional(16.0),
                        egui::Color32::WHITE,
                    );

                    if rect.1.clicked() && signal != AudioThreadControlSignal::RELOAD {
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

        // Removed global Plugin UI section; per-plugin windows are rendered globally below.
    }

    fn left_panel_ui(&mut self, ctx: &Context) {
        const WIDTH: f32 = 45.0;

        egui::SidePanel::left("navbar")
            .resizable(false)
            .default_width(WIDTH)
            // .width_range(150.0..=300.0)
            .show(ctx, |ui| {
                // ui.label("Pages");
                // ui.add_space(8.0);

                let button_count = AppPage::iter().count();
                let spacing_top_bottom = 3.0;
                let spacing = 6.0;
                let button_height = ui.available_height() / button_count as f32;
                let button_size = ButtonSize::Large
                    .with_height(button_height - spacing - spacing_top_bottom)
                    .with_width(WIDTH)
                    .with_font_size(20.0);

                ui.add_space(spacing_top_bottom);

                for (idx, page) in AppPage::iter().enumerate() {
                    let is_selected = self.current_page == page;
                    let label = page.short().to_uppercase();
                    let label = page.icon();

                    // ui.label(
                    //     ,
                    // );

                    if components::button(ui, is_selected, &label, button_size) && !is_selected {
                        self.current_page = page;
                    }

                    if idx + 1 < button_count {
                        ui.add_space(spacing);
                    }
                }

                // // TODO: loop here.
                // if ui
                //     .selectable_label(self.current_page == AppPage::Main, "Main")
                //     .clicked()
                // {
                //     self.current_page = AppPage::Main;
                // }
                // if ui
                //     .selectable_label(self.current_page == AppPage::Fixtures, "Fixtures")
                //     .clicked()
                // {
                //     self.current_page = AppPage::Fixtures;
                // }
                // if ui
                //     .selectable_label(self.current_page == AppPage::Animations, "Animations")
                //     .clicked()
                // {
                //     self.current_page = AppPage::Animations;
                // }

                // ui.add_space(16.0);
                // ui.label("Tools");
                // ui.add_space(8.0);
                //
                // if ui.button("Calculator").clicked() {
                //     // Placeholder action
                // }
                // if ui.button("Notes").clicked() {
                //     // Placeholder action
                // }
                //
                // ui.add_space(16.0);
                // ui.label("Status");
                // ui.add_space(8.0);
                // ui.label("Online");
                // ui.label("Last updated: Now");
            });
    }
}

fn powered_by_egui_and_eframe(ui: &mut egui::Ui) {
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = 0.0;
        ui.label("Powered by ");
        ui.hyperlink_to("egui", "https://github.com/emilk/egui");
        ui.label(" and ");
        ui.hyperlink_to(
            "eframe",
            "https://github.com/emilk/egui/tree/master/crates/eframe",
        );
        ui.label(".");
    });
}
