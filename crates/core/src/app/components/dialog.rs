use egui::{vec2, Color32, CornerRadius, Frame, Id, LayerId, Margin, Order, Stroke, Ui, Vec2};
use std::cell::Cell;

thread_local! {
    static MODAL_DEPTH: Cell<usize> = Cell::new(0);
}

struct ModalDepthGuard {
    previous_depth: usize,
}

impl ModalDepthGuard {
    fn push() -> Self {
        let previous_depth = MODAL_DEPTH.with(|depth| {
            let prev = depth.get();
            depth.set(prev + 1);
            prev
        });

        Self { previous_depth }
    }

    fn previous_depth(&self) -> usize {
        self.previous_depth
    }
}

impl Drop for ModalDepthGuard {
    fn drop(&mut self) {
        MODAL_DEPTH.with(|depth| {
            depth.set(self.previous_depth);
        });
    }
}

pub struct DialogBuilder {
    label: String,
    popup_size: Vec2,
    fixed_pos: Option<Vec2>,
    moveable: bool,
    with_backdrop: bool,
    backdrop_color: Option<Color32>,
    /// Anchor to the top-right of the screen (keeps auto-height) instead of
    /// centering.
    anchor_right: bool,
}

pub struct Dialog {
    builder: DialogBuilder,
    backdrop_clicked: bool,
}

impl Dialog {
    const BACKDROP_COLOR: Color32 = Color32::from_black_alpha(180);

    pub fn new(label: String, popup_size: Vec2) -> Self {
        Self {
            builder: DialogBuilder {
                label,
                popup_size,
                fixed_pos: None,
                moveable: false,
                with_backdrop: false,
                backdrop_color: None,
                anchor_right: false,
            },
            backdrop_clicked: false,
        }
    }

    /// Anchor the dialog to the top-right edge of the screen (auto-height).
    pub fn anchor_right(self) -> Self {
        Self {
            builder: DialogBuilder {
                anchor_right: true,
                ..self.builder
            },
            ..self
        }
    }

    pub fn moveable(self) -> Self {
        Self {
            builder: DialogBuilder {
                moveable: true,
                ..self.builder
            },
            ..self
        }
    }

    pub fn with_backdrop(self) -> Self {
        Self {
            builder: DialogBuilder {
                with_backdrop: true,
                ..self.builder
            },
            ..self
        }
    }

    // pub fn raw_layout(self) -> Self {
    //     Self {
    //         raw_layout: true,
    //         ..self
    //     }
    // }

    // pub fn with_backdrop_clickable(self) -> Self {
    //     Self {
    //         with_backdrop: true,
    //         ..self
    //     }
    // }

    pub fn backdrop_color(self, color: Color32) -> Self {
        Self {
            builder: DialogBuilder {
                backdrop_color: Some(color),
                ..self.builder
            },
            ..self
        }
    }

    pub fn fixed_pos(self, pos: Vec2) -> Self {
        Self {
            builder: DialogBuilder {
                fixed_pos: Some(pos),
                ..self.builder
            },
            ..self
        }
    }

    pub fn backdrop_clicked(&self) -> bool {
        debug_assert!(
            self.builder.with_backdrop,
            "Function can only be used when backdrop is active"
        );
        self.backdrop_clicked
    }

    pub fn show(&mut self, ctx: &egui::Context, add_contents: impl FnOnce(&mut Ui)) {
        let modal_guard = self.builder.with_backdrop.then(ModalDepthGuard::push);

        let order = if modal_guard
            .as_ref()
            .map_or(false, |guard| guard.previous_depth() > 0)
        {
            Order::Tooltip
        } else {
            Order::Foreground
        };
        let window_id = Id::new(format!("dialog::{}", self.builder.label));
        let window_layer = LayerId::new(order, window_id);

        let backdrop_layer = if self.builder.with_backdrop {
            let (layer, clicked) = Self::render_popup_backdrop(
                ctx,
                window_id.with("backdrop"),
                order,
                self.builder.backdrop_color,
            );

            self.backdrop_clicked = clicked;

            Some(layer)
        } else {
            None
        };

        ctx.memory_mut(|mem| {
            let areas = mem.areas_mut();

            if let Some(backdrop_layer) = backdrop_layer {
                areas.move_to_top(backdrop_layer);
                areas.set_sublayer(backdrop_layer, window_layer);
            }

            areas.move_to_top(window_layer);
            mem.set_modal_layer(window_layer);
        });

        match self.builder.fixed_pos {
            Some(pos) => self.show_fixed_pos(ctx, window_id, order, pos, add_contents),
            None => self.show_dynamic_pos(ctx, window_id, order, add_contents),
        }
    }

    fn show_dynamic_pos(
        &self,
        ctx: &egui::Context,
        window_id: Id,
        order: Order,
        add_contents: impl FnOnce(&mut Ui),
    ) {
        let screen_rect = ctx.screen_rect();

        // println!(
        //     "center: {} {}",
        //     screen_rect.center().x,
        //     screen_rect.center().y
        // );
        let center_pos = if self.builder.anchor_right {
            const MARGIN: f32 = 8.0;
            egui::Pos2::new(
                screen_rect.right() - self.builder.popup_size.x - MARGIN,
                screen_rect.top() + MARGIN,
            )
        } else {
            egui::Pos2::new(
                screen_rect.center().x - self.builder.popup_size.x / 2.0,
                screen_rect.center().y - self.builder.popup_size.y / 2.0,
            )
        };

        // Clamp to screen boundaries
        // center_pos.x = center_pos
        //     .x
        //     .clamp(screen_rect.left(), screen_rect.right() - popup_size.x);
        // center_pos.y = center_pos
        //     .y
        //     .clamp(screen_rect.top(), screen_rect.bottom() - popup_size.y);

        let window_proto = egui::Window::new(&self.builder.label)
            // .min_size(popup_size)
            .fixed_size(vec2(self.builder.popup_size.x, 0.0))
            .collapsible(false)
            .resizable(false)
            .title_bar(false)
            .id(window_id)
            .order(order);

        let window_proto = match self.builder.moveable {
            true => window_proto.default_pos(center_pos),
            false => window_proto.fixed_pos(center_pos),
        };

        window_proto
            .frame(Frame {
                corner_radius: CornerRadius::same(1),
                fill: Color32::from_gray(40),
                stroke: Stroke::new(1.0, Color32::from_gray(60)),
                inner_margin: Margin::symmetric(6, 12),
                ..Frame::default()
            })
            .show(ctx, |ui| {
                // ui.allocate_ui_with_layout(
                //     egui::vec2(ui.available_width(), ui.available_height()), // fixed width, max height
                //     egui::Layout::top_down(egui::Align::Min),
                //     |ui| {
                ui.vertical_centered(|ui| {
                    add_contents(ui);
                });
            });
    }

    fn show_fixed_pos(
        &self,
        ctx: &egui::Context,
        window_id: Id,
        order: Order,
        fixed_pos: Vec2,
        add_contents: impl FnOnce(&mut Ui),
    ) {
        let window_proto = egui::Window::new(&self.builder.label)
            // .min_size(popup_size)
            .min_size(self.builder.popup_size)
            .fixed_size(self.builder.popup_size)
            .collapsible(false)
            .resizable(false)
            .title_bar(false)
            .id(window_id)
            .order(order);

        let window_proto = match self.builder.moveable {
            true => window_proto.default_pos(fixed_pos.to_pos2()),
            false => window_proto.fixed_pos(fixed_pos.to_pos2()),
        };

        window_proto
            .frame(Frame {
                corner_radius: CornerRadius::same(1),
                fill: Color32::from_gray(40),
                stroke: Stroke::new(1.0, Color32::from_gray(60)),
                inner_margin: Margin::symmetric(6, 12),
                ..Frame::default()
            })
            .show(ctx, |ui| {
                let frame = Frame::NONE;

                frame.show(ui, |ui| {
                    ui.set_min_size(self.builder.popup_size);
                    ui.vertical_centered(|ui| {
                        add_contents(ui);
                    });
                });
            });
    }

    pub fn render_popup_backdrop(
        ctx: &egui::Context,
        id: egui::Id,
        order: egui::Order,
        color: Option<Color32>,
    ) -> (egui::LayerId, bool) {
        let color = color.unwrap_or(Self::BACKDROP_COLOR);

        let area = egui::Area::new(id)
            .kind(egui::UiKind::Modal)
            .interactable(true) // Blocks clicks from going through to the widgets behind
            .fixed_pos(egui::pos2(0.0, 0.0))
            .order(order);

        let layer_id = area.layer();

        ctx.memory_mut(|mem| {
            mem.areas_mut().move_to_top(layer_id);
        });

        let mut clicked = false;

        area.show(ctx, |ui| {
            // Get the full screen rect
            let screen_rect = ctx.screen_rect();

            // Allocate a rect that covers the whole screen to catch clicks
            let response = ui.allocate_rect(screen_rect, egui::Sense::click());

            // Optional: Close popup if user clicks the dark background
            if response.clicked() {
                {
                    clicked = true;
                }
                // TODO: hook this in.
            }

            // Paint the semi-transparent black color
            let painter = ui.painter();
            painter.rect_filled(screen_rect, egui::CornerRadius::ZERO, color);
        });

        (layer_id, clicked)
    }
}

// impl Default for Dialog {
//     fn default() -> Self {
//         Self::new()
//     }
// }
