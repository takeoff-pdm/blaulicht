//! Design tokens of the "Console" UI direction (navy palette) and the UI
//! [`Density`].
//!
//! Every colour, radius, stroke and size of the desktop style lives here, so
//! the values can be aligned with the design system **Blaulicht Console**
//! (tickets 032–038) in one place. The names follow its vocabulary: `bg`,
//! `surface`, `line`, `muted`, `accent`, `selection`.
//!
//! - [`Density::Touch`] is the kiosk (800x480 touchscreen, mobile layout). Its
//!   values are today's kiosk sizes, so components render exactly as before.
//! - [`Density::Compact`] is the desktop layout: 24 px controls, 13 px IBM Plex
//!   Sans Condensed, hairline borders. [`install_desktop`] installs it.
//!
//! Components read the density with [`Density::of`] instead of checking the
//! layout, so pages need no per-layout code to pick a size.

use egui::{
    epaint::Shadow, style, Color32, CornerRadius, FontFamily, FontId, Margin, RichText, Stroke,
    Style, TextStyle, Vec2,
};

/// The colour tokens of one theme. [`DARK`] is the Console navy palette;
/// [`LIGHT`] its light counterpart. Components pick one with [`palette`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Palette {
    /// Content background (the page area).
    pub bg: Color32,
    /// Chrome one step up from `bg`: sidebar, status bar, windows.
    pub surface: Color32,
    /// Controls at rest: buttons, inputs, slider rails.
    pub surface_raised: Color32,
    /// Controls under the pointer.
    pub surface_hover: Color32,
    /// Controls while pressed.
    pub surface_pressed: Color32,
    /// Recessed fields: text edits, plots, scroll tracks.
    pub surface_sunken: Color32,

    /// Hairline separators and panel borders.
    pub line: Color32,
    /// Control borders.
    pub line_strong: Color32,
    /// Control borders under the pointer.
    pub line_hover: Color32,

    /// Body text.
    pub text: Color32,
    /// Emphasised text (hovered and pressed controls, headings).
    pub text_strong: Color32,
    /// Secondary text: captions, section headers, idle icons.
    pub muted: Color32,
    /// Disabled text.
    pub faint: Color32,

    /// The one accent: active, armed, selected.
    pub accent: Color32,
    pub accent_hover: Color32,
    /// Fill of a selected row or nav entry.
    pub selection: Color32,

    /// A toggle that is on ("lit"): fill, fill under the pointer, label.
    pub on_fill: Color32,
    pub on_fill_hover: Color32,
    pub on_text: Color32,

    /// The primary action of a dialog or toolbar: fill, fill under the
    /// pointer, label.
    pub primary_fill: Color32,
    pub primary_fill_hover: Color32,
    pub primary_text: Color32,

    /// Destructive actions and errors.
    pub danger: Color32,
    pub danger_fill_hover: Color32,
    pub warn: Color32,
    pub ok: Color32,
}

/// The dark theme: the Console navy palette.
pub const DARK: Palette = Palette {
    bg: Color32::from_rgb(17, 21, 30),
    surface: Color32::from_rgb(23, 28, 39),
    surface_raised: Color32::from_rgb(30, 37, 51),
    surface_hover: Color32::from_rgb(39, 48, 66),
    surface_pressed: Color32::from_rgb(47, 58, 80),
    surface_sunken: Color32::from_rgb(12, 16, 23),
    line: Color32::from_rgb(37, 44, 59),
    line_strong: Color32::from_rgb(52, 61, 80),
    line_hover: Color32::from_rgb(74, 86, 112),
    text: Color32::from_rgb(213, 220, 234),
    text_strong: Color32::from_rgb(241, 244, 250),
    muted: Color32::from_rgb(135, 144, 166),
    faint: Color32::from_rgb(86, 95, 117),
    accent: Color32::from_rgb(61, 155, 255),
    accent_hover: Color32::from_rgb(99, 175, 255),
    selection: Color32::from_rgb(27, 49, 80),
    on_fill: Color32::from_rgb(29, 67, 115),
    on_fill_hover: Color32::from_rgb(36, 80, 137),
    on_text: Color32::from_rgb(214, 234, 255),
    primary_fill: Color32::from_rgb(40, 118, 214),
    primary_fill_hover: Color32::from_rgb(56, 134, 232),
    primary_text: Color32::from_rgb(250, 252, 255),
    danger: Color32::from_rgb(240, 86, 95),
    danger_fill_hover: Color32::from_rgb(66, 30, 38),
    warn: Color32::from_rgb(242, 177, 76),
    ok: Color32::from_rgb(76, 195, 138),
};

/// The light theme (System › General › LIGHT on the desktop).
pub const LIGHT: Palette = Palette {
    bg: Color32::from_rgb(246, 247, 250),
    surface: Color32::from_rgb(235, 238, 243),
    surface_raised: Color32::from_rgb(255, 255, 255),
    surface_hover: Color32::from_rgb(229, 234, 243),
    surface_pressed: Color32::from_rgb(214, 222, 236),
    surface_sunken: Color32::from_rgb(255, 255, 255),
    line: Color32::from_rgb(218, 222, 230),
    line_strong: Color32::from_rgb(194, 201, 214),
    line_hover: Color32::from_rgb(150, 162, 184),
    text: Color32::from_rgb(34, 40, 52),
    text_strong: Color32::from_rgb(10, 14, 22),
    muted: Color32::from_rgb(98, 108, 128),
    faint: Color32::from_rgb(160, 168, 182),
    accent: Color32::from_rgb(18, 108, 228),
    accent_hover: Color32::from_rgb(40, 126, 240),
    selection: Color32::from_rgb(214, 229, 251),
    on_fill: Color32::from_rgb(214, 229, 251),
    on_fill_hover: Color32::from_rgb(198, 218, 249),
    on_text: Color32::from_rgb(10, 70, 160),
    primary_fill: Color32::from_rgb(24, 112, 224),
    primary_fill_hover: Color32::from_rgb(40, 128, 240),
    primary_text: Color32::from_rgb(255, 255, 255),
    danger: Color32::from_rgb(206, 40, 52),
    danger_fill_hover: Color32::from_rgb(252, 228, 230),
    warn: Color32::from_rgb(186, 118, 0),
    ok: Color32::from_rgb(28, 148, 88),
};

/// The palette of the current theme of `ctx`.
pub fn palette(ctx: &egui::Context) -> &'static Palette {
    if ctx.global_style().visuals.dark_mode {
        &DARK
    } else {
        &LIGHT
    }
}

/// Corner radius of controls (buttons, inputs, rows): egui's default 2 px.
pub const RADIUS: u8 = 2;
/// Corner radius of windows, menus and popups.
pub const RADIUS_WINDOW: u8 = 4;
/// Width of hairline borders and separators.
pub const HAIRLINE: f32 = 1.0;
/// Width of the accent bar on an active nav entry or a selected row.
pub const ACCENT_BAR: f32 = 2.0;

/// Font weight of IBM Plex Sans Condensed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Weight {
    Regular,
    Medium,
    SemiBold,
}

/// How dense the UI is. See the module docs.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Density {
    /// The kiosk touchscreen: today's sizes.
    #[default]
    Touch,
    /// The desktop layout: compact controls and text.
    Compact,
}

impl Density {
    fn id() -> egui::Id {
        egui::Id::new("blaulicht_ui_density")
    }

    /// The density of `ctx`; [`Density::Touch`] unless [`Density::set`] (or
    /// [`install_desktop`]) chose another one.
    pub fn of(ctx: &egui::Context) -> Self {
        ctx.data(|data| data.get_temp(Self::id()))
            .unwrap_or_default()
    }

    pub fn set(self, ctx: &egui::Context) {
        ctx.data_mut(|data| data.insert_temp(Self::id(), self));
    }

    pub fn is_compact(self) -> bool {
        self == Density::Compact
    }

    /// Height of a standard control (buttons, toolbar items).
    pub fn control_height(self) -> f32 {
        match self {
            Density::Touch => 32.0,
            Density::Compact => 24.0,
        }
    }

    /// Height of a navigation or list row.
    pub fn row_height(self) -> f32 {
        match self {
            Density::Touch => 28.0,
            Density::Compact => 24.0,
        }
    }

    /// Height of a list row with a second (detail) line.
    pub fn row_height_with_detail(self) -> f32 {
        match self {
            Density::Touch => 50.0,
            Density::Compact => 34.0,
        }
    }

    /// The minimum interaction size of standard egui widgets (drag values,
    /// combo boxes, check boxes). Touch keeps the kiosk's 44x36 finger size.
    pub fn interact_size(self) -> Vec2 {
        match self {
            Density::Touch => egui::vec2(44.0, 36.0),
            Density::Compact => egui::vec2(40.0, 24.0),
        }
    }

    /// Horizontal padding inside a button, on each side of the label.
    pub fn button_padding_x(self) -> f32 {
        match self {
            Density::Touch => 4.0,
            Density::Compact => 8.0,
        }
    }

    /// Size of body text.
    pub fn body_size(self) -> f32 {
        match self {
            Density::Touch => 14.0,
            Density::Compact => 13.0,
        }
    }

    /// Size of button labels.
    pub fn button_text_size(self) -> f32 {
        match self {
            Density::Touch => 11.0,
            Density::Compact => 13.0,
        }
    }

    /// The font of button labels: regular weight, like egui's own buttons.
    pub fn button_font(self) -> FontId {
        self.font(Weight::Regular, self.button_text_size())
    }

    /// Size of captions, details and section headers.
    pub fn small_size(self) -> f32 {
        match self {
            Density::Touch => 12.0,
            Density::Compact => 11.0,
        }
    }

    /// A UI font. Compact uses the IBM Plex Sans Condensed weights; Touch
    /// keeps the kiosk's proportional font (it has no weights).
    pub fn font(self, weight: Weight, size: f32) -> FontId {
        let family = match (self, weight) {
            (Density::Touch, _) | (Density::Compact, Weight::Regular) => FontFamily::Proportional,
            (Density::Compact, Weight::Medium) => {
                FontFamily::Name(blaulicht_assets::PLEX_MEDIUM_FAMILY.into())
            }
            (Density::Compact, Weight::SemiBold) => {
                FontFamily::Name(blaulicht_assets::PLEX_SEMIBOLD_FAMILY.into())
            }
        };
        FontId::new(size, family)
    }

    /// A small uppercase section header ("PERFORM", "SETUP") in `muted`.
    pub fn overline(self, text: &str) -> RichText {
        RichText::new(text.to_uppercase())
            .font(self.font(Weight::SemiBold, 10.5))
            .extra_letter_spacing(0.8)
            .weak()
    }
}

/// Text sizes of the Compact density's egui text styles.
fn compact_text_styles(style: &mut Style) {
    let density = Density::Compact;
    style.text_styles = [
        (
            TextStyle::Small,
            FontId::new(density.small_size(), FontFamily::Proportional),
        ),
        (
            TextStyle::Body,
            FontId::new(density.body_size(), FontFamily::Proportional),
        ),
        (
            TextStyle::Button,
            density.button_font(),
        ),
        (TextStyle::Heading, density.font(Weight::SemiBold, 16.0)),
        (
            TextStyle::Monospace,
            FontId::new(12.0, FontFamily::Monospace),
        ),
    ]
    .into();
}

/// Spacing of the Compact density.
fn compact_spacing(spacing: &mut style::Spacing) {
    let density = Density::Compact;
    spacing.item_spacing = egui::vec2(6.0, 4.0);
    spacing.button_padding = egui::vec2(8.0, 3.0);
    spacing.interact_size = density.interact_size();
    spacing.indent = 14.0;
    spacing.icon_width = 13.0;
    spacing.icon_width_inner = 7.0;
    spacing.icon_spacing = 5.0;
    spacing.slider_width = 140.0;
    spacing.combo_width = 120.0;
    spacing.combo_height = 240.0;
    spacing.text_edit_width = 200.0;
    spacing.window_margin = Margin::same(8);
    spacing.menu_margin = Margin::same(4);
    spacing.scroll = style::ScrollStyle {
        bar_width: 8.0,
        floating_width: 3.0,
        floating_allocated_width: 0.0,
        ..style::ScrollStyle::floating()
    };
}

/// One state of a standard widget: `fill` is also the button fill.
fn widget_visuals(fill: Color32, border: Color32, text: Color32) -> style::WidgetVisuals {
    style::WidgetVisuals {
        bg_fill: fill,
        weak_bg_fill: fill,
        bg_stroke: Stroke::new(HAIRLINE, border),
        corner_radius: CornerRadius::same(RADIUS),
        fg_stroke: Stroke::new(1.5_f32, text),
        expansion: 0.0,
    }
}

/// The desktop layout's visuals from a palette.
fn compact_visuals(p: &Palette, dark: bool) -> egui::Visuals {
    let mut visuals = if dark {
        egui::Visuals::dark()
    } else {
        egui::Visuals::light()
    };
    visuals.widgets = style::Widgets {
        // Labels and separators: separators draw `bg_stroke`, labels `fg_stroke`.
        noninteractive: style::WidgetVisuals {
            fg_stroke: Stroke::new(1.0_f32, p.text),
            ..widget_visuals(p.bg, p.line, p.text)
        },
        inactive: widget_visuals(p.surface_raised, p.line_strong, p.text),
        hovered: widget_visuals(p.surface_hover, p.line_hover, p.text_strong),
        active: widget_visuals(p.surface_pressed, p.accent, p.text_strong),
        open: widget_visuals(p.surface_raised, p.line_strong, p.text_strong),
    };
    visuals.selection = style::Selection {
        bg_fill: p.selection,
        stroke: Stroke::new(1.0_f32, p.accent_hover),
    };
    visuals.hyperlink_color = p.accent_hover;
    visuals.faint_bg_color = p.surface;
    visuals.extreme_bg_color = p.surface_sunken;
    visuals.code_bg_color = p.surface_sunken;
    visuals.warn_fg_color = p.warn;
    visuals.error_fg_color = p.danger;
    visuals.weak_text_color = Some(p.muted);
    visuals.panel_fill = p.bg;
    visuals.window_fill = p.surface;
    visuals.window_stroke = Stroke::new(HAIRLINE, p.line_strong);
    visuals.window_corner_radius = CornerRadius::same(RADIUS_WINDOW);
    visuals.menu_corner_radius = CornerRadius::same(RADIUS_WINDOW);
    let shadow_alpha = if dark { 110 } else { 40 };
    visuals.window_shadow = Shadow {
        offset: [0, 6],
        blur: 18,
        spread: 0,
        color: Color32::from_black_alpha(shadow_alpha),
    };
    visuals.popup_shadow = Shadow {
        offset: [0, 3],
        blur: 10,
        spread: 0,
        color: Color32::from_black_alpha(shadow_alpha),
    };
    visuals.slider_trailing_fill = true;
    visuals.indent_has_left_vline = true;
    visuals
}

/// The desktop layout's egui style for `theme`: Compact text and spacing and
/// the [`DARK`] or [`LIGHT`] palette.
pub fn desktop_style(theme: egui::Theme) -> Style {
    let mut style = Style {
        visuals: match theme {
            egui::Theme::Dark => compact_visuals(&DARK, true),
            egui::Theme::Light => compact_visuals(&LIGHT, false),
        },
        ..Style::default()
    };
    compact_text_styles(&mut style);
    compact_spacing(&mut style.spacing);
    style
}

/// Switches `ctx` to the desktop layout's look: the IBM Plex fonts (added to
/// `fonts`, which the caller installs), the [`desktop_style`] for both themes
/// and [`Density::Compact`].
pub fn install_desktop(ctx: &egui::Context, fonts: &mut egui::FontDefinitions) {
    blaulicht_assets::add_plex_fonts(fonts);
    ctx.set_style_of(egui::Theme::Dark, desktop_style(egui::Theme::Dark));
    ctx.set_style_of(egui::Theme::Light, desktop_style(egui::Theme::Light));
    Density::Compact.set(ctx);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn density_defaults_to_touch_and_can_be_set() {
        let ctx = egui::Context::default();
        assert_eq!(Density::of(&ctx), Density::Touch);
        Density::Compact.set(&ctx);
        assert_eq!(Density::of(&ctx), Density::Compact);
    }

    #[test]
    fn touch_keeps_the_kiosk_sizes() {
        // The kiosk layout must not change: these are the sizes it used
        // before the tokens existed.
        let touch = Density::Touch;
        assert_eq!(touch.interact_size(), egui::vec2(44.0, 36.0));
        assert_eq!(touch.control_height(), 32.0);
        assert_eq!(
            touch.font(Weight::SemiBold, 14.0),
            FontId::proportional(14.0)
        );
    }

    #[test]
    fn compact_style_is_dense_with_hairlines() {
        let style = desktop_style(egui::Theme::Dark);
        assert_eq!(style.spacing.interact_size.y, 24.0);
        assert_eq!(style.text_styles[&TextStyle::Body].size, 13.0);
        let inactive = style.visuals.widgets.inactive;
        assert_eq!(inactive.bg_stroke.width, HAIRLINE);
        assert_eq!(inactive.corner_radius, CornerRadius::same(RADIUS));
        assert_eq!(style.visuals.selection.bg_fill, DARK.selection);
    }
}
