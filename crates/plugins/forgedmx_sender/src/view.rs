//! Plugin panel: a status/preview canvas on top, then the settings.

use blaulicht_plugin_framework::ui;

use super::color::{self, Rgb};
use super::health::Health;
use super::{
    ForgeDmxSender, BTN_APPLY, BTN_RESYNC, CHK_ENABLED, CMB_PRIMARY, CMB_SECONDARY, ID_API_PORT,
    ID_HOST, ID_PORT,
};

const CANVAS: u8 = 20;
const FRAME_COLORS: u8 = 21;
const FRAME_CONNECTION: u8 = 22;
const DETAILS: u8 = 23;

// The host's plugin window scrolls both ways, so content must stay narrower
// than its default width (~570 px) or the right edge gets cut off.
const PANEL_MAX_WIDTH: i32 = 540;
const CANVAS_W: i32 = 540;
const CANVAS_H: i32 = 140;

// Desk dark-theme tokens (crates/ui_egui/src/app/ui_tokens.rs).
type Tok = (u8, u8, u8);
const SURFACE: Tok = (23, 28, 39);
const SUNKEN: Tok = (12, 16, 23);
const LINE: Tok = (37, 44, 59);
const LINE_STRONG: Tok = (52, 61, 80);
const TEXT: Tok = (213, 220, 234);
const TEXT_STRONG: Tok = (241, 244, 250);
const MUTED: Tok = (135, 144, 166);
const FAINT: Tok = (86, 95, 117);
const ACCENT: Tok = (61, 155, 255);
const DANGER: Tok = (240, 86, 95);
const WARN: Tok = (242, 177, 76);
const OK: Tok = (76, 195, 138);

fn rect((r, g, b): Tok, x: i32, y: i32, w: i32, h: i32) {
    ui::painter_rect(x, y, w, h, r, g, b, 255);
}

fn stroke((r, g, b): Tok, x: i32, y: i32, w: i32, h: i32) {
    ui::painter_rect_stroke(x, y, w, h, r, g, b, 255, 1);
}

fn text((r, g, b): Tok, x: i32, y: i32, size: i32, s: &str) {
    ui::painter_text(x, y, size, r, g, b, 255, s);
}

/// Painter text can't be measured, so long names are cut to fit their column.
fn fit(s: &str, max_chars: usize) -> String {
    if s.chars().count() <= max_chars {
        s.to_string()
    } else {
        let cut: String = s.chars().take(max_chars.saturating_sub(2)).collect();
        format!("{cut}..")
    }
}

/// What a preview swatch shows.
enum Swatch {
    Color { name: String, rgb: Rgb },
    /// Nothing selected: forgeDMX keeps A / derives B.
    Empty(&'static str),
    /// Selected palette was deleted or is no longer a color palette.
    Missing(u8),
}

impl ForgeDmxSender {
    fn palette_name(&self, id: u8) -> Option<&str> {
        self.palette_options.iter().find(|(p, _)| *p == id).map(|(_, n)| n.as_str())
    }

    fn swatch(&self, selected: Option<u8>, rgb: Option<Rgb>, empty: &'static str) -> Swatch {
        match (selected, rgb) {
            (None, _) => Swatch::Empty(empty),
            (Some(id), Some(rgb)) => Swatch::Color {
                name: self.palette_name(id).unwrap_or("?").to_string(),
                rgb,
            },
            (Some(id), None) => Swatch::Missing(id),
        }
    }

    fn status(&self) -> (Tok, String) {
        if !self.state.enabled {
            return (FAINT, "Disabled".into());
        }
        let h = &self.check.health;
        let tone = match h {
            Health::Receiving => OK,
            Health::Idle | Health::Unknown => WARN,
            _ => DANGER,
        };
        // The canvas column is narrow; the full text is under Details.
        let text = match h {
            Health::Unreachable(_) => format!("Not reachable: {}", h.short_reason()),
            Health::OscNotListening => "Not listening for OSC".into(),
            Health::NotArriving => "Messages don't arrive".into(),
            _ => h.text(),
        };
        (tone, text)
    }

    pub(crate) fn render_ui(&self) {
        ui::begin();
        ui::set_max_width(PANEL_MAX_WIDTH);
        self.render_canvas();

        if super::udp_host(&self.state.host).is_none() {
            ui::label("Host is not an IP address, so nothing is sent. Enter an IP and press Apply.");
        }
        if !self.error.is_empty() {
            ui::label(&format!("Settings not applied: {}", self.error));
        }
        if let Some(src) = self.check.sync_source.as_ref().filter(|s| !s.eq_ignore_ascii_case("api")) {
            ui::label(&format!(
                "forgeDMX sync source is {src}; its visuals follow this clock only on API."
            ));
        }

        self.render_colors();
        self.render_connection();
        self.render_details();
    }

    fn render_canvas(&self) {
        ui::painter_begin(CANVAS, CANVAS_W, CANVAS_H);
        rect(SURFACE, 0, 0, CANVAS_W, CANVAS_H);
        stroke(LINE, 0, 0, CANVAS_W, CANVAS_H);

        // Status.
        let (tone, status) = self.status();
        ui::painter_circle(22, 23, 6, tone.0, tone.1, tone.2, 255);
        text(TEXT_STRONG, 36, 13, 14, &fit(&status, 34));
        text(MUTED, 36, 34, 12, &format!("OSC -> {}", self.target()));

        // Tempo and beat.
        match self.last_bpm_sent.filter(|_| self.state.enabled) {
            Some(bpm) => {
                text(TEXT_STRONG, 16, 54, 36, &format!("{bpm:.1}"));
                text(MUTED, 124, 76, 13, "BPM");
            }
            None => text(MUTED, 16, 70, 14, "No tempo yet"),
        }
        let beat = self.last_beat.filter(|_| self.state.enabled);
        for i in 0..4u8 {
            let lit = beat.is_some_and(|b| b.position == i + 1);
            let tone = match (lit, i) {
                (true, 0) => TEXT_STRONG,
                (true, _) => ACCENT,
                (false, _) => LINE_STRONG,
            };
            rect(tone, 16 + i32::from(i) * 36, 112, 30, 8);
        }
        if beat.is_some_and(|b| b.estimated) {
            text(FAINT, 164, 109, 11, "bar counted");
        }

        // Palette previews.
        let a = self.swatch(self.state.primary_palette, self.colors.0, "unchanged");
        let b = self.swatch(self.state.secondary_palette, self.colors.1, "derived from A");
        self.render_swatch(296, "A", &a);
        self.render_swatch(418, "B", &b);

        ui::painter_end();
    }

    fn render_swatch(&self, x: i32, slot: &str, swatch: &Swatch) {
        const W: i32 = 108;
        const H: i32 = 78;
        const Y: i32 = 14;
        let label_y = Y + H + 8;
        match swatch {
            Swatch::Color { name, rgb } => {
                ui::painter_rect(x, Y, W, H, rgb[0], rgb[1], rgb[2], 255);
                stroke(LINE_STRONG, x, Y, W, H);
                text(TEXT, x, label_y, 12, &format!("{slot}  {}", fit(name, 12)));
                text(MUTED, x, label_y + 17, 11, &color::hex(*rgb));
            }
            Swatch::Empty(what) => {
                rect(SUNKEN, x, Y, W, H);
                stroke(LINE, x, Y, W, H);
                text(FAINT, x + 8, Y + H / 2 - 7, 11, what);
                text(MUTED, x, label_y, 12, &format!("{slot}  no palette"));
            }
            Swatch::Missing(id) => {
                rect(SUNKEN, x, Y, W, H);
                stroke(DANGER, x, Y, W, H);
                text(DANGER, x + 8, Y + H / 2 - 7, 11, "palette missing");
                text(MUTED, x, label_y, 12, &format!("{slot}  palette {id}"));
            }
        }
    }

    fn render_colors(&self) {
        ui::begin_frame_styled(FRAME_COLORS, "Colors", 8, 6, 0, 6);
        let mut names = vec!["None".to_string()];
        names.extend(self.palette_options.iter().map(|(id, name)| format!("{id}: {name}")));
        ui::begin_horizontal();
        ui::combo_box(
            "Primary (A)",
            CMB_PRIMARY,
            &names,
            self.index_for_palette(self.state.primary_palette),
        );
        ui::combo_box(
            "Secondary (B)",
            CMB_SECONDARY,
            &names,
            self.index_for_palette(self.state.secondary_palette),
        );
        ui::end_horizontal();
        if self.palette_options.is_empty() {
            ui::label("This show has no color palettes yet.");
        }
        ui::end_frame();
    }

    fn render_connection(&self) {
        ui::begin_frame_styled(FRAME_CONNECTION, "Connection", 8, 6, 0, 6);
        ui::begin_horizontal();
        ui::switch("Enabled", CHK_ENABLED, self.state.enabled);
        ui::button("Resend sync", BTN_RESYNC);
        ui::end_horizontal();
        // egui text fields have a fixed width, so one per row.
        ui::text_edit("Host      ", ID_HOST, &self.host_text);
        ui::text_edit("OSC port", ID_PORT, &self.port_text);
        ui::text_edit("API port ", ID_API_PORT, &self.api_port_text);
        ui::button("Apply", BTN_APPLY);
        ui::end_frame();
    }

    fn render_details(&self) {
        ui::begin_collapsing(DETAILS, "Details", false);
        if self.state.enabled {
            ui::label(&self.check.health.text());
        }
        match &self.check.remote {
            Some(r) => {
                let bpm = r.bpm.map_or("-".into(), |b| format!("{b:.1}"));
                let beat = r.beat_in_bar.map_or("-".into(), |b| b.to_string());
                let age = r.last_beat_age.map_or("-".into(), |a| format!("{a:.2} s"));
                ui::label(&format!(
                    "forgeDMX has BPM {bpm}, beat {beat}, last beat {age} ago, {} messages received",
                    r.messages
                ));
            }
            None => ui::label("No reply from forgeDMX's API yet."),
        }
        ui::label(&format!("{} OSC messages sent", self.messages));
        ui::label(&format!("API: {}", self.api_url("state")));
        ui::end_collapsing();
    }
}
