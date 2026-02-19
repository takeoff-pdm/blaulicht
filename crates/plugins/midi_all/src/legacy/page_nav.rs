use crate::legacy::LegacyState;
use blaulicht_plugin_framework::MidiConnection;
use blaulicht_plugin_framework::{self as bpf};
use blaulicht_shared::{AppPage, ControlEvent, MainUiEvent};

const APC_PAGE_PADS: [u8; 8] = [63, 55, 47, 39, 31, 23, 15, 7];

impl LegacyState {
    pub fn sync_app_page(&mut self, conn: &MidiConnection) {
        for pad in APC_PAGE_PADS {
            conn.send(0x96, pad, 0);
        }

        if let Some(ref page) = self.current_app_page {
            if let Some(pad) = Self::pad_for_app_page(page) {
                conn.send(0x96, pad, 10);
            }

            if !self.page_update_from_ui {
                bpf::send_event(ControlEvent::MainUi(MainUiEvent::NavigatePage(*page)));
            }
        }

        self.last_app_page = self.current_app_page;
        self.page_update_from_ui = false;
        self.pending_app_page_sync = false;
    }

    pub fn app_page_from_pad(pad: u8) -> Option<AppPage> {
        debug_assert!(APC_PAGE_PADS.contains(&pad));

        match pad {
            63 => Some(AppPage::Logs),
            55 => Some(AppPage::System),
            47 => Some(AppPage::Audio),
            39 => Some(AppPage::FixturesSetup),
            31 => Some(AppPage::View),
            23 => Some(AppPage::ViewPerformance),
            15 => Some(AppPage::FixturesPerformance),
            7 => Some(AppPage::Animations),
            _ => None,
        }
    }

    pub fn pad_for_app_page(page: &AppPage) -> Option<u8> {
        let pad = match page {
            AppPage::Logs => 63,
            AppPage::System => 55,
            AppPage::Audio => 47,
            AppPage::FixturesSetup => 39,
            AppPage::View => 31,
            AppPage::ViewPerformance => 23,
            AppPage::FixturesPerformance => 15,
            AppPage::Animations => 7,
            AppPage::Visualizer => return None,
        };

        debug_assert!(APC_PAGE_PADS.contains(&pad));

        Some(pad)
    }
}
