use std::path::{Path, PathBuf};
use std::time::Instant;

use eframe::egui::{self, Color32, RichText, ScrollArea};

use crate::model::format_size;
use crate::platform;

pub use super::context_menu::ActiveContextMenu;
use super::FreeDataDietApp;

#[derive(Clone, PartialEq)]
pub enum DeleteConfirmState {
    None,
    Confirm {
        path: PathBuf,
        name: String,
        is_system: bool,
    },
    Batch {
        paths: Vec<PathBuf>,
        total_size: u64,
        has_system: bool,
    },
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum DeleteModalFocus {
    Trash,
    Permanent,
    Cancel,
}

fn delete_path_permanently(path: &Path) -> std::io::Result<()> {
    if crate::model::is_root_or_system(path) || crate::model::is_drive_root(path) {
        return Err(std::io::Error::new(
            std::io::ErrorKind::PermissionDenied,
            "Cannot delete root or critical system folder",
        ));
    }
    if path.is_dir() {
        // Links are removed, never followed. Windows needs rmdir, Unix needs unlink.
        let is_link = std::fs::symlink_metadata(path)
            .map(|m| m.file_type().is_symlink())
            .unwrap_or(false);

        if is_link {
            #[cfg(windows)]
            {
                std::fs::remove_dir(path)
            }
            #[cfg(not(windows))]
            {
                std::fs::remove_file(path)
            }
        } else {
            std::fs::remove_dir_all(path)
        }
    } else {
        std::fs::remove_file(path)
    }
}

impl FreeDataDietApp {
    pub(super) fn render_modals(&mut self, ui: &mut egui::Ui, is_dark: bool) {
        // --- UNIFIED CONTEXT MENU (KEYBOARD & MOUSE RIGHT-CLICK) ---
        self.render_context_menu(ui, is_dark);

        // --- CONFIRMATION DIALOG MODAL ---
        let mut trigger_rescan = false;
        let mut close_modal = false;
        let mut modal_error = None;
        let mut modal_toast = None;

        if self.delete_modal != DeleteConfirmState::None {
            let nav_next = ui.input(|i| {
                i.key_pressed(egui::Key::ArrowRight)
                    || (!i.modifiers.shift && i.key_pressed(egui::Key::Tab))
            });
            let nav_prev = ui.input(|i| {
                i.key_pressed(egui::Key::ArrowLeft)
                    || (i.modifiers.shift && i.key_pressed(egui::Key::Tab))
            });

            if nav_next {
                self.modal_focus = match self.modal_focus {
                    DeleteModalFocus::Trash => DeleteModalFocus::Permanent,
                    DeleteModalFocus::Permanent => DeleteModalFocus::Cancel,
                    DeleteModalFocus::Cancel => DeleteModalFocus::Trash,
                };
            } else if nav_prev {
                self.modal_focus = match self.modal_focus {
                    DeleteModalFocus::Trash => DeleteModalFocus::Cancel,
                    DeleteModalFocus::Permanent => DeleteModalFocus::Trash,
                    DeleteModalFocus::Cancel => DeleteModalFocus::Permanent,
                };
            }

            let modal_enter = ui.input(|i| i.key_pressed(egui::Key::Enter) || i.key_pressed(egui::Key::Space));
            let modal_escape = ui.input(|i| i.key_pressed(egui::Key::Escape));

            let del_blocked = match &self.delete_modal {
                DeleteConfirmState::Confirm { path, is_system, .. } => {
                    *is_system || crate::model::is_root_or_system(path)
                }
                DeleteConfirmState::Batch { paths, has_system, .. } => {
                    *has_system || paths.iter().any(|p| crate::model::is_root_or_system(p))
                }
                DeleteConfirmState::None => false,
            };

            if del_blocked {
                self.modal_focus = DeleteModalFocus::Cancel;
            }

            if modal_escape {
                close_modal = true;
            }

            let prompt_text = match &self.delete_modal {
                DeleteConfirmState::Confirm { name, .. } => {
                    format!("Do you want to delete \"{}\"?", name)
                }
                DeleteConfirmState::Batch { paths, total_size, .. } => {
                    format!(
                        "Do you want to delete {} selected items ({})?",
                        paths.len(),
                        format_size(*total_size)
                    )
                }
                DeleteConfirmState::None => String::new(),
            };

            let state_clone = self.delete_modal.clone();

            egui::Window::new("Confirm Deletion")
                .collapsible(false)
                .resizable(false)
                .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
                .show(ui.ctx(), |ui| {
                    if del_blocked {
                        ui.label(RichText::new("[!] Protected system path or drive root: deletion is disabled for safety.").color(Color32::RED).strong());
                        ui.add_space(4.0);
                    }
                    ui.label(RichText::new(prompt_text).size(14.0));
                    ui.add_space(10.0);

                    ui.horizontal(|ui| {
                        let inactive_bg = ui.visuals().widgets.inactive.bg_fill;
                        // 1. Move to Trash (default selection when allowed)
                        let is_trash = self.modal_focus == DeleteModalFocus::Trash;
                        let trash_text = if is_trash {
                            RichText::new("> Move to Trash <").color(Color32::WHITE).strong()
                        } else {
                            RichText::new("Move to Trash")
                        };
                        let trash_btn = egui::Button::new(trash_text)
                            .fill(if is_trash { Color32::from_rgb(30, 110, 200) } else { inactive_bg });

                        // 2. Delete Permanently
                        let is_perm = self.modal_focus == DeleteModalFocus::Permanent;
                        let perm_text = if is_perm {
                            RichText::new("> Delete Permanently <").color(Color32::WHITE).strong()
                        } else {
                            RichText::new("Delete Permanently").color(Color32::from_rgb(255, 120, 120))
                        };
                        let perm_btn = egui::Button::new(perm_text)
                            .fill(if is_perm { Color32::from_rgb(180, 40, 40) } else { inactive_bg });

                        // 3. Cancel
                        let is_cancel = self.modal_focus == DeleteModalFocus::Cancel;
                        let cancel_text = if is_cancel {
                            RichText::new("> Cancel <").color(Color32::WHITE).strong()
                        } else {
                            RichText::new("Cancel")
                        };
                        let cancel_btn = egui::Button::new(cancel_text)
                            .fill(if is_cancel { Color32::from_rgb(90, 90, 90) } else { inactive_bg });

                        let do_trash = ui.add_enabled(!del_blocked, trash_btn).clicked() || (is_trash && modal_enter && !del_blocked);
                        let do_perm = ui.add_enabled(!del_blocked, perm_btn).clicked() || (is_perm && modal_enter && !del_blocked);
                        let do_cancel = ui.add(cancel_btn).clicked() || (is_cancel && modal_enter);

                        let mut deleted_paths: Vec<PathBuf> = Vec::new();

                        if do_trash {
                            match &state_clone {
                                DeleteConfirmState::Confirm { path, .. } => {
                                    if let Err(e) = trash::delete(path) {
                                        modal_error = Some(format!("Trash Error: {}", e));
                                    } else {
                                        deleted_paths.push(path.clone());
                                        modal_toast = Some("Moved item to Trash".to_string());
                                    }
                                }
                                DeleteConfirmState::Batch { paths, .. } => {
                                    for p in paths {
                                        if !crate::model::is_root_or_system(p) && trash::delete(p).is_ok() {
                                            deleted_paths.push(p.clone());
                                        }
                                    }
                                    modal_toast = Some(format!("Moved {} items to Trash", deleted_paths.len()));
                                }
                                DeleteConfirmState::None => {}
                            }
                            close_modal = true;
                        } else if do_perm {
                            match &state_clone {
                                DeleteConfirmState::Confirm { path, .. } => {
                                    if let Err(e) = delete_path_permanently(path) {
                                        modal_error = Some(format!("Delete Error: {}", e));
                                    } else {
                                        deleted_paths.push(path.clone());
                                        modal_toast = Some("Permanently deleted item".to_string());
                                    }
                                }
                                DeleteConfirmState::Batch { paths, .. } => {
                                    for p in paths {
                                        if !crate::model::is_root_or_system(p) && delete_path_permanently(p).is_ok() {
                                            deleted_paths.push(p.clone());
                                        }
                                    }
                                    modal_toast = Some(format!("Permanently deleted {} items", deleted_paths.len()));
                                }
                                DeleteConfirmState::None => {}
                            }
                            close_modal = true;
                        } else if do_cancel {
                            close_modal = true;
                        }

                        if !deleted_paths.is_empty() {
                            if self.config.show_duplicates_only {
                                let del_set: std::collections::HashSet<PathBuf> = deleted_paths.iter().cloned().collect();
                                let mut candidate_parents: std::collections::HashSet<PathBuf> = std::collections::HashSet::new();
                                for p in &deleted_paths {
                                    if let Some(parent) = p.parent() {
                                        candidate_parents.insert(parent.to_path_buf());
                                    }
                                }

                                for cluster in &mut self.duplicate_clusters {
                                    cluster.occurrences.retain(|o| !del_set.contains(&o.path));
                                }
                                self.duplicate_clusters.retain(|c| c.occurrences.len() >= 2);
                                self.entries.retain(|e| !del_set.contains(&e.path));
                                self.recompute_duplicate_stats();
                                self.update_duplicate_assistant();
                                self.visible_indices_dirty = true;
                                self.duplicate_flat_rows_dirty = true;

                                let mut empty_parents: Vec<PathBuf> = Vec::new();
                                for parent in candidate_parents {
                                    if parent.exists()
                                        && !crate::model::is_root_or_system(&parent)
                                        && !crate::model::is_drive_root(&parent)
                                        && std::fs::read_dir(&parent).map(|mut d| d.next().is_none()).unwrap_or(false)
                                    {
                                        empty_parents.push(parent);
                                    }
                                }
                                if !empty_parents.is_empty() {
                                    self.empty_folders_prompt = Some(empty_parents);
                                }
                            } else {
                                trigger_rescan = true;
                            }
                        }
                    });
                });
        }

        if let Some(err) = modal_error {
            self.error_message = Some(err);
        }
        if let Some(t) = modal_toast {
            self.status_toast = Some((t, Instant::now()));
        }
        if close_modal {
            self.delete_modal = DeleteConfirmState::None;
        }
        if trigger_rescan {
            self.trigger_scan();
        }

        // --- EMPTY FOLDER CLEANUP MODAL ---
        if let Some(empty_folders) = &self.empty_folders_prompt.clone() {
            let mut close_empty_modal = false;
            let mut delete_empty = false;
            let mut open_empty: Option<PathBuf> = None;

            egui::Window::new("Clean Up Empty Folders")
                .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
                .collapsible(false)
                .resizable(false)
                .order(egui::Order::Foreground)
                .show(ui.ctx(), |ui| {
                    ui.set_width(420.0);
                    ui.add_space(4.0);
                    ui.label(
                        RichText::new("The following folder is now empty after deleting duplicate files:")
                            .strong(),
                    );
                    ui.add_space(6.0);

                    ScrollArea::vertical()
                        .max_height(180.0)
                        .show(ui, |ui| {
                            for p in empty_folders {
                                ui.horizontal(|ui| {
                                    ui.add(egui::Label::new(RichText::new(p.to_string_lossy()).small()).truncate());
                                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                                        if ui.small_button("Open").on_hover_text("Open folder in file explorer").clicked() {
                                            open_empty = Some(p.clone());
                                        }
                                    });
                                });
                            }
                        });

                    ui.add_space(10.0);
                    ui.horizontal(|ui| {
                        if ui
                            .button(RichText::new("Delete Empty Folder(s)").color(Color32::from_rgb(255, 120, 120)))
                            .clicked()
                        {
                            delete_empty = true;
                        }
                        if ui.button("Keep Folder(s)").clicked() {
                            close_empty_modal = true;
                        }
                    });
                    ui.add_space(4.0);
                });

            if let Some(target) = open_empty {
                let _ = platform::open_in_file_manager(&target);
            }
            if delete_empty {
                let mut deleted_count = 0usize;
                for p in empty_folders {
                    if !crate::model::is_root_or_system(p) && !crate::model::is_drive_root(p)
                        && (std::fs::remove_dir(p).is_ok() || trash::delete(p).is_ok()) {
                            deleted_count += 1;
                        }
                }
                self.status_toast = Some((
                    format!("Cleaned up {} empty folder(s)", deleted_count),
                    Instant::now(),
                ));
                close_empty_modal = true;
            }
            if close_empty_modal {
                self.empty_folders_prompt = None;
            }
        }

        // --- KEYBOARD SHORTCUTS POPUP (FROM 9d69210) ---
        if self.show_shortcuts_modal {
            self.render_shortcuts_modal(ui, is_dark);
        }

        // --- UPDATE MODAL ---
        if self.show_update_modal {
            let mut close_modal = ui.input(|i| i.key_pressed(egui::Key::Escape));
            let status = self.updater.status();

            if let crate::updater::UpdateStatus::Available(ref update) = status {
                let win = egui::Window::new("FreeDataDiet Update Available")
                    .collapsible(false)
                    .resizable(false)
                    .anchor(egui::Align2::CENTER_CENTER, egui::vec2(0.0, 0.0))
                    .fixed_size(egui::vec2(500.0, 340.0));

                let win_resp = win.show(ui.ctx(), |ui| {
                    ui.spacing_mut().item_spacing = egui::vec2(8.0, 10.0);

                    ui.horizontal(|ui| {
                        ui.heading(format!("Version v{} is available!", update.latest_version));
                        ui.label(RichText::new(format!("(current: v{})", update.current_version)).weak());
                    });

                    ui.separator();

                    ui.label(RichText::new("What's New:").strong());
                    ScrollArea::vertical().max_height(140.0).show(ui, |ui| {
                        if update.release_notes.trim().is_empty() {
                            ui.label("Bug fixes and performance improvements.");
                        } else {
                            ui.label(&update.release_notes);
                        }
                    });

                    ui.separator();

                    ui.horizontal(|ui| {
                        if update.download_url.is_some()
                            && ui.button(RichText::new("Download & Restart").strong().color(Color32::from_rgb(60, 220, 120))).clicked() {
                                self.updater.start_download((**update).clone());
                                close_modal = true;
                            }

                        if ui.button("Open GitHub Release").clicked() {
                            ui.ctx().open_url(egui::OpenUrl::new_tab(&update.release_url));
                        }

                        if ui.button("Close").clicked() {
                            close_modal = true;
                        }
                    });

                    ui.add_space(4.0);
                    ui.collapsing("Package Manager update instructions", |ui| {
                        ui.label("- Ubuntu/Debian:  sudo apt update && sudo apt install --only-upgrade freedatadiet");
                        ui.label("- Arch Linux:     yay -S freedatadiet-bin");
                        ui.label("- Windows Winget: winget upgrade freedatadiet");
                    });
                });

                if let Some(resp) = win_resp
                    && ui.input(|i| i.pointer.any_pressed())
                        && let Some(pos) = ui.input(|i| i.pointer.interact_pos())
                            && !resp.response.rect.contains(pos) {
                                close_modal = true;
                            }
            } else {
                close_modal = true;
            }

            if close_modal {
                self.show_update_modal = false;
            }
        }

        // --- WHOLE-DRIVE INCLUDE SUBFOLDERS CONFIRMATION MODAL ---
        if self.show_root_subfolder_confirm {
            let mut close_confirm = ui.input(|i| i.key_pressed(egui::Key::Escape));
            let mut do_proceed = false;

            egui::Window::new("Include Subfolders on Drive Root")
                .collapsible(false)
                .resizable(false)
                .anchor(egui::Align2::CENTER_CENTER, egui::vec2(0.0, 0.0))
                .show(ui.ctx(), |ui| {
                    ui.set_max_width(450.0);
                    ui.label(RichText::new("Whole-Drive Recursive Scan Warning").heading().color(Color32::from_rgb(255, 180, 60)));
                    ui.add_space(4.0);
                    ui.separator();
                    ui.add_space(4.0);
                    ui.label(RichText::new(format!(
                        "Scanning entire drive \"{}\" with subfolders enabled will analyze millions of files across your entire storage volume and can take several minutes.",
                        self.current_path.display()
                    )).size(13.5));
                    ui.add_space(4.0);
                    ui.label(RichText::new("Do you want to proceed with scanning the entire drive?").strong().small());
                    ui.add_space(10.0);
                    ui.horizontal(|ui| {
                        let btn_proceed = egui::Button::new(
                            RichText::new("Proceed - Scan Entire Drive").color(Color32::WHITE).strong()
                        ).fill(Color32::from_rgb(200, 110, 0));

                        if ui.add(btn_proceed).clicked() {
                            do_proceed = true;
                        }
                        if ui.button("Cancel").clicked() {
                            close_confirm = true;
                        }
                    });
                });

            if do_proceed {
                self.config.flatten_subfolders = true;
                self.save_config();
                self.trigger_scan();
                self.show_root_subfolder_confirm = false;
            } else if close_confirm {
                self.show_root_subfolder_confirm = false;
            }
        }
    }
}
