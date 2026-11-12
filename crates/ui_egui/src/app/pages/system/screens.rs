use crate::app::{
    components::{self, clickable, ButtonSize, Dialog},
    BlaulichtApp,
};
use blaulicht_engine::client::EngineClient;
use egui::{Color32, Context, FontId, Frame, Margin, RichText, ScrollArea};
use std::path::Path;

impl BlaulichtApp {
    pub fn render_screens_dialog(&mut self, ctx: &Context) {
        if !self.system_ui_state.screens_dialog_open {
            return;
        }

        Dialog::new("Screens".to_string(), egui::vec2(560.0, 460.0))
            .with_backdrop()
            .show(ctx, |ui| {
                ui.heading(RichText::new("External Screens").strong());

                ui.add_space(8.0);
                ui.label(
                    RichText::new(
                        "External screens are extra windows mirroring the UI. They are restored \
                         from the showfile on startup.",
                    )
                    .color(Color32::from_gray(150)),
                );

                ui.add_space(12.0);

                if components::button(ui, true, "Add Screen", ButtonSize::Medium) {
                    self.add_external_screen();
                }

                ui.add_space(12.0);
                ui.separator();
                ui.add_space(12.0);

                let count = self.external_screens.len();
                ui.label(RichText::new(format!("Active Screens ({count})")).strong());

                if self.external_screens.is_empty() {
                    ui.add_space(6.0);
                    ui.label("No external screens.");
                    self.render_screens_close_button(ui);
                    return;
                }

                let row_height = ButtonSize::Medium.dim().0.y.max(44.0);
                let mut delete_index: Option<usize> = None;

                ScrollArea::vertical()
                    .auto_shrink([false, true])
                    .max_height(280.0)
                    .show(ui, |ui| {
                        for (index, screen) in self.external_screens.iter().enumerate() {
                            let dimensions = screen.dimensions;
                            let position = screen.position;
                            let owner_plugin_id = screen.owner_plugin_id;
                            let is_plugin_owned = owner_plugin_id.is_some();

                            ui.add_space(6.0);

                            Frame::NONE
                                .fill(ui.visuals().faint_bg_color)
                                .inner_margin(Margin::symmetric(12, 4))
                                .show(ui, |ui| {
                                    ui.set_min_height(row_height);
                                    ui.allocate_ui_with_layout(
                                        egui::vec2(ui.available_width(), row_height),
                                        egui::Layout::left_to_right(egui::Align::Center),
                                        |ui| {
                                            ui.label(
                                                RichText::new(format!("#{index}"))
                                                    .font(FontId::monospace(15.0))
                                                    .strong(),
                                            );

                                            ui.add_space(12.0);

                                            let geometry = match position {
                                                Some(pos) => format!(
                                                    "{:.0}x{:.0}  @ ({:.0}, {:.0})",
                                                    dimensions.x, dimensions.y, pos.x, pos.y
                                                ),
                                                None => format!(
                                                    "{:.0}x{:.0}",
                                                    dimensions.x, dimensions.y
                                                ),
                                            };
                                            ui.label(
                                                RichText::new(geometry)
                                                    .font(FontId::monospace(13.0)),
                                            );

                                            if let Some(pid) = owner_plugin_id {
                                                ui.add_space(8.0);
                                                ui.label(
                                                    RichText::new(format!("PLUGIN({pid})"))
                                                        .color(Color32::from_rgb(120, 170, 255))
                                                        .strong(),
                                                );
                                            }

                                            ui.with_layout(
                                                egui::Layout::right_to_left(egui::Align::Center),
                                                |ui| {
                                                    if is_plugin_owned {
                                                        return;
                                                    }

                                                    let delete_color =
                                                        Color32::from_rgb(160, 45, 45);
                                                    if clickable(
                                                        ui,
                                                        false,
                                                        delete_color,
                                                        ButtonSize::Medium.with_width(row_height),
                                                        |ui, rect, fg_color| {
                                                            ui.painter().text(
                                                                rect.center(),
                                                                egui::Align2::CENTER_CENTER,
                                                                egui_phosphor::regular::TRASH,
                                                                FontId::monospace(18.0),
                                                                fg_color,
                                                            );
                                                        },
                                                    ) {
                                                        delete_index = Some(index);
                                                    }
                                                },
                                            );
                                        },
                                    );
                                });
                        }
                    });

                if let Some(index) = delete_index {
                    if self.remove_external_screen(index) {
                        tracing::info!("Removed external screen #{index}");
                    }
                }

                self.render_screens_close_button(ui);
            });
    }

    fn render_screens_close_button(&mut self, ui: &mut egui::Ui) {
        ui.add_space(12.0);
        if components::button(ui, true, "Close", ButtonSize::Medium) {
            self.system_ui_state.screens_dialog_open = false;
        }
    }
}

/// Height of a two-line screen row (`.li.scr`).
const SCREEN_ROW: f32 = 38.0;
/// Width of the screens list pane (`.md`).
const SCREEN_LIST_W: f32 = 240.0;

impl BlaulichtApp {
    /// System › Screens on the desktop (084, `system.html#screens`): the
    /// screens list on the left, the selected screen on the right. Rows
    /// say who controls the screen: the owning plugin with a link icon, or
    /// "Desk window", then the size. The kiosk keeps its own list.
    pub(super) fn render_screens_desktop(&mut self, ui: &mut egui::Ui) {
        use super::super::setup::duo;
        use crate::app::components::studio;
        use crate::app::ui_tokens::{studio as tokens, Density, Weight, TEXT, TEXT_MIN};
        use egui::{pos2, vec2, Align2, Rect, Sense, Stroke};

        let t = tokens(ui.ctx());
        let full = ui.max_rect().expand(8.0).intersect(ui.clip_rect());
        ui.allocate_rect(ui.max_rect(), Sense::hover());
        ui.painter().rect_filled(full, 0.0, t.gutter);
        let list_rect = Rect::from_min_size(full.min, vec2(SCREEN_LIST_W, full.height()));
        let detail_rect = Rect::from_min_max(pos2(list_rect.right() + 1.0, full.top()), full.max);
        let child = |ui: &mut egui::Ui, rect: Rect, fill, salt: &str| {
            ui.painter().rect_filled(rect, 0.0, fill);
            let mut c = ui.new_child(
                egui::UiBuilder::new()
                    .id_salt(salt)
                    .max_rect(rect)
                    .layout(egui::Layout::top_down(egui::Align::Min)),
            );
            c.set_clip_rect(rect);
            c.spacing_mut().item_spacing = vec2(0.0, 0.0);
            c
        };
        let mut list = child(ui, list_rect, t.list, "screens-list");
        let mut detail = child(ui, detail_rect, t.pane, "screens-detail");

        // (owner plugin name, width, height) per screen.
        let screens: Vec<(Option<String>, f32, f32)> = self
            .external_screens
            .iter()
            .map(|screen| {
                let owner = screen.owner_plugin_id.map(|pid| {
                    self.client.with_plugins(|plugins| {
                        plugins
                            .get(&pid)
                            .and_then(|p| {
                                Path::new(p.path.as_ref())
                                    .file_stem()
                                    .map(|s| s.to_string_lossy().into_owned())
                            })
                            .unwrap_or_else(|| format!("plugin {pid}"))
                    })
                });
                (owner, screen.dimensions.x, screen.dimensions.y)
            })
            .collect();
        if self.system_ui_state.selected_screen >= screens.len() {
            self.system_ui_state.selected_screen = screens.len().saturating_sub(1);
        }
        let selected = self.system_ui_state.selected_screen;

        let mut add = false;
        let pane_bg = list.painter().add(egui::Shape::Noop);
        let header = studio::pane_header(&mut list, None, "Screens", false, |ui| {
            add = duo::chip(ui, "Add");
        });
        list.painter()
            .set(pane_bg, egui::Shape::rect_filled(header, 0.0, t.pane));
        let name_font = Density::of(ui.ctx()).font(Weight::Regular, TEXT + 1.0);
        let sub_font = Density::of(ui.ctx()).font(Weight::Regular, TEXT_MIN);
        if screens.is_empty() {
            list.add_space(8.0);
            list.horizontal(|ui| {
                ui.add_space(8.0);
                duo::note(ui, "Nothing here yet.");
            });
        }
        for (index, (owner, w, h)) in screens.iter().enumerate() {
            let (rect, response) =
                list.allocate_exact_size(vec2(list.available_width(), SCREEN_ROW), Sense::click());
            let active = index == selected;
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
            // The status square: a screen in the list is open.
            p.rect_filled(
                Rect::from_center_size(pos2(rect.left() + 12.0, rect.center().y), vec2(8.0, 8.0)),
                0.0,
                t.ok,
            );
            let x = rect.left() + 24.0;
            let name = format!("Screen {index}");
            p.text(
                pos2(x, rect.center().y - 7.0),
                Align2::LEFT_CENTER,
                &name,
                name_font.clone(),
                if active { t.strong } else { t.text },
            );
            let size = format!("{w:.0} x {h:.0}");
            let sub_y = rect.center().y + 8.0;
            let sub_color = if active { t.strong } else { t.muted };
            match owner {
                Some(plugin) => {
                    let icon = p.text(
                        pos2(x, sub_y),
                        Align2::LEFT_CENTER,
                        egui_phosphor::regular::LINK,
                        FontId::proportional(TEXT_MIN),
                        t.blue,
                    );
                    let mut sx = icon.right() + 3.0;
                    let name_rect = p.text(
                        pos2(sx, sub_y),
                        Align2::LEFT_CENTER,
                        plugin,
                        Density::of(ui.ctx()).font(Weight::SemiBold, TEXT_MIN),
                        t.blue,
                    );
                    sx = name_rect.right();
                    p.text(
                        pos2(sx, sub_y),
                        Align2::LEFT_CENTER,
                        format!(", {size}"),
                        sub_font.clone(),
                        sub_color,
                    );
                    let plugin = plugin.clone();
                    response
                        .clone()
                        .on_hover_text(format!("Controlled by plugin {plugin}"));
                }
                None => {
                    p.text(
                        pos2(x, sub_y),
                        Align2::LEFT_CENTER,
                        format!("Desk window, {size}"),
                        sub_font.clone(),
                        sub_color,
                    );
                }
            }
            response.widget_info(|| {
                egui::WidgetInfo::selected(egui::WidgetType::SelectableLabel, true, active, &name)
            });
            if response.clicked() {
                self.system_ui_state.selected_screen = index;
            }
        }

        let mut remove = None;
        match screens.get(selected) {
            Some((owner, w, h)) => {
                studio::pane_header(
                    &mut detail,
                    None,
                    &format!("Screen {selected}"),
                    false,
                    |ui| {
                        let tag = match owner {
                            Some(plugin) => format!("Owned by plugin {plugin}"),
                            None => "Desk window".to_string(),
                        };
                        let f = Density::of(ui.ctx()).font(Weight::Regular, TEXT);
                        let tw = studio::text_width(ui, &tag, f.clone());
                        let (r, _) = ui.allocate_exact_size(vec2(tw + 6.0, 24.0), Sense::hover());
                        ui.painter().text(
                            pos2(r.left(), r.center().y),
                            Align2::LEFT_CENTER,
                            tag,
                            f,
                            t.muted,
                        );
                        if owner.is_none() && duo::chip(ui, "Remove") {
                            remove = Some(selected);
                        }
                    },
                );
                // The preview: the screen's shape, centred in a well.
                let area = detail.available_rect_before_wrap().shrink(8.0);
                detail.painter().rect_filled(area, 0.0, t.well);
                let aspect = (*w / h.max(1.0)).max(0.01);
                // `.screenprev`: at most 560px wide inside 16px padding.
                let max = area.shrink(16.0);
                let mut size = vec2(max.width().min(560.0), max.width().min(560.0) / aspect);
                if size.y > max.height() {
                    size = vec2(max.height() * aspect, max.height());
                }
                let preview = Rect::from_center_size(area.center(), size);
                detail.painter().rect_filled(preview, 0.0, t.ctl);
                detail.painter().rect_stroke(
                    preview,
                    0.0,
                    Stroke::new(1.0_f32, t.line),
                    egui::StrokeKind::Outside,
                );
                detail.painter().text(
                    preview.center(),
                    Align2::CENTER_CENTER,
                    format!("{w:.0} x {h:.0}"),
                    FontId::monospace(TEXT),
                    t.strong,
                );
            }
            None => {
                studio::pane_header(&mut detail, None, "Screens", false, |_| {});
                detail.add_space(8.0);
                detail.horizontal(|ui| {
                    ui.add_space(8.0);
                    duo::note(
                        ui,
                        "No external screens. Add one to open another output window.",
                    );
                });
            }
        }

        if add {
            self.add_external_screen();
            self.system_ui_state.selected_screen = self.external_screens.len().saturating_sub(1);
        }
        if let Some(index) = remove {
            if self.remove_external_screen(index) {
                tracing::info!("Removed external screen #{index}");
            }
        }
    }
}
