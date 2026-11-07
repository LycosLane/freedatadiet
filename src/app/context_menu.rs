use std::path::{Path, PathBuf};
use std::time::Instant;

use eframe::egui::{self, Color32, RichText};

use crate::config::{ExcludeScope, ExcludedLocation};
use crate::locations::LocationManager;
use crate::platform;

use super::modals::{DeleteConfirmState, DeleteModalFocus};
use super::FreeDataDietApp;

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

impl FreeDataDietApp {
    /// The folder a context menu entry refers to (the entry itself for folders, its parent for files).
    pub(super) fn context_target_folder(menu: &ActiveContextMenu) -> PathBuf {
        if menu.is_dir {
            menu.path.clone()
        } else {
            menu.path.parent().map(Path::to_path_buf).unwrap_or_else(|| menu.path.clone())
        }
    }

    /// Adds the menu's folder to the permanent exclude list (once per scope), saves the
    /// config and returns the folder's display name.
    fn add_excluded_location(&mut self, menu: &ActiveContextMenu, scope: ExcludeScope) -> String {
        let path = Self::context_target_folder(menu);
        let name = path
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_else(|| path.to_string_lossy().into_owned());
        let exists = self.config.excluded_locations.iter().any(|l| l.path == path && l.scope == scope);
        if !exists {
            let millis = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_millis())
                .unwrap_or(0);
            self.config.excluded_locations.push(ExcludedLocation {
                id: format!("excl_{millis}"),
                name: name.clone(),
                path,
                scope,
                enabled: true,
            });
            self.save_config();
        }
        name
    }

    /// Renders the unified popup context menu (via mouse right-click or keyboard Menu/F10).
    pub(super) fn render_context_menu(&mut self, ui: &mut egui::Ui, is_dark: bool) {
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

            if let Some(wresp) = resp
                && !menu.just_opened && ui.input(|i| i.pointer.any_pressed())
                    && let Some(pos) = ui.input(|i| i.pointer.interact_pos())
                        && !wresp.response.rect.contains(pos) {
                            close_menu = true;
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
                        let folder_name = self.add_excluded_location(&menu, ExcludeScope::IgnoreForDuplicates);
                        // Drop the folder's copies from the current results and rebuild the row cache.
                        let target = Self::context_target_folder(&menu);
                        for cluster in &mut self.duplicate_clusters {
                            cluster.occurrences.retain(|o| !o.path.starts_with(&target));
                        }
                        self.duplicate_clusters.retain(|c| c.occurrences.len() >= 2);
                        self.duplicate_flat_rows_dirty = true;
                        self.recompute_duplicate_stats();
                        self.status_toast = Some((
                            format!("Permanently ignored folder for duplicates: {folder_name}"),
                            Instant::now(),
                        ));
                    }
                    ContextAction::HideFolderFromScan => {
                        let folder_name = self.add_excluded_location(&menu, ExcludeScope::HideFromScan);
                        self.status_toast = Some((
                            format!("Folder added to scan exclusions: {folder_name}"),
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
    }
}
