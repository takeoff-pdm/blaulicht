use crate::{
    app::{
        components::{
            self, ButtonSize, Dialog, HFader, TreeAction, TreeKey, NARROW_TREE_WIDTH,
            SETUP_GROUP_TREE,
        },
        BlaulichtApp, Selection,
    },
    client::{EngineClient, LocalClient},
    dmx::EngineState,
    state::NUM_DMX_UNIVERSES,
};
use blaulicht_shared::{
    fixture::state::{FixtureState, ResolvedFixtureState},
    palette::Palette,
    scene::BLANK_SCENE_ID,
    ControlEvent, ControlEventMessage, EngineGroups, EventOriginator, FixtureProperty, RGBColor,
};
use egui::{Align2, Color32, Context, FontId, Frame, Margin, Vec2, Widget};
use map_range::MapRange;
use std::collections::{BTreeMap, HashSet};

pub const DEFAULT_NEW_SCENE_NAME: &str = "My Scene";
pub const DEFAULT_NEW_GROUP_NAME: &str = "My Group";

/// Collapse state of the Programmer's group tree (kept for the session).
const PROGRAMMER_GROUP_TREE: &str = "programmer_groups";
/// Collapse state of the Programmer's scene tree (kept for the session).
const PROGRAMMER_SCENE_TREE: &str = "programmer_scenes";
/// Width of the Programmer's scene column: the old scene pager's 100 px.
/// The Programmer row at 800x480 has no room to spare (a 108 px column
/// already cut off the right column's "M. Speed"), so this is narrower than
/// [`NARROW_TREE_WIDTH`]: an 18 px toggle and a 70 px minimum body leave
/// 8 px of indentation, so every nested level indents 8 px and the guide
/// lines and chevrons show the depth.
const SCENE_COLUMN_WIDTH: f32 = 100.0;

#[derive(Default, Clone, Copy)]
pub struct DmxSimulator {
    pub open: bool,
    /// Universe shown in the simulation dialog (selected via the tab bar).
    pub universe: usize,
    pub map_fixtures: bool,
}

impl DmxSimulator {
    pub fn simulate_dmx(
        &mut self,
        ui: &mut egui::Ui,
        groups: &EngineGroups,
        dmx: &[u8; 513],
        universe_no: usize,
    ) {
        ui.vertical(|ui| {
            if components::Switch::new(&mut self.map_fixtures)
                .ui(ui)
                .changed()
            {
                tracing::debug!("changed map fixtures");
            }

            match self.map_fixtures {
                true => Self::render_mapped_ui(ui, groups, dmx, universe_no),
                false => Self::render_dmx(ui, dmx),
            }
        });
    }

    fn render_mapped_ui(
        ui: &mut egui::Ui,
        groups: &EngineGroups,
        dmx: &[u8; 513],
        universe_no: usize,
    ) {
        struct FixtureVisual {
            group_id: u8,
            fixture_id: u8,
            name: String,
            start_addr: usize,
            state: ResolvedFixtureState,
        }

        ui.set_min_height(300.0);

        let empty_palettes: BTreeMap<u8, Palette> = BTreeMap::new();
        let mut fixtures: Vec<FixtureVisual> = Vec::new();
        for (group_id, group) in groups {
            for (fixture_id, fixture) in &group.fixtures {
                if fixture.universe_no != universe_no {
                    continue;
                }

                let state = fixture.state_from_dmx(dmx).resolve(&empty_palettes);
                fixtures.push(FixtureVisual {
                    group_id: *group_id,
                    fixture_id: *fixture_id,
                    name: fixture.name.clone(),
                    start_addr: fixture.start_addr,
                    state,
                });
            }
        }

        fixtures.sort_by(|a, b| {
            a.start_addr
                .cmp(&b.start_addr)
                .then_with(|| a.group_id.cmp(&b.group_id))
                .then_with(|| a.fixture_id.cmp(&b.fixture_id))
        });

        if fixtures.is_empty() {
            ui.label("No fixtures mapped yet.");
            return;
        }

        const CARD_SIZE: Vec2 = Vec2::new(112.0, 88.0);
        const CARD_ROUNDING: f32 = 5.0;
        const CARD_MARGIN: f32 = 5.0;

        egui::ScrollArea::vertical()
            .id_salt(("dmx_fixture_map", universe_no))
            .show(ui, |ui| {
                ui.scope(|ui| {
                    ui.spacing_mut().item_spacing = egui::vec2(6.0, 8.0);
                    ui.horizontal_wrapped(|ui| {
                        for fixture in &fixtures {
                            let (rect, _) = ui.allocate_exact_size(CARD_SIZE, egui::Sense::hover());
                            let painter = ui.painter_at(rect);

                            let card_bg = Color32::from_rgb(28, 28, 36);
                            let border_color = Color32::from_rgb(68, 68, 82);
                            painter.rect_filled(rect, CARD_ROUNDING, card_bg);
                            painter.rect_stroke(
                                rect,
                                CARD_ROUNDING,
                                egui::Stroke::new(1.0, border_color),
                                egui::StrokeKind::Middle,
                            );

                            let title_font = FontId::proportional(12.0);
                            painter.text(
                                egui::pos2(rect.center().x, rect.top() + 3.0),
                                Align2::CENTER_TOP,
                                &fixture.name,
                                title_font.clone(),
                                Color32::from_rgb(220, 220, 230),
                            );

                            let inner_rect = egui::Rect::from_min_max(
                                egui::pos2(rect.min.x + CARD_MARGIN, rect.top() + 18.0),
                                egui::pos2(rect.max.x - CARD_MARGIN, rect.bottom() - CARD_MARGIN),
                            );

                            let color_rect_height = inner_rect.height() * 0.42;
                            let color_rect = egui::Rect::from_min_max(
                                inner_rect.min,
                                egui::pos2(inner_rect.max.x, inner_rect.min.y + color_rect_height),
                            );

                            let intensity = (fixture.state.alpha as f32 / 255.0).clamp(0.0, 1.0);
                            let color: RGBColor = fixture.state.color.into();
                            let color = color.with_alpha(fixture.state.alpha);
                            let color = Color32::from_rgb(color.r, color.g, color.b);

                            painter.rect_filled(color_rect, 3.0, color);
                            painter.rect_stroke(
                                color_rect,
                                3.0,
                                egui::Stroke::new(1.0, Color32::from_rgb(20, 20, 26)),
                                egui::StrokeKind::Middle,
                            );

                            if fixture.state.strobe_speed > 0 {
                                let strobe_alpha =
                                    (fixture.state.strobe_speed as f32 / 255.0 * 120.0) as u8;
                                painter.rect_filled(
                                    color_rect,
                                    3.0,
                                    Color32::from_rgba_premultiplied(255, 255, 255, strobe_alpha),
                                );
                            }

                            let alpha_bar_rect = egui::Rect::from_min_max(
                                egui::pos2(inner_rect.min.x, color_rect.max.y + 4.0),
                                egui::pos2(inner_rect.max.x, color_rect.max.y + 8.5),
                            );
                            painter.rect_filled(alpha_bar_rect, 3.0, Color32::from_rgb(36, 36, 44));

                            if intensity > 0.0 {
                                let filled_width = alpha_bar_rect.width() * intensity;
                                let filled_rect = egui::Rect::from_min_max(
                                    alpha_bar_rect.min,
                                    egui::pos2(
                                        alpha_bar_rect.min.x + filled_width,
                                        alpha_bar_rect.max.y,
                                    ),
                                );
                                painter.rect_filled(
                                    filled_rect,
                                    3.0,
                                    Color32::from_rgb(124, 208, 244),
                                );
                            }

                            let detail_font = FontId::monospace(10.0);
                            let detail_text = format!(
                                "Addr {:>3} | G{} F{}",
                                fixture.start_addr, fixture.group_id, fixture.fixture_id
                            );
                            painter.text(
                                egui::pos2(rect.center().x, rect.bottom() - 18.0),
                                Align2::CENTER_TOP,
                                detail_text,
                                detail_font.clone(),
                                Color32::from_rgb(200, 200, 208),
                            );

                            let orientation_text = format!(
                                "Pan {:>3}  Tilt {:>3}",
                                fixture.state.orientation.pan, fixture.state.orientation.tilt
                            );
                            painter.text(
                                egui::pos2(rect.center().x, rect.bottom() - 32.0),
                                Align2::CENTER_TOP,
                                orientation_text,
                                detail_font,
                                Color32::from_rgb(170, 170, 180),
                            );
                        }
                    });
                });
            });
    }

    fn render_dmx(ui: &mut egui::Ui, dmx: &[u8; 513]) {
        let len = dmx.len() as f32;
        let dimensions = len.sqrt() as usize + 1;

        let base_height = 16.0;
        let padding = 1.0;
        let dim_pixels = dimensions as f32 * (base_height + padding);
        let dmx_buffer = dmx;

        ui.horizontal(|ui| {
            // Remove spacing in this container
            ui.spacing_mut().item_spacing = egui::vec2(0.0, 0.0);

            // Allocate fixed square space for the matrix
            let (rect, _response) =
                ui.allocate_exact_size(egui::vec2(dim_pixels, dim_pixels), egui::Sense::hover());
            let painter = ui.painter();

            // Draw the cells manually
            for row in 0..dimensions {
                for col in 0..dimensions {
                    let value = dmx_buffer.get(row * dimensions + col);

                    // Calculate top-left corner of this cell
                    let x = rect.min.x + col as f32 * (base_height + padding);
                    let y = rect.min.y + row as f32 * (base_height + padding);

                    let cell_rect = egui::Rect::from_min_size(
                        egui::pos2(x, y),
                        egui::vec2(base_height, base_height),
                    );

                    // Color based on value
                    let (bg_color, fg_color) = match value {
                        Some(0) => (Color32::from_rgb(10, 10, 10), Color32::WHITE),
                        Some(1..=85) => (Color32::from_rgb(255, 0, 0), Color32::WHITE),
                        Some(86..=170) => (Color32::from_rgb(255, 255, 0), Color32::BLACK),
                        Some(171..=255) => (Color32::from_rgb(0, 255, 0), Color32::MAGENTA),
                        None => (Color32::TRANSPARENT, Color32::TRANSPARENT),
                    };

                    painter.rect_filled(cell_rect, 0.0, bg_color);
                    if let Some(value) = value {
                        if *value > 0 {
                            painter.text(
                                egui::Pos2 { x, y },
                                Align2::LEFT_TOP,
                                value.to_string(),
                                egui::FontId {
                                    size: 9.0,
                                    family: egui::FontFamily::Monospace,
                                },
                                fg_color,
                            );
                        }
                    }
                }
            }
        });
    }
}

impl BlaulichtApp {
    pub fn render_dmx_simulation_dialog(&mut self, ctx: &Context, groups: &EngineGroups) {
        const WIDTH: f32 = 500.0;
        const TAB_GAP: f32 = 4.0;

        let simulator = &mut self.dmx_simulator;
        if !simulator.open {
            return;
        }

        Dialog::new("Simulate DMX".to_string(), egui::vec2(WIDTH, 500.0))
            .moveable()
            .show(ctx, |ui| {
                ui.horizontal_wrapped(|ui| {
                    ui.spacing_mut().item_spacing = egui::vec2(TAB_GAP, TAB_GAP);
                    // Slack of one pixel so rounding never pushes the last tab onto a second row.
                    let tab_width = ((ui.available_width()
                        - TAB_GAP * (NUM_DMX_UNIVERSES - 1) as f32)
                        / NUM_DMX_UNIVERSES as f32)
                        .floor()
                        - 1.0;
                    for universe in 0..NUM_DMX_UNIVERSES {
                        if components::Button::new(
                            &format!("U{universe}"),
                            ButtonSize::Medium.with_width(tab_width),
                        )
                        .ui(ui, simulator.universe == universe)
                        {
                            simulator.universe = universe;
                        }
                    }
                });
                ui.separator();

                let universe = simulator.universe.min(NUM_DMX_UNIVERSES - 1);
                self.client.with_dmx_universe(universe, |dmx| {
                    simulator.simulate_dmx(ui, groups, dmx, universe)
                });
            });
    }

    /// Setup › Fixtures' picker: the group tree (one group at a time) and the
    /// fixtures of the selected group. Returns `(group, fixture, changed)`.
    pub fn group_selection(
        &mut self,
        groups: &EngineGroups,
        selected_group: Option<u8>,
        selected_fixture: u8,
        ui: &mut egui::Ui,
    ) -> (u8, u8, bool) {
        let dmx_engine = self.data.state.dmx_engine.read().unwrap();

        let mut group_result = selected_group.unwrap_or(0);
        let mut fixture_result = selected_fixture;
        let mut changed = false;

        ui.set_min_height(ui.available_height());

        ui.allocate_ui_with_layout(
            egui::vec2(ui.available_width(), ui.available_height()), // fixed width, max height
            egui::Layout::left_to_right(egui::Align::Min),
            |ui| {
                ui.set_min_height(ui.available_height());
                ui.allocate_ui_with_layout(
                    egui::vec2(NARROW_TREE_WIDTH, ui.available_height()),
                    egui::Layout::top_down(egui::Align::Min),
                    |ui| {
                        ui.set_width(NARROW_TREE_WIDTH);
                        ui.label("Groups:");
                        ui.add_space(8.0);

                        // Same folders and collapse state as Setup › Groups.
                        let items = components::group_tree_items(&dmx_engine.0);
                        let action = egui::ScrollArea::vertical()
                            .id_salt("sgroups")
                            .show(ui, |ui| {
                                ui.set_height(400.0);
                                components::narrow_tree(SETUP_GROUP_TREE, &items)
                                    .selected(selected_group.map(TreeKey::Leaf))
                                    .show(ui)
                            })
                            .inner;

                        match action {
                            Some(TreeAction::SelectLeaf(group_id))
                                if selected_group != Some(group_id) =>
                            {
                                group_result = group_id;
                                changed = true;
                            }
                            // A picker has no use for folder selection: a tap
                            // opens or closes the folder, like its chevron.
                            Some(TreeAction::TapFolder(folder)) => {
                                components::toggle_folder(ui.ctx(), SETUP_GROUP_TREE, folder);
                            }
                            _ => {}
                        }
                    },
                );

                ui.vertical(|ui| {
                    ui.label("Fixtures");
                    ui.add_space(8.0);

                    if dmx_engine.groups().is_empty() {
                        return;
                    }

                    let Some(group_id) = self.add_fixture_group else {
                        return;
                    };

                    egui::ScrollArea::vertical()
                        .id_salt("sfixtures")
                        .show(ui, |ui| {
                            ui.set_height(400.0);
                            let Some(group) = dmx_engine.groups().get(&group_id) else {
                                return;
                            };
                            for (fix_id, _fixture) in &group.fixtures {
                                let Some(fixture) = groups
                                    .get(&group_id)
                                    .and_then(|group| group.fixtures.get(fix_id))
                                else {
                                    continue;
                                };

                                let mut name = fixture.name.clone();
                                name.truncate(10);

                                let button_clicked = components::clickable(
                                    ui,
                                    true,
                                    if self.setup_fixture_id == *fix_id {
                                        Selection::Limited.color()
                                    } else {
                                        Selection::Off.color()
                                    },
                                    ButtonSize::Large.with_height(50.0),
                                    |ui, rect, fg| {
                                        let painter = ui.painter();

                                        painter.text(
                                            rect.left_center() + egui::vec2(12.0, 0.0),
                                            egui::Align2::LEFT_CENTER,
                                            name,
                                            egui::FontId::proportional(14.0),
                                            fg,
                                        );

                                        painter.text(
                                            rect.left_center() + egui::vec2(12.0, 12.0),
                                            egui::Align2::LEFT_CENTER,
                                            format!(
                                                "#{} | {}",
                                                fix_id,
                                                fixture.type_.model_string()
                                            ),
                                            egui::FontId::proportional(7.0),
                                            fg,
                                        );
                                    },
                                );

                                if button_clicked {
                                    fixture_result = *fix_id;
                                    changed = true;
                                }

                                ui.add_space(2.0);
                            }
                        });
                });
            },
        );

        (group_result, fixture_result, changed)
    }

    /// The Programmer's group tree, the selected groups' fixtures and the
    /// attribute faders, drawn from `dmx_engine` (the page's snapshot as this
    /// client's session sees it, see `EngineState::clone_for_session`).
    pub fn fixture_selection(&mut self, dmx_engine: &EngineState, ui: &mut egui::Ui) {
        let groups = dmx_engine.groups();
        let selection = dmx_engine.selection();

        let highlight_fixtures = &selection.fixtures_in_group;

        let mut total_fixtures = vec![];
        for g_id in selection.group_ids.iter() {
            let Some(group) = groups.get(g_id) else {
                tracing::warn!("Ignoring stale fixture selection: {selection:?}");
                continue;
            };
            if selection.fixtures_in_group.is_empty() {
                total_fixtures.extend(
                    group
                        .fixtures
                        .keys()
                        .map(|f_id| (*g_id, *f_id, Selection::Cascading)),
                );
            } else {
                total_fixtures.extend(group.fixtures.keys().map(|f_id| {
                    let is_limited = selection.fixtures_in_group.contains(f_id);
                    let selection = match is_limited {
                        true => Selection::Limited,
                        false => Selection::Off,
                    };

                    (*g_id, *f_id, selection)
                }));
            }
        }

        ui.allocate_ui_with_layout(
            egui::vec2(ui.available_width(), ui.available_height()), // fixed width, max height
            egui::Layout::left_to_right(egui::Align::Min),
            |ui| {
                ui.set_min_height(ui.available_height());
                ui.allocate_ui_with_layout(
                    egui::vec2(NARROW_TREE_WIDTH, ui.available_height()),
                    egui::Layout::top_down(egui::Align::Min),
                    |ui| {
                        ui.set_width(NARROW_TREE_WIDTH);
                        ui.label("Groups:");
                        ui.add_space(8.0);
                        ui.set_min_height(ui.available_height());

                        let items = components::group_tree_items(&dmx_engine.0);
                        let highlighted = selection
                            .group_ids
                            .iter()
                            .map(|id| TreeKey::Leaf(*id))
                            .chain(
                                components::fully_selected_group_folders(&dmx_engine.0)
                                    .into_iter()
                                    .map(TreeKey::Folder),
                            );

                        let action = egui::ScrollArea::vertical()
                            .id_salt("sgroups-p")
                            .show(ui, |ui| {
                                ui.set_height(400.0);
                                ui.add_space(8.0);
                                components::narrow_tree(PROGRAMMER_GROUP_TREE, &items)
                                    .selected_keys(highlighted)
                                    .show(ui)
                            })
                            .inner;

                        let msg = match action {
                            // Toggle group selection
                            Some(TreeAction::SelectLeaf(group_id)) => {
                                Some(if selection.group_ids.contains(&group_id) {
                                    ControlEvent::DeSelectGroup(group_id)
                                } else {
                                    ControlEvent::SelectGroup(group_id)
                                })
                            }
                            // Select every group under the folder (or deselect
                            // them when all are selected).
                            Some(TreeAction::TapFolder(folder)) => {
                                components::folder_tap_event(&dmx_engine.0, folder)
                            }
                            Some(TreeAction::MoveRequested(_)) | None => None,
                        };
                        if let Some(msg) = msg {
                            self.client
                                .send(ControlEventMessage::new(EventOriginator::Web, msg));
                        }
                    },
                );

                // Right: fixtures in selected group
                // Get selection info
                // Layout: left (fixtures), right (controls)

                // ui.horizontal(|ui| {
                // Fixtures list
                if !selection.group_ids.is_empty() {
                    ui.vertical(|ui| {
                        egui::ScrollArea::vertical()
                            .id_salt("sfix-p")
                            .show(ui, |ui| {
                                ui.label("Fixtures");
                                ui.add_space(8.0);

                                for (group_id, fix_id, fixture_selection) in &total_fixtures {
                                    let Some(fixture) = groups
                                        .get(group_id)
                                        .and_then(|group| group.fixtures.get(fix_id))
                                    else {
                                        continue;
                                    };

                                    let button_clicked = components::clickable(
                                        ui,
                                        true, //*fixture_selection == Selection::Limited,
                                        fixture_selection.color(),
                                        ButtonSize::Large.with_height(50.0),
                                        |ui, rect, fg| {
                                            let painter = ui.painter();

                                            // painter.rect_filled(rect, 0.0, bg);

                                            painter.text(
                                                rect.left_center() + egui::vec2(12.0, 0.0),
                                                egui::Align2::LEFT_CENTER,
                                                &fixture.name,
                                                egui::FontId::proportional(14.0),
                                                fg,
                                            );

                                            painter.text(
                                                rect.left_center() + egui::vec2(12.0, 12.0),
                                                egui::Align2::LEFT_CENTER,
                                                fixture.type_.kind_string(),
                                                egui::FontId::proportional(9.0),
                                                fg,
                                            );
                                        },
                                    );

                                    if button_clicked
                                        && selection.group_ids.len() == 1
                                        && total_fixtures.len() != 1
                                    {
                                        let is_fix_selected = highlight_fixtures.contains(fix_id);

                                        let msg = if is_fix_selected {
                                            ControlEvent::UnLimitSelectionToFixtureInCurrentGroup(
                                                *fix_id,
                                            )
                                        } else {
                                            ControlEvent::LimitSelectionToFixtureInCurrentGroup(
                                                *fix_id,
                                            )
                                        };

                                        self.client.send(ControlEventMessage::new(
                                            EventOriginator::Web,
                                            msg,
                                        ));
                                    }

                                    ui.add_space(2.0);
                                }
                            });
                    });
                } else {
                    let _ = ui.allocate_exact_size(ButtonSize::Large.dim().0, egui::Sense::empty());
                }
            },
        );

        ui.separator();

        {
            let selected_fixture = if selection.fixtures_in_group.len() == 1 {
                total_fixtures
                    .iter()
                    .filter(|(_, _, select)| *select == Selection::Limited)
                    .next()
                    .cloned()
            } else if total_fixtures.len() == 1 {
                total_fixtures.first().cloned()
            } else {
                None
            };

            let buf = match selected_fixture {
                Some((g_id, f_id, _)) => {
                    let fixture = dmx_engine
                        .curr_scene()
                        .sink
                        .fixture_states
                        .get(&(g_id, f_id))
                        .unwrap();
                    fixture.clone()
                }
                None => dmx_engine.0.control_buffer.clone(),
            };

            let animations: Vec<u8> = dmx_engine.0.animation_templates.keys().copied().collect();

            let mut frozen_props: HashSet<FixtureProperty> = HashSet::new();
            let curr_scene = dmx_engine.curr_scene();
            for (gid, fid) in &dmx_engine.get_selection().fixtures {
                let Some(fixture_state) = curr_scene.sink.fixture_states.get(&(*gid, *fid)) else {
                    continue;
                };
                for prop in <FixtureProperty as strum::IntoEnumIterator>::iter() {
                    if fixture_state.slot(prop).is_frozen() {
                        frozen_props.insert(prop);
                    }
                }
            }

            Self::fixture_controls(
                ui,
                &buf,
                &dmx_engine.0.palettes,
                &frozen_props,
                &self.client,
                animations.as_slice(),
            );
        }
    }

    fn fixture_controls(
        ui: &mut egui::Ui,
        buf: &FixtureState,
        palettes: &BTreeMap<u8, Palette>,
        frozen_props: &HashSet<FixtureProperty>,
        client: &LocalClient,
        _animations: &[u8],
    ) {
        let resolved = buf.resolve(palettes);

        Frame::new()
            .fill(ui.visuals().widgets.inactive.weak_bg_fill)
            .inner_margin(Margin::symmetric(12, 6))
            .show(ui, |ui| {
                ui.set_max_width(200.0);
                ui.vertical(|ui| {
                    let row =
                        |ui: &mut egui::Ui,
                         prop: FixtureProperty,
                         label: &str,
                         range: std::ops::RangeInclusive<f32>,
                         resolved_val: f32,
                         build_event: &dyn Fn(f32) -> ControlEvent| {
                            let frozen = frozen_props.contains(&prop);
                            let mut value = resolved_val;
                            ui.add_enabled_ui(!frozen, |ui| {
                                if ui
                                    .add(HFader::new(&mut value, range).with_label(label))
                                    .changed()
                                {
                                    client.send(ControlEventMessage::new(
                                        EventOriginator::Web,
                                        build_event(value),
                                    ));
                                }
                            });
                        };

                    row(
                        ui,
                        FixtureProperty::Alpha,
                        "Alpha",
                        0.0..=255.0,
                        resolved.alpha as f32,
                        &|v| ControlEvent::SetAlpha(v as u8),
                    );

                    ui.add_space(3.0);
                    ui.separator();
                    ui.add_space(3.0);

                    row(
                        ui,
                        FixtureProperty::Strobe,
                        "Strobe",
                        0.0..=255.0,
                        resolved.strobe_speed as f32,
                        &|v| ControlEvent::SetStrobeSpeed(v as u8),
                    );

                    ui.add_space(3.0);
                    ui.separator();
                    ui.add_space(3.0);

                    row(
                        ui,
                        FixtureProperty::Focus,
                        "Focus",
                        0.0..=255.0,
                        resolved.focus as f32,
                        &|v| ControlEvent::SetFocus(v as u8),
                    );

                    ui.add_space(3.0);
                    ui.separator();
                    ui.add_space(3.0);

                    row(
                        ui,
                        FixtureProperty::Tilt,
                        "Tilt",
                        0.0..=255.0,
                        resolved.orientation.tilt as f32,
                        &|v| ControlEvent::SetTilt(v as u8),
                    );

                    ui.add_space(3.0);
                    ui.separator();
                    ui.add_space(3.0);

                    row(
                        ui,
                        FixtureProperty::Pan,
                        "Pan",
                        0.0..=255.0,
                        resolved.orientation.pan as f32,
                        &|v| ControlEvent::SetPan(v as u8),
                    );

                    ui.add_space(3.0);
                    ui.separator();
                    ui.add_space(3.0);

                    row(
                        ui,
                        FixtureProperty::ColorHue,
                        "Hue",
                        0.0..=360.0,
                        resolved.color.h as f32,
                        &|v| ControlEvent::SetColorHue(v as u16),
                    );

                    ui.add_space(3.0);
                    ui.separator();
                    ui.add_space(3.0);

                    row(
                        ui,
                        FixtureProperty::ColorSaturation,
                        "Saturation",
                        0.0..=255.0,
                        resolved.color.s.map_range(0.0..1.0, 0.0..255.0) as f32,
                        &|v| ControlEvent::SetColorSaturation(v as u8),
                    );

                    ui.add_space(3.0);
                    ui.separator();
                    ui.add_space(3.0);

                    row(
                        ui,
                        FixtureProperty::ColorValue,
                        "Value",
                        0.0..=255.0,
                        resolved.color.v.map_range(0.0..1.0, 0.0..255.0) as f32,
                        &|v| ControlEvent::SetColorValue(v as u8),
                    );

                    ui.add_space(3.0);
                    ui.separator();
                    ui.add_space(3.0);

                    // Color picker
                    let b_color: RGBColor = resolved.color.into();
                    let mut color = [
                        b_color.r as f32 / 255.0,
                        b_color.g as f32 / 255.0,
                        b_color.b as f32 / 255.0,
                    ];
                    let color_frozen = frozen_props.contains(&FixtureProperty::ColorHue)
                        || frozen_props.contains(&FixtureProperty::ColorSaturation)
                        || frozen_props.contains(&FixtureProperty::ColorValue);
                    ui.add_enabled_ui(!color_frozen, |ui| {
                        if ui.color_edit_button_rgb(&mut color).changed() {
                            let r = (color[0] * 255.0) as u8;
                            let g = (color[1] * 255.0) as u8;
                            let b = (color[2] * 255.0) as u8;

                            let tup = (r, g, b);
                            if RGBColor::from(tup) != b_color {
                                tracing::debug!(
                                    "RGBColor::from(tup) != b_color ({:?} != {:?})",
                                    RGBColor::from(tup),
                                    b_color
                                );
                                client.send(ControlEventMessage::new(
                                    EventOriginator::Web,
                                    ControlEvent::SetColor(tup),
                                ));
                            }
                        }
                    });
                });
            });
    }

    /// The Programmer's scene column: a narrow scene tree (folders and user
    /// scenes; BLANK is never user-selectable). Tapping a scene focuses it;
    /// tapping a folder opens or closes it, since this picker focuses exactly
    /// one scene. Folders are managed in Setup › Scenes. `compact` (narrow
    /// dynamic panes) puts the tree in a full-width strip on top.
    pub fn scene_overview(&mut self, ui: &mut egui::Ui, dmx_engine: &EngineState, compact: bool) {
        let state = &dmx_engine.0;
        let items = components::scene_tree_items(state, |id| id != BLANK_SCENE_ID);
        let focus = state.current_scene_focus;

        let show_tree = |ui: &mut egui::Ui| {
            ui.label(format!("Scenes: {}", state.user_scenes().count()));
            ui.add_space(4.0);
            egui::ScrollArea::vertical()
                .id_salt("programmer-scenes")
                .show(ui, |ui| {
                    components::narrow_tree(PROGRAMMER_SCENE_TREE, &items)
                        .toggle_width(18.0)
                        .min_body_width(70.0)
                        .selected(Some(TreeKey::Leaf(focus)))
                        .show(ui)
                })
                .inner
        };

        let action = if compact {
            ui.allocate_ui_with_layout(
                egui::vec2(ui.available_width(), 150.0),
                egui::Layout::top_down(egui::Align::Min),
                show_tree,
            )
            .inner
        } else {
            ui.allocate_ui_with_layout(
                egui::vec2(SCENE_COLUMN_WIDTH, ui.available_height()), // fixed width, max height
                egui::Layout::top_down(egui::Align::Min),
                |ui| {
                    ui.set_width(SCENE_COLUMN_WIDTH);
                    show_tree(ui)
                },
            )
            .inner
        };

        match action {
            Some(TreeAction::SelectLeaf(scene_id)) if scene_id != focus => {
                self.client.send(ControlEventMessage::new(
                    EventOriginator::Web,
                    ControlEvent::SetSceneFocus(scene_id),
                ));
            }
            Some(TreeAction::TapFolder(folder)) => {
                components::toggle_folder(ui.ctx(), PROGRAMMER_SCENE_TREE, folder);
            }
            _ => {}
        }
    }
}
