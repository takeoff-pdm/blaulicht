use core::fmt::Display;

use bincode::{Decode, Encode};
use serde::{Deserialize, Serialize};
use strum::EnumIter;

use crate::AppPage;

/// A tab inside an [`AppPage`], shown in the page's sub-page tab bar.
///
/// Every variant belongs to exactly one page ([`AppSubPage::page`]) and must be
/// listed, in tab order, by that page's [`AppPage::list_subpages`]; the first
/// entry there is the page's default tab.
///
/// To add sub-pages to a page:
/// 1. Append variants named `<Page>_<Tab>` **at the end** of this enum. The
///    bincode index is part of the plugin ABI, so bump `PLUGIN_ABI_VERSION`.
/// 2. Add them to [`AppSubPage::page`] and [`AppSubPage::label`] (the compiler
///    points at both).
/// 3. List them in [`AppPage::list_subpages`].
///
/// Page code that matches on its own tabs needs a `_` arm, so that adding
/// another page's variants doesn't break it.
///
/// Variant names are also the JSON names used by `NavigateSubPage` events, e.g.
/// `{"MainUi":{"NavigateSubPage":"System_Dmx"}}`.
#[allow(non_camel_case_types)]
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, Hash, EnumIter, Serialize, Deserialize, Encode, Decode,
)]
pub enum AppSubPage {
    // System page. `System_System` is the "General" tab.
    System_System,
    System_Dmx,
    System_ArtNet,
    System_Midi,
    System_Serial,
    System_Plugins,
    System_Screens,
    // Setup page.
    Setup_Fixtures,
    Setup_Groups,
    Setup_Scenes,
    // Setup page, tabs added by ticket 010. They took over the old Animations
    // and Palettes pages (see `AppPage::moved_to`).
    Setup_Animations,
    Setup_Palettes,
    // System page, added after Setup's so the existing indices stay.
    System_Remote,
}

impl AppSubPage {
    /// The page this sub-page is a tab of.
    pub const fn page(self) -> AppPage {
        match self {
            AppSubPage::System_System
            | AppSubPage::System_Dmx
            | AppSubPage::System_ArtNet
            | AppSubPage::System_Midi
            | AppSubPage::System_Serial
            | AppSubPage::System_Plugins
            | AppSubPage::System_Screens
            | AppSubPage::System_Remote => AppPage::System,
            AppSubPage::Setup_Fixtures
            | AppSubPage::Setup_Groups
            | AppSubPage::Setup_Scenes
            | AppSubPage::Setup_Animations
            | AppSubPage::Setup_Palettes => AppPage::Setup,
        }
    }

    /// Tab label.
    pub const fn label(self) -> &'static str {
        match self {
            AppSubPage::System_System => "General",
            AppSubPage::System_Dmx => "DMX",
            AppSubPage::System_ArtNet => "Art-Net",
            AppSubPage::System_Midi => "MIDI",
            AppSubPage::System_Serial => "Serial",
            AppSubPage::System_Plugins => "Plugins",
            AppSubPage::System_Screens => "Screens",
            AppSubPage::System_Remote => "Remote UI",
            AppSubPage::Setup_Fixtures => "Fixtures",
            AppSubPage::Setup_Groups => "Groups",
            AppSubPage::Setup_Scenes => "Scenes",
            AppSubPage::Setup_Animations => "Animations",
            AppSubPage::Setup_Palettes => "Palettes",
        }
    }
}

impl Display for AppSubPage {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.write_str(self.label())
    }
}

#[cfg(test)]
mod tests {
    use super::AppSubPage;
    use crate::{AppPage, MainUiEvent};
    use bincode::config;
    use strum::IntoEnumIterator;

    #[test]
    fn every_subpage_is_listed_exactly_once_by_its_page() {
        for subpage in AppSubPage::iter() {
            let listed_by: Vec<AppPage> = AppPage::iter()
                .filter(|page| page.list_subpages().contains(&subpage))
                .collect();
            assert_eq!(listed_by, vec![subpage.page()], "{subpage:?}");
            let count = subpage
                .page()
                .list_subpages()
                .iter()
                .filter(|listed| **listed == subpage)
                .count();
            assert_eq!(count, 1, "{subpage:?} listed twice");
        }
    }

    #[test]
    fn default_subpage_is_the_first_listed() {
        assert_eq!(
            AppPage::System.default_subpage(),
            Some(AppSubPage::System_System)
        );
        assert_eq!(AppPage::Logs.default_subpage(), None);
        assert_eq!(
            AppPage::Setup.default_subpage(),
            Some(AppSubPage::Setup_Fixtures)
        );
    }

    #[test]
    fn existing_bincode_indices_are_stable() {
        let index =
            |subpage: AppSubPage| bincode::encode_to_vec(subpage, config::standard()).unwrap();
        assert_eq!(index(AppSubPage::System_System), [0]);
        assert_eq!(index(AppSubPage::System_Dmx), [1]);
        assert_eq!(index(AppSubPage::System_ArtNet), [2]);
        assert_eq!(index(AppSubPage::System_Midi), [3]);
        assert_eq!(index(AppSubPage::System_Serial), [4]);
        assert_eq!(index(AppSubPage::System_Plugins), [5]);
        assert_eq!(index(AppSubPage::System_Screens), [6]);
        assert_eq!(index(AppSubPage::Setup_Fixtures), [7]);
        assert_eq!(index(AppSubPage::Setup_Groups), [8]);
        assert_eq!(index(AppSubPage::Setup_Scenes), [9]);
        assert_eq!(index(AppSubPage::Setup_Animations), [10]);
        assert_eq!(index(AppSubPage::Setup_Palettes), [11]);
        assert_eq!(index(AppSubPage::System_Remote), [12]);

        let event = MainUiEvent::NavigateSubPage(AppSubPage::System_Dmx);
        assert_eq!(
            bincode::encode_to_vec(event, config::standard()).unwrap(),
            [7, 1]
        );
    }

    #[test]
    fn navigate_subpage_json_matches_the_documented_blctl_form() {
        let event: MainUiEvent =
            serde_json::from_str(r#"{"NavigateSubPage":"System_Plugins"}"#).unwrap();
        assert!(matches!(
            event,
            MainUiEvent::NavigateSubPage(AppSubPage::System_Plugins)
        ));
        let event: MainUiEvent =
            serde_json::from_str(r#"{"NavigateSubPage":"Setup_Groups"}"#).unwrap();
        assert!(matches!(
            event,
            MainUiEvent::NavigateSubPage(AppSubPage::Setup_Groups)
        ));
    }

    #[test]
    fn setup_keeps_the_fixtures_setup_index_and_name() {
        // `Setup` took over `FixturesSetup`'s slot: same bincode index, and the
        // old JSON name still parses (showfile dock layouts, plugin state, blctl).
        assert_eq!(
            bincode::encode_to_vec(AppPage::Setup, config::standard()).unwrap(),
            [3]
        );
        let old: AppPage = serde_json::from_str(r#""FixturesSetup""#).unwrap();
        assert_eq!(old, AppPage::Setup);
        let new: AppPage = serde_json::from_str(r#""Setup""#).unwrap();
        assert_eq!(new, AppPage::Setup);
        let event: MainUiEvent =
            serde_json::from_str(r#"{"NavigatePage":"FixturesSetup"}"#).unwrap();
        assert!(matches!(event, MainUiEvent::NavigatePage(AppPage::Setup)));
    }

    #[test]
    fn programmer_keeps_the_fixtures_performance_index_and_name() {
        // `Programmer` took over `FixturesPerformance`'s slot (ticket 011).
        assert_eq!(
            bincode::encode_to_vec(AppPage::Programmer, config::standard()).unwrap(),
            [6]
        );
        let old: AppPage = serde_json::from_str(r#""FixturesPerformance""#).unwrap();
        assert_eq!(old, AppPage::Programmer);
        let new: AppPage = serde_json::from_str(r#""Programmer""#).unwrap();
        assert_eq!(new, AppPage::Programmer);
        let event: MainUiEvent =
            serde_json::from_str(r#"{"NavigatePage":"FixturesPerformance"}"#).unwrap();
        assert!(matches!(
            event,
            MainUiEvent::NavigatePage(AppPage::Programmer)
        ));
    }

    #[test]
    fn animations_and_palettes_moved_into_setup() {
        assert_eq!(
            AppPage::Animations.moved_to(),
            Some(AppSubPage::Setup_Animations)
        );
        assert_eq!(
            AppPage::Palettes.moved_to(),
            Some(AppSubPage::Setup_Palettes)
        );
        for page in AppPage::iter() {
            if let Some(subpage) = page.moved_to() {
                // A moved page points at a tab of a page that is still shown.
                assert_eq!(subpage.page().moved_to(), None, "{page:?}");
                assert!(subpage.page().list_subpages().contains(&subpage));
            }
        }
        // The variants stay, with their indices and JSON names, so old plugin
        // events and showfile dock layouts still parse.
        assert_eq!(
            bincode::encode_to_vec(AppPage::Animations, config::standard()).unwrap(),
            [7]
        );
        assert_eq!(
            bincode::encode_to_vec(AppPage::Palettes, config::standard()).unwrap(),
            [8]
        );
        let event: MainUiEvent = serde_json::from_str(r#"{"NavigatePage":"Palettes"}"#).unwrap();
        assert!(matches!(
            event,
            MainUiEvent::NavigatePage(AppPage::Palettes)
        ));
    }

    #[test]
    fn the_navbar_leaves_out_moved_pages() {
        let pages: Vec<AppPage> = AppPage::navbar_pages().collect();
        assert!(!pages.contains(&AppPage::Animations));
        assert!(!pages.contains(&AppPage::Palettes));
        assert_eq!(pages.len(), AppPage::iter().count() - 2);
        assert_eq!(pages.first(), Some(&AppPage::Logs));
        assert_eq!(
            AppPage::Setup.list_subpages(),
            &[
                AppSubPage::Setup_Fixtures,
                AppSubPage::Setup_Groups,
                AppSubPage::Setup_Scenes,
                AppSubPage::Setup_Animations,
                AppSubPage::Setup_Palettes,
            ]
        );
    }
}
