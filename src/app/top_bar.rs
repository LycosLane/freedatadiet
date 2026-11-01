use std::path::PathBuf;
use std::time::Instant;

use eframe::egui::{self, Color32, Panel, RichText, Vec2};

use crate::model::format_size;
use crate::platform;

use super::FreeDataDietApp;

impl FreeDataDietApp {
    pub(super) fn render_top_bar(&mut self, ui: &mut egui::Ui, is_dark: bool) {
        let mut target_drive = None;

        Panel::top("top_panel").show(ui, |ui| {
            ui.add_space(6.0);

            // Drive selection row
            ui.horizontal(|ui| {
                ui.label(RichText::new("Drives:").strong());
                for drive in &self.drives {
                    let is_current = self.current_path.starts_with(&drive.path);
                    let label = format!(
                        "{} ({} free of {})",
                        drive.name,
                        format_size(drive.available_space),
                        format_size(drive.total_space)
                    );

                    let btn = egui::Button::new(
                        RichText::new(label).color(if is_current {
                            if is_dark { Color32::from_rgb(100, 200, 255) } else { Color32::from_rgb(0, 110, 220) }
                        } else if is_dark {
                            Color32::LIGHT_GRAY
                        } else {
                            Color32::DARK_GRAY
                        }),
                    );

                    if ui.add(btn).clicked() {
                        target_drive = Some(drive.path.clone());
                    }
                }

                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if self.is_admin {
                        ui.label(
                            RichText::new("[Administrator]")
                                .color(Color32::from_rgb(120, 220, 120))
                                .small(),
                        );
                    } else if ui
                        .button(RichText::new("Restart as Admin").small())
                        .clicked()
                        && let Err(msg) = platform::restart_as_admin(Some(&self.current_path)) {
                            self.status_toast = Some((msg, Instant::now()));
                        }

                    // --- UPDATE BUTTON / INDICATOR ---
                    match self.updater.status() {
                        crate::updater::UpdateStatus::Available(ref update) => {
                            let btn = egui::Button::new(
                                RichText::new(format!("Update v{}", update.latest_version))
                                    .color(Color32::from_rgb(60, 220, 120))
                                    .strong(),
                            );
                            if ui.add(btn).on_hover_text("Update available. Click to review and update.").clicked() {
                                self.show_update_modal = true;
                            }
                        }
                        crate::updater::UpdateStatus::Downloading { progress_pct, ref message } => {
                            let text = format!("Downloading ({:.0}%)...", progress_pct * 100.0);
                            ui.label(RichText::new(text).color(Color32::from_rgb(255, 200, 80)).small())
                                .on_hover_text(message);
                        }
                        crate::updater::UpdateStatus::DownloadedReadyToRestart { ref new_exe_path } => {
                            let btn = egui::Button::new(
                                RichText::new("Restart to Apply Update")
                                    .color(Color32::WHITE)
                                    .background_color(Color32::from_rgb(40, 160, 80))
                                    .strong(),
                            );
                            if ui.add(btn).clicked() {
                                self.updater.restart_and_apply(new_exe_path.clone());
                            }
                        }
                        _ => {}
                    }
                });
            });

            ui.add_space(4.0);

            // Path & Navigation row
            ui.horizontal(|ui| {
                let has_parent = self.current_path.parent().is_some();
                if ui
                    .add_enabled(has_parent, egui::Button::new(" ^ Up "))
                    .clicked()
                {
                    self.navigate_up();
                }

                let mut do_browse = false;
                let mut do_refresh = false;
                let mut toggle_duplicates = false;
                let mut toggle_flatten = false;
                let mut clear_fav_selection = false;

                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if ui
                        .selectable_label(self.config.show_sidebar, RichText::new("Settings & Menu").strong())
                        .on_hover_text("Toggle live Settings, Filters & Favorites panel (F12 / Ctrl+,)")
                        .clicked()
                    {
                        self.config.show_sidebar = !self.config.show_sidebar;
                        self.save_config();
                    }

                    if ui
                        .selectable_label(
                            self.config.show_duplicates_only,
                            RichText::new("Duplicates").color(if self.config.show_duplicates_only {
                                Color32::from_rgb(255, 140, 0)
                            } else if is_dark {
                                Color32::LIGHT_GRAY
                            } else {
                                Color32::DARK_GRAY
                            }),
                        )
                        .on_hover_text("Find duplicate files across this folder and its subfolders (Ctrl+D)")
                        .clicked()
                    {
                        toggle_duplicates = true;
                    }

                    if ui
                        .selectable_label(
                            self.config.flatten_subfolders,
                            RichText::new("Include Subfolders"),
                        )
                        .on_hover_text("Scan and list all files and subfolders in one flat view (F6)")
                        .clicked()
                    {
                        toggle_flatten = true;
                    }

                    if ui.button("Refresh").on_hover_text("Rescan current folder (F5)").clicked() {
                        do_refresh = true;
                    }

                    if ui.button("Browse...").clicked() {
                        do_browse = true;
                    }

                    if self.selected_favorites.len() > 1 {
                        ui.horizontal(|ui| {
                            ui.label(
                                RichText::new(format!("[Multi-Select: {} locations]", self.selected_favorites.len()))
                                    .strong()
                                    .color(if is_dark { Color32::from_rgb(100, 200, 255) } else { Color32::from_rgb(0, 110, 220) }),
                            );
                            if ui.small_button("Clear Selection").clicked() {
                                clear_fav_selection = true;
                            }
                        });
                    } else {
                        let path_width = (ui.available_width() - 8.0).max(120.0);
                        let text_edit = ui.add_sized(
                            Vec2::new(path_width, 24.0),
                            egui::TextEdit::singleline(&mut self.path_input),
                        );

                        if text_edit.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)) {
                            let candidate = PathBuf::from(&self.path_input);
                            if candidate.exists() {
                                self.navigate_to(candidate);
                            }
                        }
                    }
                });

                if clear_fav_selection {
                    self.selected_favorites.clear();
                    self.trigger_scan();
                }

                if do_browse
                    && let Some(folder) = rfd::FileDialog::new()
                        .set_directory(&self.current_path)
                        .pick_folder()
                    {
                        self.navigate_to(folder);
                    }
                if toggle_duplicates {
                    self.config.show_duplicates_only = !self.config.show_duplicates_only;
                    self.save_config();
                    self.trigger_scan();
                }
                if toggle_flatten {
                    if !self.config.flatten_subfolders && crate::model::is_drive_root(&self.current_path) {
                        self.show_root_subfolder_confirm = true;
                    } else {
                        self.config.flatten_subfolders = !self.config.flatten_subfolders;
                        self.save_config();
                        self.trigger_scan();
                    }
                }
                if do_refresh {
                    self.trigger_scan();
                }
            });

            ui.add_space(6.0);
        });

        if let Some(drive_path) = target_drive {
            self.navigate_to(drive_path);
        }
    }
}
