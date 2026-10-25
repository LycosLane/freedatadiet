use std::path::{Path, PathBuf};
use eframe::egui::{self, Color32, RichText, Vec2};

use crate::platform;
use crate::updater::{UpdateManager, UpdateStatus};

pub struct TopBarAction {
    pub navigate_to: Option<PathBuf>,
    pub navigate_up: bool,
    pub trigger_browse: bool,
    pub trigger_rescan: bool,
    pub open_settings: bool,
    pub open_update_modal: bool,
    pub restart_and_apply_update: Option<PathBuf>,
    pub restart_admin_error: Option<String>,
}

pub fn render_top_bar(
    ui: &mut egui::Ui,
    current_path: &Path,
    path_input: &mut String,
    is_admin: bool,
    updater: &UpdateManager,
    _is_dark: bool,
) -> TopBarAction {
    let mut action = TopBarAction {
        navigate_to: None,
        navigate_up: false,
        trigger_browse: false,
        trigger_rescan: false,
        open_settings: false,
        open_update_modal: false,
        restart_and_apply_update: None,
        restart_admin_error: None,
    };

    ui.horizontal(|ui| {
        let has_parent = current_path.parent().is_some();
        if ui
            .add_enabled(has_parent, egui::Button::new(" ^ Up "))
            .on_hover_text("Navigate up to parent folder (Backspace)")
            .clicked()
        {
            action.navigate_up = true;
        }

        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            if ui.button("Settings").on_hover_text("Open Settings dialog (F12)").clicked() {
                action.open_settings = true;
            }

            if ui.button("Rescan").on_hover_text("Refresh and rescan current directory (F5)").clicked() {
                action.trigger_rescan = true;
            }

            if ui.button("Browse...").on_hover_text("Select folder using system file dialog").clicked() {
                action.trigger_browse = true;
            }

            if is_admin {
                ui.label(
                    RichText::new("[Administrator]")
                        .color(Color32::from_rgb(120, 220, 120))
                        .small(),
                );
            } else if ui
                .button(RichText::new("Restart as Admin").small())
                .on_hover_text("Relaunch FreeDataDiet with elevated administrator privileges")
                .clicked()
            {
                if let Err(msg) = platform::restart_as_admin(Some(current_path)) {
                    action.restart_admin_error = Some(msg);
                }
            }

            // In-App Update Indicator
            match updater.status() {
                UpdateStatus::Available(ref update) => {
                    let btn = egui::Button::new(
                        RichText::new(format!("Update v{}", update.latest_version))
                            .color(Color32::from_rgb(60, 220, 120))
                            .strong(),
                    );
                    if ui.add(btn).on_hover_text("Update available. Click to review and install.").clicked() {
                        action.open_update_modal = true;
                    }
                }
                UpdateStatus::Downloading { progress_pct, ref message } => {
                    let text = format!("Downloading ({:.0}%)...", progress_pct * 100.0);
                    ui.label(RichText::new(text).color(Color32::from_rgb(255, 200, 80)).small())
                        .on_hover_text(message);
                }
                UpdateStatus::DownloadedReadyToRestart { ref new_exe_path } => {
                    let btn = egui::Button::new(
                        RichText::new("Restart to Apply Update")
                            .color(Color32::WHITE)
                            .background_color(Color32::from_rgb(40, 160, 80))
                            .strong(),
                    );
                    if ui.add(btn).clicked() {
                        action.restart_and_apply_update = Some(new_exe_path.clone());
                    }
                }
                _ => {}
            }

            // Breadcrumb / Path Text Input
            let path_width = (ui.available_width() - 8.0).max(120.0);
            let text_edit = ui.add_sized(
                Vec2::new(path_width, 24.0),
                egui::TextEdit::singleline(path_input),
            );

            if text_edit.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)) {
                let candidate = PathBuf::from(&*path_input);
                if candidate.exists() {
                    action.navigate_to = Some(candidate);
                }
            }
        });
    });

    action
}
