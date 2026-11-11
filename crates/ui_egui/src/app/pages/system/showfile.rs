use crate::app::{
    components::{self, ButtonSize, Dialog},
    BlaulichtApp, PopupSpec,
};
use blaulicht_engine::{client::EngineClient, hub::HostCommand, persistence::SaveStatus};
use egui::{Context, Ui};
use std::{path::PathBuf, time::Duration};

impl BlaulichtApp {
    /// Closes the showfile: the engine starts a new, empty show
    /// (`SystemMessage::ShowLoaded` resets the view state).
    fn close_showfile(&mut self) {
        self.send_command(HostCommand::CloseShowfile, |_, _| {});
    }

    fn load_showfile_path(&mut self, file: PathBuf) {
        self.send_command(HostCommand::LoadShowfile(file), |app, _| {
            app.show_popup(PopupSpec::with_duration(
                Duration::from_secs(2),
                "Loaded Showfile".to_string(),
            ));
        });
    }

    fn render_pending_showfile_load_dialog(&mut self, ctx: &Context) {
        let Some(file) = self.system_ui_state.pending_load_file.clone() else {
            return;
        };

        let response = Dialog::new("Replace Showfile".to_string(), egui::vec2(320.0, 150.0))
            .with_backdrop()
            .show(ctx, |ui| {
                ui.label("Loading this showfile will replace the current show.");
                if self.layout.is_desktop() && self.save_status.has_unsaved_changes() {
                    ui.label(
                        egui::RichText::new("The current show has unsaved changes.")
                            .color(egui::Color32::from_rgb(0xe0, 0xa1, 0x3a)),
                    );
                }
                ui.add_space(12.0);
                ui.horizontal(|ui| {
                    let needs_guard = self.save_status.has_unsaved_changes();
                    let saving = matches!(self.save_status, SaveStatus::Saving);
                    if needs_guard
                        && components::action_button(
                            ui,
                            !saving,
                            "Save",
                            ButtonSize::Medium,
                            Some("A save is already in progress"),
                        )
                    {
                        self.request_showfile_save();
                    }
                    let load_label = if needs_guard {
                        "Discard & Load"
                    } else {
                        "Load"
                    };
                    if components::action_button(
                        ui,
                        !saving,
                        load_label,
                        ButtonSize::Medium,
                        Some("Wait for the active save to finish"),
                    ) {
                        self.system_ui_state.pending_load_file = None;
                        self.load_showfile_path(file.clone());
                    }
                    if components::button_as(
                        ui,
                        true,
                        components::ButtonVariant::Tonal,
                        "Cancel",
                        ButtonSize::Medium,
                    ) {
                        self.system_ui_state.pending_load_file = None;
                    }
                });
            });
        if response.cancel_requested {
            self.system_ui_state.pending_load_file = None;
        }
    }

    fn render_close_showfile_dialog(&mut self, ctx: &Context) {
        if !self.system_ui_state.close_showfile_confirm_open {
            return;
        }
        if self.layout.is_desktop() && self.client.is_remote() {
            self.system_ui_state.close_showfile_confirm_open = false;
            return;
        }

        let response = Dialog::new("Close Showfile".to_string(), egui::vec2(320.0, 150.0))
            .with_backdrop()
            .show(ctx, |ui| {
                // Desktop (059): warn only when there is something to lose.
                let text = if !self.layout.is_desktop() || self.save_status.has_unsaved_changes() {
                    "Close the current showfile? Unsaved changes will be lost."
                } else {
                    "Close the current showfile? Lights go dark until another show is loaded."
                };
                ui.label(text);
                ui.add_space(12.0);
                ui.horizontal(|ui| {
                    let needs_guard = self.save_status.has_unsaved_changes();
                    let saving = matches!(self.save_status, SaveStatus::Saving);
                    if needs_guard
                        && components::action_button(
                            ui,
                            !saving,
                            "Save",
                            ButtonSize::Medium,
                            Some("A save is already in progress"),
                        )
                    {
                        self.request_showfile_save();
                    }
                    let close_label = if needs_guard {
                        "Discard & Close"
                    } else {
                        "Close"
                    };
                    if components::action_button(
                        ui,
                        !saving,
                        close_label,
                        ButtonSize::Medium,
                        Some("Wait for the active save to finish"),
                    ) {
                        self.system_ui_state.close_showfile_confirm_open = false;
                        self.close_showfile();
                    }
                    if components::button_as(
                        ui,
                        true,
                        components::ButtonVariant::Tonal,
                        "Cancel",
                        ButtonSize::Medium,
                    ) {
                        self.system_ui_state.close_showfile_confirm_open = false;
                    }
                });
            });
        if response.cancel_requested {
            self.system_ui_state.close_showfile_confirm_open = false;
        }
    }

    //
    // TODO: i need to remove this
    //
    pub fn save_showfile(&mut self) {
        self.request_showfile_save();
    }

    pub fn render_showfile_buttons(&mut self, ui: &mut Ui, button_size: ButtonSize) {
        ui.horizontal(|ui| {
            // The file dialogs browse this machine, so they're no use to a
            // remote UI: showfile paths are the engine host's.
            #[cfg(feature = "file-dialog")]
            if !self.client.is_remote() {
                if components::button(ui, false, "Load Showfile", button_size) {
                    self.open_load_showfile_dialog();
                }

                if components::button(ui, false, "Save to Showfile", button_size) {
                    self.open_save_showfile_dialog();
                }
            }

            let showfile_open = self.client.showfile_path().is_some();
            // Desktop (059): a remote UI can't close the show for everyone.
            let hide_close = self.layout.is_desktop() && self.client.is_remote();
            if !hide_close
                && components::action_button(
                    ui,
                    showfile_open,
                    "Close Showfile",
                    button_size,
                    Some("No showfile is currently open"),
                )
            {
                self.system_ui_state.close_showfile_confirm_open = true;
            }

            if components::action_button(
                ui,
                showfile_open,
                "Save Showfile",
                button_size,
                Some("Open or create a showfile before saving"),
            ) {
                self.save_showfile();
            }
        });
    }

    pub fn render_showfile_dialog(&mut self, ctx: &Context) {
        self.render_pending_showfile_load_dialog(ctx);
        self.render_close_showfile_dialog(ctx);

        #[cfg(feature = "file-dialog")]
        self.render_file_dialog(ctx);
    }
}
