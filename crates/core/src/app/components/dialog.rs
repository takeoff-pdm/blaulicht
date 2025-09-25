use egui::{vec2, Color32, CornerRadius, Frame, Margin, Stroke, Ui, Vec2};

pub fn dialog(
    ctx: &egui::Context,
    label: &str,
    popup_size: Vec2,
    moveable: bool,
    add_contents: impl FnOnce(&mut Ui),
) {
    let screen_rect = ctx.screen_rect();

    // println!(
    //     "center: {} {}",
    //     screen_rect.center().x,
    //     screen_rect.center().y
    // );
    let center_pos = egui::Pos2::new(
        screen_rect.center().x - popup_size.x / 2.0,
        screen_rect.center().y - popup_size.y / 2.0,
    );

    // Clamp to screen boundaries
    // center_pos.x = center_pos
    //     .x
    //     .clamp(screen_rect.left(), screen_rect.right() - popup_size.x);
    // center_pos.y = center_pos
    //     .y
    //     .clamp(screen_rect.top(), screen_rect.bottom() - popup_size.y);

    let window_proto = egui::Window::new(label)
        // .min_size(popup_size)
        .fixed_size(vec2(popup_size.x, 0.0))
        .collapsible(false)
        .resizable(false)
        .title_bar(false);

    let window_proto = match moveable {
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

pub fn dialog_fixed_pos(
    ctx: &egui::Context,
    label: &str,
    popup_size: Vec2,
    pos: Vec2,
    moveable: bool,
    add_contents: impl FnOnce(&mut Ui),
) {
    let window_proto = egui::Window::new(label)
        // .min_size(popup_size)
        .min_size(popup_size)
        .fixed_size(popup_size)
        .collapsible(false)
        .resizable(false)
        .title_bar(false);

    let window_proto = match moveable {
        true => window_proto.default_pos(pos.to_pos2()),
        false => window_proto.fixed_pos(pos.to_pos2()),
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
                ui.set_min_size(popup_size);
                ui.vertical_centered(|ui| {
                    add_contents(ui);
                });
            });
        });
}
