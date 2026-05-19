#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")] // hide console window on Windows in release

use crate::{
    app::{BlaulichtApp, ExternalScreen},
    config::{
        ShowfileDockNode, ShowfileDockSplitAxis, ShowfileDockTab, ShowfileExternalScreen,
        ShowfileUiState,
    },
    state::{PluginOpenState, ScreenId},
};
use blaulicht_shared::{AppPage, ExternalScreenInfo};
use egui::{pos2, vec2, Color32, Context, Frame, Id, Margin, Sense, Stroke, Vec2, WidgetText};
use egui_dock::tab_viewer::OnCloseResponse;
use egui_dock::{DockArea, DockState, Node, NodeIndex, Split, Style, Tree};
use std::collections::HashSet;
use strum::IntoEnumIterator;

#[derive(Clone)]
pub(crate) struct Pane {
    kind: PaneKind,
}

#[derive(Clone, Copy)]
enum PaneKind {
    Page(AppPage),
    PluginUi { plugin_id: u8 },
}

impl Pane {
    fn new(page: AppPage) -> Self {
        Self {
            kind: PaneKind::Page(page),
        }
    }

    fn plugin_ui(plugin_id: u8) -> Self {
        Self {
            kind: PaneKind::PluginUi { plugin_id },
        }
    }

    fn label(&self) -> String {
        match self.kind {
            PaneKind::Page(AppPage::Logs) => "Logs".to_string(),
            PaneKind::Page(AppPage::System) => "System".to_string(),
            PaneKind::Page(AppPage::Audio) => "Audio".to_string(),
            PaneKind::Page(AppPage::FixturesSetup) => "Fixtures Setup".to_string(),
            PaneKind::Page(AppPage::View) => "View".to_string(),
            PaneKind::Page(AppPage::ViewPerformance) => "View Performance".to_string(),
            PaneKind::Page(AppPage::FixturesPerformance) => "Fixtures Performance".to_string(),
            PaneKind::Page(AppPage::Animations) => "Animations".to_string(),
            PaneKind::Page(AppPage::Visualizer) => "Visualizer".to_string(),
            PaneKind::PluginUi { plugin_id } => format!("Plugin UI #{plugin_id}"),
        }
    }

    fn title_text(&self) -> WidgetText {
        self.label().into()
    }

    fn to_showfile_tab(&self) -> ShowfileDockTab {
        match self.kind {
            PaneKind::Page(page) => ShowfileDockTab::Page(page),
            PaneKind::PluginUi { plugin_id } => ShowfileDockTab::PluginUi { plugin_id },
        }
    }
}

impl From<ShowfileDockTab> for Pane {
    fn from(tab: ShowfileDockTab) -> Self {
        match tab {
            ShowfileDockTab::Page(page) => Pane::new(page),
            ShowfileDockTab::PluginUi { plugin_id } => Pane::plugin_ui(plugin_id),
        }
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
        Self::new_owned(dimensions, None)
    }

    pub(crate) fn new_owned(dimensions: Vec2, owner_plugin_id: Option<u8>) -> Self {
        // let tabs = (1..=8).map(|idx| Pane::new(format!("Tab {idx}"))).collect();

        let tabs = AppPage::iter().map(|page| Pane::new(page)).collect();

        Self {
            dock_state: DockState::new(tabs),
            dimensions,
            position: None,
            owner_plugin_id,
        }
    }

    fn ensure_tabs(&mut self, plugin_ids: &[u8]) {
        let present_pages = {
            let mut pages = Vec::new();
            for surface in self.dock_state.iter_surfaces() {
                for (_, pane) in surface.iter_all_tabs() {
                    if let PaneKind::Page(page) = pane.kind {
                        if !pages.contains(&page) {
                            pages.push(page);
                        }
                    }
                }
            }
            pages
        };

        let present_plugin_ids = {
            let mut ids = Vec::new();
            for surface in self.dock_state.iter_surfaces() {
                for (_, pane) in surface.iter_all_tabs() {
                    if let PaneKind::PluginUi { plugin_id } = pane.kind {
                        if !ids.contains(&plugin_id) {
                            ids.push(plugin_id);
                        }
                    }
                }
            }
            ids
        };

        for page in AppPage::iter() {
            if !present_pages.contains(&page) {
                self.dock_state.push_to_first_leaf(Pane::new(page));
            }
        }

        for plugin_id in plugin_ids {
            if !present_plugin_ids.contains(plugin_id) {
                self.dock_state
                    .push_to_first_leaf(Pane::plugin_ui(*plugin_id));
            }
        }
    }

    fn ensure_core_tabs(&mut self) {
        self.ensure_tabs(&[]);
    }

    pub(crate) fn to_showfile(&self) -> ShowfileExternalScreen {
        ShowfileExternalScreen {
            width: self.dimensions.x,
            height: self.dimensions.y,
            x: self.position.map(|position| position.x),
            y: self.position.map(|position| position.y),
            owner_plugin_id: self.owner_plugin_id,
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
            owner_plugin_id: saved.owner_plugin_id,
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
        tabs: AppPage::iter().map(ShowfileDockTab::Page).collect(),
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
        tabs.into_iter().map(Pane::from).collect()
    }
}

fn saved_dock_node(tree: &Tree<Pane>, index: NodeIndex) -> Option<ShowfileDockNode> {
    if index.0 >= tree.len() {
        return None;
    }

    match &tree[index] {
        Node::Leaf(leaf) => Some(ShowfileDockNode::Leaf {
            tabs: leaf.tabs.iter().map(Pane::to_showfile_tab).collect(),
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
                    leaf.tabs = tabs.iter().copied().map(Pane::from).collect();
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
    visible_plugin_tabs: &'bl mut Vec<u8>,
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

                match tab.kind {
                    PaneKind::Page(page) => {
                        self.app.page_content_based_on_tab(
                            page,
                            &mut child_ui,
                            self.ctx,
                            self.screen_id,
                        );
                    }
                    PaneKind::PluginUi { plugin_id } => {
                        self.visible_plugin_tabs.push(plugin_id);
                        self.app.render_plugin_ui_contents(&mut child_ui, plugin_id);
                    }
                }
            });
    }

    fn on_close(&mut self, tab: &mut Self::Tab) -> OnCloseResponse {
        tracing::info!("Closed tab: {}", tab.label());
        OnCloseResponse::Close
    }
}

impl BlaulichtApp {
    fn external_screen_infos(&self) -> Vec<ExternalScreenInfo> {
        self.external_screens
            .iter()
            .enumerate()
            .map(|(index, screen)| ExternalScreenInfo {
                index: index as u32,
                width: screen.dimensions.x,
                height: screen.dimensions.y,
                x: screen.position.map(|position| position.x),
                y: screen.position.map(|position| position.y),
                owner_plugin_id: screen.owner_plugin_id,
            })
            .collect()
    }

    pub(crate) fn sync_external_screen_infos(&self) {
        let mut external_screens = self.data.state.external_screens.write().unwrap();
        *external_screens = self.external_screen_infos();
    }

    fn reset_external_screen_plugin_ui_tracking(&mut self) {
        self.plugin_ui_visible_tabs.clear();

        let popped_out = self.data.state.plugin_ui_popped_out.read().unwrap().clone();
        let mut visibility = self.data.state.plugin_ui_visibility.write().unwrap();
        for (plugin_id, entry) in visibility.iter_mut() {
            if entry.screen_id == ScreenId::MAIN {
                continue;
            }

            entry.screen_id = ScreenId::MAIN;
            if !popped_out.get(plugin_id).copied().unwrap_or(false) {
                entry.open = false;
            }
        }
    }

    pub(crate) fn add_external_screen_with_dimensions(&mut self, dimensions: Vec2) {
        self.add_external_screen_with_dimensions_owned(dimensions, None);
    }

    pub(crate) fn add_external_screen_with_dimensions_owned(
        &mut self,
        dimensions: Vec2,
        owner_plugin_id: Option<u8>,
    ) {
        self.external_screens
            .push(ExternalScreen::new_owned(dimensions, owner_plugin_id));
        self.sync_external_screen_infos();
    }

    pub(crate) fn add_external_screen(&mut self) {
        self.add_external_screen_with_dimensions(egui::vec2(1920.0, 1080.0));
    }

    pub(crate) fn remove_external_screen(&mut self, index: usize) -> bool {
        if index >= self.external_screens.len() {
            return false;
        }

        self.external_screens.remove(index);
        self.reset_external_screen_plugin_ui_tracking();
        self.sync_external_screen_infos();
        true
    }

    fn external_screen_indices_for_owner(&self, owner_plugin_id: u8) -> Vec<usize> {
        self.external_screens
            .iter()
            .enumerate()
            .filter_map(|(index, screen)| {
                (screen.owner_plugin_id == Some(owner_plugin_id)).then_some(index)
            })
            .collect()
    }

    pub(crate) fn upsert_external_screen_for_owner(
        &mut self,
        owner_plugin_id: u8,
        dimensions: Vec2,
    ) -> bool {
        let owned_indices = self.external_screen_indices_for_owner(owner_plugin_id);
        let had_existing = !owned_indices.is_empty();

        if let Some(&primary_index) = owned_indices.first() {
            if let Some(screen) = self.external_screens.get_mut(primary_index) {
                screen.dimensions = dimensions;
                screen.owner_plugin_id = Some(owner_plugin_id);
            }
        } else {
            self.external_screens
                .push(ExternalScreen::new_owned(dimensions, Some(owner_plugin_id)));
        }

        let mut removed_any = false;
        for index in owned_indices.iter().skip(1).rev() {
            self.external_screens.remove(*index);
            removed_any = true;
        }

        if removed_any {
            self.reset_external_screen_plugin_ui_tracking();
        }

        self.sync_external_screen_infos();
        !had_existing || removed_any
    }

    pub(crate) fn remove_external_screen_for_owner(&mut self, owner_plugin_id: u8) -> bool {
        let owned_indices = self.external_screen_indices_for_owner(owner_plugin_id);
        if owned_indices.is_empty() {
            return false;
        }

        for index in owned_indices.iter().rev() {
            self.external_screens.remove(*index);
        }

        self.reset_external_screen_plugin_ui_tracking();
        self.sync_external_screen_infos();
        true
    }

    fn plugin_ui_tab_ids(&self) -> Vec<u8> {
        let mut plugin_ids: Vec<u8> = self
            .data
            .state
            .plugins
            .read()
            .unwrap()
            .keys()
            .copied()
            .collect();
        plugin_ids.sort_unstable();
        plugin_ids
    }

    fn sync_plugin_ui_tab_visibility(&mut self, screen_id: ScreenId, visible_plugin_tabs: &[u8]) {
        let current_visible: HashSet<u8> = visible_plugin_tabs.iter().copied().collect();
        let previous_visible: HashSet<u8> = self
            .plugin_ui_visible_tabs
            .iter()
            .filter_map(|(visible_screen_id, plugin_id)| {
                (*visible_screen_id == screen_id).then_some(*plugin_id)
            })
            .collect();

        for plugin_id in current_visible.difference(&previous_visible) {
            self.set_plugin_ui_tab_open(*plugin_id, screen_id, true);
        }

        for plugin_id in previous_visible.difference(&current_visible) {
            let visible_elsewhere =
                self.plugin_ui_visible_tabs
                    .iter()
                    .any(|(visible_screen_id, visible_plugin_id)| {
                        *visible_plugin_id == *plugin_id && *visible_screen_id != screen_id
                    });

            if !visible_elsewhere {
                self.set_plugin_ui_tab_open(*plugin_id, screen_id, false);
            }
        }

        self.plugin_ui_visible_tabs
            .retain(|(visible_screen_id, _)| *visible_screen_id != screen_id);
        self.plugin_ui_visible_tabs.extend(
            current_visible
                .into_iter()
                .map(|plugin_id| (screen_id, plugin_id)),
        );
    }

    fn set_plugin_ui_tab_open(&mut self, plugin_id: u8, screen_id: ScreenId, open: bool) {
        let changed = {
            let mut map = self.data.state.plugin_ui_visibility.write().unwrap();
            let entry = map.entry(plugin_id).or_insert(PluginOpenState::CLOSED);

            if !open && entry.screen_id != screen_id {
                return;
            }

            let changed = entry.open != open;
            entry.open = open;
            if open {
                entry.screen_id = screen_id;
            }
            changed
        };

        if changed {
            super::plugin_ui::notify_plugin_ui_open(&self.data, plugin_id, open);
        }
    }

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
        self.reset_external_screen_plugin_ui_tracking();
        self.sync_external_screen_infos();
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
        self.sync_external_screen_infos();
    }

    pub fn draw_external_screen_contents(
        &mut self,
        ctx: &Context,
        screen_id: ScreenId,
        screen: &mut ExternalScreen,
    ) {
        screen.update_viewport_geometry(ctx);
        let plugin_ids = self.plugin_ui_tab_ids();
        screen.ensure_tabs(&plugin_ids);

        let mut visible_plugin_tabs = Vec::new();

        {
            let mut tab_viewer = TabViewer {
                app: self,
                ctx,
                screen_id,
                visible_plugin_tabs: &mut visible_plugin_tabs,
            };

            DockArea::new(screen.dock_state())
                .id(Id::new(("external_screen_dock", screen_id)))
                .style(Style::from_egui(ctx.style().as_ref()))
                .show_close_buttons(false)
                .show_leaf_close_all_buttons(false)
                .show_leaf_collapse_buttons(false)
                .show(ctx, &mut tab_viewer);
        }

        self.sync_plugin_ui_tab_visibility(screen_id, &visible_plugin_tabs);
        screen.ensure_tabs(&plugin_ids);
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

    #[test]
    fn external_screen_showfile_round_trip_keeps_plugin_tabs() {
        let mut screen = ExternalScreen::new(vec2(1024.0, 768.0));
        screen.ensure_tabs(&[2]);

        let saved = screen.to_showfile();
        let restored = ExternalScreen::from_showfile(saved);
        let saved_again = restored.to_showfile();

        match saved_again.layout {
            ShowfileDockNode::Leaf { tabs, .. } => {
                assert!(tabs.contains(&ShowfileDockTab::PluginUi { plugin_id: 2 }));
            }
            ShowfileDockNode::Split { .. } => panic!("expected leaf layout"),
        }
    }

    #[test]
    fn external_screen_showfile_round_trip_keeps_owner_plugin_id() {
        let screen = ExternalScreen::new_owned(vec2(1024.0, 768.0), Some(7));

        let saved = screen.to_showfile();
        let restored = ExternalScreen::from_showfile(saved);
        let saved_again = restored.to_showfile();

        assert_eq!(saved_again.owner_plugin_id, Some(7));
    }
}
