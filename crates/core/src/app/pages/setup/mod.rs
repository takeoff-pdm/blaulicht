//! The Setup page ([`AppPage::Setup`]): one sub-page per thing being set up.
//!
//! - [`fixtures`]: Setup › Fixtures, the group picker and fixture editor.
//! - [`groups`]: Setup › Groups, the group list and group dialogs.
//! - [`scenes`]: Setup › Scenes, the scene list and scene dialogs.
//!
//! To add a tab, append an `AppSubPage::Setup_<Tab>` variant (see
//! `blaulicht_shared::AppSubPage`), add a module here and a match arm in
//! [`BlaulichtApp::setup_ui`].

mod animations;
mod fixtures;
mod groups;
mod palettes;
mod scenes;

pub use animations::AnimationUI;
pub use fixtures::{AddFixtureIncrementAxis, AddFixtureKind};
pub use palettes::PaletteUI;

use crate::app::{page::PageRenderContext, BlaulichtApp};
use blaulicht_shared::{AppPage, AppSubPage};
use egui::Context;

impl BlaulichtApp {
    pub fn setup_ui(
        &mut self,
        ui: &mut egui::Ui,
        ctx: &Context,
        render_context: PageRenderContext,
    ) {
        // Make controls touch-friendly within this page
        ui.spacing_mut().interact_size = egui::vec2(44.0, 36.0);
        let dmx_engine = { self.data.state.dmx_engine.read().unwrap().clone() };

        // Page-wide tool windows: opened from the Fixtures tab, but they stay
        // open while switching tabs.
        self.render_dmx_override_dialog(ctx, &dmx_engine);
        self.render_dmx_simulation_dialog(ctx, dmx_engine.groups());

        self.subpage_ui(ui, AppPage::Setup, |app, ui, subpage| match subpage {
            AppSubPage::Setup_Fixtures => {
                app.setup_fixtures_ui(ui, ctx, render_context, &dmx_engine)
            }
            AppSubPage::Setup_Groups => app.setup_groups_ui(ui, ctx, render_context, &dmx_engine),
            AppSubPage::Setup_Scenes => app.setup_scenes_ui(ui, ctx, render_context, &dmx_engine),
            _ => {}
        });
    }
}
