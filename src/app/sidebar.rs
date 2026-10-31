use std::path::PathBuf;
use std::time::Instant;

use eframe::egui::{self, Color32, Rect, RichText, ScrollArea, Sense, Stroke, Vec2};

use crate::config::{AppConfig, AppTheme, QuickSearchMode, StartupLocationMode, UsageBasis};
use crate::locations::LocationManager;
use crate::model::format_size;

use super::{ActivePanelFocus, FreeDataDietApp};

fn setting_pill(
    ui: &mut egui::Ui,
    is_active: bool,
    is_focused: bool,
    label: &str,
    shortcut: Option<&str>,
    show_shortcuts: bool,
    is_orange: bool,
    is_dark: bool,
) -> egui::Response {
    let active_bg = if is_orange {
        Color32::from_rgb(235, 115, 15)
    } else {
        Color32::from_rgb(30, 120, 220)
    };
    let inactive_bg = if is_dark {
        Color32::from_gray(38)
    } else {
        Color32::from_gray(230)
    };

    let bg_color = if is_active { active_bg } else { inactive_bg };
    let stroke = if is_focused {
        Stroke::new(1.5, Color32::from_rgb(100, 185, 255))
    } else if is_active {
        Stroke::new(1.0, active_bg)
    } else {
        Stroke::new(1.0, if is_dark { Color32::from_gray(55) } else { Color32::from_gray(205) })
    };

    let text_color = if is_active {
        Color32::WHITE
    } else if is_dark {
        Color32::from_gray(215)
    } else {
        Color32::from_gray(35)
    };

    let frame_resp = egui::Frame::new()
        .fill(bg_color)
        .stroke(stroke)
        .corner_radius(4.0)
        .inner_margin(egui::Margin::symmetric(8, 5))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.horizontal(|ui| {
                ui.label(RichText::new(label).strong().color(text_color));
                if show_shortcuts {
                    if let Some(sc) = shortcut {
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            let badge_col = if is_active {
                                Color32::from_rgb(230, 240, 255)
                            } else if is_dark {
                                Color32::from_rgb(100, 180, 240)
                            } else {
                                Color32::from_rgb(0, 90, 180)
                            };
                            ui.label(RichText::new(format!("[{}]", sc)).small().monospace().color(badge_col));
                        });
                    }
                }
            });
        })
        .response;

    ui.interact(frame_resp.rect, ui.scope_id().with(label), Sense::click())
        .on_hover_cursor(egui::CursorIcon::PointingHand)
}

impl FreeDataDietApp {
    pub(super) fn render_sidebar(&mut self, ui: &mut egui::Ui, is_dark: bool) {
        ui.set_width(300.0);
        ui.add_space(4.0);

        let is_panel_focused = self.panel_focus == ActivePanelFocus::SettingsPanel;

        ui.horizontal(|ui| {
            ui.label(RichText::new("Settings & Locations").size(14.0).strong());
            if is_panel_focused {
                ui.label(RichText::new("[Focused - Tab to view]").small().color(Color32::from_rgb(100, 185, 255)));
            }
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if ui
                    .small_button("Hide (F12)")
                    .on_hover_text("Collapse settings panel (F12 / F4 / Ctrl+,)")
                    .clicked()
                {
                    self.config.show_sidebar = false;
                    self.save_config();
                }
            });
        });
        ui.add_space(2.0);
        ui.separator();

        let mut nav_target: Option<PathBuf> = None;
        let mut trigger_rescan = false;
        let mut recompute_dups = false;
        let show_sc = self.config.show_shortcuts_in_settings;

        const TOTAL_SETTINGS_ITEMS: usize = 29;

        let nav_down = is_panel_focused && ui.input(|i| i.key_pressed(egui::Key::ArrowDown));
        let nav_up = is_panel_focused && ui.input(|i| i.key_pressed(egui::Key::ArrowUp));
        let key_activate = is_panel_focused && ui.input(|i| i.key_pressed(egui::Key::Enter) || i.key_pressed(egui::Key::Space));

        if nav_down {
            self.settings_nav_idx = (self.settings_nav_idx + 1) % TOTAL_SETTINGS_ITEMS;
        } else if nav_up {
            self.settings_nav_idx = (self.settings_nav_idx + TOTAL_SETTINGS_ITEMS - 1) % TOTAL_SETTINGS_ITEMS;
        }

        ScrollArea::vertical()
            .auto_shrink([false, false])
            .show(ui, |ui| {
                // --- 1. DRIVES ---
                ui.add_space(4.0);
                ui.label(RichText::new("Drives").size(13.5).strong());
                ui.add_space(2.0);

                for drive in &self.drives {
                    let is_current = self.current_path.starts_with(&drive.path);
                    let free_str = format_size(drive.available_space);
                    let total_str = format_size(drive.total_space);
                    let used_ratio = if drive.total_space > 0 {
                        (drive.total_space.saturating_sub(drive.available_space)) as f32
                            / drive.total_space as f32
                    } else {
                        0.0
                    };

                    let card_bg = if is_current {
                        if is_dark {
                            Color32::from_rgb(25, 45, 70)
                        } else {
                            Color32::from_rgb(220, 235, 252)
                        }
                    } else if is_dark {
                        Color32::from_rgb(32, 38, 48)
                    } else {
                        Color32::from_rgb(238, 242, 248)
                    };

                    let card_stroke = if is_current {
                        Stroke::new(1.0, Color32::from_rgb(70, 150, 230))
                    } else {
                        Stroke::new(1.0, if is_dark { Color32::from_rgb(48, 58, 72) } else { Color32::from_rgb(210, 218, 228) })
                    };

                    let resp = egui::Frame::new()
                        .fill(card_bg)
                        .stroke(card_stroke)
                        .corner_radius(3.0)
                        .inner_margin(egui::Margin::symmetric(8, 5))
                        .show(ui, |ui| {
                            ui.set_width(ui.available_width());
                            ui.horizontal(|ui| {
                                let name_col = if is_current {
                                    if is_dark { Color32::from_rgb(120, 215, 255) } else { Color32::from_rgb(0, 90, 180) }
                                } else if is_dark {
                                    Color32::WHITE
                                } else {
                                    Color32::from_rgb(20, 25, 35)
                                };
                                ui.label(RichText::new(&drive.name).strong().color(name_col));
                                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                                    ui.label(RichText::new(format!("{free_str} free of {total_str}")).small().weak());
                                });
                            });
                            ui.add_space(3.0);
                            let bar_h = 4.0;
                            let bar_rect = Rect::from_min_size(ui.cursor().min, Vec2::new(ui.available_width(), bar_h));
                            ui.painter().rect_filled(bar_rect, 2.0, if is_dark { Color32::from_gray(45) } else { Color32::from_gray(210) });
                            let bar_col = if used_ratio > 0.90 {
                                Color32::from_rgb(255, 90, 90)
                            } else if used_ratio > 0.75 {
                                Color32::from_rgb(255, 180, 60)
                            } else {
                                Color32::from_rgb(80, 180, 255)
                            };
                            let fill_w = (bar_rect.width() * used_ratio.clamp(0.0, 1.0)).max(1.0);
                            ui.painter().rect_filled(Rect::from_min_size(bar_rect.min, Vec2::new(fill_w, bar_h)), 2.0, bar_col);
                            ui.advance_cursor_after_rect(bar_rect);
                        })
                        .response;

                    let click = ui
                        .interact(resp.rect, ui.scope_id().with(&drive.path), Sense::click())
                        .on_hover_cursor(egui::CursorIcon::PointingHand)
                        .on_hover_text(format!("Click to scan {}", drive.path.to_string_lossy()));

                    if click.clicked() {
                        nav_target = Some(drive.path.clone());
                    }
                    ui.add_space(3.0);
                }

                ui.add_space(6.0);
                ui.separator();

                // --- 2. FAVORITES (WITH CTRL+CLICK MULTI-SELECT) ---
                ui.horizontal(|ui| {
                    ui.label(RichText::new("Favorites").size(13.5).strong());
                    if self.selected_favorites.len() > 1 {
                        if ui.small_button(format!("Clear ({})", self.selected_favorites.len())).clicked() {
                            self.selected_favorites.clear();
                            trigger_rescan = true;
                        }
                    }
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if ui
                            .small_button("+ Add Current")
                            .on_hover_text("Bookmark current folder to Favorites (Ctrl+B)")
                            .clicked()
                        {
                            LocationManager::add_location(&mut self.config, self.current_path.clone(), None);
                            self.status_toast = Some(("Added current folder to Favorites".to_string(), Instant::now()));
                        }
                    });
                });
                ui.label(RichText::new("Hold Ctrl to select multiple folders to search across").weak().small());
                ui.add_space(2.0);

                let mut remove_fav = None;
                let mut startup_fav = None;
                let mut move_fav = None;

                for (i, loc) in self.config.saved_locations.iter().enumerate() {
                    let is_multi_selected = self.selected_favorites.contains(&loc.path);
                    let is_active = self.current_path == loc.path || is_multi_selected;
                    let is_accessible = self.location_mgr.is_accessible(&loc.path);
                    let is_startup = i == 0 && self.config.startup_mode == StartupLocationMode::FirstFavorite;

                    let row_bg = if is_multi_selected {
                        if is_dark { Color32::from_rgb(20, 60, 110) } else { Color32::from_rgb(190, 220, 255) }
                    } else if is_active {
                        if is_dark { Color32::from_rgb(32, 54, 82) } else { Color32::from_rgb(205, 228, 252) }
                    } else {
                        Color32::TRANSPARENT
                    };

                    let row_resp = egui::Frame::new()
                        .fill(row_bg)
                        .corner_radius(3.0)
                        .inner_margin(egui::Margin::symmetric(6, 3))
                        .show(ui, |ui| {
                            ui.set_width(ui.available_width());
                            ui.horizontal(|ui| {
                                let prefix = if is_multi_selected {
                                    "[V]"
                                } else if is_startup {
                                    "[Startup]"
                                } else {
                                    "*"
                                };
                                ui.label(RichText::new(prefix).monospace().small().color(if is_multi_selected {
                                    Color32::from_rgb(100, 200, 255)
                                } else if is_startup {
                                    Color32::from_rgb(255, 200, 80)
                                } else {
                                    Color32::GRAY
                                }));
                                let col = if !is_accessible {
                                    Color32::from_rgb(200, 100, 100)
                                } else if is_active {
                                    if is_dark { Color32::from_rgb(120, 215, 255) } else { Color32::from_rgb(0, 90, 180) }
                                } else if is_dark {
                                    Color32::LIGHT_GRAY
                                } else {
                                    Color32::from_rgb(20, 25, 35)
                                };
                                ui.label(RichText::new(&loc.name).strong().color(col));
                                if !is_accessible {
                                    ui.label(RichText::new("[Offline]").small().color(Color32::from_rgb(220, 100, 100)));
                                }
                            });
                        })
                        .response;

                    let fav_click = ui
                        .interact(row_resp.rect, ui.scope_id().with(&loc.path), Sense::click())
                        .on_hover_cursor(egui::CursorIcon::PointingHand)
                        .on_hover_text(format!("{} (Ctrl+click to multi-select, Right-click for options)", loc.path.to_string_lossy()));

                    if fav_click.clicked() && is_accessible {
                        let ctrl = ui.input(|inp| inp.modifiers.ctrl || inp.modifiers.command);
                        if ctrl {
                            if self.selected_favorites.contains(&loc.path) {
                                self.selected_favorites.remove(&loc.path);
                            } else {
                                self.selected_favorites.insert(loc.path.clone());
                            }
                            if self.selected_favorites.is_empty() {
                                self.selected_favorites.insert(self.current_path.clone());
                            }
                            trigger_rescan = true;
                        } else {
                            self.selected_favorites.clear();
                            self.selected_favorites.insert(loc.path.clone());
                            nav_target = Some(loc.path.clone());
                        }
                    }

                    fav_click.context_menu(|ui| {
                        if ui.button("Set as Startup Folder").clicked() {
                            startup_fav = Some(i);
                            ui.close();
                        }
                        if i > 0 && ui.button("Move Up").clicked() {
                            move_fav = Some((i, true));
                            ui.close();
                        }
                        if i + 1 < self.config.saved_locations.len() && ui.button("Move Down").clicked() {
                            move_fav = Some((i, false));
                            ui.close();
                        }
                        ui.separator();
                        if ui.button("Remove from Favorites").clicked() {
                            remove_fav = Some(loc.path.clone());
                            ui.close();
                        }
                    });
                }

                if let Some(idx) = startup_fav {
                    LocationManager::set_as_startup(&mut self.config, idx);
                }
                if let Some((idx, up)) = move_fav {
                    LocationManager::move_location(&mut self.config, idx, up);
                }
                if let Some(p) = remove_fav {
                    LocationManager::remove_location(&mut self.config, &p);
                }

                ui.add_space(6.0);
                ui.separator();

                // --- 3. INCLUDE SUBFOLDERS ---
                ui.label(RichText::new("Include Subfolders").size(13.5).strong());
                let is_f_0 = is_panel_focused && self.settings_nav_idx == 0;
                let pill_flatten = setting_pill(
                    ui,
                    self.config.flatten_subfolders,
                    is_f_0,
                    "Include Subfolders",
                    Some("F6"),
                    show_sc,
                    false,
                    is_dark,
                ).on_hover_text("Show all files and folders from current folder and all subfolders in one flat list");

                if pill_flatten.clicked() || (is_f_0 && key_activate) {
                    if !self.config.flatten_subfolders && crate::model::is_drive_root(&self.current_path) {
                        self.show_root_subfolder_confirm = true;
                    } else {
                        self.config.flatten_subfolders = !self.config.flatten_subfolders;
                        self.save_config();
                        trigger_rescan = true;
                    }
                }

                ui.add_space(6.0);
                ui.separator();

                // --- 4. FIND DUPLICATES (PROMINENT ORANGE THEME) ---
                ui.horizontal(|ui| {
                    ui.label(RichText::new("Duplicate Search").size(13.5).strong().color(Color32::from_rgb(255, 140, 0)));
                });
                let is_f_1 = is_panel_focused && self.settings_nav_idx == 1;
                let pill_dup = setting_pill(
                    ui,
                    self.config.show_duplicates_only,
                    is_f_1,
                    "Find Duplicates View",
                    Some("Ctrl+D"),
                    show_sc,
                    true,
                    is_dark,
                ).on_hover_text("Group duplicate files across folders by size, name, timestamp, or SHA-256 hash");

                if pill_dup.clicked() || (is_f_1 && key_activate) {
                    self.config.show_duplicates_only = !self.config.show_duplicates_only;
                    self.save_config();
                    trigger_rescan = true;
                }

                ui.add_space(4.0);
                ui.label(RichText::new("Duplicate Matching Criteria:").weak().small());

                let mut crit_changed = false;
                let criteria_items = [
                    (2, "Match same file size", self.config.duplicate_criteria.match_size),
                    (3, "Match same file name", self.config.duplicate_criteria.match_name),
                    (4, "Match modification timestamp", self.config.duplicate_criteria.match_timestamp),
                    (5, "Match bit-exact SHA-256 hash", self.config.duplicate_criteria.verify_hash),
                ];

                for (idx, label, val) in criteria_items {
                    let is_f = is_panel_focused && self.settings_nav_idx == idx;
                    let pill = setting_pill(ui, val, is_f, label, None, false, false, is_dark);
                    if pill.clicked() || (is_f && key_activate) {
                        match idx {
                            2 => self.config.duplicate_criteria.match_size = !val,
                            3 => self.config.duplicate_criteria.match_name = !val,
                            4 => self.config.duplicate_criteria.match_timestamp = !val,
                            5 => self.config.duplicate_criteria.verify_hash = !val,
                            _ => {}
                        }
                        crit_changed = true;
                    }
                }

                if crit_changed {
                    self.save_config();
                    if self.config.show_duplicates_only {
                        recompute_dups = true;
                    }
                }

                ui.add_space(6.0);
                ui.separator();

                // --- 5. CATEGORY FILTERS (WHAT'S SHOWN) ---
                ui.horizontal(|ui| {
                    ui.label(RichText::new("Category Filter (What's Shown)").size(13.5).strong());
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if ui.small_button("All").clicked() {
                            self.config.category_filters.show_images = true;
                            self.config.category_filters.show_videos = true;
                            self.config.category_filters.show_audio = true;
                            self.config.category_filters.show_documents = true;
                            self.config.category_filters.show_archives = true;
                            self.save_config();
                        }
                    });
                });
                ui.add_space(2.0);

                let mut cat_changed = false;
                let cat_items = [
                    (6, "Images", self.config.category_filters.show_images),
                    (7, "Videos", self.config.category_filters.show_videos),
                    (8, "Audio", self.config.category_filters.show_audio),
                    (9, "Documents", self.config.category_filters.show_documents),
                    (10, "Archives", self.config.category_filters.show_archives),
                ];

                for (idx, label, val) in cat_items {
                    let is_f = is_panel_focused && self.settings_nav_idx == idx;
                    let pill = setting_pill(ui, val, is_f, label, None, false, false, is_dark);
                    if pill.clicked() || (is_f && key_activate) {
                        match idx {
                            6 => self.config.category_filters.show_images = !val,
                            7 => self.config.category_filters.show_videos = !val,
                            8 => self.config.category_filters.show_audio = !val,
                            9 => self.config.category_filters.show_documents = !val,
                            10 => self.config.category_filters.show_archives = !val,
                            _ => {}
                        }
                        cat_changed = true;
                    }
                }

                if cat_changed {
                    self.save_config();
                }

                ui.add_space(6.0);
                ui.separator();

                // --- 6. THEME & APPEARANCE ---
                ui.label(RichText::new("Theme & Appearance").size(13.5).strong());
                let theme_items = [
                    (11, AppTheme::Dark, "Dark Mode"),
                    (12, AppTheme::Light, "Light Mode"),
                    (13, AppTheme::System, "System (Auto)"),
                ];

                for (idx, theme_opt, label) in theme_items {
                    let is_active = self.config.theme == theme_opt;
                    let is_f = is_panel_focused && self.settings_nav_idx == idx;
                    let pill = setting_pill(ui, is_active, is_f, label, None, false, false, is_dark);
                    if pill.clicked() || (is_f && key_activate) {
                        self.config.theme = theme_opt;
                        self.save_config();
                        ui.ctx().request_repaint();
                    }
                }

                ui.add_space(4.0);
                ui.horizontal(|ui| {
                    ui.label("UI Scale:");
                    let mut scale = self.config.ui_scale;
                    if ui.add(egui::Slider::new(&mut scale, 0.8..=1.5).step_by(0.05)).changed() {
                        self.config.ui_scale = scale;
                        ui.ctx().set_pixels_per_point(scale);
                        self.save_config();
                    }
                    if ui.small_button("100%").clicked() {
                        self.config.ui_scale = 1.0;
                        ui.ctx().set_pixels_per_point(1.0);
                        self.save_config();
                    }
                });

                ui.add_space(6.0);
                ui.separator();

                // --- 7. USAGE BAR SCALING ---
                ui.label(RichText::new("Usage Bar Scaling").size(13.5).strong());
                ui.label(RichText::new("Scale usage bars relative to:").weak().small());
                ui.add_space(2.0);

                let basis_items = [
                    (14, UsageBasis::LargestItem, "Relative to largest item", "Largest item fills the bar; others scale proportionally."),
                    (15, UsageBasis::Folder, "Current folder total", "Each item's share of this folder."),
                    (16, UsageBasis::DriveUsed, "Used drive space", "Each item's share of all used space on the drive."),
                    (17, UsageBasis::DriveTotal, "Total drive capacity", "Each item's share of the entire drive capacity."),
                ];

                for (idx, basis, label, desc) in basis_items {
                    let is_chosen = self.config.usage_basis == basis;
                    let is_f = is_panel_focused && self.settings_nav_idx == idx;
                    let pill = setting_pill(ui, is_chosen, is_f, label, None, false, false, is_dark).on_hover_text(desc);
                    if pill.clicked() || (is_f && key_activate) {
                        self.config.usage_basis = basis;
                        self.save_config();
                    }
                }

                ui.add_space(6.0);
                ui.separator();

                // --- 8. QUICK SEARCH ---
                ui.label(RichText::new("Quick Search").size(13.5).strong());
                ui.add_space(2.0);

                let s_modes = [
                    (18, QuickSearchMode::JumpToPrefix, "Jump to name (Explorer)", Some("F2")),
                    (19, QuickSearchMode::LiveFilter, "Filter list as you type (FilePilot)", Some("Ctrl+F")),
                    (20, QuickSearchMode::Disabled, "Disabled", None),
                ];

                for (idx, mode, label, sc) in s_modes {
                    let is_active = self.config.quick_search_mode == mode;
                    let is_f = is_panel_focused && self.settings_nav_idx == idx;
                    let pill = setting_pill(ui, is_active, is_f, label, sc, show_sc, false, is_dark);
                    if pill.clicked() || (is_f && key_activate) {
                        self.config.quick_search_mode = mode;
                        self.quick_search_buffer.clear();
                        self.save_config();
                    }
                }

                ui.add_space(6.0);
                ui.separator();

                // --- 9. FILE VISIBILITY ---
                ui.label(RichText::new("File Visibility").size(13.5).strong());
                let is_f_21 = is_panel_focused && self.settings_nav_idx == 21;
                let pill_hidden = setting_pill(ui, self.config.show_hidden_files, is_f_21, "Show Hidden Files", Some("Ctrl+H"), show_sc, false, is_dark);
                if pill_hidden.clicked() || (is_f_21 && key_activate) {
                    self.config.show_hidden_files = !self.config.show_hidden_files;
                    self.save_config();
                    self.sort_entries();
                }

                let is_f_22 = is_panel_focused && self.settings_nav_idx == 22;
                let pill_sys = setting_pill(ui, self.config.show_system_files, is_f_22, "Show System Files & Links", Some("Ctrl+Shift+H"), show_sc, false, is_dark);
                if pill_sys.clicked() || (is_f_22 && key_activate) {
                    self.config.show_system_files = !self.config.show_system_files;
                    self.save_config();
                    self.sort_entries();
                }

                let is_f_23 = is_panel_focused && self.settings_nav_idx == 23;
                let pill_lock = setting_pill(ui, self.config.show_locked_folders, is_f_23, "Show Locked Folders", None, false, false, is_dark);
                if pill_lock.clicked() || (is_f_23 && key_activate) {
                    self.config.show_locked_folders = !self.config.show_locked_folders;
                    self.save_config();
                    self.sort_entries();
                }

                ui.add_space(6.0);
                ui.separator();

                // --- 10. APPLICATION PREFERENCES ---
                ui.label(RichText::new("Application Preferences").size(13.5).strong());
                let is_f_24 = is_panel_focused && self.settings_nav_idx == 24;
                let pill_rem = setting_pill(ui, self.config.remember_last_path, is_f_24, "Remember last visited path", None, false, false, is_dark);
                if pill_rem.clicked() || (is_f_24 && key_activate) {
                    self.config.remember_last_path = !self.config.remember_last_path;
                    if !self.config.remember_last_path {
                        self.config.last_path = None;
                    }
                    self.save_config();
                }

                ui.add_space(6.0);
                ui.separator();

                // --- 11. DEVELOPER CACHE FILTER ---
                ui.label(RichText::new("Developer Cache Filter").size(13.5).strong());
                let is_f_25 = is_panel_focused && self.settings_nav_idx == 25;
                let pill_dev = setting_pill(ui, self.ignore_dev_caches, is_f_25, "Ignore dev caches", None, false, false, is_dark);
                if pill_dev.clicked() || (is_f_25 && key_activate) {
                    self.ignore_dev_caches = !self.ignore_dev_caches;
                    self.config.ignore_dev_caches = self.ignore_dev_caches;
                    self.save_config();
                    trigger_rescan = true;
                }

                ui.label(RichText::new("Custom Exclude Names (comma-separated):").weak().small());
                let edit = ui.add(
                    egui::TextEdit::singleline(&mut self.custom_excludes)
                        .hint_text(".git, node_modules, target, .cache, .venv"),
                );
                if edit.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)) {
                    self.config.custom_excludes = self.custom_excludes.clone();
                    self.save_config();
                    trigger_rescan = true;
                }

                if ui.button("Apply Filter Changes").clicked() {
                    self.config.custom_excludes = self.custom_excludes.clone();
                    self.save_config();
                    trigger_rescan = true;
                }

                ui.add_space(6.0);
                ui.separator();

                // --- 12. SHORTCUTS DISPLAY TOGGLE & FACTORY RESET ---
                ui.label(RichText::new("Settings Options").size(13.5).strong());
                let is_f_26 = is_panel_focused && self.settings_nav_idx == 26;
                let pill_sc_opt = setting_pill(ui, self.config.show_shortcuts_in_settings, is_f_26, "Show shortcuts in settings", None, false, false, is_dark);
                if pill_sc_opt.clicked() || (is_f_26 && key_activate) {
                    self.config.show_shortcuts_in_settings = !self.config.show_shortcuts_in_settings;
                    self.save_config();
                }

                ui.add_space(4.0);
                let is_f_27 = is_panel_focused && self.settings_nav_idx == 27;
                let btn_sc = ui.button("Configure Shortcuts & Hotkeys (F1)");
                if is_f_27 {
                    ui.painter().rect_stroke(
                        btn_sc.rect.expand(2.0),
                        3.0,
                        Stroke::new(1.5, Color32::from_rgb(100, 185, 255)),
                        egui::StrokeKind::Outside,
                    );
                }
                if btn_sc.clicked() || (is_f_27 && key_activate) {
                    self.show_shortcuts_modal = true;
                    self.shortcuts_just_opened = true;
                    self.shortcuts_selected_idx = 0;
                }

                ui.add_space(4.0);
                let is_f_28 = is_panel_focused && self.settings_nav_idx == 28;
                if !self.confirm_factory_reset {
                    let btn_reset = ui.button(RichText::new("Reset All Settings to Defaults").color(Color32::from_rgb(255, 120, 120)));
                    if is_f_28 {
                        ui.painter().rect_stroke(
                            btn_reset.rect.expand(2.0),
                            3.0,
                            Stroke::new(1.5, Color32::from_rgb(100, 185, 255)),
                            egui::StrokeKind::Outside,
                        );
                    }
                    if btn_reset.clicked() || (is_f_28 && key_activate) {
                        self.confirm_factory_reset = true;
                    }
                } else {
                    ui.label(RichText::new("Confirm resetting all preferences to defaults?").small().strong());
                    ui.horizontal(|ui| {
                        if ui.button(RichText::new("Yes, Reset").color(Color32::RED)).clicked() {
                            self.config = AppConfig::default();
                            self.ignore_dev_caches = self.config.ignore_dev_caches;
                            self.custom_excludes = self.config.custom_excludes.clone();
                            self.confirm_factory_reset = false;
                            ui.ctx().set_pixels_per_point(1.0);
                            self.save_config();
                            trigger_rescan = true;
                        }
                        if ui.button("Cancel").clicked() {
                            self.confirm_factory_reset = false;
                        }
                    });
                }

                ui.add_space(10.0);
            });

        if let Some(target) = nav_target {
            self.navigate_to(target);
        }
        if trigger_rescan {
            self.trigger_scan();
        }
        if recompute_dups {
            self.compute_duplicates();
        }
    }
}
