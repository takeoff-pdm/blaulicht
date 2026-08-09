use std::collections::BTreeMap;
use std::fmt::Display;

use crate::app::{
    components::{self, ButtonSize, Dialog, HFader},
    BlaulichtApp,
};
use blaulicht_shared::{
    fixture::state::FixtureState,
    palette::{Palette, PaletteKind, PaletteOp},
    ControlEvent, ControlEventMessage, EventOriginator, FixtureProperty, HSVColor, RGBColor,
};
use egui::{Context, RichText, Widget};

#[derive(Debug, Clone, Copy, PartialEq)]
enum PaletteKindSelection {
    Color,
    Position,
    Beam,
    Single,
    Pointer,
}

impl Display for PaletteKindSelection {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Color => write!(f, "Color"),
            Self::Position => write!(f, "Position"),
            Self::Beam => write!(f, "Beam"),
            Self::Single => write!(f, "Single Property"),
            Self::Pointer => write!(f, "Pointer"),
        }
    }
}

impl PaletteKindSelection {
    const ALL: [Self; 5] = [
        Self::Color,
        Self::Position,
        Self::Beam,
        Self::Single,
        Self::Pointer,
    ];
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

#[derive(Debug, Clone, Copy, PartialEq)]
enum PaletteOpKind {
    Add,
    Mul,
    Div,
    Clamp,
    Min,
    Max,
}

impl Display for PaletteOpKind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Add => write!(f, "Add"),
            Self::Mul => write!(f, "Mul"),
            Self::Div => write!(f, "Div"),
            Self::Clamp => write!(f, "Clamp"),
            Self::Min => write!(f, "Min"),
            Self::Max => write!(f, "Max"),
        }
    }
}

impl PaletteOpKind {
    const ALL: [Self; 6] = [
        Self::Add,
        Self::Mul,
        Self::Div,
        Self::Clamp,
        Self::Min,
        Self::Max,
    ];

    fn from_op(op: &PaletteOp) -> Self {
        match op {
            PaletteOp::Add(_) => Self::Add,
            PaletteOp::Mul(_) => Self::Mul,
            PaletteOp::Div(_) => Self::Div,
            PaletteOp::Clamp { .. } => Self::Clamp,
            PaletteOp::Min(_) => Self::Min,
            PaletteOp::Max(_) => Self::Max,
        }
    }

    fn default_op(self) -> PaletteOp {
        match self {
            Self::Add => PaletteOp::Add(0.0),
            Self::Mul => PaletteOp::Mul(1.0),
            Self::Div => PaletteOp::Div(1.0),
            Self::Clamp => PaletteOp::Clamp { min: 0, max: 255 },
            Self::Min => PaletteOp::Min(255),
            Self::Max => PaletteOp::Max(0),
        }
    }
}

/// Which property a pointer palette's operations transform. `None` applies the
/// ops to all of the target's properties; `Some(p)` applies them only to `p`
/// and passes every other property through untouched.
#[derive(Debug, Clone, Copy, PartialEq)]
struct PointerPropertyOption(Option<FixtureProperty>);

impl Display for PointerPropertyOption {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self.0 {
            None => write!(f, "All properties"),
            Some(p) => write!(f, "{p}"),
        }
    }
}

impl PointerPropertyOption {
    fn all() -> Vec<Self> {
        let mut options = vec![Self(None)];
        options.extend(
            PropertySelection::ALL
                .iter()
                .map(|p| Self(Some(p.to_fixture_property()))),
        );
        options
    }
}

#[derive(Debug, Clone, PartialEq)]
struct PaletteTargetOption {
    id: u8,
    label: String,
}

impl Display for PaletteTargetOption {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "[{}] {}", self.id, self.label)
    }
}

pub struct PaletteUI {
    create_dialog_open: bool,
    edit_dialog_open: bool,
    delete_dialog_open: bool,
    delete_palette_id: Option<u8>,
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
    pointer_target: Option<u8>,
    pointer_target_dialog_open: bool,
    /// Which property the pointer's ops transform. `None` = all; `Some(p)` =
    /// only `p`, with every other property passing through untouched.
    pointer_property: Option<FixtureProperty>,
    pointer_property_dialog_open: bool,
    pointer_ops: Vec<PaletteOp>,
    /// `Some(idx)` while a kind dialog is open for op row `idx`;
    /// `idx == pointer_ops.len()` means the "Add op" dialog.
    pointer_op_kind_dialog: Option<usize>,
}

impl Default for PaletteUI {
    fn default() -> Self {
        Self {
            create_dialog_open: false,
            edit_dialog_open: false,
            delete_dialog_open: false,
            delete_palette_id: None,
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
            pointer_target: None,
            pointer_target_dialog_open: false,
            pointer_property: None,
            pointer_property_dialog_open: false,
            pointer_ops: Vec::new(),
            pointer_op_kind_dialog: None,
        }
    }
}

impl PaletteUI {
    fn build_palette_kind(&self) -> Option<PaletteKind> {
        match self.kind_selection {
            PaletteKindSelection::Color => {
                let rgb = RGBColor {
                    r: (self.color_rgb[0] * 255.0) as u8,
                    g: (self.color_rgb[1] * 255.0) as u8,
                    b: (self.color_rgb[2] * 255.0) as u8,
                };
                let hsv: HSVColor = rgb.into();
                Some(PaletteKind::Color(hsv))
            }
            PaletteKindSelection::Position => Some(PaletteKind::Position(
                blaulicht_shared::fixture::state::FixtureOrientation {
                    pan: self.pan,
                    tilt: self.tilt,
                },
            )),
            PaletteKindSelection::Beam => Some(PaletteKind::Beam {
                focus: self.focus,
                strobe_speed: self.strobe_speed,
            }),
            PaletteKindSelection::Single => Some(PaletteKind::Single(
                self.property_selection.to_fixture_property(),
                self.single_value,
            )),
            PaletteKindSelection::Pointer => {
                self.pointer_target.map(|target| PaletteKind::Pointer {
                    target,
                    property: self.pointer_property,
                    ops: self.pointer_ops.clone(),
                })
            }
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
            PaletteKind::Pointer {
                target,
                property,
                ops,
            } => {
                self.kind_selection = PaletteKindSelection::Pointer;
                self.pointer_target = Some(*target);
                self.pointer_property = *property;
                self.pointer_ops = ops.clone();
            }
        }
    }
}

impl BlaulichtApp {
    fn render_delete_palette_dialog(&mut self, ctx: &Context) {
        if !self.palette_ui_state.delete_dialog_open {
            return;
        }

        Dialog::new("Delete Palette".to_string(), egui::vec2(260.0, 130.0))
            .with_backdrop()
            .show(ctx, |ui| {
                ui.heading(RichText::new("Delete this palette?").strong());
                ui.add_space(12.0);
                ui.horizontal(|ui| {
                    if components::button(ui, false, "Confirm", ButtonSize::Medium) {
                        if let Some(id) = self.palette_ui_state.delete_palette_id {
                            let _ = self
                                .data
                                .event_bus_connection
                                .send(ControlEventMessage::new(
                                    EventOriginator::Web,
                                    ControlEvent::DeletePalette(id),
                                ));
                        }
                        self.palette_ui_state.delete_dialog_open = false;
                        self.palette_ui_state.delete_palette_id = None;
                    }
                    if components::button(ui, true, "Cancel", ButtonSize::Medium) {
                        self.palette_ui_state.delete_dialog_open = false;
                        self.palette_ui_state.delete_palette_id = None;
                    }
                });
            });
    }

    pub fn palettes_ui(
        &mut self,
        ui: &mut egui::Ui,
        ctx: &Context,
        render_context: crate::app::page::PageRenderContext,
    ) {
        let palettes_snapshot: Vec<(u8, Palette)> = {
            let engine = self.data.state.dmx_engine.read().unwrap();
            engine
                .0
                .palettes
                .iter()
                .map(|(id, p)| (*id, p.clone()))
                .collect()
        };

        self.render_palette_create_dialog(ctx, &palettes_snapshot);
        self.render_palette_edit_dialog(ctx, &palettes_snapshot);
        self.render_delete_palette_dialog(ctx);

        ui.heading("Palettes");
        ui.add_space(8.0);

        if components::button(ui, false, "Create Palette", ButtonSize::Medium) {
            self.palette_ui_state.create_dialog_open = true;
            self.palette_ui_state.new_name = "New Palette".to_string();
            self.palette_ui_state.kind_selection = PaletteKindSelection::Color;
            self.palette_ui_state.pointer_target = None;
            self.palette_ui_state.pointer_property = None;
            self.palette_ui_state.pointer_ops.clear();
        }

        ui.add_space(12.0);

        if palettes_snapshot.is_empty() {
            ui.label(RichText::new("No palettes defined.").weak());
            return;
        }

        let mut color_update: Option<(u8, HSVColor)> = None;

        // Map view of the snapshot so previews can resolve pointer chains.
        let palettes_map: BTreeMap<u8, Palette> = palettes_snapshot.iter().cloned().collect();

        egui::ScrollArea::vertical().show(ui, |ui| {
            for (id, palette) in &palettes_snapshot {
                render_context.horizontal(ui, egui::Align::Center, |ui| {
                        if let Some(new_hsv) =
                            Self::render_palette_preview(ui, palette, &palettes_map)
                        {
                            color_update = Some((*id, new_hsv));
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
                            PaletteKind::Pointer { .. } => "Pointer",
                        };

                        ui.label(
                            RichText::new(format!("[{}] {} ({})", id, palette.name, kind_label))
                                .strong(),
                        );

                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            if components::button(ui, false, "Delete", ButtonSize::Small) {
                                self.palette_ui_state.delete_palette_id = Some(*id);
                                self.palette_ui_state.delete_dialog_open = true;
                            }

                            if components::button(ui, false, "Edit", ButtonSize::Small) {
                                let engine = self.data.state.dmx_engine.read().unwrap();
                                if let Some(p) = engine.0.palettes.get(id) {
                                    self.palette_ui_state.load_from_palette(p);
                                    self.palette_ui_state.edit_palette_id = Some(*id);
                                    self.palette_ui_state.edit_dialog_open = true;
                                }
                            }

                            if components::button(ui, false, "Unassign", ButtonSize::Small) {
                                self.data
                                    .event_bus_connection
                                    .send(ControlEventMessage::new(
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
            self.data
                .event_bus_connection
                .send(ControlEventMessage::new(
                    EventOriginator::Web,
                    ControlEvent::UpdatePalette(id, PaletteKind::Color(new_color)),
                ));
        }
    }

    /// Renders a small color preview for a palette without the caller needing
    /// to know the palette's kind.
    ///
    /// - A direct `Color` palette gets an editable swatch; when the user picks a
    ///   new color it is returned as `Some(new_hsv)` for the caller to persist.
    /// - Any other palette that resolves to a color (e.g. a `Pointer` aimed at a
    ///   color palette, possibly through a chain) gets a read-only swatch.
    /// - Palettes that don't yield a color (Position, Beam, non-color Single,
    ///   pointers to those) render nothing.
    ///
    /// Always returns `None` for the non-editable cases.
    fn render_palette_preview(
        ui: &mut egui::Ui,
        palette: &Palette,
        palettes: &BTreeMap<u8, Palette>,
    ) -> Option<HSVColor> {
        // Direct color palette: editable swatch.
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
                return Some(new_rgb.into());
            }
            return None;
        }

        // Anything else that resolves to a color (e.g. a pointer): read-only.
        // Match the editable color button's footprint (`interact_size`) so the
        // swatch lines up with direct color palettes.
        if let Some(rgb) = palette_resolved_rgb(&palette.kind, palettes) {
            egui::color_picker::show_color(
                ui,
                egui::Color32::from_rgb(rgb.r, rgb.g, rgb.b),
                ui.spacing().interact_size,
            );
        }

        None
    }

    fn render_palette_meta(ui: &mut egui::Ui, ctx: &Context, state: &mut PaletteUI) {
        ui.label(RichText::new("Name").weak());
        ui.add(egui::TextEdit::singleline(&mut state.new_name).desired_width(f32::INFINITY));
        ui.add_space(12.0);

        ui.label(RichText::new("Type").weak());
        if components::button(
            ui,
            state.kind_dialog_open,
            &state.kind_selection.to_string(),
            ButtonSize::Medium,
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

        if matches!(state.kind_selection, PaletteKindSelection::Single) {
            ui.add_space(12.0);
            ui.label(RichText::new("Property").weak());
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
        }
    }

    fn render_palette_value(
        ui: &mut egui::Ui,
        ctx: &Context,
        state: &mut PaletteUI,
        palettes: &[(u8, Palette)],
        editing_palette_id: Option<u8>,
    ) {
        match state.kind_selection {
            PaletteKindSelection::Color => {
                let mut color = egui::Color32::from_rgb(
                    (state.color_rgb[0] * 255.0) as u8,
                    (state.color_rgb[1] * 255.0) as u8,
                    (state.color_rgb[2] * 255.0) as u8,
                );
                if egui::color_picker::color_picker_color32(
                    ui,
                    &mut color,
                    egui::color_picker::Alpha::Opaque,
                ) {
                    state.color_rgb = [
                        color.r() as f32 / 255.0,
                        color.g() as f32 / 255.0,
                        color.b() as f32 / 255.0,
                    ];
                }
            }
            PaletteKindSelection::Position => {
                let mut pan = state.pan as f32;
                if HFader::new(&mut pan, 0.0..=255.0)
                    .with_label("Pan")
                    .ui(ui)
                    .changed()
                {
                    state.pan = pan as u8;
                }
                ui.add_space(8.0);
                let mut tilt = state.tilt as f32;
                if HFader::new(&mut tilt, 0.0..=255.0)
                    .with_label("Tilt")
                    .ui(ui)
                    .changed()
                {
                    state.tilt = tilt as u8;
                }
            }
            PaletteKindSelection::Beam => {
                let mut focus = state.focus as f32;
                if HFader::new(&mut focus, 0.0..=255.0)
                    .with_label("Focus")
                    .ui(ui)
                    .changed()
                {
                    state.focus = focus as u8;
                }
                ui.add_space(8.0);
                let mut strobe = state.strobe_speed as f32;
                if HFader::new(&mut strobe, 0.0..=255.0)
                    .with_label("Strobe")
                    .ui(ui)
                    .changed()
                {
                    state.strobe_speed = strobe as u8;
                }
            }
            PaletteKindSelection::Single => {
                let mut val_f = state.single_value as f32;
                if HFader::new(&mut val_f, 0.0..=255.0)
                    .with_label("Value")
                    .ui(ui)
                    .changed()
                {
                    state.single_value = val_f as u16;
                }
            }
            PaletteKindSelection::Pointer => {
                Self::render_pointer_editor(ui, ctx, state, palettes, editing_palette_id);
            }
        }
    }

    fn render_pointer_editor(
        ui: &mut egui::Ui,
        ctx: &Context,
        state: &mut PaletteUI,
        palettes: &[(u8, Palette)],
        editing_palette_id: Option<u8>,
    ) {
        ui.label(RichText::new("Target").weak());

        let target_options: Vec<PaletteTargetOption> = palettes
            .iter()
            .filter(|(id, _)| Some(*id) != editing_palette_id)
            .map(|(id, p)| PaletteTargetOption {
                id: *id,
                label: p.name.clone(),
            })
            .collect();

        let current_target_label = match state.pointer_target {
            Some(id) => match palettes.iter().find(|(pid, _)| *pid == id) {
                Some((pid, p)) => format!("[{}] {}", pid, p.name),
                None => format!("[{}] (missing)", id),
            },
            None => "Pick target…".to_string(),
        };

        if components::button(
            ui,
            state.pointer_target_dialog_open,
            &current_target_label,
            ButtonSize::Medium,
        ) {
            state.pointer_target_dialog_open = true;
        }

        let current_target_option = state
            .pointer_target
            .and_then(|id| target_options.iter().find(|opt| opt.id == id).cloned())
            .unwrap_or(PaletteTargetOption {
                id: 0,
                label: String::new(),
            });

        let (new_target, target_changed) = components::selection_dialog(
            ctx,
            target_options.clone(),
            current_target_option,
            &mut state.pointer_target_dialog_open,
            "Select Target Palette".to_string(),
        );
        if target_changed {
            state.pointer_target = Some(new_target.id);
        }

        ui.add_space(12.0);
        ui.label(RichText::new("Apply operations to").weak());

        let current_property = PointerPropertyOption(state.pointer_property);
        if components::button(
            ui,
            state.pointer_property_dialog_open,
            &current_property.to_string(),
            ButtonSize::Medium,
        ) {
            state.pointer_property_dialog_open = true;
        }

        let (new_property, property_changed) = components::selection_dialog(
            ctx,
            PointerPropertyOption::all(),
            current_property,
            &mut state.pointer_property_dialog_open,
            "Select Pointer Property".to_string(),
        );
        if property_changed {
            state.pointer_property = new_property.0;
        }

        ui.add_space(12.0);
        ui.label(RichText::new("Operations").weak());

        let mut delete_idx: Option<usize> = None;
        let mut kind_change: Option<(usize, PaletteOpKind)> = None;

        for (idx, op) in state.pointer_ops.iter_mut().enumerate() {
            ui.horizontal(|ui| {
                let kind = PaletteOpKind::from_op(op);
                let is_open = state.pointer_op_kind_dialog == Some(idx);
                if components::button(ui, is_open, &kind.to_string(), ButtonSize::Small) {
                    state.pointer_op_kind_dialog = Some(idx);
                }

                match op {
                    PaletteOp::Add(n) | PaletteOp::Mul(n) | PaletteOp::Div(n) => {
                        ui.add(egui::DragValue::new(n).speed(0.1));
                    }
                    PaletteOp::Clamp { min, max } => {
                        ui.label("min");
                        ui.add(egui::DragValue::new(min).range(0..=u16::MAX));
                        ui.label("max");
                        ui.add(egui::DragValue::new(max).range(0..=u16::MAX));
                    }
                    PaletteOp::Min(n) | PaletteOp::Max(n) => {
                        ui.add(egui::DragValue::new(n).range(0..=u16::MAX));
                    }
                }

                if components::button(ui, false, "✕", ButtonSize::Small) {
                    delete_idx = Some(idx);
                }
            });

            // Per-row kind selection dialog (only renders when open).
            let mut row_dialog_open = state.pointer_op_kind_dialog == Some(idx);
            if row_dialog_open {
                let current_kind = PaletteOpKind::from_op(op);
                let (new_kind, changed) = components::selection_dialog(
                    ctx,
                    PaletteOpKind::ALL,
                    current_kind,
                    &mut row_dialog_open,
                    format!("Op {} Type", idx),
                );
                if changed {
                    kind_change = Some((idx, new_kind));
                }
                if !row_dialog_open {
                    state.pointer_op_kind_dialog = None;
                }
            }

            ui.add_space(4.0);
        }

        if let Some(idx) = delete_idx {
            state.pointer_ops.remove(idx);
        }
        if let Some((idx, kind)) = kind_change {
            if let Some(slot) = state.pointer_ops.get_mut(idx) {
                *slot = kind.default_op();
            }
        }

        ui.add_space(8.0);
        if components::button(ui, false, "Add Operation", ButtonSize::Small) {
            state.pointer_ops.push(PaletteOp::Add(0.0));
        }
    }

    fn render_palette_create_dialog(&mut self, ctx: &Context, palettes: &[(u8, Palette)]) {
        if !self.palette_ui_state.create_dialog_open {
            return;
        }

        let mut should_close = false;
        let mut should_create = false;

        let state = &mut self.palette_ui_state;
        Dialog::new("Create Palette".to_string(), egui::vec2(720.0, 500.0))
            .with_backdrop()
            .show(ctx, |ui| {
                Self::render_palette_dialog_body(ui, ctx, state, palettes, None);

                ui.add_space(12.0);
                ui.separator();
                ui.add_space(8.0);

                ui.vertical_centered(|ui| {
                    ui.horizontal(|ui| {
                        if components::button(ui, false, "Create", ButtonSize::Medium) {
                            should_create = true;
                        }
                        if components::button(ui, false, "Cancel", ButtonSize::Medium) {
                            should_close = true;
                        }
                    });
                });
            });

        if should_create {
            let name = self.palette_ui_state.new_name.clone();
            if let Some(kind) = self.palette_ui_state.build_palette_kind() {
                self.data
                    .event_bus_connection
                    .send(ControlEventMessage::new(
                        EventOriginator::Web,
                        ControlEvent::CreatePalette(name, kind),
                    ));
                self.palette_ui_state.create_dialog_open = false;
            }
        }

        if should_close {
            self.palette_ui_state.create_dialog_open = false;
        }
    }

    fn render_palette_dialog_body(
        ui: &mut egui::Ui,
        ctx: &Context,
        state: &mut PaletteUI,
        palettes: &[(u8, Palette)],
        editing_palette_id: Option<u8>,
    ) {
        let avail_w = ui.available_width();
        let col_gap = 16.0;
        let col_w = ((avail_w - col_gap) / 2.0).max(120.0);

        ui.allocate_ui_with_layout(
            egui::vec2(avail_w, 0.0),
            egui::Layout::left_to_right(egui::Align::Min),
            |ui| {
                ui.allocate_ui_with_layout(
                    egui::vec2(col_w, ui.available_height()),
                    egui::Layout::top_down(egui::Align::Min),
                    |ui| {
                        ui.set_min_width(col_w);
                        Self::render_palette_meta(ui, ctx, state);
                    },
                );
                ui.add_space(col_gap);
                ui.allocate_ui_with_layout(
                    egui::vec2(col_w, ui.available_height()),
                    egui::Layout::top_down(egui::Align::Min),
                    |ui| {
                        ui.set_min_width(col_w);
                        Self::render_palette_value(ui, ctx, state, palettes, editing_palette_id);
                    },
                );
            },
        );
    }

    fn render_palette_edit_dialog(&mut self, ctx: &Context, palettes: &[(u8, Palette)]) {
        if !self.palette_ui_state.edit_dialog_open {
            return;
        }

        let Some(palette_id) = self.palette_ui_state.edit_palette_id else {
            self.palette_ui_state.edit_dialog_open = false;
            return;
        };

        let mut should_close = false;
        let mut should_save = false;

        let state = &mut self.palette_ui_state;
        Dialog::new("Edit Palette".to_string(), egui::vec2(720.0, 500.0))
            .with_backdrop()
            .show(ctx, |ui| {
                Self::render_palette_dialog_body(ui, ctx, state, palettes, Some(palette_id));

                ui.add_space(12.0);
                ui.separator();
                ui.add_space(8.0);

                ui.vertical_centered(|ui| {
                    ui.horizontal(|ui| {
                        if components::button(ui, false, "Save", ButtonSize::Medium) {
                            should_save = true;
                        }
                        if components::button(ui, false, "Cancel", ButtonSize::Medium) {
                            should_close = true;
                        }
                    });
                });
            });

        if should_save {
            let name = self.palette_ui_state.new_name.clone();
            if let Some(kind) = self.palette_ui_state.build_palette_kind() {
                self.data
                    .event_bus_connection
                    .send(ControlEventMessage::new(
                        EventOriginator::Web,
                        ControlEvent::RenamePalette(palette_id, name),
                    ));
                self.data
                    .event_bus_connection
                    .send(ControlEventMessage::new(
                        EventOriginator::Web,
                        ControlEvent::UpdatePalette(palette_id, kind),
                    ));
                self.palette_ui_state.edit_dialog_open = false;
            }
        }

        if should_close {
            self.palette_ui_state.edit_dialog_open = false;
        }
    }
}

/// Resolves the RGB color a palette would produce, following pointer chains, if
/// it covers color properties. Returns `None` for palettes that don't yield a
/// color so previews can skip drawing a swatch.
fn palette_resolved_rgb(kind: &PaletteKind, palettes: &BTreeMap<u8, Palette>) -> Option<RGBColor> {
    if !kind
        .properties(palettes)
        .contains(&FixtureProperty::ColorHue)
    {
        return None;
    }

    // Run the kind through the same resolution path the engine uses, then read
    // back the concrete color.
    let mut state = FixtureState::default();
    kind.apply_to(&mut state, palettes);
    Some(state.resolve(palettes).color.into())
}
