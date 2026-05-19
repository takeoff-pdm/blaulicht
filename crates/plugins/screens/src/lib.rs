use blaulicht_plugin_framework as bpf;
use blaulicht_plugin_framework::Plugin;
use blaulicht_shared::{LogLevel, TickInput};
use serde::Deserialize;
use std::time::Duration;

const POLL_INTERVAL_MS: u32 = 1_000;
const ALERT_DURATION: Duration = Duration::from_secs(4);
const MONITOR_WATCHER_PATH: &str = "/usr/bin/monitor_watcher.sh";
const STATUS_PREFIX: &str = "__BLMW_STATUS__:";
const STDOUT_BEGIN: &str = "\n__BLMW_STDOUT_BEGIN__\n";
const STDOUT_END: &str = "\n__BLMW_STDOUT_END__\n";
const STDERR_BEGIN: &str = "__BLMW_STDERR_BEGIN__\n";
const STDERR_END: &str = "\n__BLMW_STDERR_END__\n";
const MAX_LOG_FIELD_LEN: usize = 1_200;

#[derive(Debug, Clone, Deserialize)]
struct MonitorWatcherResult {
    changed: bool,
    external_connected: bool,
    external_should_exist: bool,
    applied: bool,
    layout: String,
    main_output: MonitorOutput,
    external_output: Option<MonitorOutput>,
    signature: String,
}

#[derive(Debug, Clone, Deserialize)]
struct MonitorOutput {
    name: String,
    width: u32,
    height: u32,
    x: i32,
    y: i32,
    #[serde(default)]
    primary: bool,
}

#[derive(Debug, Clone)]
struct CommandOutput {
    status: i32,
    stdout: String,
    stderr: String,
}

#[derive(Debug, Clone, Copy)]
enum WatcherCommand {
    Probe,
    Apply,
}

impl WatcherCommand {
    fn as_str(self) -> &'static str {
        match self {
            Self::Probe => "probe",
            Self::Apply => "apply",
        }
    }
}

#[derive(Default)]
pub struct ScreensPlugin {
    plugin_id: u8,
    last_poll_clock: Option<u32>,
    last_transition_notified_signature: Option<String>,
    last_error_fingerprint: Option<String>,
}

impl ScreensPlugin {
    fn maybe_poll(&mut self, input: &TickInput, force: bool) {
        if !force {
            if let Some(last_poll_clock) = self.last_poll_clock {
                if input.clock.wrapping_sub(last_poll_clock) < POLL_INTERVAL_MS {
                    return;
                }
            }
        }

        self.last_poll_clock = Some(input.clock);
        self.poll_once();
    }

    fn poll_once(&mut self) {
        let probe = match self.run_watcher(WatcherCommand::Probe) {
            Ok(probe) => probe,
            Err(err) => {
                self.log_failure_once(format!("probe:{err}"), format!("probe failed: {err}"));
                return;
            }
        };

        if probe.changed {
            self.notify_transition(&probe);

            let applied = match self.run_watcher(WatcherCommand::Apply) {
                Ok(applied) => applied,
                Err(err) => {
                    self.log_failure_once(
                        format!("apply:{err}"),
                        format!("apply failed for signature {}: {err}", probe.signature),
                    );
                    return;
                }
            };

            if !applied.applied {
                let message = format!(
                    "apply returned success without applied=true for signature {}",
                    applied.signature
                );
                self.log_failure_once(format!("apply-flag:{message}"), message);
                return;
            }

            self.log(
                LogLevel::Info,
                format!(
                    "applied monitor layout {} with main {} at {}x{}+{}+{}{} (signature={})",
                    applied.layout,
                    applied.main_output.name,
                    applied.main_output.width,
                    applied.main_output.height,
                    applied.main_output.x,
                    applied.main_output.y,
                    if applied.main_output.primary {
                        " [primary]"
                    } else {
                        ""
                    },
                    applied.signature
                ),
            );

            if let Err(err) = self.reconcile_owned_screen(&applied) {
                self.log_failure_once(
                    format!("reconcile:{err}"),
                    format!("reconcile failed after apply: {err}"),
                );
                return;
            }
        } else if let Err(err) = self.reconcile_owned_screen(&probe) {
            self.log_failure_once(
                format!("reconcile:{err}"),
                format!("reconcile failed: {err}"),
            );
            return;
        }

        self.last_error_fingerprint = None;
    }

    fn notify_transition(&mut self, state: &MonitorWatcherResult) {
        if self.last_transition_notified_signature.as_deref() == Some(state.signature.as_str()) {
            return;
        }

        self.last_transition_notified_signature = Some(state.signature.clone());

        let label = transition_label(state);
        bpf::ui::alert_for(label, ALERT_DURATION);
        self.log(
            LogLevel::Info,
            format!(
                "detected monitor change: {label} (layout={}, external_connected={}, signature={})",
                state.layout, state.external_connected, state.signature
            ),
        );
    }

    fn reconcile_owned_screen(&mut self, state: &MonitorWatcherResult) -> Result<(), String> {
        let screens = bpf::ui::external_screens();
        let owned_screens: Vec<_> = screens
            .iter()
            .filter(|screen| screen.owner_plugin_id == Some(self.plugin_id))
            .collect();

        if state.external_should_exist {
            let external_output = state.external_output.as_ref().ok_or_else(|| {
                format!(
                    "signature {} requests external screen but external_output is missing",
                    state.signature
                )
            })?;

            let desired_width = external_output.width;
            let desired_height = external_output.height;

            let already_matches = owned_screens.len() == 1
                && dimensions_match(owned_screens[0].width, desired_width)
                && dimensions_match(owned_screens[0].height, desired_height);

            if already_matches {
                return Ok(());
            }

            if !bpf::ui::create_external_screen(desired_width, desired_height) {
                return Err(format!(
                    "host refused to create/update owned external screen to {}x{}",
                    desired_width, desired_height
                ));
            }

            let action = if owned_screens.is_empty() {
                "created"
            } else {
                "updated"
            };

            self.log(
                LogLevel::Info,
                format!(
                    "{action} owned external screen {}x{} on {} at {}x{}+{}+{}",
                    desired_width,
                    desired_height,
                    external_output.name,
                    external_output.width,
                    external_output.height,
                    external_output.x,
                    external_output.y
                ),
            );

            return Ok(());
        }

        if owned_screens.is_empty() {
            return Ok(());
        }

        if !bpf::ui::remove_external_screen(owned_screens[0].index) {
            return Err("host refused to remove owned external screen".to_string());
        }

        self.log(
            LogLevel::Info,
            format!("removed owned external screen after {}", state.signature),
        );

        Ok(())
    }

    fn run_watcher(&self, command: WatcherCommand) -> Result<MonitorWatcherResult, String> {
        let output = bpf::system(&build_watcher_command(command));
        let command_output = parse_command_output(&output)
            .map_err(|err| format!("{err}; raw={}", truncate_for_log(&output)))?;

        if command_output.status != 0 {
            return Err(format_command_failure(command, &command_output));
        }

        let stdout = command_output.stdout.trim();
        if stdout.is_empty() {
            return Err(format!("{} returned empty stdout", command.as_str()));
        }

        serde_json::from_str(stdout).map_err(|err| {
            format!(
                "{} returned invalid JSON: {err}; stdout={}",
                command.as_str(),
                truncate_for_log(stdout)
            )
        })
    }

    fn log_failure_once(&mut self, fingerprint: String, message: String) {
        if self.last_error_fingerprint.as_deref() == Some(fingerprint.as_str()) {
            return;
        }

        self.last_error_fingerprint = Some(fingerprint);
        self.log(LogLevel::Err, message);
    }

    fn log(&self, level: LogLevel, message: String) {
        bpf::bl_log(&format!("screens: {message}"), level);
    }
}

impl Plugin for ScreensPlugin {
    fn initialize(&mut self, input: TickInput) {
        self.plugin_id = input.id;
        self.maybe_poll(&input, true);
    }

    fn run(&mut self, input: TickInput) {
        self.maybe_poll(&input, false);
    }
}

fn transition_label(state: &MonitorWatcherResult) -> &'static str {
    if state.external_connected {
        "External screen attached"
    } else {
        "External screen removed"
    }
}

fn dimensions_match(actual: f32, expected: u32) -> bool {
    actual.round() as u32 == expected
}

fn build_watcher_command(command: WatcherCommand) -> String {
    format!(
        r#"stdout_file="$(mktemp)"; stderr_file="$(mktemp)"; status=0; if {path} {subcommand} >"$stdout_file" 2>"$stderr_file"; then status=0; else status="$?"; fi; printf '{status_prefix}%s{stdout_begin}' "$status"; cat "$stdout_file"; printf '{stdout_end}{stderr_begin}'; cat "$stderr_file"; printf '{stderr_end}'; rm -f "$stdout_file" "$stderr_file""#,
        path = MONITOR_WATCHER_PATH,
        subcommand = command.as_str(),
        status_prefix = STATUS_PREFIX,
        stdout_begin = STDOUT_BEGIN,
        stdout_end = STDOUT_END,
        stderr_begin = STDERR_BEGIN,
        stderr_end = STDERR_END,
    )
}

fn parse_command_output(raw: &str) -> Result<CommandOutput, String> {
    let (status_raw, after_status) = raw
        .split_once(STDOUT_BEGIN)
        .ok_or_else(|| "missing stdout section marker".to_string())?;

    let status = status_raw
        .trim()
        .strip_prefix(STATUS_PREFIX)
        .ok_or_else(|| "missing command status marker".to_string())?
        .parse::<i32>()
        .map_err(|err| format!("invalid command status: {err}"))?;

    let (stdout, after_stdout) = after_status
        .split_once(STDOUT_END)
        .ok_or_else(|| "missing stdout end marker".to_string())?;

    let stderr_body = after_stdout
        .strip_prefix(STDERR_BEGIN)
        .ok_or_else(|| "missing stderr section marker".to_string())?;
    let (stderr, _) = stderr_body
        .split_once(STDERR_END)
        .ok_or_else(|| "missing stderr end marker".to_string())?;

    Ok(CommandOutput {
        status,
        stdout: stdout.to_string(),
        stderr: stderr.to_string(),
    })
}

fn format_command_failure(command: WatcherCommand, output: &CommandOutput) -> String {
    let mut parts = vec![format!(
        "{} exited with status {}",
        command.as_str(),
        output.status
    )];

    let stderr = output.stderr.trim();
    if !stderr.is_empty() {
        parts.push(format!("stderr={}", truncate_for_log(stderr)));
    }

    let stdout = output.stdout.trim();
    if !stdout.is_empty() {
        parts.push(format!("stdout={}", truncate_for_log(stdout)));
    }

    parts.join("; ")
}

fn truncate_for_log(value: &str) -> String {
    let mut truncated = value.trim().replace('\n', " | ");
    if truncated.len() > MAX_LOG_FIELD_LEN {
        truncated.truncate(MAX_LOG_FIELD_LEN);
        truncated.push_str("...");
    }
    truncated
}

#[cfg_attr(not(test), no_mangle)]
extern "C" fn main() {
    bpf::hook_plugin(Box::new(ScreensPlugin::default()));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_command_output_extracts_sections() {
        let raw = "__BLMW_STATUS__:0\n__BLMW_STDOUT_BEGIN__\n{\"ok\":true}\n__BLMW_STDOUT_END__\n__BLMW_STDERR_BEGIN__\nhelper log\n__BLMW_STDERR_END__\n";

        let parsed = parse_command_output(raw).expect("expected command output to parse");

        assert_eq!(parsed.status, 0);
        assert_eq!(parsed.stdout.trim(), "{\"ok\":true}");
        assert_eq!(parsed.stderr.trim(), "helper log");
    }

    #[test]
    fn parse_command_output_rejects_missing_markers() {
        let err = parse_command_output("not an envelope").expect_err("expected parse failure");
        assert!(err.contains("marker"));
    }

    #[test]
    fn transition_label_uses_connection_state() {
        let attached = MonitorWatcherResult {
            changed: true,
            external_connected: true,
            external_should_exist: true,
            applied: false,
            layout: "hdmi_above_vga".to_string(),
            main_output: MonitorOutput {
                name: "VGA-1".to_string(),
                width: 800,
                height: 480,
                x: 0,
                y: 1080,
                primary: true,
            },
            external_output: Some(MonitorOutput {
                name: "HDMI-1".to_string(),
                width: 1920,
                height: 1080,
                x: 0,
                y: 0,
                primary: false,
            }),
            signature: "sig".to_string(),
        };

        let mut removed = attached.clone();
        removed.external_connected = false;
        removed.external_should_exist = false;

        assert_eq!(transition_label(&attached), "External screen attached");
        assert_eq!(transition_label(&removed), "External screen removed");
    }
}
