//! Plugin UI ops. The type lives in `blaulicht-shared` (with serde and
//! bincode derives) so remote frontends can decode the plugin UI stream; it
//! is host-only and not part of the plugin ABI.

pub use blaulicht_shared::ui_ops::WasmUiOp;
