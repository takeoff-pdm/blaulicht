mod html;
mod api;
mod state;
mod ws;
pub use state::{AppStateWrapper, AppState, PluginFlags};
pub use html::*;
pub use api::*;
pub use ws::*;

#[derive(serde::Serialize)]
pub struct GenericResponse<'response> {
    success: bool,
    message: &'response str,
    error: Option<&'response str>,
}

impl<'response> GenericResponse<'response> {
    pub fn success(message: &'response str) -> Self {
        Self {
            success: true,
            message,
            error: None,
        }
    }
    pub fn err(message: &'response str, error: &'response str) -> Self {
        Self {
            success: false,
            message,
            error: Some(error),
        }
    }
}
