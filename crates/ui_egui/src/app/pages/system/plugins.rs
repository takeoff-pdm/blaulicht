//! System › Plugins on the desktop (086, `system.html#plugins`): the
//! plugins list on the left; on the right the selected plugin (status,
//! file, reload on change, Show UI) over its Interface pane, where the
//! plugin's own UI is drawn embedded (`RULES.md` decision 3), with Pop out
//! to move it into its own window. The kiosk keeps its list of boxes and
//! floating plugin windows.

use super::desk::{self, State};
use crate::app::components::studio;
use crate::app::pages::setup::duo;
use crate::app::ui_tokens::{studio as tokens, Density, Weight, LIST_ROW_HEIGHT, TEXT};
use crate::app::BlaulichtApp;
use blaulicht_engine::{client::EngineClient, hub::HostCommand};
use egui::{pos2, vec2, Align2, Rect, Sense, Stroke, Ui};
use std::path::Path;

/// One plugin as the page shows it.
struct Row {
    id: u8,
    name: String,
    path: String,
    enabled: bool,
    errored: bool,
    reload_on_change: bool,
}

impl Row {
    fn state(&self) -> (State, &'static str) {
        if !self.enabled {
            (State::Off, "Disabled")
        } else if self.errored {
            (State::Bad, "Crashed")
        } else {
            (State::Ok, "Running")
        }
    }
}

impl BlaulichtApp {
    pub(super) fn render_plugins_desktop(&mut self, ui: &mut Ui) {
        let t = tokens(ui.ctx());
        let mut rows: Vec<Row> = self.client.with_plugins(|plugins| {
            plugins
                .iter()
                .map(|(id, p)| Row {
                    id: *id,
                    name: Path::new(p.path.as_ref())
                        .file_stem()
                        .map_or_else(|| p.path.to_string(), |s| s.to_string_lossy().into_owned()),
                    path: p.path.to_string(),
                    enabled: p.is_enabled(),
                    errored: p.has_errored(),
                    reload_on_change: p.reload_on_change,
                })
                .collect()
        });
        rows.sort_by_key(|r| r.id);
        let selected = match self.system_ui_state.selected_plugin {
            Some(id) if rows.iter().any(|r| r.id == id) => Some(id),
            _ => rows.first().map(|r| r.id),
        };
        self.system_ui_state.selected_plugin = selected;

        let (mut list, mut detail) = duo::duo(ui);

        // The list.
        let pane_bg = list.painter().add(egui::Shape::Noop);
        let header = studio::pane_header(&mut list, None, "Plugins", false, |_| {});
        list.painter()
            .set(pane_bg, egui::Shape::rect_filled(header, 0.0, t.pane));
        if rows.is_empty() {
            list.add_space(8.0);
            list.horizontal(|ui| {
                ui.add_space(8.0);
                duo::note(ui, "No plugins in config.toml.");
            });
        }
        let name_font = Density::of(ui.ctx()).font(Weight::Regular, TEXT + 1.0);
        for row in &rows {
            let (rect, response) = list.allocate_exact_size(
                vec2(list.available_width(), LIST_ROW_HEIGHT),
                Sense::click(),
            );
            let active = Some(row.id) == selected;
            let p = list.painter();
            if active {
                p.rect_filled(rect, 0.0, t.blue_dim);
                p.rect_filled(
                    Rect::from_min_size(rect.min, vec2(2.0, rect.height())),
                    0.0,
                    t.blue,
                );
            } else if response.hovered() {
                p.rect_filled(rect, 0.0, t.head);
            }
            p.hline(
                rect.x_range(),
                rect.bottom() - 0.5,
                Stroke::new(1.0_f32, t.hair),
            );
            desk::paint_square(
                p,
                pos2(rect.left() + 12.0, rect.center().y),
                row.state().0,
                t,
            );
            let cut = studio::paint_truncated(
                &list,
                pos2(rect.left() + 24.0, rect.center().y),
                &row.name,
                name_font.clone(),
                if active { t.strong } else { t.text },
                rect.width() - 32.0,
            );
            let response = if cut {
                response.on_hover_text(&row.name)
            } else {
                response
            };
            response.widget_info(|| {
                egui::WidgetInfo::selected(
                    egui::WidgetType::SelectableLabel,
                    true,
                    active,
                    &row.name,
                )
            });
            if response.clicked() {
                self.system_ui_state.selected_plugin = Some(row.id);
            }
        }

        // The detail.
        let Some(row) = rows.iter().find(|r| Some(r.id) == selected) else {
            studio::pane_header(&mut detail, None, "Plugins", false, |_| {});
            return;
        };
        let open = self.view_state.plugin_open_state(row.id).open;
        let popped_out = cfg!(feature = "plugin-popout") && self.view_state.is_popped_out(row.id);
        let mut toggle_ui = false;
        studio::pane_header(&mut detail, None, &row.name, false, |ui| {
            ui.add_enabled_ui(row.enabled, |ui| {
                let label = if open { "Hide UI" } else { "Show UI" };
                toggle_ui = studio::mini_button(ui, label, open, crate::app::ui_tokens::TARGET_MIN)
                    .clicked();
            });
        });
        let (state, state_text) = row.state();
        let info_h = 8.0
            + 3.0 * duo::KV_ROW
            + 2.0 * duo::KV_GAP
            + 8.0
            + if state == State::Bad {
                6.0 + 16.0 + 6.0 + 24.0
            } else {
                0.0
            };
        let mut reload = false;
        duo::body(&mut detail, Some(info_h), |ui| {
            ui.spacing_mut().item_spacing.y = duo::KV_GAP;
            duo::kv_row(ui, "Status", |ui| desk::state_label(ui, state, state_text));
            duo::kv_row(ui, "File", |ui| {
                ui.label(
                    egui::RichText::new(&row.path)
                        .font(duo::mono_font())
                        .color(t.strong),
                );
            });
            duo::kv_row(ui, "Reload on change", |ui| {
                duo::text(ui, if row.reload_on_change { "On" } else { "Off" });
            });
            if state == State::Bad {
                ui.add_space(6.0);
                desk::error_text(
                    ui,
                    "The plugin stopped after an error. Its log is on the Logs page.",
                );
                ui.add_space(6.0);
                // `mini primary` in the mockup.
                reload = studio::mini_button(
                    ui,
                    "Reload plugin",
                    true,
                    crate::app::ui_tokens::TARGET_MIN,
                )
                .clicked();
            }
        });
        // The 1px gutter between the panes.
        let (gap, _) =
            detail.allocate_exact_size(vec2(detail.available_width(), 1.0), Sense::hover());
        detail.painter().rect_filled(gap, 0.0, t.gutter);

        let mut pop = false;
        studio::pane_header(&mut detail, None, "Interface", false, |ui| {
            if open && !popped_out && cfg!(feature = "plugin-popout") {
                pop = duo::chip(ui, "Pop out");
            }
        });
        let well = detail.available_rect_before_wrap().shrink(8.0);
        detail.allocate_rect(detail.available_rect_before_wrap(), Sense::hover());
        detail.painter().rect_filled(well, 0.0, t.well);
        let message = if !row.enabled {
            Some("This plugin is disabled in config.toml.")
        } else if !open {
            Some("Show UI to open this plugin's interface here.")
        } else if popped_out {
            Some("The interface is in its own window.")
        } else {
            None
        };
        match message {
            Some(message) => {
                detail.painter().text(
                    well.center(),
                    Align2::CENTER_CENTER,
                    message,
                    Density::of(ui.ctx()).font(Weight::Regular, TEXT),
                    t.muted,
                );
            }
            None => {
                let mut inner = detail.new_child(
                    egui::UiBuilder::new()
                        .id_salt(("plugin-interface", row.id))
                        .max_rect(well.shrink(8.0))
                        .layout(egui::Layout::top_down(egui::Align::Min)),
                );
                inner.set_clip_rect(well);
                egui::ScrollArea::both()
                    .id_salt(("plugin-interface-scroll", row.id))
                    .auto_shrink([false, false])
                    .show(&mut inner, |ui| {
                        self.render_plugin_ui_contents(ui, row.id);
                    });
            }
        }

        if toggle_ui {
            self.view_state.set_plugin_open(row.id, !open);
        }
        if pop {
            self.view_state.set_popped_out(row.id, true);
        }
        if reload {
            self.send_command(HostCommand::ReloadPlugins, |_, _| {});
        }
    }
}
