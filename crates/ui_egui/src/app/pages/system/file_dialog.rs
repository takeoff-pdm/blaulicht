//! Native file dialogs for showfile Load and Save As (feature
//! `file-dialog`).

use crate::app::{BlaulichtApp, PopupSpec};
use blaulicht_engine::{client::EngineClient, hub::HostCommand};
use egui::{Context, FontFamily, FontId};
use egui_file_dialog::{FileDialog, Filter};
use egui_phosphor::regular as ph;
use std::{collections::HashSet, path::PathBuf, time::Duration};
use sysinfo::Disks;

pub enum FileDialogOpenOrigin {
    Save,
    Load,
}

fn places_quick_access_entries(showfile_home: Option<PathBuf>) -> Vec<(String, PathBuf)> {
    let disks = Disks::new_with_refreshed_list();
    let mut seen = HashSet::new();
    let mut entries = Vec::new();
    let mut disk_count = 0;
    let mut usb_count = 0;
    let mut sd_count = 0;

    if let Some(path) = showfile_home {
        if !path.as_os_str().is_empty() {
            entries.push((format!("{}  Workspace", ph::FOLDER_SIMPLE), path));
        }
    }

    for disk in disks.iter() {
        let mount = disk.mount_point().to_path_buf();
        if mount.as_os_str().is_empty() || !seen.insert(mount.clone()) {
            continue;
        }

        let name = disk.name().to_string_lossy().to_lowercase();
        let is_sd = disk.is_removable()
            && (name.contains("mmc")
                || name.contains("sdcard")
                || name.contains("sdhc")
                || name.contains("sdxc"));

        let label = if is_sd {
            sd_count += 1;
            format!("{}  SD {sd_count}", ph::SIM_CARD)
        } else if disk.is_removable() {
            usb_count += 1;
            format!("{}  USB {usb_count}", ph::USB)
        } else {
            disk_count += 1;
            format!("{}  Disk {disk_count}", ph::HARD_DRIVE)
        };

        entries.push((label, mount));
    }

    entries
}

impl BlaulichtApp {
    /// Where the file dialogs start: `showfile_home`, else the open
    /// showfile's folder, else the config's.
    fn showfile_dialog_dir(&self) -> Option<PathBuf> {
        let last_dir = self
            .client
            .showfile_path()
            .and_then(|path| path.parent().map(|path| path.to_path_buf()));
        self.showfile_home
            .clone()
            .or(last_dir)
            .or_else(|| self.config_dir.clone())
    }

    pub(super) fn open_load_showfile_dialog(&mut self) {
        let preferred_dir = self.showfile_dialog_dir();
        let places_entries = places_quick_access_entries(self.showfile_home.clone());

        let mut dialog = FileDialog::new()
            .add_file_filter_extensions("Showfiles", vec!["json"])
            .default_file_filter("Showfiles")
            .set_file_icon(
                ph::FILE_CODE,
                Filter::new(|path: &std::path::Path| {
                    path.extension()
                        .and_then(|ext| ext.to_str())
                        .map(|ext| ext.eq_ignore_ascii_case("json"))
                        .unwrap_or(false)
                }),
            )
            .default_file_icon(ph::FILE_TEXT)
            .default_folder_icon(ph::FOLDER)
            .device_icon(ph::HARD_DRIVE)
            .removable_device_icon(ph::USB)
            .parent_directory_icon(ph::ARROW_UP)
            .back_icon(ph::ARROW_LEFT)
            .forward_icon(ph::ARROW_RIGHT)
            .new_folder_icon(ph::FOLDER_PLUS)
            .menu_icon(ph::LIST_BULLETS)
            .search_icon(ph::MAGNIFYING_GLASS)
            .path_edit_icon(ph::PENCIL)
            .directory_separator("›")
            .add_quick_access("Places", move |qa| {
                for (label, path) in places_entries {
                    qa.add_path(&label, path);
                }
            })
            .as_modal(true)
            .modal_overlay_color(egui::Color32::from_black_alpha(150))
            .default_size(egui::vec2(900.0, 560.0))
            .min_size(egui::vec2(700.0, 420.0))
            .resizable(true)
            .show_left_panel(true)
            .show_search(false)
            .show_path_edit_button(true)
            .show_menu_button(true)
            .show_new_folder_button(false)
            .show_pinned_folders(false)
            .show_places(false)
            .show_devices(false)
            .show_removable_devices(false);

        if let Some(dir) = preferred_dir.as_ref() {
            dialog = dialog.initial_directory(dir.to_path_buf());
        }

        let mut dialog = dialog;
        dialog.pick_file();
        self.system_ui_state.open_file_dialog = Some(dialog);
        self.system_ui_state.file_dialog_open_origin = FileDialogOpenOrigin::Load;
    }

    pub(super) fn open_save_showfile_dialog(&mut self) {
        let preferred_dir = self.showfile_dialog_dir();
        let places_entries = places_quick_access_entries(self.showfile_home.clone());

        let mut dialog = FileDialog::new()
            .add_file_filter_extensions("Showfiles", vec!["json"])
            .default_file_filter("Showfiles")
            .set_file_icon(
                ph::FILE_CODE,
                Filter::new(|path: &std::path::Path| {
                    path.extension()
                        .and_then(|ext| ext.to_str())
                        .map(|ext| ext.eq_ignore_ascii_case("json"))
                        .unwrap_or(false)
                }),
            )
            .default_file_icon(ph::FILE_TEXT)
            .default_folder_icon(ph::FOLDER)
            .device_icon(ph::HARD_DRIVE)
            .removable_device_icon(ph::USB)
            .parent_directory_icon(ph::ARROW_UP)
            .back_icon(ph::ARROW_LEFT)
            .forward_icon(ph::ARROW_RIGHT)
            .new_folder_icon(ph::FOLDER_PLUS)
            .menu_icon(ph::LIST_BULLETS)
            .search_icon(ph::MAGNIFYING_GLASS)
            .path_edit_icon(ph::PENCIL)
            .directory_separator("›")
            .add_quick_access("Places", move |qa| {
                for (label, path) in places_entries {
                    qa.add_path(&label, path);
                }
            })
            .as_modal(true)
            .modal_overlay_color(egui::Color32::from_black_alpha(150))
            .default_size(egui::vec2(900.0, 560.0))
            .min_size(egui::vec2(700.0, 420.0))
            .resizable(true)
            .show_left_panel(true)
            .show_search(false)
            .show_path_edit_button(true)
            .show_menu_button(true)
            .show_new_folder_button(true)
            .show_pinned_folders(false)
            .show_places(false)
            .show_devices(false)
            .show_removable_devices(false);

        if let Some(dir) = preferred_dir.as_ref() {
            dialog = dialog.initial_directory(dir.to_path_buf());
        }

        let mut dialog = dialog;
        dialog.save_file();
        self.system_ui_state.open_file_dialog = Some(dialog);
        self.system_ui_state.file_dialog_open_origin = FileDialogOpenOrigin::Save;
    }

    /// Draws the open Load / Save As dialog and acts on a picked file.
    pub(super) fn render_file_dialog(&mut self, ctx: &Context) {
        let mut dialog = match self.system_ui_state.open_file_dialog.take() {
            Some(dialog) => dialog,
            None => return,
        };
        let popup_dialog = !self.desktop_mode;

        if popup_dialog {
            let viewport = ctx.viewport_rect().size();
            let max_size = egui::vec2((viewport.x - 32.0).max(0.0), (viewport.y - 32.0).max(0.0));
            let desired = egui::vec2(900.0, 560.0);
            let size = egui::vec2(desired.x.min(max_size.x), desired.y.min(max_size.y));
            let config = dialog.config_mut();
            config.title_bar = false;
            config.resizable = false;
            config.movable = false;
            config.anchor = Some((egui::Align2::CENTER_CENTER, egui::vec2(0.0, 0.0)));
            config.default_size = size;
            config.min_size = size;
            config.max_size = Some(size);
        }

        let old_style = ctx.global_style().clone();
        let mut dialog_style = (*old_style).clone();
        dialog_style.visuals = egui::Visuals::dark();
        dialog_style.visuals.window_fill = egui::Color32::from_rgb(16, 18, 22);
        dialog_style.visuals.panel_fill = egui::Color32::from_rgb(19, 22, 26);
        dialog_style.visuals.faint_bg_color = egui::Color32::from_rgb(28, 32, 38);
        dialog_style.visuals.extreme_bg_color = egui::Color32::from_rgb(11, 13, 17);
        dialog_style.visuals.override_text_color = Some(egui::Color32::from_gray(235));
        dialog_style.visuals.window_corner_radius = egui::CornerRadius::same(8);
        dialog_style.spacing.interact_size.y = 30.0;
        dialog_style.spacing.item_spacing = egui::vec2(6.0, 8.0);
        dialog_style.spacing.button_padding = egui::vec2(6.0, 6.0);
        dialog_style.spacing.window_margin = egui::Margin::same(10);
        dialog_style.text_styles.insert(
            egui::TextStyle::Body,
            FontId::new(18.0, FontFamily::Proportional),
        );
        dialog_style.text_styles.insert(
            egui::TextStyle::Button,
            FontId::new(15.0, FontFamily::Proportional),
        );
        ctx.set_global_style(dialog_style);

        dialog.update(ctx);

        let picked = dialog.take_picked();

        ctx.set_global_style(old_style);

        if picked.is_none() {
            self.system_ui_state.open_file_dialog = Some(dialog);
        }

        if let Some(file) = picked {
            self.system_ui_state.open_file_dialog = None;

            match self.system_ui_state.file_dialog_open_origin {
                // The engine copies the stage's model assets next to the new
                // file, saves there and makes it the open showfile.
                FileDialogOpenOrigin::Save => {
                    self.send_command(HostCommand::SaveShowfileAs(file), |app, _| {
                        app.show_popup(PopupSpec::with_duration(
                            Duration::from_secs(2),
                            "Saved Showfile".to_string(),
                        ));
                    });
                }
                FileDialogOpenOrigin::Load => {
                    self.system_ui_state.pending_load_file = Some(file);
                }
            }
        }
    }
}
