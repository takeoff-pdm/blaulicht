use egui::{pos2, vec2, Color32, Rect, Response, Sense, Stroke, StrokeKind, TextStyle, Ui, Widget};

/// A hardware-style toggle switch widget.
///
/// Draws a chunky base plate with a sliding lever that snaps between two
/// positions. Clicking toggles the boolean value; dragging across the plate is
/// also supported.
pub struct Switch<'a> {
    state: &'a mut bool,
    label: Option<String>,
}

impl<'a> Switch<'a> {
    pub fn new(state: &'a mut bool) -> Self {
        Self { state, label: None }
    }

    pub fn with_label(mut self, label: impl Into<String>) -> Self {
        self.label = Some(label.into());
        self
    }
}

impl<'a> Widget for Switch<'a> {
    fn ui(self, ui: &mut Ui) -> Response {
        let label_space = if self.label.is_some() { 20.0 } else { 0.0 };
        let desired_size = vec2(64.0, 44.0 + label_space);

        let (rect, mut response) = ui.allocate_exact_size(desired_size, Sense::click_and_drag());
        let body_height = (rect.height() - label_space).max(32.0);
        let switch_rect = Rect::from_min_size(rect.min, vec2(rect.width(), body_height));

        if response.clicked() {
            *self.state = !*self.state;
            response.mark_changed();
        }

        if response.dragged() {
            if let Some(pointer) = response.interact_pointer_pos() {
                let new_value = pointer.x >= switch_rect.center().x;
                if new_value != *self.state {
                    *self.state = new_value;
                    response.mark_changed();
                }
            }
        }

        let painter = ui.painter();

        // Base plate
        let base_color = if response.hovered() {
            Color32::from_rgb(62, 62, 70)
        } else {
            Color32::from_rgb(46, 46, 54)
        };
        painter.rect_filled(switch_rect, 6.0, base_color);

        let stroke_color = ui.visuals().widgets.noninteractive.bg_stroke.color;
        painter.rect_stroke(
            switch_rect,
            6.0,
            Stroke::new(1.5, stroke_color),
            StrokeKind::Inside,
        );

        // Groove the lever slides in
        let groove_rect = Rect::from_center_size(
            switch_rect.center(),
            vec2(switch_rect.width() * 0.65, switch_rect.height() * 0.42),
        );
        painter.rect_filled(groove_rect, 4.0, Color32::from_rgb(18, 18, 21));
        painter.rect_stroke(
            groove_rect,
            4.0,
            Stroke::new(1.0, Color32::from_gray(60)),
            StrokeKind::Inside,
        );

        // Lever
        let lever_offset = groove_rect.width() * 0.32;
        let lever_center = pos2(
            groove_rect.center().x
                + if *self.state {
                    lever_offset
                } else {
                    -lever_offset
                },
            groove_rect.center().y,
        );
        let lever_rect = Rect::from_center_size(
            lever_center,
            vec2(groove_rect.width() * 0.35, switch_rect.height() * 0.7),
        );

        let lever_base = Color32::from_rgb(196, 196, 205);
        painter.rect_filled(lever_rect, 3.0, lever_base);
        painter.rect_stroke(
            lever_rect,
            3.0,
            Stroke::new(1.0, Color32::from_rgb(90, 90, 98)),
            StrokeKind::Inside,
        );

        // Subtle top highlight on the lever for a metallic look.
        let highlight_rect = Rect::from_min_max(
            pos2(lever_rect.left(), lever_rect.top()),
            pos2(lever_rect.right(), lever_rect.center().y),
        );
        painter.rect_filled(
            highlight_rect,
            3.0,
            Color32::from_rgba_premultiplied(255, 255, 255, 24),
        );

        // Optional label centered below the switch.
        if let Some(label) = self.label {
            let galley = painter.layout_no_wrap(
                label,
                TextStyle::Body.resolve(ui.style()),
                ui.visuals().text_color(),
            );
            let text_pos = pos2(
                rect.center().x - galley.size().x / 2.0,
                switch_rect.bottom() + (label_space - galley.size().y) / 2.0,
            );
            painter.galley(text_pos, galley, ui.visuals().text_color());
        }

        response
    }
}
