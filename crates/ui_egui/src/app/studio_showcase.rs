//! A showcase of the desktop studio components (ticket 058), shown in place
//! of the page on the desktop layout only. Toggle it with Ctrl+Shift+F12, or
//! start with `BLAULICHT_STUDIO_SHOWCASE=1`. The kiosk never draws it.

use crate::app::components::studio::{self, KnobSize, ListRow, ListRowState, ModalOutcome};
use crate::app::ui_tokens::{studio as tokens, TARGET_MIN, TARGET_SHOW};
use egui::{Id, Ui};

#[derive(Clone)]
struct State {
    open: bool,
    folders_open: [bool; 2],
    active: usize,
    faders: [f32; 4],
    master: f32,
    speed: f32,
    gain: f32,
    sw: [bool; 2],
    seg: usize,
    view: usize,
    preview: bool,
    identify: bool,
    num: [f64; 2],
    name: String,
    last: &'static str,
}

impl Default for State {
    fn default() -> Self {
        Self {
            open: std::env::var_os("BLAULICHT_STUDIO_SHOWCASE").is_some(),
            folders_open: [true, false],
            active: 1,
            faders: [255.0, 0.0, 128.0, 64.0],
            master: 100.0,
            speed: 1.0,
            gain: 0.0,
            sw: [true, false],
            seg: 0,
            view: 0,
            preview: false,
            identify: false,
            num: [1.25, 90.0],
            name: "New scene".into(),
            last: "",
        }
    }
}

fn state_id() -> Id {
    Id::new("studio_showcase")
}

/// Handles the toggle key and tells whether the showcase replaces the page.
pub fn showcase_open(ctx: &egui::Context) -> bool {
    let mut s: State = ctx.data_mut(|d| d.get_temp_mut_or_default::<State>(state_id()).clone());
    if ctx.input(|i| i.modifiers.ctrl && i.modifiers.shift && i.key_pressed(egui::Key::F12)) {
        s.open = !s.open;
        ctx.data_mut(|d| d.insert_temp(state_id(), s.clone()));
    }
    s.open
}

const NAMES: [&str; 6] = [
    "AT10",
    "MAC250E",
    "A very long fixture group name of forty+ characters",
    "DIMMER",
    "WAIT WALL",
    "DPACK BAR",
];

/// Draws the showcase into the page area.
pub fn showcase_ui(ui: &mut Ui) {
    let ctx = ui.ctx().clone();
    let mut s: State = ctx.data_mut(|d| d.get_temp_mut_or_default::<State>(state_id()).clone());
    let t = tokens(&ctx);
    egui::Frame::new().fill(t.gutter).show(ui, |ui| {
        ui.set_min_size(ui.available_size());
        ui.spacing_mut().item_spacing = egui::vec2(2.0, 2.0);
        let views = ["Components", "States"];
        let mut theme = if ctx.global_style().visuals.dark_mode {
            0
        } else {
            1
        };
        studio::page_bar(
            ui,
            |ui| {
                studio::view_tabs(ui, &mut s.view, &views);
            },
            |ui| {
                if studio::segmented(ui, &mut theme, &["Dark", "Light"], TARGET_MIN).clicked() {
                    ctx.set_theme(if theme == 0 {
                        egui::Theme::Dark
                    } else {
                        egui::Theme::Light
                    });
                }
                if studio::live_button(ui, "Exclusive preview", s.preview, TARGET_SHOW).clicked() {
                    s.preview = !s.preview;
                }
                if studio::mini_button(ui, "Rename…", false, TARGET_SHOW).clicked() {
                    studio::open_modal(&ctx, Id::new("showcase_rename"));
                }
                if studio::mini_button(ui, "Delete…", false, TARGET_SHOW).clicked()
                    && !studio::open_modal(&ctx, Id::new("showcase_delete"))
                {
                    s.last = "second dialog refused";
                }
            },
        );
        ui.horizontal_top(|ui| {
            pane_flush(ui, None, "Groups", 230.0, |ui| {
                studio::list_frame(ui, |ui| {
                    ui.set_width(230.0);
                    ui.set_min_height(394.0);
                    let groups: [(&str, &[usize]); 2] =
                        [("STAGE", &[0, 1, 2]), ("REST", &[3, 4, 5])];
                    for (fi, (folder, items)) in groups.iter().enumerate() {
                        let open = s.folders_open[fi];
                        if studio::folder_band(ui, folder, Some(items.len()), open, fi == 0)
                            .clicked()
                        {
                            s.folders_open[fi] = !open;
                        }
                        if open {
                            for &i in items.iter() {
                                let state = if i == s.active {
                                    ListRowState::Active
                                } else if i == 2 || i == 3 {
                                    ListRowState::Included
                                } else {
                                    ListRowState::Idle
                                };
                                let r = ListRow::new(NAMES[i])
                                    .count(i * 3 + 1)
                                    .state(state)
                                    .in_folder(true)
                                    .show(ui);
                                if r.clicked() {
                                    s.active = i;
                                }
                            }
                        }
                    }
                    for (i, id) in ["1", "12", "305"].iter().enumerate() {
                        let state = if s.active == 10 + i {
                            ListRowState::Active
                        } else {
                            ListRowState::Idle
                        };
                        if ListRow::new(NAMES[i + 1])
                            .id(id)
                            .state(state)
                            .show(ui)
                            .clicked()
                        {
                            s.active = 10 + i;
                        }
                    }
                });
            });
            pane(ui, Some("Selected group:"), "STAGE", 260.0, |ui| {
                ui.horizontal(|ui| {
                    ui.spacing_mut().item_spacing.x = 4.0;
                    for (i, name) in ["Intensity", "Strobe", "Hue", "Sat"].iter().enumerate() {
                        studio::fader(ui, &mut s.faders[i], 0.0..=255.0, name, i % 2 == 0, 300.0);
                    }
                });
            });
            pane(ui, Some("Fixture:"), "MAC250E 1", 420.0, |ui| {
                ui.horizontal(|ui| {
                    ui.spacing_mut().item_spacing.x = 22.0;
                    studio::knob(
                        ui,
                        &mut s.master,
                        0.0..=100.0,
                        "Intensity",
                        KnobSize::Normal,
                    );
                    studio::knob(ui, &mut s.speed, 0.25..=4.0, "Speed", KnobSize::Normal);
                    studio::knob(ui, &mut s.master, 0.0..=100.0, "Small", KnobSize::Small);
                });
                ui.add_space(10.0);
                studio::hfader(ui, &mut s.gain, -24.0..=24.0, "Audio gain", 200.0);
                ui.add_space(10.0);
                for (i, label) in ["Start with the show", "Remote UI"].iter().enumerate() {
                    ui.horizontal(|ui| {
                        ui.spacing_mut().item_spacing.x = 8.0;
                        studio::switch(ui, &mut s.sw[i]);
                        ui.label(*label);
                    });
                }
                ui.add_space(10.0);
                studio::segmented(ui, &mut s.seg, &["Top", "Front", "Side"], TARGET_SHOW);
                ui.add_space(10.0);
                studio::pane_header(ui, None, "Number fields and lit buttons", false, |_| {});
                ui.add_space(6.0);
                ui.horizontal(|ui| {
                    ui.spacing_mut().item_spacing.x = 8.0;
                    ui.label("Across (X)");
                    studio::num_field(ui, &mut s.num[0], 0.05, -50.0..=50.0, 2, 72.0);
                    ui.label("Rotation");
                    studio::num_field(ui, &mut s.num[1], 1.0, -180.0..=180.0, 0, 64.0);
                });
                ui.add_space(6.0);
                ui.horizontal(|ui| {
                    ui.spacing_mut().item_spacing.x = 6.0;
                    if studio::live_button(ui, "Identify", s.identify, TARGET_MIN).clicked() {
                        s.identify = !s.identify;
                    }
                    let _ = studio::live_button(ui, "Initialize", false, TARGET_MIN);
                });
                ui.add_space(10.0);
                ui.label(egui::RichText::new("Changed value").color(t.changed));
                ui.label(egui::RichText::new("Output at risk").color(t.red_text));
                ui.label(egui::RichText::new("Graph data").color(t.data));
                if !s.last.is_empty() {
                    ui.label(
                        egui::RichText::new(format!("Last dialog: {}", s.last)).color(t.muted),
                    );
                }
            });
        });
    });

    let mut name = s.name.clone();
    match studio::modal(
        &ctx,
        Id::new("showcase_rename"),
        "Rename scene",
        "Rename",
        false,
        |ui| {
            ui.horizontal(|ui| {
                ui.label("Name");
                ui.add(egui::TextEdit::singleline(&mut name).desired_width(280.0));
            });
            // A second dialog cannot open on top of this one.
            if ui.button("Open another dialog").clicked()
                && !studio::open_modal(ui.ctx(), Id::new("showcase_delete"))
            {
                ui.ctx()
                    .data_mut(|d| d.insert_temp(Id::new("showcase_refused"), true));
            }
            if ui
                .ctx()
                .data(|d| d.get_temp::<bool>(Id::new("showcase_refused")))
                .unwrap_or(false)
            {
                ui.label("Another dialog is already open.");
            }
        },
    ) {
        ModalOutcome::Confirmed => {
            s.name = name;
            s.last = "renamed";
        }
        ModalOutcome::Cancelled => s.last = "cancelled",
        ModalOutcome::Open => s.name = name,
    }
    match studio::modal(
        &ctx,
        Id::new("showcase_delete"),
        "Delete 3 scenes?",
        "Delete",
        true,
        |ui| {
            ui.label("Their changes are removed too. There is no undo.");
        },
    ) {
        ModalOutcome::Confirmed => s.last = "deleted",
        ModalOutcome::Cancelled => s.last = "cancelled",
        ModalOutcome::Open => {}
    }
    if studio::open_modal_of(&ctx).is_none() {
        ctx.data_mut(|d| d.remove::<bool>(Id::new("showcase_refused")));
    }
    ctx.data_mut(|d| d.insert_temp(state_id(), s));
}

fn pane(ui: &mut Ui, prefix: Option<&str>, title: &str, width: f32, add: impl FnOnce(&mut Ui)) {
    pane_with(ui, prefix, title, width, 8, add);
}

/// A pane whose list runs edge to edge under the header (`.pane > .body`).
fn pane_flush(
    ui: &mut Ui,
    prefix: Option<&str>,
    title: &str,
    width: f32,
    add: impl FnOnce(&mut Ui),
) {
    pane_with(ui, prefix, title, width, 0, add);
}

/// A pane in the first row of a master-detail page: 40px header.
fn pane_with(
    ui: &mut Ui,
    prefix: Option<&str>,
    title: &str,
    width: f32,
    margin: i8,
    add: impl FnOnce(&mut Ui),
) {
    let t = tokens(ui.ctx());
    egui::Frame::new().fill(t.pane).show(ui, |ui| {
        ui.vertical(|ui| {
            ui.set_width(width);
            ui.set_min_height(420.0);
            ui.spacing_mut().item_spacing.y = 0.0;
            studio::pane_header(ui, prefix, title, true, |ui| {
                if prefix.is_some() {
                    let _ = studio::mini_button(ui, "Edit", false, TARGET_SHOW);
                }
            });
            egui::Frame::new().inner_margin(margin).show(ui, |ui| {
                ui.vertical(|ui| {
                    ui.spacing_mut().item_spacing.y = 4.0;
                    add(ui)
                });
            });
        })
    });
}
