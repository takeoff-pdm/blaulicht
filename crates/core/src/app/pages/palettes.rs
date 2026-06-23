use std::fmt::Display;

use crate::app::{
    components::{self, ButtonSize, Dialog},
    BlaulichtApp,
};
use blaulicht_shared::{
    palette::{Palette, PaletteKind},
    ControlEvent, ControlEventMessage, EventOriginator, FixtureProperty, HSVColor, RGBColor,
};
use egui::{Context, RichText};

#[derive(Debug, Clone, Copy, PartialEq)]
enum PaletteKindSelection {
    Color,
    Position,
    Beam,
    Single,
}

impl Display for PaletteKindSelection {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Color => write!(f, "Color"),
            Self::Position => write!(f, "Position"),
            Self::Beam => write!(f, "Beam"),
            Self::Single => write!(f, "Single Property"),
        }
    }
}

impl PaletteKindSelection {
    const ALL: [Self; 4] = [Self::Color, Self::Position, Self::Beam, Self::Single];
}

#[derive(Debug, Clone, Copy, PartialEq)]
enum PropertySelection {
    Alpha,
    Strobe,
    Focus,
    ColorHue,
    ColorSaturation,
    ColorValue,
    Tilt,
    Pan,
}

impl Display for PropertySelection {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Alpha => write!(f, "Alpha"),
            Self::Strobe => write!(f, "Strobe"),
            Self::Focus => write!(f, "Focus"),
            Self::ColorHue => write!(f, "Color Hue"),
            Self::ColorSaturation => write!(f, "Color Saturation"),
            Self::ColorValue => write!(f, "Color Value"),
            Self::Tilt => write!(f, "Tilt"),
            Self::Pan => write!(f, "Pan"),
        }
    }
}

impl PropertySelection {
    const ALL: [Self; 8] = [
        Self::Alpha,
        Self::Strobe,
        Self::Focus,
        Self::ColorHue,
        Self::ColorSaturation,
        Self::ColorValue,
        Self::Tilt,
        Self::Pan,
    ];

    fn to_fixture_property(self) -> FixtureProperty {
        match self {
            Self::Alpha => FixtureProperty::Alpha,
            Self::Strobe => FixtureProperty::Strobe,
            Self::Focus => FixtureProperty::Focus,
            Self::ColorHue => FixtureProperty::ColorHue,
            Self::ColorSaturation => FixtureProperty::ColorSaturation,
            Self::ColorValue => FixtureProperty::ColorValue,
            Self::Tilt => FixtureProperty::Tilt,
            Self::Pan => FixtureProperty::Pan,
        }
    }

    fn from_fixture_property(prop: FixtureProperty) -> Self {
        match prop {
            FixtureProperty::Alpha => Self::Alpha,
            FixtureProperty::Strobe => Self::Strobe,
            FixtureProperty::Focus => Self::Focus,
            FixtureProperty::ColorHue => Self::ColorHue,
            FixtureProperty::ColorSaturation => Self::ColorSaturation,
            FixtureProperty::ColorValue => Self::ColorValue,
            FixtureProperty::Tilt => Self::Tilt,
            FixtureProperty::Pan => Self::Pan,
        }
    }
}

pub struct PaletteUI {
    create_dialog_open: bool,
    edit_dialog_open: bool,
    edit_palette_id: Option<u8>,
    new_name: String,
    kind_selection: PaletteKindSelection,
    kind_dialog_open: bool,
    property_selection: PropertySelection,
    property_dialog_open: bool,
    color_rgb: [f32; 3],
    pan: u8,
    tilt: u8,
    focus: u8,
    strobe_speed: u8,
    single_value: u16,
}

impl Default for PaletteUI {
    fn default() -> Self {
        Self {
            create_dialog_open: false,
            edit_dialog_open: false,
            edit_palette_id: None,
            new_name: "New Palette".to_string(),
            kind_selection: PaletteKindSelection::Color,
            kind_dialog_open: false,
            property_selection: PropertySelection::Alpha,
            property_dialog_open: false,
            color_rgb: [1.0, 0.0, 0.0],
            pan: 128,
            tilt: 128,
            focus: 0,
            strobe_speed: 0,
            single_value: 0,
        }
    }
}

impl PaletteUI {
    fn build_palette_kind(&self) -> PaletteKind {
        match self.kind_selection {
            PaletteKindSelection::Color => {
                let rgb = RGBColor {
                    r: (self.color_rgb[0] * 255.0) as u8,
                    g: (self.color_rgb[1] * 255.0) as u8,
                    b: (self.color_rgb[2] * 255.0) as u8,
                };
                let hsv: HSVColor = rgb.into();
                PaletteKind::Color(hsv)
            }
            PaletteKindSelection::Position => {
                PaletteKind::Position(blaulicht_shared::fixture::state::FixtureOrientation {
                    pan: self.pan,
                    tilt: self.tilt,
                })
            }
            PaletteKindSelection::Beam => PaletteKind::Beam {
                focus: self.focus,
                strobe_speed: self.strobe_speed,
            },
            PaletteKindSelection::Single => PaletteKind::Single(
                self.property_selection.to_fixture_property(),
                self.single_value,
            ),
        }
    }

    fn load_from_palette(&mut self, palette: &Palette) {
        self.new_name = palette.name.clone();
        match &palette.kind {
            PaletteKind::Color(c) => {
                self.kind_selection = PaletteKindSelection::Color;
                let rgb: RGBColor = c.clone().into();
                self.color_rgb = [
                    rgb.r as f32 / 255.0,
                    rgb.g as f32 / 255.0,
                    rgb.b as f32 / 255.0,
                ];
            }
            PaletteKind::Position(o) => {
                self.kind_selection = PaletteKindSelection::Position;
                self.pan = o.pan;
                self.tilt = o.tilt;
            }
            PaletteKind::Beam {
                focus,
                strobe_speed,
            } => {
                self.kind_selection = PaletteKindSelection::Beam;
                self.focus = *focus;
                self.strobe_speed = *strobe_speed;
            }
            PaletteKind::Single(prop, val) => {
                self.kind_selection = PaletteKindSelection::Single;
                self.property_selection = PropertySelection::from_fixture_property(*prop);
                self.single_value = *val;
            }
        }
    }
}

impl BlaulichtApp {
    pub fn palettes_ui(&mut self, ui: &mut egui::Ui, ctx: &Context) {
        self.render_palette_create_dialog(ctx);
        self.render_palette_edit_dialog(ctx);

        ui.heading("Palettes");
        ui.add_space(8.0);

        if components::button(ui, false, "Create Palette", ButtonSize::Medium) {
            self.palette_ui_state.create_dialog_open = true;
            self.palette_ui_state.new_name = "New Palette".to_string();
            self.palette_ui_state.kind_selection = PaletteKindSelection::Color;
        }

        ui.add_space(12.0);

        let engine = self.data.state.dmx_engine.read().unwrap();
        let palettes: Vec<(u8, Palette)> = engine
            .0
            .palettes
            .iter()
            .map(|(id, p)| (*id, p.clone()))
            .collect();
        drop(engine);

        if palettes.is_empty() {
            ui.label(RichText::new("No palettes defined.").weak());
            return;
        }

        let mut color_update: Option<(u8, HSVColor)> = None;

        egui::ScrollArea::vertical().show(ui, |ui| {
            for (id, palette) in &palettes {
                ui.horizontal(|ui| {
                    if let PaletteKind::Color(c) = &palette.kind {
                        let rgb: RGBColor = c.clone().into();
                        let mut color = [
                            rgb.r as f32 / 255.0,
                            rgb.g as f32 / 255.0,
                            rgb.b as f32 / 255.0,
                        ];
                        if ui.color_edit_button_rgb(&mut color).changed() {
                            let new_rgb = RGBColor {
                                r: (color[0] * 255.0) as u8,
                                g: (color[1] * 255.0) as u8,
                                b: (color[2] * 255.0) as u8,
                            };
                            let new_hsv: HSVColor = new_rgb.into();
                            color_update = Some((*id, new_hsv));
                        }
                    }

                    let kind_label = match &palette.kind {
                        PaletteKind::Color(_) => "Color",
                        PaletteKind::Position(_) => "Position",
                        PaletteKind::Beam { .. } => "Beam",
                        PaletteKind::Single(prop, _) => match prop {
                            FixtureProperty::Alpha => "Alpha",
                            FixtureProperty::Strobe => "Strobe",
                            FixtureProperty::Focus => "Focus",
                            _ => "Single",
                        },
                    };

                    ui.label(
                        RichText::new(format!("[{}] {} ({})", id, palette.name, kind_label))
                            .strong(),
                    );

                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if components::button(ui, false, "Delete", ButtonSize::Small) {
                            self.data.event_bus_connection.send(ControlEventMessage::new(
                                EventOriginator::Web,
                                ControlEvent::DeletePalette(*id),
                            ));
                        }

                        if components::button(ui, false, "Edit", ButtonSize::Small) {
                            let engine = self.data.state.dmx_engine.read().unwrap();
                            if let Some(p) = engine.0.palettes.get(id) {
                                self.palette_ui_state.load_from_palette(p);
                                self.palette_ui_state.edit_palette_id = Some(*id);
                                self.palette_ui_state.edit_dialog_open = true;
                            }
                        }

                        if components::button(ui, false, "Assign", ButtonSize::Small) {
                            self.data.event_bus_connection.send(ControlEventMessage::new(
                                EventOriginator::Web,
                                ControlEvent::AssignPalette(*id),
                            ));
                        }

                        if components::button(ui, false, "Unassign", ButtonSize::Small) {
                            self.data.event_bus_connection.send(ControlEventMessage::new(
                                EventOriginator::Web,
                                ControlEvent::UnassignPalette(*id),
                            ));
                        }
                    });
                });

                ui.separator();
            }
        });

        if let Some((id, new_color)) = color_update {
            self.data.event_bus_connection.send(ControlEventMessage::new(
                EventOriginator::Web,
                ControlEvent::UpdatePalette(id, PaletteKind::Color(new_color)),
            ));
        }
    }

    fn render_palette_kind_editor(
        ui: &mut egui::Ui,
        ctx: &Context,
        state: &mut PaletteUI,
    ) {
        let kind_button_size = ButtonSize::Medium;
        if components::button(
            ui,
            state.kind_dialog_open,
            &state.kind_selection.to_string(),
            kind_button_size,
        ) {
            state.kind_dialog_open = true;
        }

        let (new_kind, kind_changed) = components::selection_dialog(
            ctx,
            PaletteKindSelection::ALL,
            state.kind_selection,
            &mut state.kind_dialog_open,
            "Select Palette Type".to_string(),
        );
        if kind_changed {
            state.kind_selection = new_kind;
        }

        ui.add_space(8.0);

        match state.kind_selection {
            PaletteKindSelection::Color => {
                ui.color_edit_button_rgb(&mut state.color_rgb);
            }
            PaletteKindSelection::Position => {
                ui.add(egui::Slider::new(&mut state.pan, 0..=255).text("Pan"));
                ui.add(egui::Slider::new(&mut state.tilt, 0..=255).text("Tilt"));
            }
            PaletteKindSelection::Beam => {
                ui.add(egui::Slider::new(&mut state.focus, 0..=255).text("Focus"));
                ui.add(egui::Slider::new(&mut state.strobe_speed, 0..=255).text("Strobe"));
            }
            PaletteKindSelection::Single => {
                if components::button(
                    ui,
                    state.property_dialog_open,
                    &state.property_selection.to_string(),
                    ButtonSize::Medium,
                ) {
                    state.property_dialog_open = true;
                }

                let (new_prop, prop_changed) = components::selection_dialog(
                    ctx,
                    PropertySelection::ALL,
                    state.property_selection,
                    &mut state.property_dialog_open,
                    "Select Property".to_string(),
                );
                if prop_changed {
                    state.property_selection = new_prop;
                }

                let mut val_f = state.single_value as f32;
                ui.add(egui::Slider::new(&mut val_f, 0.0..=255.0).text("Value"));
                state.single_value = val_f as u16;
            }
        }
    }

    fn render_palette_create_dialog(&mut self, ctx: &Context) {
        if !self.palette_ui_state.create_dialog_open {
            return;
        }

        let mut should_close = false;
        let mut should_create = false;

        Dialog::new("Create Palette".to_string(), egui::vec2(300.0, 280.0))
            .with_backdrop()
            .show(ctx, |ui| {
                ui.text_edit_singleline(&mut self.palette_ui_state.new_name);
                ui.add_space(8.0);

                Self::render_palette_kind_editor(ui, ctx, &mut self.palette_ui_state);

                ui.add_space(12.0);
                ui.horizontal(|ui| {
                    if components::button(ui, false, "Create", ButtonSize::Medium) {
                        should_create = true;
                    }
                    if components::button(ui, false, "Cancel", ButtonSize::Medium) {
                        should_close = true;
                    }
                });
            });

        if should_create {
            let name = self.palette_ui_state.new_name.clone();
            let kind = self.palette_ui_state.build_palette_kind();
            self.data.event_bus_connection.send(ControlEventMessage::new(
                EventOriginator::Web,
                ControlEvent::CreatePalette(name, kind),
            ));
            self.palette_ui_state.create_dialog_open = false;
        }

        if should_close {
            self.palette_ui_state.create_dialog_open = false;
        }
    }

    fn render_palette_edit_dialog(&mut self, ctx: &Context) {
        if !self.palette_ui_state.edit_dialog_open {
            return;
        }

        let Some(palette_id) = self.palette_ui_state.edit_palette_id else {
            self.palette_ui_state.edit_dialog_open = false;
            return;
        };

        let mut should_close = false;
        let mut should_save = false;

        Dialog::new("Edit Palette".to_string(), egui::vec2(300.0, 280.0))
            .with_backdrop()
            .show(ctx, |ui| {
                ui.text_edit_singleline(&mut self.palette_ui_state.new_name);
                ui.add_space(8.0);

                Self::render_palette_kind_editor(ui, ctx, &mut self.palette_ui_state);

                ui.add_space(12.0);
                ui.horizontal(|ui| {
                    if components::button(ui, false, "Save", ButtonSize::Medium) {
                        should_save = true;
                    }
                    if components::button(ui, false, "Cancel", ButtonSize::Medium) {
                        should_close = true;
                    }
                });
            });

        if should_save {
            let name = self.palette_ui_state.new_name.clone();
            let kind = self.palette_ui_state.build_palette_kind();
            self.data.event_bus_connection.send(ControlEventMessage::new(
                EventOriginator::Web,
                ControlEvent::RenamePalette(palette_id, name),
            ));
            self.data.event_bus_connection.send(ControlEventMessage::new(
                EventOriginator::Web,
                ControlEvent::UpdatePalette(palette_id, kind),
            ));
            self.palette_ui_state.edit_dialog_open = false;
        }

        if should_close {
            self.palette_ui_state.edit_dialog_open = false;
        }
    }
}
