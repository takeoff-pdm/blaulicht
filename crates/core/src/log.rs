//
// Host logging.
//
// `tracing` is the only logging API of the host. Every event goes to the
// terminal (fmt layer) and to the Logs page (`SystemOutLayer`), except events
// with `TERMINAL_ONLY_LOG_TARGET`. The Logs page source column is derived from
// the event target: an explicit `target: target::…` wins, otherwise the module
// path of the call site is mapped to a subsystem (see `source_for_target`).
// Messages therefore carry no `[PREFIX]`s.
//

use std::borrow::Cow;

use blaulicht_shared::LogLevel;
use crossbeam_channel::Sender;
use tracing::{Event, Level, Subscriber};
use tracing_log::NormalizeEvent;
use tracing_subscriber::layer::{Context as TraceContext, SubscriberExt};
use tracing_subscriber::util::SubscriberInitExt;
use tracing_subscriber::{EnvFilter, Layer};

use crate::msg::{SystemMessage, TERMINAL_ONLY_LOG_TARGET};

/// Explicit tracing targets. Use them only where the call site's module does
/// not already map to the right subsystem, e.g. audio handling inside the
/// supervisor: `tracing::warn!(target: target::AUDIO, "...")`.
pub mod target {
    pub const ENGINE: &str = "bl::engine";
    pub const INIT: &str = "bl::init";
    pub const CONFIG: &str = "bl::config";
    pub const AUDIO: &str = "bl::audio";
    pub const PLUGIN: &str = "bl::plugin";
    pub const DMX: &str = "bl::dmx";
    pub const MIDI: &str = "bl::midi";
    pub const UDP: &str = "bl::udp";
    pub const SERIAL: &str = "bl::serial";
    pub const UI: &str = "bl::ui";
    pub const WEB: &str = "bl::web";
}

/// Maps a tracing target to the source shown in the Logs page.
pub fn source_for_target(target: &str) -> Cow<'static, str> {
    let source = match target {
        target::ENGINE => "Engine",
        target::INIT => "Init",
        target::CONFIG => "Config",
        target::AUDIO => "Audio",
        target::PLUGIN => "Plugin",
        target::DMX => "DMX",
        target::MIDI => "MIDI",
        target::UDP => "UDP",
        target::SERIAL => "Serial",
        target::UI => "UI",
        target::WEB => "Web",
        _ => return source_for_module_path(target),
    };
    Cow::Borrowed(source)
}

fn source_for_module_path(path: &str) -> Cow<'static, str> {
    let mut segments = path.split("::");
    let krate = segments.next().unwrap_or_default();
    let module = segments.next().unwrap_or_default();
    let submodule = segments.next().unwrap_or_default();

    let source = match krate {
        "blaulicht_core" => match (module, submodule) {
            ("plugin", "midi") => "MIDI",
            ("plugin", "udp") => "UDP",
            ("plugin", "serial") => "Serial",
            ("plugin", _) => "Plugin",
            ("dmx", _) => "DMX",
            ("audio", _) => "Audio",
            ("app", _) => "UI",
            ("config", _) => "Config",
            ("routes", _) => "Web",
            // `main.rs` logs with the bare crate name.
            ("", _) => "Init",
            _ => "Engine",
        },
        "blaulicht_audio_engine" => "Audio",
        "blaulicht_shared" => "Engine",
        "" => "Unknown",
        other => return Cow::Owned(other.to_string()),
    };
    Cow::Borrowed(source)
}

/// Forwards tracing events to the UI log window.
struct SystemOutLayer {
    system_out: Sender<SystemMessage>,
}

#[derive(Default)]
struct LogVisitor {
    message: Option<String>,
    fields: Vec<String>,
}

impl tracing::field::Visit for LogVisitor {
    fn record_str(&mut self, field: &tracing::field::Field, value: &str) {
        if field.name().starts_with("log.") {
            // Metadata of `log` records forwarded by `LogTracer`.
        } else if field.name() == "message" {
            self.message = Some(value.to_string());
        } else {
            self.fields.push(format!("{}={value}", field.name()));
        }
    }

    fn record_debug(&mut self, field: &tracing::field::Field, value: &dyn std::fmt::Debug) {
        if field.name().starts_with("log.") {
            // Metadata of `log` records forwarded by `LogTracer`.
        } else if field.name() == "message" {
            self.message = Some(format!("{value:?}"));
        } else {
            self.fields.push(format!("{}={value:?}", field.name()));
        }
    }
}

impl<S> Layer<S> for SystemOutLayer
where
    S: Subscriber,
{
    fn on_event(&self, event: &Event<'_>, _ctx: TraceContext<'_, S>) {
        // `log` crate records forwarded by `LogTracer` carry their original
        // target in fields; normalize them to look like tracing events.
        let normalized = event.normalized_metadata();
        let metadata = normalized.as_ref().unwrap_or_else(|| event.metadata());
        if metadata.target() == TERMINAL_ONLY_LOG_TARGET {
            return;
        }

        let level = match *metadata.level() {
            Level::ERROR => LogLevel::Err,
            Level::WARN => LogLevel::Warn,
            Level::INFO => LogLevel::Info,
            Level::DEBUG | Level::TRACE => LogLevel::Debug,
        };

        let mut visitor = LogVisitor::default();
        event.record(&mut visitor);

        let message = match visitor.message {
            Some(msg) if visitor.fields.is_empty() => msg,
            Some(msg) => format!("{msg} {}", visitor.fields.join(" ")),
            None if !visitor.fields.is_empty() => visitor.fields.join(" "),
            None => metadata.target().to_string(),
        };

        // The UI receiver may already be gone during shutdown.
        let _ = self.system_out.send(SystemMessage::Log {
            message,
            level,
            source: source_for_target(metadata.target()),
        });
    }
}

/// Installs the global subscriber: terminal output plus the UI log window.
/// `RUST_LOG` overrides the default `info` filter for both.
pub fn init_tracing(system_out: Sender<SystemMessage>) {
    let _ = tracing_log::LogTracer::init();
    let env_filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info"));

    let _ = tracing_subscriber::registry()
        .with(env_filter)
        .with(tracing_subscriber::fmt::layer())
        .with(SystemOutLayer { system_out })
        .try_init();
}

#[cfg(test)]
mod tests {
    use super::{source_for_target, target};

    #[test]
    fn explicit_targets_win() {
        assert_eq!(source_for_target(target::AUDIO), "Audio");
        assert_eq!(source_for_target(target::DMX), "DMX");
        assert_eq!(source_for_target(target::INIT), "Init");
    }

    #[test]
    fn core_module_paths_map_to_subsystems() {
        assert_eq!(source_for_target("blaulicht_core"), "Init");
        assert_eq!(
            source_for_target("blaulicht_core::mainloop::supervisor"),
            "Engine"
        );
        assert_eq!(source_for_target("blaulicht_core::plugin::wasm"), "Plugin");
        assert_eq!(source_for_target("blaulicht_core::plugin::midi"), "MIDI");
        assert_eq!(source_for_target("blaulicht_core::plugin::udp"), "UDP");
        assert_eq!(source_for_target("blaulicht_core::dmx"), "DMX");
        assert_eq!(source_for_target("blaulicht_core::app::pages::audio"), "UI");
        assert_eq!(
            source_for_target("blaulicht_audio_engine::collector"),
            "Audio"
        );
    }

    #[test]
    fn foreign_crates_use_their_crate_name() {
        assert_eq!(source_for_target("wgpu_core::device"), "wgpu_core");
        assert_eq!(source_for_target(""), "Unknown");
    }
}
