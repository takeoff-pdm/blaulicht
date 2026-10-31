//! The Setup page ([`AppPage::Setup`]): one sub-page per thing being set up.
//!
//! - [`fixtures`]: Setup › Fixtures, the group picker and fixture editor.
//! - [`groups`]: Setup › Groups, the group tree (folders) and group dialogs.
//! - [`scenes`]: Setup › Scenes, the scene tree (folders) and scene dialogs.
//! - [`animations`]: Setup › Animations, the animation template editor.
//! - [`palettes`]: Setup › Palettes, the palette tree (folders) and palette
//!   dialogs.
//!
//! Groups, Scenes and Palettes share one layout and their folder handling
//! (`components::SetupFolderUi`, `FolderDialog`, `folder_details`).
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
pub(crate) use palettes::palette_kind_label;
pub use palettes::PaletteUI;

use crate::app::{page::PageRenderContext, BlaulichtApp, PopupButtonSpec, PopupSpec};
use crate::client::EngineClient;
use crate::hub::{CommandOutcome, CreatedId, HostCommand};
use blaulicht_shared::{AppPage, AppSubPage};
use egui::Context;
use std::time::Duration;

impl BlaulichtApp {
    pub fn setup_ui(
        &mut self,
        ui: &mut egui::Ui,
        ctx: &Context,
        render_context: PageRenderContext,
    ) {
        // Make controls touch-friendly within this page
        ui.spacing_mut().interact_size = egui::vec2(44.0, 36.0);
        // One snapshot per frame, as this client's session sees the show (its
        // focused scene is the selected one on Setup › Scenes).
        let session = self.client.session();
        let dmx_engine = self.client.with_show(|show| {
            show.clone_for_session(session)
                .unwrap_or_else(|| show.clone_for_ui())
        });

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
            AppSubPage::Setup_Animations => app.setup_animations_ui(ui, ctx, render_context),
            AppSubPage::Setup_Palettes => app.setup_palettes_ui(ui, ctx, render_context),
            _ => {}
        });
    }
}

impl BlaulichtApp {
    /// Sends a Setup command. `on_ok` runs once the engine has applied it
    /// (with the id it created, if any); a refusal shows as a popup. Dialogs
    /// that close only on success close in `on_ok`, and wait for the reply
    /// before sending again ([`Self::setup_reply_pending`]).
    fn send_setup_command(
        &mut self,
        command: HostCommand,
        on_ok: impl FnOnce(&mut Self, Option<CreatedId>) + 'static,
    ) {
        let seq = self
            .reply_handlers
            .send_then(&self.client, command, move |app, outcome| match outcome {
                CommandOutcome::Ok { created } => on_ok(app, *created),
                CommandOutcome::Refused(why) => app.show_setup_refusal(why.clone()),
            });
        self.setup_reply = Some(seq);
    }

    /// Whether the last Setup command is still waiting for its reply.
    fn setup_reply_pending(&self) -> bool {
        self.setup_reply
            .is_some_and(|seq| self.reply_handlers.is_waiting(seq))
    }

    fn show_setup_refusal(&mut self, why: String) {
        self.show_popup(PopupSpec {
            label: why,
            label_size: Some(18.0),
            lifetime_duration: Duration::from_secs(5),
            button: Some(PopupButtonSpec {
                label: "OK".to_string(),
            }),
        });
    }
}
