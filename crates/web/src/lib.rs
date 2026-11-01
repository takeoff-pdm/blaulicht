//! The web frontend: the egui UI (`blaulicht-ui-egui`) compiled to wasm32.
//!
//! The engine serves this bundle on its remote-client port (`index.html`,
//! `blaulicht_web.js`, `blaulicht_web_bg.wasm`; see the engine's
//! `server::http`). The page connects back to the same host and port over the
//! websocket protocol as a `RemoteClient`, in an operator session of its own,
//! and runs `BlaulichtApp` in the kiosk layout. No visualizer, file dialogs
//! or plugin pop-outs (their features are off).
//!
//! Build it with `crates/web/build.sh` (or `scripts/blenv build web`).
#![cfg(target_arch = "wasm32")]

use std::cell::RefCell;

use blaulicht_engine::client::RemoteClient;
use blaulicht_ui_egui::{app::BlaulichtApp, client::remote_streams};
use wasm_bindgen::{prelude::*, JsCast};

thread_local! {
    /// The running app; it must outlive `start`.
    static RUNNER: RefCell<Option<eframe::WebRunner>> = const { RefCell::new(None) };
}

/// Runs when the module is loaded (`index.html` calls the generated `init`).
#[wasm_bindgen(start)]
pub fn start() {
    // eframe logs through `log`; tracing events reach it too (tracing's
    // `log` feature), and the WebLogger prints both to the browser console.
    eframe::WebLogger::init(log::LevelFilter::Info).ok();
    // Installs eframe's panic handler (it logs the panic); ours is chained
    // after it and shows the message on the page.
    let runner = eframe::WebRunner::new();
    install_crash_notice();
    RUNNER.with(|slot| *slot.borrow_mut() = Some(runner.clone()));

    wasm_bindgen_futures::spawn_local(async move {
        let Some(canvas) = element("blaulicht_canvas")
            .and_then(|element| element.dyn_into::<web_sys::HtmlCanvasElement>().ok())
        else {
            show_crash("The page has no canvas #blaulicht_canvas.");
            return;
        };
        let url = engine_url();
        tracing::info!(
            "Blaulicht web UI {}: connecting to {url}",
            env!("CARGO_PKG_VERSION")
        );
        let started = runner
            .start(
                canvas,
                eframe::WebOptions::default(),
                Box::new(move |cc| {
                    let ctx = cc.egui_ctx.clone();
                    let client =
                        RemoteClient::connect_web(&url, &remote_streams(false), move || {
                            ctx.request_repaint()
                        })?;
                    Ok(Box::new(BlaulichtApp::new(
                        cc,
                        client.into(),
                        None,
                        false,
                        None,
                        None,
                        Vec::new(),
                    )))
                }),
            )
            .await;
        match started {
            Ok(()) => {
                if let Some(loading) = element("loading") {
                    loading.remove();
                }
            }
            Err(err) => show_crash(&format!("The web UI could not start: {err:?}")),
        }
    });
}

/// The engine's websocket: the host and port this page came from.
fn engine_url() -> String {
    let location = web_sys::window().expect("a window").location();
    let secure = location
        .protocol()
        .is_ok_and(|protocol| protocol == "https:");
    let host = location
        .host()
        .unwrap_or_else(|_| "localhost:1234".to_string());
    format!("{}://{host}/", if secure { "wss" } else { "ws" })
}

fn element(id: &str) -> Option<web_sys::Element> {
    web_sys::window()?.document()?.get_element_by_id(id)
}

/// Shows `message` in the page's `#crash` box (the canvas stops updating
/// after a panic).
fn show_crash(message: &str) {
    if let Some(crash) = element("crash") {
        crash.set_text_content(Some(message));
        let _ = crash.set_attribute("style", "display: block");
    }
}

fn install_crash_notice() {
    let previous = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        previous(info);
        show_crash(&format!(
            "The web UI crashed: {info}\n\nReload the page to start again. \
             The browser console has the details."
        ));
    }));
}
