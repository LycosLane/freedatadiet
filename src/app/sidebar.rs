use std::path::PathBuf;
use std::time::Instant;

use eframe::egui::{self, Color32, Rect, RichText, ScrollArea, Sense, Stroke, Vec2};

use crate::config::{AppConfig, AppTheme, QuickSearchMode, StartupLocationMode, UsageBasis};
use crate::locations::LocationManager;
use crate::model::format_size;

use super::FreeDataDietApp;

impl FreeDataDietApp {
    pub(super) fn render_left_panel(&mut self, ui: &mut egui::Ui, is_dark: bool) {
        ui.set_width(290.0);
        ui.add_space(4.0);

        ui.horizontal(|ui| {
            ui.label(RichText::new("Settings, Filters & Locations").size(14.0).strong());
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if ui
                    .small_button("Hide (F12)")
                    .on_hover_text("Collapse left panel (F12 / F4 / Ctrl+,)")
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
                        if is_dark { Color32::from_rgb(32, 54, 82) } else { Color32::from_rgb(205, 228, 252) }
                    } else if is_dark {
                        Color32::from_rgb(26, 33, 44)
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

                // --- 2. FAVORITES ---
                ui.horizontal(|ui| {
                    ui.label(RichText::new("Favorites").size(13.5).strong());
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
                ui.add_space(2.0);

                let mut remove_fav = None;
                let mut startup_fav = None;
                let mut move_fav = None;

                for (i, loc) in self.config.saved_locations.iter().enumerate() {
                    let is_active = self.current_path == loc.path;
                    let is_accessible = self.location_mgr.is_accessible(&loc.path);
                    let is_startup = i == 0 && self.config.startup_mode == StartupLocationMode::FirstFavorite;

                    let row_bg = if is_active {
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
                                let prefix = if is_startup { "[Startup]" } else { "*" };
                                ui.label(RichText::new(prefix).monospace().small().color(if is_startup {
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
                        .on_hover_text(format!("{} (Right-click for options)", loc.path.to_string_lossy()));

                    if fav_click.clicked() && is_accessible {
                        nav_target = Some(loc.path.clone());
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

                // --- 3. SCAN & DUPLICATE MODE ---
                ui.label(RichText::new("Scan & Duplicate Mode").size(13.5).strong());
                if ui
                    .checkbox(&mut self.config.flatten_subfolders, "Recursive / Flatten Subfolders (F6)")
                    .on_hover_text("Scan and flatten all files in all subfolders into one list")
                    .changed()
                {
                    self.save_config();
                    trigger_rescan = true;
                }
                if ui
                    .checkbox(&mut self.config.show_duplicates_only, "Find Duplicates View (Ctrl+D)")
                    .on_hover_text("Group duplicate files across subfolders by size, name, timestamp, or SHA-256 hash")
                    .changed()
                {
                    self.save_config();
                    trigger_rescan = true;
                }

                ui.add_space(4.0);
                ui.label(RichText::new("Duplicate Matching Criteria:").weak().small());
                let mut crit_changed = false;
                if ui.checkbox(&mut self.config.duplicate_criteria.match_size, "Match same file size").changed() {
                    crit_changed = true;
                }
                if ui.checkbox(&mut self.config.duplicate_criteria.match_name, "Match same file name").changed() {
                    crit_changed = true;
                }
                if ui.checkbox(&mut self.config.duplicate_criteria.match_timestamp, "Match modification timestamp").changed() {
                    crit_changed = true;
                }
                if ui
                    .checkbox(&mut self.config.duplicate_criteria.verify_hash, "Verify bit-exact SHA-256 hash")
                    .on_hover_text("Two-pass check: 4 KB prefix hash followed by 64 KB streaming SHA-256")
                    .changed()
                {
                    crit_changed = true;
                }
                if crit_changed {
                    self.save_config();
                    if self.config.show_duplicates_only {
                        recompute_dups = true;
                    }
                }

                ui.add_space(6.0);
                ui.separator();

                // --- 4. CATEGORY FILTERS (WHAT'S SHOWN) ---
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
                ui.horizontal_wrapped(|ui| {
                    if ui.checkbox(&mut self.config.category_filters.show_images, "Images").changed() {
                        self.save_config();
                    }
                    if ui.checkbox(&mut self.config.category_filters.show_videos, "Videos").changed() {
                        self.save_config();
                    }
                    if ui.checkbox(&mut self.config.category_filters.show_audio, "Audio").changed() {
                        self.save_config();
                    }
                    if ui.checkbox(&mut self.config.category_filters.show_documents, "Documents").changed() {
                        self.save_config();
                    }
                    if ui.checkbox(&mut self.config.category_filters.show_archives, "Archives").changed() {
                        self.save_config();
                    }
                });

                ui.add_space(6.0);
                ui.separator();

                // --- 5. THEME & APPEARANCE (FROM 9d69210) ---
                ui.label(RichText::new("Theme & Appearance").size(13.5).strong());
                ui.horizontal(|ui| {
                    for theme in [AppTheme::Dark, AppTheme::Light, AppTheme::System] {
                        if ui.selectable_label(self.config.theme == theme, theme.label()).clicked() {
                            self.config.theme = theme;
                            self.save_config();
                            ui.ctx().request_repaint();
                        }
                    }
                });
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

                // --- 6. USAGE BAR SCALING (FROM 9d69210) ---
                ui.label(RichText::new("Usage Bar Scaling").size(13.5).strong());
                ui.label(RichText::new("Scale usage bars relative to:").weak().small());
                ui.add_space(2.0);

                for basis in [
                    UsageBasis::LargestItem,
                    UsageBasis::Folder,
                    UsageBasis::DriveUsed,
                    UsageBasis::DriveTotal,
                ] {
                    let is_chosen = self.config.usage_basis == basis;
                    if ui
                        .selectable_label(is_chosen, RichText::new(basis.display_name()).strong())
                        .on_hover_text(basis.description())
                        .clicked()
                    {
                        self.config.usage_basis = basis;
                        self.save_config();
                    }
                }

                ui.add_space(6.0);
                ui.separator();

                // --- 7. QUICK SEARCH (FROM 9d69210) ---
                ui.label(RichText::new("Quick Search").size(13.5).strong());
                ui.add_space(2.0);

                let s_modes = [
                    (QuickSearchMode::JumpToPrefix, "Jump to name (Explorer style)"),
                    (QuickSearchMode::LiveFilter, "Filter list as you type (FilePilot style)"),
                    (QuickSearchMode::Disabled, "Disabled"),
                ];
                for (mode, label) in s_modes {
                    if ui.selectable_label(self.config.quick_search_mode == mode, label).clicked() {
                        self.config.quick_search_mode = mode;
                        self.quick_search_buffer.clear();
                        self.save_config();
                    }
                }

                ui.add_space(6.0);
                ui.separator();

                // --- 8. FILE VISIBILITY (FROM 9d69210) ---
                ui.label(RichText::new("File Visibility").size(13.5).strong());
                if ui
                    .checkbox(&mut self.config.show_hidden_files, "Show Hidden Files (Ctrl+H)")
                    .on_hover_text("Show hidden files and dotfiles")
                    .changed()
                {
                    self.save_config();
                    self.sort_entries();
                }
                if ui
                    .checkbox(&mut self.config.show_system_files, "Show System Files & Links (Ctrl+Shift+H)")
                    .on_hover_text("Show protected system files and directory links")
                    .changed()
                {
                    self.save_config();
                    self.sort_entries();
                }
                if ui
                    .checkbox(&mut self.config.show_locked_folders, "Show Locked Folders")
                    .on_hover_text("Show folders that cannot be scanned due to permission restrictions")
                    .changed()
                {
                    self.save_config();
                    self.sort_entries();
                }

                ui.add_space(6.0);
                ui.separator();

                // --- 9. APPLICATION PREFERENCES (FROM 9d69210) ---
                ui.label(RichText::new("Application Preferences").size(13.5).strong());
                if ui
                    .checkbox(&mut self.config.remember_last_path, "Remember last visited path on startup")
                    .changed()
                {
                    if !self.config.remember_last_path {
                        self.config.last_path = None;
                    }
                    self.save_config();
                }

                ui.add_space(6.0);
                ui.separator();

                // --- 10. DEVELOPER CACHE FILTER (FROM 9d69210) ---
                ui.label(RichText::new("Developer Cache Filter").size(13.5).strong());
                if ui
                    .checkbox(&mut self.ignore_dev_caches, "Ignore dev caches (.git, node_modules, target, .cache, .venv)")
                    .changed()
                {
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

                // --- 11. SHORTCUTS & FACTORY RESET ---
                if ui.button("Configure Shortcuts & Hotkeys (F1)").clicked() {
                    self.show_shortcuts_modal = true;
                    self.shortcuts_just_opened = true;
                    self.shortcuts_selected_idx = 0;
                }

                ui.add_space(4.0);
                if !self.confirm_factory_reset {
                    if ui
                        .button(RichText::new("Reset All Settings to Defaults").color(Color32::from_rgb(255, 120, 120)))
                        .clicked()
                    {
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
