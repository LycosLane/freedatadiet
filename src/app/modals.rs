use std::path::{Path, PathBuf};
use std::time::Instant;

use eframe::egui::{self, Color32, RichText, ScrollArea};

use crate::locations::LocationManager;
use crate::model::format_size;
use crate::platform;

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

#[derive(Clone)]
pub struct ActiveContextMenu {
    pub path: PathBuf,
    pub name: String,
    pub is_dir: bool,
    pub is_system_protected: bool,
    pub pos: Option<egui::Pos2>,
    pub selected_idx: usize,
    pub just_opened: bool,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum ContextAction {
    Open,
    ShowInExplorer,
    ToggleFavorite,
    CopyName,
    CopyFolderPath,
    CopyFullPath,
    #[cfg(windows)]
    Properties,
    MoveToTrash,
    DeletePermanently,
    IgnoreFolderForDuplicates,
    HideFolderFromScan,
}

struct ContextMenuItem {
    action: ContextAction,
    label: String,
    color: Option<Color32>,
}

fn get_context_menu_items(entry: &ActiveContextMenu, is_favorite: bool) -> Vec<ContextMenuItem> {
    let mut items = Vec::new();
    items.push(ContextMenuItem {
        action: ContextAction::Open,
        label: "Open".to_string(),
        color: None,
    });
    items.push(ContextMenuItem {
        action: ContextAction::ShowInExplorer,
        label: if cfg!(windows) { "Show in Explorer" } else { "Show in File Manager" }.to_string(),
        color: None,
    });
    if entry.is_dir {
        items.push(ContextMenuItem {
            action: ContextAction::ToggleFavorite,
            label: if is_favorite {
                "Remove from Favorites".to_string()
            } else {
                "Add to Favorites (Ctrl+B)".to_string()
            },
            color: None,
        });
    }
    items.push(ContextMenuItem {
        action: ContextAction::CopyName,
        label: if entry.is_dir { "Copy Folder Name".to_string() } else { "Copy File Name".to_string() },
        color: None,
    });
    items.push(ContextMenuItem {
        action: ContextAction::CopyFolderPath,
        label: "Copy Folder Path".to_string(),
        color: None,
    });
    items.push(ContextMenuItem {
        action: ContextAction::CopyFullPath,
        label: "Copy Full Path".to_string(),
        color: None,
    });
    #[cfg(windows)]
    items.push(ContextMenuItem {
        action: ContextAction::Properties,
        label: "Properties...".to_string(),
        color: None,
    });
    items.push(ContextMenuItem {
        action: ContextAction::MoveToTrash,
        label: if cfg!(windows) { "Move to Recycle Bin" } else { "Move to Trash" }.to_string(),
        color: Some(Color32::from_rgb(255, 200, 100)),
    });
    items.push(ContextMenuItem {
        action: ContextAction::DeletePermanently,
        label: "Delete Permanently".to_string(),
        color: Some(Color32::from_rgb(255, 100, 100)),
    });
    items.push(ContextMenuItem {
        action: ContextAction::IgnoreFolderForDuplicates,
        label: "Ignore Folder for Duplicates (Permanent)".to_string(),
        color: Some(Color32::from_rgb(255, 175, 60)),
    });
    items.push(ContextMenuItem {
        action: ContextAction::HideFolderFromScan,
        label: "Hide Folder from Scan (Permanent)".to_string(),
        color: Some(Color32::from_rgb(160, 200, 255)),
    });
    items
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
        if let Some(mut menu) = self.active_context_menu.take() {
            let is_fav = self.config.saved_locations.iter().any(|l| l.path == menu.path);
            let items = get_context_menu_items(&menu, is_fav);
            let total_items = items.len();

            let nav_down = ui.input(|i| {
                i.key_pressed(egui::Key::ArrowDown)
                    || (!i.modifiers.shift && i.key_pressed(egui::Key::Tab))
            });
            let nav_up = ui.input(|i| {
                i.key_pressed(egui::Key::ArrowUp)
                    || (i.modifiers.shift && i.key_pressed(egui::Key::Tab))
            });

            if nav_down {
                menu.selected_idx = (menu.selected_idx + 1) % total_items;
            } else if nav_up {
                menu.selected_idx = (menu.selected_idx + total_items - 1) % total_items;
            }

            let enter_pressed = ui.input(|i| i.key_pressed(egui::Key::Enter) || i.key_pressed(egui::Key::Space));
            let esc_pressed = ui.input(|i| i.key_pressed(egui::Key::Escape));

            let mut close_menu = esc_pressed;
            let mut triggered_action = None;

            let window = egui::Window::new("context_menu_popup")
                .title_bar(false)
                .resizable(false)
                .collapsible(false);

            let window = if let Some(pos) = menu.pos {
                window.fixed_pos(pos)
            } else {
                window.anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
            };

            let resp = window.show(ui.ctx(), |ui| {
                ui.set_min_width(240.0);
                ui.add_space(4.0);
                ui.label(RichText::new(&menu.name).strong().color(if is_dark { Color32::WHITE } else { Color32::from_rgb(20, 25, 35) }));
                ui.label(RichText::new(menu.path.to_string_lossy().as_ref()).weak().small());
                ui.add_space(2.0);
                ui.separator();

                for (idx, item) in items.iter().enumerate() {
                    if item.action == ContextAction::CopyName
                        || item.action == ContextAction::MoveToTrash
                        || item.action == ContextAction::IgnoreFolderForDuplicates
                    {
                        ui.separator();
                    }

                    let is_selected = menu.selected_idx == idx;
                    let (rect, resp) = ui.allocate_exact_size(egui::vec2(ui.available_width(), 22.0), egui::Sense::click());

                    if resp.hovered() && !is_selected {
                        menu.selected_idx = idx;
                    }

                    if is_selected {
                        ui.painter().rect_filled(rect, 3.0, Color32::from_rgb(30, 100, 180));
                    } else if resp.hovered() {
                        ui.painter().rect_filled(rect, 3.0, if is_dark { Color32::from_rgb(45, 52, 65) } else { Color32::from_rgb(230, 238, 248) });
                    }

                    let text_color = if is_selected {
                        Color32::WHITE
                    } else if let Some(color) = item.color {
                        color
                    } else if is_dark {
                        Color32::from_rgb(220, 225, 230)
                    } else {
                        Color32::from_rgb(30, 35, 45)
                    };

                    ui.painter().text(
                        egui::pos2(rect.min.x + 8.0, rect.center().y),
                        egui::Align2::LEFT_CENTER,
                        &item.label,
                        egui::FontId::proportional(12.5),
                        text_color,
                    );

                    if resp.clicked() || (is_selected && enter_pressed) {
                        triggered_action = Some(item.action);
                        close_menu = true;
                    }
                }
                ui.add_space(4.0);
            });

            if let Some(wresp) = resp {
                if !menu.just_opened && ui.input(|i| i.pointer.any_pressed()) {
                    if let Some(pos) = ui.input(|i| i.pointer.interact_pos()) {
                        if !wresp.response.rect.contains(pos) {
                            close_menu = true;
                        }
                    }
                }
            }

            if let Some(action) = triggered_action {
                match action {
                    ContextAction::Open => {
                        if menu.is_dir {
                            self.navigate_to(menu.path.clone());
                        } else {
                            let _ = platform::open_file(&menu.path);
                        }
                    }
                    ContextAction::ShowInExplorer => {
                        let _ = platform::open_in_file_manager(&menu.path);
                    }
                    ContextAction::ToggleFavorite => {
                        let added = LocationManager::toggle_favorite(&mut self.config, &menu.path);
                        let msg = if added {
                            "Added folder to Favorites"
                        } else {
                            "Removed folder from Favorites"
                        };
                        self.status_toast = Some((msg.to_string(), Instant::now()));
                    }
                    ContextAction::CopyName => {
                        let text = menu.name.clone();
                        platform::set_clipboard_text(&text);
                        ui.ctx().copy_text(text.clone());
                        self.status_toast = Some((format!("Copied name: {}", text), Instant::now()));
                    }
                    ContextAction::CopyFolderPath => {
                        let text = menu.path.parent()
                            .map(|p| p.to_string_lossy().to_string())
                            .unwrap_or_else(|| menu.path.to_string_lossy().to_string());
                        platform::set_clipboard_text(&text);
                        ui.ctx().copy_text(text.clone());
                        self.status_toast = Some((format!("Copied folder path: {}", text), Instant::now()));
                    }
                    ContextAction::CopyFullPath => {
                        let text = menu.path.to_string_lossy().to_string();
                        platform::set_clipboard_text(&text);
                        ui.ctx().copy_text(text.clone());
                        self.status_toast = Some((format!("Copied full path: {}", text), Instant::now()));
                    }
                    #[cfg(windows)]
                    ContextAction::Properties => {
                        let _ = platform::show_properties(&menu.path);
                    }
                    ContextAction::MoveToTrash => {
                        self.delete_modal = DeleteConfirmState::Confirm {
                            path: menu.path.clone(),
                            name: menu.name.clone(),
                            is_system: menu.is_system_protected,
                        };
                        self.modal_focus = DeleteModalFocus::Trash;
                    }
                    ContextAction::DeletePermanently => {
                        self.delete_modal = DeleteConfirmState::Confirm {
                            path: menu.path.clone(),
                            name: menu.name.clone(),
                            is_system: menu.is_system_protected,
                        };
                        self.modal_focus = DeleteModalFocus::Permanent;
                    }
                    ContextAction::IgnoreFolderForDuplicates => {
                        let target_folder = if menu.is_dir {
                            menu.path.clone()
                        } else {
                            menu.path.parent().map(|p| p.to_path_buf()).unwrap_or_else(|| menu.path.clone())
                        };
                        let folder_name = target_folder
                            .file_name()
                            .map(|n| n.to_string_lossy().to_string())
                            .unwrap_or_else(|| target_folder.to_string_lossy().to_string());

                        let id = format!(
                            "ign_dup_{}",
                            std::time::SystemTime::now()
                                .duration_since(std::time::UNIX_EPOCH)
                                .map(|d| d.as_millis())
                                .unwrap_or(0)
                        );
                        if !self
                            .config
                            .excluded_locations
                            .iter()
                            .any(|loc| loc.path == target_folder && loc.scope == crate::config::ExcludeScope::IgnoreForDuplicates)
                        {
                            self.config.excluded_locations.push(crate::config::ExcludedLocation {
                                id,
                                name: folder_name.clone(),
                                path: target_folder.clone(),
                                scope: crate::config::ExcludeScope::IgnoreForDuplicates,
                                enabled: true,
                            });
                            self.save_config();
                        }

                        // Remove occurrences from duplicate clusters immediately
                        for cluster in &mut self.duplicate_clusters {
                            cluster.occurrences.retain(|o| !o.path.starts_with(&target_folder));
                        }
                        self.duplicate_clusters.retain(|c| c.occurrences.len() >= 2);
                        self.recompute_duplicate_stats();
                        self.update_duplicate_assistant();
                        self.status_toast = Some((
                            format!("Permanently ignored folder for duplicates: {}", folder_name),
                            Instant::now(),
                        ));
                    }
                    ContextAction::HideFolderFromScan => {
                        let target_folder = if menu.is_dir {
                            menu.path.clone()
                        } else {
                            menu.path.parent().map(|p| p.to_path_buf()).unwrap_or_else(|| menu.path.clone())
                        };
                        let folder_name = target_folder
                            .file_name()
                            .map(|n| n.to_string_lossy().to_string())
                            .unwrap_or_else(|| target_folder.to_string_lossy().to_string());

                        let id = format!(
                            "hide_scan_{}",
                            std::time::SystemTime::now()
                                .duration_since(std::time::UNIX_EPOCH)
                                .map(|d| d.as_millis())
                                .unwrap_or(0)
                        );
                        if !self
                            .config
                            .excluded_locations
                            .iter()
                            .any(|loc| loc.path == target_folder && loc.scope == crate::config::ExcludeScope::HideFromScan)
                        {
                            self.config.excluded_locations.push(crate::config::ExcludedLocation {
                                id,
                                name: folder_name.clone(),
                                path: target_folder.clone(),
                                scope: crate::config::ExcludeScope::HideFromScan,
                                enabled: true,
                            });
                            self.save_config();
                        }
                        self.status_toast = Some((
                            format!("Folder added to scan exclusions: {}", folder_name),
                            Instant::now(),
                        ));
                        self.trigger_scan();
                    }
                }
            }

            if !close_menu {
                menu.just_opened = false;
                self.active_context_menu = Some(menu);
            }
        }

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
                        // 1. Move to Trash (default selection when allowed)
                        let is_trash = self.modal_focus == DeleteModalFocus::Trash;
                        let trash_text = if is_trash {
                            RichText::new("> Move to Trash <").color(Color32::WHITE).strong()
                        } else {
                            RichText::new("Move to Trash")
                        };
                        let trash_btn = egui::Button::new(trash_text)
                            .fill(if is_trash { Color32::from_rgb(30, 110, 200) } else { Color32::from_rgb(50, 50, 50) });

                        // 2. Delete Permanently
                        let is_perm = self.modal_focus == DeleteModalFocus::Permanent;
                        let perm_text = if is_perm {
                            RichText::new("> Delete Permanently <").color(Color32::WHITE).strong()
                        } else {
                            RichText::new("Delete Permanently").color(Color32::from_rgb(255, 120, 120))
                        };
                        let perm_btn = egui::Button::new(perm_text)
                            .fill(if is_perm { Color32::from_rgb(180, 40, 40) } else { Color32::from_rgb(50, 50, 50) });

                        // 3. Cancel
                        let is_cancel = self.modal_focus == DeleteModalFocus::Cancel;
                        let cancel_text = if is_cancel {
                            RichText::new("> Cancel <").color(Color32::WHITE).strong()
                        } else {
                            RichText::new("Cancel")
                        };
                        let cancel_btn = egui::Button::new(cancel_text)
                            .fill(if is_cancel { Color32::from_rgb(90, 90, 90) } else { Color32::from_rgb(50, 50, 50) });

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

                    for p in empty_folders {
                        ui.horizontal(|ui| {
                            ui.label(RichText::new(p.to_string_lossy()).small());
                            if ui.small_button("Open").on_hover_text("Open folder in file explorer").clicked() {
                                open_empty = Some(p.clone());
                            }
                        });
                    }

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
                    if !crate::model::is_root_or_system(p) && !crate::model::is_drive_root(p) {
                        if std::fs::remove_dir(p).is_ok() || trash::delete(p).is_ok() {
                            deleted_count += 1;
                        }
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
                        if update.download_url.is_some() {
                            if ui.button(RichText::new("Download & Restart").strong().color(Color32::from_rgb(60, 220, 120))).clicked() {
                                self.updater.start_download((**update).clone());
                                close_modal = true;
                            }
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

                if let Some(resp) = win_resp {
                    if ui.input(|i| i.pointer.any_pressed()) {
                        if let Some(pos) = ui.input(|i| i.pointer.interact_pos()) {
                            if !resp.response.rect.contains(pos) {
                                close_modal = true;
                            }
                        }
                    }
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
