//! Exercises the plugin UI. For now: the popup, a port of the desk's own
//! (`crates/ui_egui/src/app/popup.rs`), both windowed and fullscreen.

use blaulicht_plugin_framework as bpf;
use blaulicht_plugin_framework::{ui, Plugin};
use blaulicht_shared::{ControlEvent, PluginUiEvent, TickInput};

const OPEN_WINDOWED_BUTTON_ID: u8 = 1;
const OPEN_FULLSCREEN_BUTTON_ID: u8 = 2;
const CLOSE_BUTTON_ID: u8 = 3;
const POPUP_ID: u8 = 1;

const POPUP_LIFETIME_MS: u32 = 10_000;
const POPUP_LABEL_SIZE: i32 = 24;

struct Popup {
    label: &'static str,
    fullscreen: bool,
    opened_at: u32,
}

impl Popup {
    /// Draws the popup, or returns false once its lifetime is over.
    fn render(&self, clock: u32) -> bool {
        let elapsed = clock.wrapping_sub(self.opened_at);
        if elapsed >= POPUP_LIFETIME_MS {
            return false;
        }

        ui::begin_popup(POPUP_ID, self.fullscreen);
        ui::label_styled(self.label, POPUP_LABEL_SIZE, false);
        ui::button("Close", CLOSE_BUTTON_ID);
        ui::progress_bar(
            1.0 - elapsed as f32 / POPUP_LIFETIME_MS as f32,
            &format!("{} seconds remaining", (POPUP_LIFETIME_MS - elapsed) / 1000),
        );
        ui::end_popup();
        true
    }
}

#[derive(Default)]
pub struct TestPlugin {
    popup: Option<Popup>,
}

impl Plugin for TestPlugin {
    fn initialize(&mut self, _input: TickInput) {}

    fn run(&mut self, input: TickInput) {
        for ev in &input.events.events {
            let ControlEvent::PluginUi(PluginUiEvent::Button { id }, plugin_id) = ev.body() else {
                continue;
            };
            if plugin_id != input.id {
                continue;
            }
            let open = |label, fullscreen| {
                Some(Popup {
                    label,
                    fullscreen,
                    opened_at: input.clock,
                })
            };
            match id {
                OPEN_WINDOWED_BUTTON_ID => self.popup = open("Windowed popup", false),
                OPEN_FULLSCREEN_BUTTON_ID => self.popup = open("Fullscreen popup", true),
                CLOSE_BUTTON_ID => self.popup = None,
                _ => {}
            }
        }

        ui::begin();
        ui::button("Open windowed popup", OPEN_WINDOWED_BUTTON_ID);
        ui::button("Open fullscreen popup", OPEN_FULLSCREEN_BUTTON_ID);
        if self.popup.as_ref().is_some_and(|p| !p.render(input.clock)) {
            self.popup = None;
        }
    }
}

#[no_mangle]
extern "C" fn main() {
    bpf::hook_plugin(Box::new(TestPlugin::default()));
}
