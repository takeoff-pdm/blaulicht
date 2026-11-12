//! The desktop Programmer's pane layout (`design/studio/panels.js`,
//! `RULES.md` decision 1 and "Resizable panes"): where every pane goes in
//! the rack (1400px and wider) and the stack (narrower), the dividers
//! between the panes that can trade space, folding, and the sizes saved per
//! machine.
//!
//! Desktop only ([`Density::Compact`]); the kiosk Programmer never reads it.
//!
//! [`Density::Compact`]: crate::app::ui_tokens::Density::Compact

use egui::{pos2, vec2, Rect};
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

/// The window width from which the rack layout is used.
pub const RACK_FROM: f32 = 1400.0;
/// Below this window width the sidebar collapses to its rail on the
/// Programmer.
pub const SIDEBAR_RAIL_BELOW: f32 = 1200.0;

/// The 2px gap between panes.
pub const GAP: f32 = 2.0;
/// A folded pane: a 26px strip or title bar.
pub const STRIP: f32 = 26.0;
/// The page bar.
pub const BAR: f32 = 40.0;
/// The fader bank keeps its size at every width.
pub const FADERS_W: f32 = 440.0;
pub const CONTROLS_H: f32 = 320.0;
const COLOUR_W: f32 = 196.0;
const GIZMO_W: f32 = 170.0;
const STACK_GIZMO_W: f32 = 244.0;
const STACK_CONTROLS_H: f32 = 282.0;
const RACK_LIST_MIN: f32 = 120.0;
const STACK_LIST_MIN: f32 = 100.0;
const PAL_MIN: f32 = 160.0;
const SIDE_MIN_LOWER: f32 = 220.0;
const SIDE_MIN: f32 = 220.0;
const SIDE_MAX: f32 = 520.0;
/// A divider's grab area, over the 2px gap.
pub const GRIP_W: f32 = 8.0;

/// The Programmer's panes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub enum Pane {
    Groups,
    Fixtures,
    Faders,
    Colour,
    PanTilt,
    Palettes,
    Scene,
}

impl Pane {
    /// The title shown on the pane and on its folded strip.
    pub fn title(self) -> &'static str {
        match self {
            Pane::Groups => "Groups",
            Pane::Fixtures => "Fixtures",
            Pane::Faders => "Faders",
            Pane::Colour => "Colour",
            Pane::PanTilt => "Pan / Tilt",
            Pane::Palettes => "Palettes",
            Pane::Scene => "Scene",
        }
    }
}

/// Rack or stack.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mode {
    Rack,
    Stack,
}

impl Mode {
    /// The layout for a window `width` wide.
    pub fn for_width(width: f32) -> Self {
        if width >= RACK_FROM {
            Mode::Rack
        } else {
            Mode::Stack
        }
    }

    fn list_min(self) -> f32 {
        match self {
            Mode::Rack => RACK_LIST_MIN,
            Mode::Stack => STACK_LIST_MIN,
        }
    }

    /// Whether `pane` can fold here. In the rack the controls hold the row
    /// Palettes and the Scene pane sit under, so they never fold there.
    pub fn can_fold(self, pane: Pane) -> bool {
        !(self == Mode::Rack && matches!(pane, Pane::Faders | Pane::Colour | Pane::PanTilt))
    }

    /// Whether a folded `pane` is a title bar (`true`) or a vertical strip.
    pub fn folds_to_bar(self, pane: Pane) -> bool {
        self == Mode::Stack
            && matches!(
                pane,
                Pane::Faders | Pane::Colour | Pane::PanTilt | Pane::Palettes
            )
    }
}

/// The sizes and folds of one mode.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct ModeLayout {
    /// Share of the list width (`fr`) for Groups and Fixtures.
    pub groups_w: f32,
    pub fixtures_w: f32,
    /// Width of Palettes in the rack's lower row.
    pub pal_w: f32,
    /// Width of the Scene pane in the stack.
    pub side_w: f32,
    pub folded: BTreeSet<Pane>,
}

impl Default for ModeLayout {
    fn default() -> Self {
        Self {
            groups_w: 1.0,
            fixtures_w: 1.0,
            pal_w: 440.0,
            side_w: 260.0,
            folded: BTreeSet::new(),
        }
    }
}

impl ModeLayout {
    /// Folded, as far as `mode` allows.
    pub fn is_folded(&self, mode: Mode, pane: Pane) -> bool {
        mode.can_fold(pane) && self.folded.contains(&pane)
    }

    pub fn toggle_fold(&mut self, mode: Mode, pane: Pane) {
        if !mode.can_fold(pane) {
            return;
        }
        if !self.folded.remove(&pane) {
            self.folded.insert(pane);
        }
    }

    fn any_list_open(&self, mode: Mode) -> bool {
        !self.is_folded(mode, Pane::Groups) || !self.is_folded(mode, Pane::Fixtures)
    }
}

/// Both modes, saved per machine.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct PaneLayout {
    pub rack: ModeLayout,
    pub stack: ModeLayout,
}

impl PaneLayout {
    pub fn mode(&self, mode: Mode) -> &ModeLayout {
        match mode {
            Mode::Rack => &self.rack,
            Mode::Stack => &self.stack,
        }
    }

    pub fn mode_mut(&mut self, mode: Mode) -> &mut ModeLayout {
        match mode {
            Mode::Rack => &mut self.rack,
            Mode::Stack => &mut self.stack,
        }
    }

    /// The file the layout is kept in, next to the machine's config.
    pub fn path(config_dir: &Path) -> PathBuf {
        config_dir.join("programmer-layout.json")
    }

    /// The saved layout, or the default when there is none or it can't be
    /// read.
    pub fn load(path: Option<&Path>) -> Self {
        path.and_then(|p| std::fs::read_to_string(p).ok())
            .and_then(|s| serde_json::from_str(&s).ok())
            .unwrap_or_default()
    }

    pub fn save(&self, path: Option<&Path>) {
        let Some(path) = path else { return };
        match serde_json::to_string_pretty(self) {
            Ok(json) => {
                if let Err(err) = std::fs::write(path, json) {
                    tracing::warn!("Could not save the Programmer layout to {path:?}: {err}");
                }
            }
            Err(err) => tracing::warn!("Could not encode the Programmer layout: {err}"),
        }
    }
}

/// How a pane is drawn.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Fold {
    Open,
    /// Folded sideways: a 26px strip with its title running down.
    Strip,
    /// Folded to its 26px title bar.
    Bar,
}

/// What a divider resizes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum GripKind {
    /// Between Groups and Fixtures.
    Lists,
    /// Between Palettes and the Scene pane (rack).
    Palettes,
    /// Left of the Scene pane (stack).
    Side,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Grip {
    pub kind: GripKind,
    /// The 8px grab area.
    pub rect: Rect,
}

/// Where everything goes this frame.
#[derive(Debug, Clone)]
pub struct Geometry {
    pub mode: Mode,
    pub bar: Rect,
    pub panes: Vec<(Pane, Rect, Fold)>,
    pub grips: Vec<Grip>,
    /// Width of the rack's lower row (Palettes and the Scene pane).
    pub lower_w: f32,
}

impl Geometry {
    pub fn pane(&self, pane: Pane) -> Option<(Rect, Fold)> {
        self.panes
            .iter()
            .find(|(p, _, _)| *p == pane)
            .map(|(_, r, f)| (*r, *f))
    }
}

/// A grid track.
#[derive(Debug, Clone, Copy)]
enum Track {
    Fixed(f32),
    Flex { min: f32, fr: f32 },
}

/// Sizes `tracks` to fill `total` with `GAP` between them, like CSS grid:
/// fixed tracks first, the rest shared by `fr`, a flexible track never
/// below its minimum.
fn solve(tracks: &[Track], total: f32) -> Vec<f32> {
    let gaps = GAP * tracks.len().saturating_sub(1) as f32;
    let mut out = vec![0.0; tracks.len()];
    let mut pinned = vec![false; tracks.len()];
    loop {
        let mut free = total - gaps;
        let mut fr = 0.0;
        for (i, t) in tracks.iter().enumerate() {
            match t {
                Track::Fixed(w) => free -= w,
                Track::Flex { min, .. } if pinned[i] => free -= min,
                Track::Flex { fr: f, .. } => fr += f.max(0.0),
            }
        }
        let mut again = false;
        for (i, t) in tracks.iter().enumerate() {
            match *t {
                Track::Fixed(w) => out[i] = w,
                Track::Flex { min, .. } if pinned[i] => out[i] = min,
                Track::Flex { min, fr: f } => {
                    let w = if fr > 0.0 {
                        free.max(0.0) * f.max(0.0) / fr
                    } else {
                        0.0
                    };
                    if w < min {
                        pinned[i] = true;
                        again = true;
                    }
                    out[i] = w;
                }
            }
        }
        if !again {
            return out;
        }
    }
}

/// Left edges of `widths` from `x0`.
fn edges(x0: f32, widths: &[f32]) -> Vec<f32> {
    let mut x = x0;
    widths
        .iter()
        .map(|w| {
            let left = x;
            x += w + GAP;
            left
        })
        .collect()
}

fn span(xs: &[f32], ws: &[f32], from: usize, to: usize) -> (f32, f32) {
    (xs[from], xs[to] + ws[to])
}

/// The Palettes width in a lower row `lower_w` wide.
pub fn clamp_pal_w(pal_w: f32, lower_w: f32) -> f32 {
    pal_w.clamp(PAL_MIN, (lower_w - SIDE_MIN_LOWER - GAP).max(PAL_MIN))
}

/// The widest the Scene pane may get in the stack, for a work area
/// `work_w` wide.
pub fn side_max(layout: &ModeLayout, work_w: f32) -> f32 {
    let mode = Mode::Stack;
    let lists: f32 = [Pane::Groups, Pane::Fixtures]
        .iter()
        .map(|p| {
            if layout.is_folded(mode, *p) {
                STRIP
            } else {
                mode.list_min()
            }
        })
        .sum();
    let space = lists + FADERS_W + 5.0 * GAP + 2.0;
    SIDE_MIN.max(SIDE_MAX.min(work_w - space))
}

/// Clamps a dragged Scene pane width.
pub fn clamp_side_w(side_w: f32, layout: &ModeLayout, work_w: f32) -> f32 {
    side_w.clamp(SIDE_MIN, side_max(layout, work_w))
}

/// Clamps a dragged list divider: Groups gets `a`, Fixtures the rest of
/// `total`, neither below the mode's minimum.
pub fn clamp_list_split(a: f32, total: f32, mode: Mode) -> f32 {
    let min = mode.list_min();
    a.clamp(min, (total - min).max(min))
}

/// The layout of `layout` in `mode` over the page `area` (bar included).
pub fn geometry(layout: &ModeLayout, mode: Mode, area: Rect) -> Geometry {
    let folded = |p| layout.is_folded(mode, p);
    let fold_of = |p: Pane| {
        if !folded(p) {
            Fold::Open
        } else if mode.folds_to_bar(p) {
            Fold::Bar
        } else {
            Fold::Strip
        }
    };
    let list = |p: Pane, fr: f32| {
        if folded(p) {
            Track::Fixed(STRIP)
        } else {
            Track::Flex {
                min: mode.list_min(),
                fr,
            }
        }
    };
    let bar = Rect::from_min_size(area.min, vec2(area.width(), BAR));
    let top = bar.bottom() + GAP;
    let bottom = area.bottom();
    let mut panes = Vec::new();
    let mut grips = Vec::new();
    let any_list = layout.any_list_open(mode);
    let lower_w;
    let rect = |x0: f32, x1: f32, y0: f32, y1: f32| Rect::from_min_max(pos2(x0, y0), pos2(x1, y1));

    match mode {
        Mode::Rack => {
            let gizmo = if any_list {
                Track::Fixed(GIZMO_W)
            } else {
                Track::Flex {
                    min: GIZMO_W,
                    fr: 1.0,
                }
            };
            let tracks = [
                list(Pane::Groups, layout.groups_w),
                list(Pane::Fixtures, layout.fixtures_w),
                Track::Fixed(FADERS_W),
                Track::Fixed(COLOUR_W),
                gizmo,
            ];
            let ws = solve(&tracks, area.width());
            let xs = edges(area.left(), &ws);
            let row2 = top + CONTROLS_H + GAP;
            panes.push((Pane::Groups, rect(xs[0], xs[0] + ws[0], top, bottom)));
            panes.push((Pane::Fixtures, rect(xs[1], xs[1] + ws[1], top, bottom)));
            panes.push((
                Pane::Faders,
                rect(xs[2], xs[2] + ws[2], top, top + CONTROLS_H),
            ));
            panes.push((
                Pane::Colour,
                rect(xs[3], xs[3] + ws[3], top, top + CONTROLS_H),
            ));
            panes.push((
                Pane::PanTilt,
                rect(xs[4], xs[4] + ws[4], top, top + CONTROLS_H),
            ));
            let (l0, l1) = span(&xs, &ws, 2, 4);
            lower_w = l1 - l0;
            let pal_w = if folded(Pane::Palettes) {
                STRIP
            } else if folded(Pane::Scene) {
                lower_w - STRIP - GAP
            } else {
                clamp_pal_w(layout.pal_w, lower_w)
            };
            panes.push((Pane::Palettes, rect(l0, l0 + pal_w, row2, bottom)));
            panes.push((Pane::Scene, rect(l0 + pal_w + GAP, l1, row2, bottom)));
        }
        Mode::Stack => {
            let gizmo = if any_list || !folded(Pane::Scene) {
                Track::Fixed(STACK_GIZMO_W)
            } else {
                Track::Flex {
                    min: STACK_GIZMO_W,
                    fr: 1.0,
                }
            };
            let side = if folded(Pane::Scene) {
                STRIP
            } else {
                clamp_side_w(layout.side_w, layout, area.width())
            };
            let tracks = [
                list(Pane::Groups, layout.groups_w),
                list(Pane::Fixtures, layout.fixtures_w),
                Track::Fixed(COLOUR_W),
                gizmo,
                Track::Fixed(side),
            ];
            let ws = solve(&tracks, area.width());
            let xs = edges(area.left(), &ws);
            let faders_h = if folded(Pane::Faders) {
                STRIP
            } else {
                CONTROLS_H
            };
            let controls_h = if folded(Pane::Colour) && folded(Pane::PanTilt) {
                STRIP
            } else {
                STACK_CONTROLS_H
            };
            let row2 = top + faders_h + GAP;
            let row3 = row2 + controls_h + GAP;
            let (c0, c1) = span(&xs, &ws, 2, 3);
            lower_w = c1 - c0;
            panes.push((Pane::Groups, rect(xs[0], xs[0] + ws[0], top, bottom)));
            panes.push((Pane::Fixtures, rect(xs[1], xs[1] + ws[1], top, bottom)));
            panes.push((Pane::Faders, rect(c0, c1, top, top + faders_h)));
            panes.push((
                Pane::Colour,
                rect(xs[2], xs[2] + ws[2], row2, row2 + controls_h),
            ));
            panes.push((
                Pane::PanTilt,
                rect(xs[3], xs[3] + ws[3], row2, row2 + controls_h),
            ));
            panes.push((Pane::Palettes, rect(c0, c1, row3, bottom)));
            panes.push((Pane::Scene, rect(xs[4], xs[4] + ws[4], top, bottom)));
        }
    }

    let panes: Vec<(Pane, Rect, Fold)> =
        panes.into_iter().map(|(p, r)| (p, r, fold_of(p))).collect();
    let find = |p: Pane| panes.iter().find(|(q, _, _)| *q == p).map(|(_, r, _)| *r);
    // Each grip sits over the 2px gap left of its second pane.
    let grip_at = |kind, right_of: Rect| Grip {
        kind,
        rect: Rect::from_min_max(
            pos2(right_of.left() - 5.0, right_of.top()),
            pos2(right_of.left() - 5.0 + GRIP_W, right_of.bottom()),
        ),
    };
    if !folded(Pane::Groups) && !folded(Pane::Fixtures) {
        if let Some(r) = find(Pane::Fixtures) {
            grips.push(grip_at(GripKind::Lists, r));
        }
    }
    match mode {
        Mode::Rack => {
            if !folded(Pane::Palettes) && !folded(Pane::Scene) {
                if let Some(r) = find(Pane::Scene) {
                    grips.push(grip_at(GripKind::Palettes, r));
                }
            }
        }
        Mode::Stack => {
            if !folded(Pane::Scene) {
                if let Some(r) = find(Pane::Scene) {
                    grips.push(grip_at(GripKind::Side, r));
                }
            }
        }
    }

    Geometry {
        mode,
        bar,
        panes,
        grips,
        lower_w,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The work area at a window size: the window minus the 176px sidebar
    /// (rail: 48px) and the 26px status bar.
    fn area(window_w: f32, window_h: f32, sidebar: f32) -> Rect {
        Rect::from_min_max(pos2(sidebar, 0.0), pos2(window_w, window_h - 26.0))
    }

    #[test]
    fn rack_from_1400_and_stack_below() {
        assert_eq!(Mode::for_width(1440.0), Mode::Rack);
        assert_eq!(Mode::for_width(1400.0), Mode::Rack);
        assert_eq!(Mode::for_width(1399.0), Mode::Stack);
        assert_eq!(Mode::for_width(1280.0), Mode::Stack);
    }

    #[test]
    fn rack_at_1440_keeps_the_controls_size_and_shares_the_lists() {
        let a = area(1440.0, 900.0, 178.0);
        let g = geometry(&ModeLayout::default(), Mode::Rack, a);
        let (faders, _) = g.pane(Pane::Faders).unwrap();
        assert_eq!(faders.size(), vec2(440.0, 320.0));
        assert_eq!(g.pane(Pane::Colour).unwrap().0.width(), 196.0);
        assert_eq!(g.pane(Pane::PanTilt).unwrap().0.width(), 170.0);
        let groups = g.pane(Pane::Groups).unwrap().0;
        let fixtures = g.pane(Pane::Fixtures).unwrap().0;
        assert!((groups.width() - fixtures.width()).abs() < 0.01);
        assert_eq!(groups.bottom(), a.bottom());
        assert!((g.pane(Pane::PanTilt).unwrap().0.right() - a.right()).abs() < 0.01);
        // Palettes and the Scene pane fill the row under the controls.
        let pal = g.pane(Pane::Palettes).unwrap().0;
        let side = g.pane(Pane::Scene).unwrap().0;
        assert_eq!(pal.left(), faders.left());
        assert_eq!(pal.top(), faders.bottom() + GAP);
        assert_eq!(pal.width(), 440.0);
        assert!((side.right() - a.right()).abs() < 0.01);
        assert_eq!(g.grips.len(), 2);
    }

    #[test]
    fn stack_at_1280_puts_the_scene_pane_on_the_right_full_height() {
        let a = area(1280.0, 800.0, 178.0);
        let g = geometry(&ModeLayout::default(), Mode::Stack, a);
        let faders = g.pane(Pane::Faders).unwrap().0;
        assert!((faders.width() - 442.0).abs() < 0.01);
        assert_eq!(faders.height(), 320.0);
        let colour = g.pane(Pane::Colour).unwrap().0;
        let gizmo = g.pane(Pane::PanTilt).unwrap().0;
        assert_eq!(colour.top(), faders.bottom() + GAP);
        assert_eq!(colour.height(), 282.0);
        assert_eq!(gizmo.width(), 244.0);
        let pal = g.pane(Pane::Palettes).unwrap().0;
        assert_eq!(pal.top(), colour.bottom() + GAP);
        assert_eq!(pal.bottom(), a.bottom());
        let side = g.pane(Pane::Scene).unwrap().0;
        assert_eq!(side.width(), 260.0);
        assert_eq!(side.top(), faders.top());
        assert!((side.right() - a.right()).abs() < 0.01);
        assert_eq!(
            g.grips.iter().map(|g| g.kind).collect::<Vec<_>>(),
            vec![GripKind::Lists, GripKind::Side]
        );
    }

    #[test]
    fn folding_a_list_gives_its_width_to_the_other() {
        let a = area(1440.0, 900.0, 178.0);
        let mut l = ModeLayout::default();
        l.toggle_fold(Mode::Rack, Pane::Groups);
        let g = geometry(&l, Mode::Rack, a);
        assert_eq!(
            g.pane(Pane::Groups).unwrap(),
            (g.pane(Pane::Groups).unwrap().0, Fold::Strip)
        );
        assert_eq!(g.pane(Pane::Groups).unwrap().0.width(), STRIP);
        assert!(g.grips.iter().all(|g| g.kind != GripKind::Lists));
        // The controls can't fold in the rack, but can in the stack.
        l.toggle_fold(Mode::Rack, Pane::Faders);
        assert!(!l.is_folded(Mode::Rack, Pane::Faders));
        let mut s = ModeLayout::default();
        s.toggle_fold(Mode::Stack, Pane::Faders);
        let g = geometry(&s, Mode::Stack, area(1280.0, 800.0, 178.0));
        let (r, f) = g.pane(Pane::Faders).unwrap();
        assert_eq!((r.height(), f), (STRIP, Fold::Bar));
    }

    #[test]
    fn dragged_sizes_are_clamped() {
        assert_eq!(clamp_pal_w(50.0, 810.0), PAL_MIN);
        assert_eq!(clamp_pal_w(900.0, 810.0), 810.0 - 220.0 - GAP);
        assert_eq!(clamp_list_split(10.0, 500.0, Mode::Rack), 120.0);
        assert_eq!(clamp_list_split(490.0, 500.0, Mode::Rack), 380.0);
        let l = ModeLayout::default();
        assert_eq!(clamp_side_w(100.0, &l, 1100.0), SIDE_MIN);
        assert!(clamp_side_w(2000.0, &l, 1100.0) <= SIDE_MAX);
    }

    #[test]
    fn the_layout_survives_a_save_and_load() {
        let dir = tempdir::TempDir::new("programmer_layout").unwrap();
        let path = PaneLayout::path(dir.path());
        let mut layout = PaneLayout::default();
        layout.rack.pal_w = 300.0;
        layout.stack.folded.insert(Pane::Colour);
        layout.save(Some(&path));
        assert_eq!(PaneLayout::load(Some(&path)), layout);
        // Nothing saved, or no config folder: the default.
        assert_eq!(
            PaneLayout::load(Some(&dir.path().join("x.json"))),
            PaneLayout::default()
        );
        assert_eq!(PaneLayout::load(None), PaneLayout::default());
    }
}
