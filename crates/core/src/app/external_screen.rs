#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")] // hide console window on Windows in release

use crate::{
    app::{BlaulichtApp, ExternalScreen},
    config::{ShowfileDockNode, ShowfileDockSplitAxis, ShowfileExternalScreen, ShowfileUiState},
    state::ScreenId,
};
use blaulicht_shared::AppPage;
use egui::{pos2, vec2, Color32, Context, Frame, Id, Margin, Sense, Stroke, Vec2, WidgetText};
use egui_dock::tab_viewer::OnCloseResponse;
use egui_dock::{DockArea, DockState, Node, NodeIndex, Split, Style, Tree};
use strum::IntoEnumIterator;

#[derive(Clone)]
pub(crate) struct Pane {
    page: AppPage,
}

impl Pane {
    fn new(page: AppPage) -> Self {
        Self { page }
    }

    fn label(&self) -> &str {
        match self.page {
            AppPage::Logs => "Logs",
            AppPage::System => "System",
            AppPage::Audio => "Audio",
            AppPage::FixturesSetup => "Fixtures Setup",
            AppPage::View => "View",
            AppPage::ViewPerformance => "View Performance",
            AppPage::FixturesPerformance => "Fixtures Performance",
            AppPage::Animations => "Animations",
            AppPage::Visualizer => "Visualizer",
        }
    }

    fn title_text(&self) -> WidgetText {
        self.label().into()
    }
}

// pub fn create(dimensions: Vec2) -> ExternalScreen {
//     ExternalScreen::new(dimensions)
// }

impl Default for ExternalScreen {
    fn default() -> Self {
        Self::new(egui::vec2(1920.0, 1080.0))
    }
}

impl ExternalScreen {
    pub(crate) fn new(dimensions: Vec2) -> Self {
        // let tabs = (1..=8).map(|idx| Pane::new(format!("Tab {idx}"))).collect();

        let tabs = AppPage::iter().map(|page| Pane::new(page)).collect();

        Self {
            dock_state: DockState::new(tabs),
            dimensions,
            position: None,
        }
    }

    fn ensure_core_tabs(&mut self) {
        let present_pages = {
            let mut pages = Vec::new();
            for surface in self.dock_state.iter_surfaces() {
                for (_, pane) in surface.iter_all_tabs() {
                    if !pages.contains(&pane.page) {
                        pages.push(pane.page);
                    }
                }
            }
            pages
        };

        for page in AppPage::iter() {
            if !present_pages.contains(&page) {
                self.dock_state.push_to_first_leaf(Pane::new(page));
            }
        }
    }

    pub(crate) fn to_showfile(&self) -> ShowfileExternalScreen {
        ShowfileExternalScreen {
            width: self.dimensions.x,
            height: self.dimensions.y,
            x: self.position.map(|position| position.x),
            y: self.position.map(|position| position.y),
            layout: saved_dock_node(self.dock_state.main_surface(), NodeIndex::root())
                .unwrap_or_else(default_saved_dock_node),
        }
    }

    pub(crate) fn from_showfile(saved: ShowfileExternalScreen) -> Self {
        let mut dock_state = DockState::new(seed_tabs_for_saved_node(&saved.layout));
        apply_saved_dock_node(
            dock_state.main_surface_mut(),
            NodeIndex::root(),
            &saved.layout,
        );

        let mut screen = Self {
            dock_state,
            dimensions: vec2(saved.width.max(1.0), saved.height.max(1.0)),
            position: saved.x.zip(saved.y).map(|(x, y)| pos2(x, y)),
        };
        screen.ensure_core_tabs();
        screen
    }

    fn update_viewport_geometry(&mut self, ctx: &Context) {
        let inner_rect = ctx.input(|input| input.viewport().inner_rect);
        let outer_rect = ctx.input(|input| input.viewport().outer_rect);

        if let Some(rect) = inner_rect {
            self.dimensions = rect.size();
        } else {
            let size = ctx.viewport_rect().size();
            if size.x > 0.0 && size.y > 0.0 {
                self.dimensions = size;
            }
        }

        self.position = outer_rect.or(inner_rect).map(|rect| rect.min);
    }
}

fn default_saved_dock_node() -> ShowfileDockNode {
    ShowfileDockNode::Leaf {
        tabs: AppPage::iter().collect(),
        active: 0,
    }
}

fn seed_tabs_for_saved_node(node: &ShowfileDockNode) -> Vec<Pane> {
    let tabs = match node {
        ShowfileDockNode::Leaf { tabs, .. } => tabs.clone(),
        ShowfileDockNode::Split { first, .. } => match first.as_ref() {
            ShowfileDockNode::Leaf { tabs, .. } => tabs.clone(),
            _ => {
                return seed_tabs_for_saved_node(first);
            }
        },
    };

    if tabs.is_empty() {
        vec![Pane::new(AppPage::Logs)]
    } else {
        tabs.into_iter().map(Pane::new).collect()
    }
}

fn saved_dock_node(tree: &Tree<Pane>, index: NodeIndex) -> Option<ShowfileDockNode> {
    if index.0 >= tree.len() {
        return None;
    }

    match &tree[index] {
        Node::Leaf(leaf) => Some(ShowfileDockNode::Leaf {
            tabs: leaf.tabs.iter().map(|pane| pane.page).collect(),
            active: leaf.active.0,
        }),
        Node::Horizontal(split) => Some(ShowfileDockNode::Split {
            axis: ShowfileDockSplitAxis::Horizontal,
            fraction: split.fraction,
            first: Box::new(saved_dock_node(tree, index.left())?),
            second: Box::new(saved_dock_node(tree, index.right())?),
        }),
        Node::Vertical(split) => Some(ShowfileDockNode::Split {
            axis: ShowfileDockSplitAxis::Vertical,
            fraction: split.fraction,
            first: Box::new(saved_dock_node(tree, index.left())?),
            second: Box::new(saved_dock_node(tree, index.right())?),
        }),
        Node::Empty => None,
    }
}

fn apply_saved_dock_node(tree: &mut Tree<Pane>, index: NodeIndex, node: &ShowfileDockNode) {
    match node {
        ShowfileDockNode::Leaf { tabs, active } => {
            if let Ok(leaf) = tree.leaf_mut(index) {
                if !tabs.is_empty() {
                    leaf.tabs = tabs.iter().copied().map(Pane::new).collect();
                }

                if !leaf.tabs.is_empty() {
                    let active = (*active).min(leaf.tabs.len() - 1);
                    let _ = leaf.set_active_tab(active);
                }
            }
        }
        ShowfileDockNode::Split {
            axis,
            fraction,
            first,
            second,
        } => {
            apply_saved_dock_node(tree, index, first);

            let split = match axis {
                ShowfileDockSplitAxis::Horizontal => Split::Right,
                ShowfileDockSplitAxis::Vertical => Split::Below,
            };
            let fraction = fraction.clamp(0.05, 0.95);
            let [_, second_index] =
                tree.split_tabs(index, split, fraction, seed_tabs_for_saved_node(second));

            apply_saved_dock_node(tree, second_index, second);
        }
    }
}

struct TabViewer<'bl, 'ct> {
    app: &'bl mut BlaulichtApp,
    ctx: &'ct Context,
    screen_id: ScreenId,
}

impl<'bl, 'ct> egui_dock::TabViewer for TabViewer<'bl, 'ct> {
    type Tab = Pane;

    fn title(&mut self, tab: &mut Self::Tab) -> WidgetText {
        tab.title_text()
    }

    fn ui(&mut self, ui: &mut egui::Ui, tab: &mut Self::Tab) {
        Frame::NONE
            .outer_margin(Margin::same(0))
            .inner_margin(Margin::same(8))
            .stroke(Stroke::new(2.0, Color32::GRAY))
            .show(ui, |ui| {
                let (rect, _response) = ui.allocate_exact_size(vec2(723.0, 480.0), Sense::empty());
                let mut child_ui = ui.new_child(
                    egui::UiBuilder::new()
                        .max_rect(rect)
                        .layout(egui::Layout::top_down(egui::Align::Min)),
                );

                child_ui.set_clip_rect(rect);

                self.app.page_content_based_on_tab(
                    tab.page,
                    &mut child_ui,
                    self.ctx,
                    self.screen_id,
                );
            });
    }

    fn on_close(&mut self, tab: &mut Self::Tab) -> OnCloseResponse {
        tracing::info!("Closed tab: {}", tab.label());
        OnCloseResponse::Close
    }
}

impl BlaulichtApp {
    pub(crate) fn showfile_ui_state(&self) -> ShowfileUiState {
        ShowfileUiState {
            main_screen_desktop_mode: Some(self.main_screen_desktop_mode.to_showfile()),
            external_screens: self
                .external_screens
                .iter()
                .map(ExternalScreen::to_showfile)
                .collect(),
        }
    }

    pub(crate) fn apply_showfile_ui_state(&mut self, ui_state: ShowfileUiState) {
        if let Some(main_screen) = ui_state.main_screen_desktop_mode {
            self.main_screen_desktop_mode = ExternalScreen::from_showfile(main_screen);
        }

        self.external_screens = ui_state
            .external_screens
            .into_iter()
            .map(ExternalScreen::from_showfile)
            .collect();
    }

    pub fn drive_external_screen(&mut self, ctx: &Context, screen_idx: usize) {
        let viewport_id = egui::ViewportId::from_hash_of(format!("bl_ext_{screen_idx}"));

        let screen = self.external_screens[screen_idx].clone();

        let mut viewport = egui::ViewportBuilder::default()
            .with_title(format!("bl_ext_{screen_idx}"))
            .with_inner_size([screen.dimensions.x, screen.dimensions.y])
            .with_resizable(true);

        if let Some(position) = screen.position {
            viewport = viewport.with_position(position);
        }

        ctx.show_viewport_immediate(viewport_id, viewport, |ctx, _class| {
            let mut screen = self.external_screens[screen_idx].clone();
            let screen_id = ScreenId::external(screen_idx);

            self.draw_external_screen_contents(ctx, screen_id, &mut screen);

            // TODO: This is peak bullshit code.
            self.external_screens[screen_idx] = screen;
        });
    }

    pub fn draw_external_screen_contents(
        &mut self,
        ctx: &Context,
        screen_id: ScreenId,
        screen: &mut ExternalScreen,
    ) {
        screen.update_viewport_geometry(ctx);
        self.render_plugin_ui(ctx, screen_id);

        let mut tab_viewer = TabViewer {
            app: self,
            ctx,
            screen_id,
        };

        DockArea::new(screen.dock_state())
            .id(Id::new(("external_screen_dock", screen_id)))
            .style(Style::from_egui(ctx.style().as_ref()))
            .show_close_buttons(false)
            .show_leaf_close_all_buttons(false)
            .show_leaf_collapse_buttons(false)
            .show(ctx, &mut tab_viewer);

        screen.ensure_core_tabs();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn external_screen_showfile_round_trip_keeps_split_layout() {
        let mut screen = ExternalScreen::new(vec2(1024.0, 768.0));
        screen.position = Some(pos2(120.0, 80.0));
        screen.dock_state.main_surface_mut().split_right(
            NodeIndex::root(),
            0.35,
            vec![Pane::new(AppPage::Visualizer)],
        );

        let saved = screen.to_showfile();
        let restored = ExternalScreen::from_showfile(saved);
        let saved_again = restored.to_showfile();

        assert_eq!(saved_again.width, 1024.0);
        assert_eq!(saved_again.height, 768.0);
        assert_eq!(saved_again.x, Some(120.0));
        assert_eq!(saved_again.y, Some(80.0));

        match saved_again.layout {
            ShowfileDockNode::Split { axis, fraction, .. } => {
                assert!(matches!(axis, ShowfileDockSplitAxis::Horizontal));
                assert!((fraction - 0.35).abs() < f32::EPSILON);
            }
            ShowfileDockNode::Leaf { .. } => panic!("expected split layout"),
        }
    }
}
