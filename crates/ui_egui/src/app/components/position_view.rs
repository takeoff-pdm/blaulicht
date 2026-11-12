//! The fixture position view (`fxplan.js` in `design/studio`): a 2D plot of
//! the rig next to the position fields. Top shows across (X) and depth (Z)
//! with the audience at the bottom, Front shows across (X) and height (Y).
//! Every fixture is a dot; the selected one's group is brighter, the
//! selected fixture is blue with its name. Click a dot to select it, drag
//! the blue one to move it in 5 cm steps (height never below 0). Wheel or
//! -/+ zoom about the pointer (the centre for the buttons), dragging empty
//! space pans, Fit shows every fixture. View, zoom and pan are shared by
//! the small view and the large one and kept per view (Top, Front).
//!
//! Desktop only: it is drawn by the desktop Setup → Fixtures pane.

use crate::app::components::studio::{mini_button, segmented};
use crate::app::ui_tokens::{studio, Density, Weight};
use egui::{pos2, vec2, Align2, Color32, FontId, Id, Pos2, Rect, Sense, Stroke, StrokeKind, Ui};

/// The mockup's viewBox sizes; the plot scales them to the width it gets.
const SMALL: (f32, f32) = (268.0, 190.0);
const LARGE: (f32, f32) = (860.0, 540.0);
const SNAP: f32 = 0.05;
const ZOOM_MIN: f32 = 0.5;
const ZOOM_MAX: f32 = 12.0;

/// Which plane the view shows.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum PlanView {
    #[default]
    Top,
    Front,
}

impl PlanView {
    /// Indices into `[x, y, z]` of the horizontal and vertical axis.
    fn axes(self) -> (usize, usize) {
        match self {
            PlanView::Top => (0, 2),
            PlanView::Front => (0, 1),
        }
    }
    fn labels(self) -> (&'static str, &'static str) {
        match self {
            PlanView::Top => ("Across (X)", "Depth (Z)"),
            PlanView::Front => ("Across (X)", "Height (Y)"),
        }
    }
}

/// Small (in the Fixture pane) or large (in the dialog).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PlanSize {
    Small,
    Large,
}

impl PlanSize {
    fn view_box(self) -> (f32, f32) {
        match self {
            PlanSize::Small => SMALL,
            PlanSize::Large => LARGE,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Zoom {
    pub z: f32,
    /// Pan in world units.
    pub px: f32,
    pub py: f32,
}

impl Default for Zoom {
    fn default() -> Self {
        Zoom {
            z: 1.0,
            px: 0.0,
            py: 0.0,
        }
    }
}

/// View and zoom, shared by both sizes.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct PlanState {
    pub view: PlanView,
    pub zoom: [Zoom; 2],
}

impl PlanState {
    fn zoom_mut(&mut self) -> &mut Zoom {
        &mut self.zoom[self.view as usize]
    }
    fn zoom(&self) -> Zoom {
        self.zoom[self.view as usize]
    }
}

fn state_id() -> Id {
    Id::new("setup_fixture_plan_state")
}

/// One fixture on the plot.
#[derive(Debug, Clone)]
pub struct PlanDot {
    pub group: u8,
    pub fixture: u8,
    pub name: String,
    pub group_name: String,
    /// `[x, y, z]` in metres.
    pub pos: [f32; 3],
    /// In the selected fixture's group.
    pub same_group: bool,
}

/// What the user did this frame.
#[derive(Debug, Default, Clone, PartialEq)]
pub struct PlanOutput {
    /// A dot was clicked: select that fixture.
    pub select: Option<(u8, u8)>,
    /// The selected fixture moved (its position is already updated).
    pub moved: bool,
    /// A move drag ended; persist the position.
    pub move_done: bool,
    /// The expand button was pressed.
    pub open_large: bool,
}

/// World to screen for one frame: fit every fixture, then zoom about the
/// centre and pan. `k` is in viewBox units per metre; `s` scales the viewBox
/// to the screen.
#[derive(Debug, Clone, Copy, PartialEq)]
struct Fit {
    k: f32,
    mx: f32,
    my: f32,
    w: f32,
    h: f32,
}

impl Fit {
    fn new(points: &[(f32, f32)], w: f32, h: f32, zoom: Zoom) -> Self {
        let (mut x0, mut x1, mut y0, mut y1) = (0.0_f32, 0.0_f32, 0.0_f32, 0.0_f32);
        for &(a, b) in points {
            x0 = x0.min(a);
            x1 = x1.max(a);
            y0 = y0.min(b);
            y1 = y1.max(b);
        }
        let (x0, x1, y0, y1) = (x0 - 1.0, x1 + 1.0, y0 - 1.0, y1 + 1.0);
        let k = ((w - 28.0) / (x1 - x0)).min((h - 28.0) / (y1 - y0)) * zoom.z;
        Fit {
            k,
            mx: (x0 + x1) / 2.0 + zoom.px,
            my: (y0 + y1) / 2.0 + zoom.py,
            w,
            h,
        }
    }
    fn sx(&self, v: f32) -> f32 {
        self.w / 2.0 + (v - self.mx) * self.k
    }
    fn sy(&self, v: f32) -> f32 {
        self.h / 2.0 - (v - self.my) * self.k
    }
    fn ix(&self, px: f32) -> f32 {
        self.mx + (px - self.w / 2.0) / self.k
    }
    fn iy(&self, py: f32) -> f32 {
        self.my - (py - self.h / 2.0) / self.k
    }
    /// 1 m grid, or 5 m when zoomed out.
    fn grid_step(&self) -> f32 {
        if self.k < 12.0 {
            5.0
        } else {
            1.0
        }
    }
}

/// Zooms by `fac` keeping the world point under viewBox point `(px, py)`.
fn zoom_at(points: &[(f32, f32)], vb: (f32, f32), zoom: &mut Zoom, fac: f32, px: f32, py: f32) {
    let f = Fit::new(points, vb.0, vb.1, *zoom);
    let (wx, wy) = (f.ix(px), f.iy(py));
    zoom.z = (zoom.z * fac).clamp(ZOOM_MIN, ZOOM_MAX);
    let g = Fit::new(points, vb.0, vb.1, *zoom);
    zoom.px += wx - g.ix(px);
    zoom.py += wy - g.iy(py);
}

fn snap(v: f32) -> f32 {
    (v / SNAP).round() * SNAP
}

/// A move or pan in progress.
#[derive(Debug, Clone, Copy)]
struct Drag {
    moving: bool,
    fit: Fit,
    start: Pos2,
    zoom: Zoom,
}

/// Paints `text` with a 1.5px outline in `halo` (SVG `paint-order: stroke`).
fn halo_text(
    p: &egui::Painter,
    at: Pos2,
    align: Align2,
    text: &str,
    f: FontId,
    c: Color32,
    halo: Color32,
) {
    for d in [
        vec2(-1.0, 0.0),
        vec2(1.0, 0.0),
        vec2(0.0, -1.0),
        vec2(0.0, 1.0),
    ] {
        p.text(at + d, align, text, f.clone(), halo);
    }
    p.text(at, align, text, f, c);
}

/// A 24x22 icon button (`.mini.ic`) painting `icon` into its rect.
fn icon_button(
    ui: &mut Ui,
    tip: &str,
    icon: impl FnOnce(&egui::Painter, Rect, Stroke),
) -> egui::Response {
    let t = studio(ui.ctx());
    let (rect, r) = ui.allocate_exact_size(vec2(24.0, 22.0), Sense::click());
    let fill = if r.hovered() { t.ctl_hi } else { t.ctl };
    let p = ui.painter();
    p.rect_filled(rect, 0.0, fill);
    if r.has_focus() {
        p.rect_stroke(rect, 0.0, Stroke::new(1.0_f32, t.blue), StrokeKind::Inside);
    }
    icon(
        p,
        Rect::from_center_size(rect.center(), vec2(11.0, 11.0)),
        Stroke::new(1.2_f32, t.text),
    );
    r.widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::Button, true, tip));
    r.on_hover_text(tip)
}

/// The bar over the plot: Top/Front, -/Fit/+, and the expand button (small)
/// or the hint (large). Returns whether expand was pressed.
fn bar(ui: &mut Ui, st: &mut PlanState, points: &[(f32, f32)], size: PlanSize, name: &str) -> bool {
    let t = studio(ui.ctx());
    let vb = size.view_box();
    let mut open = false;
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = 8.0;
        let mut sel = st.view as usize;
        ui.scope(|ui| {
            if segmented(ui, &mut sel, &["Top", "Front"], 22.0).changed() || sel != st.view as usize
            {
                st.view = if sel == 0 {
                    PlanView::Top
                } else {
                    PlanView::Front
                };
            }
        });
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = 2.0;
            if icon_button(ui, "Zoom out", |p, r, s| {
                p.hline(r.x_range(), r.center().y, s);
            })
            .clicked()
            {
                zoom_at(points, vb, st.zoom_mut(), 1.0 / 1.5, vb.0 / 2.0, vb.1 / 2.0);
            }
            if mini_button(ui, "Fit", false, 22.0)
                .on_hover_text("Show every fixture")
                .clicked()
            {
                *st.zoom_mut() = Zoom::default();
            }
            if icon_button(ui, "Zoom in", |p, r, s| {
                p.hline(r.x_range(), r.center().y, s);
                p.vline(r.center().x, r.y_range(), s);
            })
            .clicked()
            {
                zoom_at(points, vb, st.zoom_mut(), 1.5, vb.0 / 2.0, vb.1 / 2.0);
            }
        });
        ui.with_layout(
            egui::Layout::right_to_left(egui::Align::Center),
            |ui| match size {
                PlanSize::Small => {
                    open = icon_button(ui, "Open a larger view", |p, r, s| {
                        // ph-arrows-out-simple: a diagonal with two corner heads.
                        let (a, b) = (r.left_bottom(), r.right_top());
                        p.line_segment([a, b], s);
                        p.line_segment([b, b + vec2(-4.5, 0.0)], s);
                        p.line_segment([b, b + vec2(0.0, 4.5)], s);
                        p.line_segment([a, a + vec2(4.5, 0.0)], s);
                        p.line_segment([a, a + vec2(0.0, -4.5)], s);
                    })
                    .clicked();
                }
                PlanSize::Large => {
                    ui.label(
                        egui::RichText::new(format!(
                        "Drag the blue dot to move {name}. Wheel zooms, drag empty space to pan."
                    ))
                        .font(Density::of(ui.ctx()).font(Weight::Regular, 11.0))
                        .color(t.muted),
                    );
                }
            },
        );
    });
    open
}

/// Draws the bar and the plot for `selected` (index into `dots`), whose
/// position `pos` is the live editor value. `id_salt` keeps the small and
/// the large view's drags apart.
pub fn position_view(
    ui: &mut Ui,
    dots: &[PlanDot],
    selected: usize,
    pos: &mut [f32; 3],
    size: PlanSize,
) -> PlanOutput {
    let t = studio(ui.ctx());
    let density = Density::of(ui.ctx());
    let mut out = PlanOutput::default();
    let mut st: PlanState = ui.data(|d| d.get_temp(state_id())).unwrap_or_default();
    let Some(sel) = dots.get(selected) else {
        return out;
    };

    let point_list = |view: PlanView, pos: &[f32; 3]| -> Vec<(f32, f32)> {
        let (a, b) = view.axes();
        dots.iter()
            .enumerate()
            .map(|(i, d)| {
                if i == selected {
                    (pos[a], pos[b])
                } else {
                    (d.pos[a], d.pos[b])
                }
            })
            .collect()
    };

    let points = point_list(st.view, pos);
    out.open_large = bar(ui, &mut st, &points, size, &sel.name);
    let points = point_list(st.view, pos);
    let (a, b) = st.view.axes();
    let (la, lb) = st.view.labels();

    let vb = size.view_box();
    let w = ui.available_width().max(60.0);
    let scale = w / vb.0;
    let (rect, resp) = ui.allocate_exact_size(vec2(w, vb.1 * scale), Sense::click_and_drag());
    let to_vb = |p: Pos2| ((p.x - rect.left()) / scale, (p.y - rect.top()) / scale);
    let big = size == PlanSize::Large;
    let fit = Fit::new(&points, vb.0, vb.1, st.zoom());
    let screen =
        |f: &Fit, x: f32, y: f32| pos2(rect.left() + f.sx(x) * scale, rect.top() + f.sy(y) * scale);

    // Hit test: the nearest dot under the pointer, selected first.
    let r_sel = if big { 8.0 } else { 6.0 };
    let sel_sp = screen(&fit, pos[a], pos[b]);
    let hit = |p: Pos2| -> Option<usize> {
        let sp = sel_sp;
        if sp.distance(p) <= (r_sel + 2.0) * scale {
            return Some(selected);
        }
        dots.iter()
            .enumerate()
            .filter(|(i, _)| *i != selected)
            .map(|(i, d)| (i, screen(&fit, d.pos[a], d.pos[b]).distance(p)))
            .filter(|(_, dist)| *dist <= 6.0 * scale)
            .min_by(|x, y| x.1.total_cmp(&y.1))
            .map(|(i, _)| i)
    };

    let drag_id = resp.id.with("drag");
    let mut drag: Option<Drag> = ui.data(|d| d.get_temp(drag_id));
    if resp.drag_started() {
        if let Some(p) = ui.input(|i| i.pointer.press_origin()) {
            let h = hit(p);
            // A press on another dot only selects it (on click), never pans.
            if h.is_none() || h == Some(selected) {
                drag = Some(Drag {
                    moving: h == Some(selected),
                    fit,
                    start: p,
                    zoom: st.zoom(),
                });
            }
        }
    }
    if let Some(d) = drag {
        let esc = ui.input(|i| i.key_pressed(egui::Key::Escape));
        if let Some(p) = ui
            .input(|i| i.pointer.latest_pos())
            .filter(|_| resp.dragged() && !esc)
        {
            let (px, py) = to_vb(p);
            if d.moving {
                let nx = snap(d.fit.ix(px));
                let mut ny = snap(d.fit.iy(py));
                if b == 1 {
                    ny = ny.max(0.0);
                }
                if pos[a] != nx || pos[b] != ny {
                    pos[a] = nx;
                    pos[b] = ny;
                    out.moved = true;
                }
            } else {
                let (sx, sy) = to_vb(d.start);
                let z = st.zoom_mut();
                z.px = d.zoom.px - (px - sx) / d.fit.k;
                z.py = d.zoom.py + (py - sy) / d.fit.k;
            }
        }
        if resp.drag_stopped() || esc || !resp.is_pointer_button_down_on() {
            if d.moving {
                out.move_done = true;
            }
            drag = None;
        }
    }
    match drag {
        Some(d) => ui.data_mut(|m| {
            m.insert_temp(drag_id, d);
        }),
        None => ui.data_mut(|m| {
            m.remove::<Drag>(drag_id);
        }),
    }

    if resp.clicked() {
        if let Some(i) = resp.interact_pointer_pos().and_then(hit) {
            if i != selected {
                out.select = Some((dots[i].group, dots[i].fixture));
            }
        }
    }

    // Wheel and pinch zoom about the pointer.
    if resp.hovered() {
        let (scroll, pinch) = ui.input(|i| (i.smooth_scroll_delta.y, i.zoom_delta()));
        let fac = (scroll * 0.002).exp() * pinch;
        if (fac - 1.0).abs() > 1e-4 {
            if let Some(p) = resp.hover_pos() {
                let (px, py) = to_vb(p);
                let points = point_list(st.view, pos);
                zoom_at(&points, vb, st.zoom_mut(), fac, px, py);
            }
        }
    }
    let hovered = resp.hover_pos().and_then(hit);
    if hovered.is_some() || drag.is_some_and(|d| d.moving) {
        ui.ctx().set_cursor_icon(if drag.is_some() {
            egui::CursorIcon::Grabbing
        } else if hovered == Some(selected) {
            egui::CursorIcon::Grab
        } else {
            egui::CursorIcon::PointingHand
        });
    } else if drag.is_some() {
        ui.ctx().set_cursor_icon(egui::CursorIcon::Grabbing);
    } else if resp.hovered() {
        ui.ctx().set_cursor_icon(egui::CursorIcon::Grab);
    }
    ui.data_mut(|d| d.insert_temp(state_id(), st));

    // Paint with this frame's final zoom and position.
    let points = point_list(st.view, pos);
    let f = Fit::new(&points, vb.0, vb.1, st.zoom());
    let p = ui.painter_at(rect);
    p.rect_filled(rect, 0.0, t.well);
    let font = FontId::new(11.0 * scale, density.font(Weight::Regular, 11.0).family);
    let step = f.grid_step();
    let axis = |v: f32| v.abs() < 1e-3;
    let dashed = |p: &egui::Painter, a: Pos2, b: Pos2| {
        p.extend(egui::Shape::dashed_line(
            &[a, b],
            Stroke::new(1.0_f32, t.line),
            3.0 * scale,
            3.0 * scale,
        ));
    };
    let mut v = (f.ix(0.0) / step).ceil() * step;
    while v <= f.ix(vb.0) {
        let x = rect.left() + f.sx(v) * scale;
        if axis(v) {
            dashed(&p, pos2(x, rect.top()), pos2(x, rect.bottom()));
        } else {
            p.vline(x, rect.y_range(), Stroke::new(1.0_f32, t.hair));
        }
        v += step;
    }
    let mut v = (f.iy(vb.1) / step).ceil() * step;
    while v <= f.iy(0.0) {
        let y = rect.top() + f.sy(v) * scale;
        if axis(v) {
            dashed(&p, pos2(rect.left(), y), pos2(rect.right(), y));
        } else {
            p.hline(rect.x_range(), y, Stroke::new(1.0_f32, t.hair));
        }
        v += step;
    }
    let corner = 6.0 * scale;
    p.text(
        pos2(rect.left() + corner, rect.bottom() - corner),
        Align2::LEFT_BOTTOM,
        format!("Grid {} m", step as i32),
        font.clone(),
        t.muted,
    );

    for (i, d) in dots.iter().enumerate() {
        if i == selected {
            continue;
        }
        let c = screen(&f, d.pos[a], d.pos[b]);
        let r = if d.same_group { 3.5 } else { 2.5 } * if big { 1.4 } else { 1.0 } * scale;
        p.circle_filled(c, r, if d.same_group { t.text } else { t.scale });
        if big && d.same_group {
            halo_text(
                &p,
                c + vec2(7.0 * scale, 0.0),
                Align2::LEFT_CENTER,
                &d.name,
                font.clone(),
                t.text,
                t.well,
            );
        }
    }
    let c = screen(&f, pos[a], pos[b]);
    p.circle(c, r_sel * scale, t.blue, Stroke::new(1.5_f32, t.strong));
    let right = c.x > rect.right() - 110.0 * scale;
    let (lx, align) = if right {
        (c.x - 11.0 * scale, Align2::RIGHT_CENTER)
    } else {
        (c.x + 11.0 * scale, Align2::LEFT_CENTER)
    };
    halo_text(
        &p,
        pos2(lx, c.y),
        align,
        &sel.name,
        font.clone(),
        t.strong,
        t.well,
    );

    p.text(
        pos2(rect.right() - corner, rect.bottom() - corner),
        Align2::RIGHT_BOTTOM,
        format!("{la} \u{2192}"),
        font.clone(),
        t.muted,
    );
    p.text(
        pos2(rect.left() + corner, rect.top() + 4.0 * scale),
        Align2::LEFT_TOP,
        format!("\u{2191} {lb}"),
        font.clone(),
        t.muted,
    );
    if st.view == PlanView::Top {
        p.text(
            pos2(rect.center().x, rect.bottom() - corner),
            Align2::CENTER_BOTTOM,
            "Audience",
            font,
            t.muted,
        );
    }
    p.rect_stroke(rect, 0.0, Stroke::new(1.0_f32, t.line), StrokeKind::Inside);

    if let Some(i) = hovered.filter(|_| drag.is_none()) {
        let d = &dots[i];
        resp.on_hover_text_at_pointer(format!("{} ({})", d.name, d.group_name));
    }
    if out.moved || drag.is_some() {
        ui.ctx().request_repaint();
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fit_round_trips_and_zoom_keeps_the_point_under_the_pointer() {
        let pts = [(0.0, 0.0), (4.0, 2.0), (-3.0, 6.0)];
        let f = Fit::new(&pts, 268.0, 190.0, Zoom::default());
        assert!((f.ix(f.sx(2.5)) - 2.5).abs() < 1e-4);
        assert!((f.iy(f.sy(-1.25)) + 1.25).abs() < 1e-4);
        let mut z = Zoom::default();
        let before = (f.ix(40.0), f.iy(30.0));
        zoom_at(&pts, SMALL, &mut z, 2.0, 40.0, 30.0);
        let g = Fit::new(&pts, 268.0, 190.0, z);
        assert!((g.ix(40.0) - before.0).abs() < 1e-3);
        assert!((g.iy(30.0) - before.1).abs() < 1e-3);
        zoom_at(&pts, SMALL, &mut z, 100.0, 0.0, 0.0);
        assert_eq!(z.z, ZOOM_MAX);
    }

    #[test]
    fn snaps_to_5_cm_and_grid_switches_to_5_m_when_zoomed_out() {
        assert!((snap(1.234) - 1.25).abs() < 1e-6);
        assert!((snap(-0.02) - 0.0).abs() < 1e-6);
        let wide = Fit::new(&[(-40.0, 0.0), (40.0, 10.0)], 268.0, 190.0, Zoom::default());
        assert_eq!(wide.grid_step(), 5.0);
        let near = Fit::new(&[(0.0, 0.0), (2.0, 1.0)], 860.0, 540.0, Zoom::default());
        assert_eq!(near.grid_step(), 1.0);
    }

    #[test]
    fn front_view_maps_across_and_height() {
        assert_eq!(PlanView::Front.axes(), (0, 1));
        assert_eq!(PlanView::Top.axes(), (0, 2));
    }
}
