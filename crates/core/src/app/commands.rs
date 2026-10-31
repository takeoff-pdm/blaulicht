//! Sending host commands from pages: send, show a refusal as a popup, and run
//! a follow-up once the engine has applied the command.

use crate::app::{BlaulichtApp, PopupButtonSpec, PopupSpec};
use crate::hub::{CommandOutcome, CommandSeq, CreatedId, HostCommand};
use std::time::Duration;

impl BlaulichtApp {
    /// Sends `command`. `on_ok` runs on a later frame, once the engine has
    /// applied it (with the id it created, if any); a refusal shows as a
    /// popup ([`Self::show_refusal`]). Returns the command's seq, so a dialog
    /// can wait for the reply (`ReplyHandlers::is_waiting`).
    pub(crate) fn send_command(
        &mut self,
        command: HostCommand,
        on_ok: impl FnOnce(&mut Self, Option<CreatedId>) + 'static,
    ) -> CommandSeq {
        self.reply_handlers
            .send_then(&self.client, command, move |app, outcome| match outcome {
                CommandOutcome::Ok { created } => on_ok(app, *created),
                CommandOutcome::Refused(why) => app.show_refusal(why.clone()),
            })
    }

    /// Shows why the engine refused a command, as a 5 s popup with OK.
    pub(crate) fn show_refusal(&mut self, why: String) {
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
