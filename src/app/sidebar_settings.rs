use eframe::egui::{self, Color32, RichText, Sense, Stroke};

use crate::config::{
    AppConfig, AppTheme, CategoryDuplicateRules, DuplicateRulePreference, QuickSearchMode,
    UsageBasis,
};
use super::FreeDataDietApp;

pub fn setting_pill(
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
    pub(super) fn render_sidebar_settings(
        &mut self,
        ui: &mut egui::Ui,
        is_dark: bool,
        is_panel_focused: bool,
        key_activate: bool,
        show_sc: bool,
        trigger_rescan: &mut bool,
        recompute_dups: &mut bool,
    ) {
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
                *trigger_rescan = true;
            }
        }

        ui.add_space(6.0);
        ui.separator();

        // --- 4. FIND DUPLICATES ---
        ui.label(RichText::new("Duplicate Search").size(13.5).strong().color(Color32::from_rgb(255, 140, 0)));
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
            *trigger_rescan = true;
        }

        ui.add_space(3.0);
        let pill_sugg = setting_pill(
            ui,
            self.config.enable_duplicate_suggestions,
            false,
            "Duplicate Recommendations",
            None,
            false,
            true,
            is_dark,
        ).on_hover_text("Enable smart auto-selection rules (Keep Oldest/Newest) and folder-pair pattern matching");
        if pill_sugg.clicked() {
            self.config.enable_duplicate_suggestions = !self.config.enable_duplicate_suggestions;
            self.save_config();
            if self.config.enable_duplicate_suggestions {
                self.apply_category_duplicate_rules();
            } else {
                self.active_suggestion = None;
            }
        }

        // Dedicated Visual Category Recommendations Card
        if self.config.enable_duplicate_suggestions {
            ui.add_space(3.0);
            egui::Frame::new()
                .fill(if is_dark { Color32::from_gray(30) } else { Color32::from_gray(240) })
                .stroke(Stroke::new(1.0, if is_dark { Color32::from_gray(55) } else { Color32::from_gray(210) }))
                .corner_radius(4.0)
                .inner_margin(egui::Margin::symmetric(8, 6))
                .show(ui, |ui| {
                    ui.horizontal(|ui| {
                        ui.label(RichText::new("Category Auto-Rules").size(12.5).strong().color(Color32::from_rgb(255, 140, 0)));
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            if ui.small_button("Reset Rules").clicked() {
                                self.config.category_duplicate_rules = CategoryDuplicateRules::default();
                                self.save_config();
                                if self.config.show_duplicates_only {
                                    self.apply_category_duplicate_rules();
                                }
                            }
                        });
                    });
                    ui.label(RichText::new("Auto-select redundant copies for deletion based on timestamps:").weak().small());
                    ui.add_space(3.0);

                    let cat_rules = [
                        ("Images", &mut self.config.category_duplicate_rules.images),
                        ("Videos", &mut self.config.category_duplicate_rules.videos),
                        ("Audio", &mut self.config.category_duplicate_rules.audio),
                        ("Documents", &mut self.config.category_duplicate_rules.documents),
                        ("Archives", &mut self.config.category_duplicate_rules.archives),
                    ];

                    let mut rules_changed = false;
                    for (cat_name, pref_ref) in cat_rules {
                        ui.horizontal(|ui| {
                            ui.label(RichText::new(cat_name).size(12.0).strong());
                            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                                let pref = *pref_ref;
                                let (bg, text_col) = match pref {
                                    DuplicateRulePreference::Off => (
                                        if is_dark { Color32::from_gray(45) } else { Color32::from_gray(225) },
                                        if is_dark { Color32::from_gray(180) } else { Color32::from_gray(90) },
                                    ),
                                    DuplicateRulePreference::KeepOldest => (
                                        Color32::from_rgb(235, 115, 15),
                                        Color32::WHITE,
                                    ),
                                    DuplicateRulePreference::KeepNewest => (
                                        Color32::from_rgb(30, 140, 220),
                                        Color32::WHITE,
                                    ),
                                    DuplicateRulePreference::KeepFirst => (
                                        Color32::from_rgb(50, 160, 90),
                                        Color32::WHITE,
                                    ),
                                };

                                let btn = egui::Button::new(
                                    RichText::new(format!("{} v", pref.label())).size(11.5).strong().color(text_col),
                                )
                                .fill(bg)
                                .corner_radius(3.0);

                                let tooltip = match pref {
                                    DuplicateRulePreference::Off => "Disabled: No auto-selection for this category",
                                    DuplicateRulePreference::KeepOldest => "Keep Oldest: Selects newer copies for deletion, preserving oldest timestamp",
                                    DuplicateRulePreference::KeepNewest => "Keep Newest: Selects older copies for deletion, preserving latest timestamp",
                                    DuplicateRulePreference::KeepFirst => "Keep First: Keeps first occurrence, selects all duplicate copies",
                                };

                                if ui.add(btn).on_hover_text(format!("{}\nClick to cycle preference", tooltip)).clicked() {
                                    *pref_ref = pref.cycle();
                                    rules_changed = true;
                                }
                            });
                        });
                    }

                    if rules_changed {
                        self.save_config();
                        if self.config.show_duplicates_only {
                            self.apply_category_duplicate_rules();
                        }
                    }
                });
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
                *recompute_dups = true;
            }
        }

        ui.add_space(6.0);
        ui.separator();

        // --- 5. EXCLUDE & IGNORE FILTERS ---
        ui.horizontal(|ui| {
            ui.label(RichText::new("Exclude & Ignore Filters").size(13.5).strong());
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                let plus_label = if self.show_custom_excludes_input { "[-] Less" } else { "[+] Custom" };
                if ui.small_button(plus_label).on_hover_text("Toggle custom exclude pattern field").clicked() {
                    self.show_custom_excludes_input = !self.show_custom_excludes_input;
                }
            });
        });
        ui.add_space(2.0);

        let is_f_6 = is_panel_focused && self.settings_nav_idx == 6;
        let pill_media = setting_pill(ui, self.ignore_media_caches, is_f_6, "Ignore app & media caches", None, false, false, is_dark)
            .on_hover_text("Ignore DaVinci (CacheClip, .gallery), Adobe (Media Cache), Blender, ShaderCache, thumbs.db");
        if pill_media.clicked() || (is_f_6 && key_activate) {
            self.ignore_media_caches = !self.ignore_media_caches;
            self.config.ignore_media_caches = self.ignore_media_caches;
            self.save_config();
            *trigger_rescan = true;
        }

        let is_f_7 = is_panel_focused && self.settings_nav_idx == 7;
        let pill_dev = setting_pill(ui, self.ignore_dev_caches, is_f_7, "Ignore dev & repo caches", None, false, false, is_dark)
            .on_hover_text("Ignore .git, node_modules, target, .venv, .cargo, .gradle, __pycache__");
        if pill_dev.clicked() || (is_f_7 && key_activate) {
            self.ignore_dev_caches = !self.ignore_dev_caches;
            self.config.ignore_dev_caches = self.ignore_dev_caches;
            self.save_config();
            *trigger_rescan = true;
        }

        let is_f_8 = is_panel_focused && self.settings_nav_idx == 8;
        let pill_temp = setting_pill(ui, self.ignore_temp_caches, is_f_8, "Ignore temp & crash dumps", None, false, false, is_dark)
            .on_hover_text("Ignore Temp, Prefetch, CrashDumps, $RECYCLE.BIN, *.dmp, *.tmp");
        if pill_temp.clicked() || (is_f_8 && key_activate) {
            self.ignore_temp_caches = !self.ignore_temp_caches;
            self.config.ignore_temp_caches = self.ignore_temp_caches;
            self.save_config();
            *trigger_rescan = true;
        }

        if self.show_custom_excludes_input {
            ui.add_space(3.0);
            ui.label(RichText::new("Custom Exclude Patterns (comma-separated):").weak().small());
            let edit = ui.add(
                egui::TextEdit::singleline(&mut self.custom_excludes)
                    .hint_text("e.g. *.iso, backup, my_cache"),
            );
            if edit.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)) {
                self.config.custom_excludes = self.custom_excludes.clone();
                self.save_config();
                *trigger_rescan = true;
            }
            ui.horizontal(|ui| {
                if ui.small_button("Apply").clicked() {
                    self.config.custom_excludes = self.custom_excludes.clone();
                    self.save_config();
                    *trigger_rescan = true;
                }
                if ui.small_button("Reset Excludes").clicked() {
                    self.custom_excludes = ".git, node_modules, target, .cache, .venv".to_string();
                    self.config.custom_excludes = self.custom_excludes.clone();
                    self.save_config();
                    *trigger_rescan = true;
                }
            });
        }

        // --- 5B. IGNORED & EXCLUDED LOCATIONS ---
        ui.add_space(6.0);
        ui.horizontal(|ui| {
            ui.label(RichText::new("Ignored Locations").size(13.0).strong());
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if ui.small_button("+ Add Folder...").on_hover_text("Add a folder to scan exclusions or duplicate ignore list").clicked() {
                    if let Some(folder) = rfd::FileDialog::new().pick_folder() {
                        let folder_name = folder
                            .file_name()
                            .map(|n| n.to_string_lossy().to_string())
                            .unwrap_or_else(|| folder.to_string_lossy().to_string());
                        let id = format!("excl_loc_{}", std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_millis()).unwrap_or(0));
                        self.config.excluded_locations.push(crate::config::ExcludedLocation {
                            id,
                            name: folder_name,
                            path: folder,
                            scope: crate::config::ExcludeScope::IgnoreForDuplicates,
                            enabled: true,
                        });
                        self.save_config();
                        if self.config.show_duplicates_only {
                            *recompute_dups = true;
                        }
                    }
                }
            });
        });
        ui.label(RichText::new("Click scope badge to switch between Hide and Ignore Duplicates").weak().small());
        ui.add_space(2.0);

        let mut remove_excl_idx = None;
        let mut excl_changed = false;

        for (e_idx, loc) in self.config.excluded_locations.iter_mut().enumerate() {
            ui.horizontal(|ui| {
                if ui.checkbox(&mut loc.enabled, "").changed() {
                    excl_changed = true;
                }
                ui.label(RichText::new(&loc.name).strong()).on_hover_text(loc.path.to_string_lossy());

                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if ui.small_button("x").on_hover_text("Remove location from ignored list").clicked() {
                        remove_excl_idx = Some(e_idx);
                    }

                    let (scope_txt, scope_col) = match loc.scope {
                        crate::config::ExcludeScope::HideFromScan => ("Hide", Color32::from_rgb(100, 180, 255)),
                        crate::config::ExcludeScope::IgnoreForDuplicates => ("Ignore Dups", Color32::from_rgb(255, 175, 50)),
                    };
                    let scope_btn = egui::Button::new(
                        RichText::new(format!("[{}]", scope_txt)).small().color(scope_col),
                    );
                    if ui.add(scope_btn).on_hover_text("Toggle scope: Hide completely from scan vs Ignore for duplicates only").clicked() {
                        loc.scope = match loc.scope {
                            crate::config::ExcludeScope::HideFromScan => crate::config::ExcludeScope::IgnoreForDuplicates,
                            crate::config::ExcludeScope::IgnoreForDuplicates => crate::config::ExcludeScope::HideFromScan,
                        };
                        excl_changed = true;
                    }
                });
            });
        }

        if let Some(idx) = remove_excl_idx {
            self.config.excluded_locations.remove(idx);
            excl_changed = true;
        }

        if excl_changed {
            self.save_config();
            if self.config.show_duplicates_only {
                *recompute_dups = true;
            } else {
                *trigger_rescan = true;
            }
        }

        ui.add_space(6.0);
        ui.separator();

        // --- 6. FILE TYPES & CATEGORY FILTER ---
        ui.horizontal(|ui| {
            ui.label(RichText::new("File Type Filter").size(13.5).strong());
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
            (9, "Scan Images", self.config.category_filters.show_images),
            (10, "Scan Videos", self.config.category_filters.show_videos),
            (11, "Scan Audio", self.config.category_filters.show_audio),
            (12, "Scan Documents", self.config.category_filters.show_documents),
            (13, "Scan Archives", self.config.category_filters.show_archives),
        ];

        for (idx, label, val) in cat_items {
            let is_f = is_panel_focused && self.settings_nav_idx == idx;
            let pill = setting_pill(ui, val, is_f, label, None, false, false, is_dark);
            if pill.clicked() || (is_f && key_activate) {
                match idx {
                    9 => self.config.category_filters.show_images = !val,
                    10 => self.config.category_filters.show_videos = !val,
                    11 => self.config.category_filters.show_audio = !val,
                    12 => self.config.category_filters.show_documents = !val,
                    13 => self.config.category_filters.show_archives = !val,
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

        // --- 7. QUICK SEARCH ---
        ui.label(RichText::new("Quick Search").size(13.5).strong());
        ui.add_space(2.0);

        let s_modes = [
            (14, QuickSearchMode::JumpToPrefix, "Jump to name (Explorer)", Some("F2")),
            (15, QuickSearchMode::LiveFilter, "Filter list as you type (FilePilot)", Some("Ctrl+F")),
            (16, QuickSearchMode::Disabled, "Disabled", None),
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

        // --- 8. FILE VISIBILITY ---
        ui.label(RichText::new("File Visibility").size(13.5).strong());
        let is_f_17 = is_panel_focused && self.settings_nav_idx == 17;
        let pill_hidden = setting_pill(ui, self.config.show_hidden_files, is_f_17, "Show Hidden Files", Some("Ctrl+H"), show_sc, false, is_dark);
        if pill_hidden.clicked() || (is_f_17 && key_activate) {
            self.config.show_hidden_files = !self.config.show_hidden_files;
            self.save_config();
            self.sort_entries();
        }

        let is_f_18 = is_panel_focused && self.settings_nav_idx == 18;
        let pill_sys = setting_pill(ui, self.config.show_system_files, is_f_18, "Show System Files & Links", Some("Ctrl+Shift+H"), show_sc, false, is_dark);
        if pill_sys.clicked() || (is_f_18 && key_activate) {
            self.config.show_system_files = !self.config.show_system_files;
            self.save_config();
            self.sort_entries();
        }

        let is_f_19 = is_panel_focused && self.settings_nav_idx == 19;
        let pill_lock = setting_pill(ui, self.config.show_locked_folders, is_f_19, "Show Locked Folders", None, false, false, is_dark);
        if pill_lock.clicked() || (is_f_19 && key_activate) {
            self.config.show_locked_folders = !self.config.show_locked_folders;
            self.save_config();
            self.sort_entries();
        }

        ui.add_space(6.0);
        ui.separator();

        // --- 9. USAGE BAR SCALING ---
        ui.label(RichText::new("Usage Bar Scaling").size(13.5).strong());
        ui.label(RichText::new("Scale usage bars relative to:").weak().small());
        ui.add_space(2.0);

        let basis_items = [
            (20, UsageBasis::LargestItem, "Relative to largest item", "Largest item fills the bar; others scale proportionally."),
            (21, UsageBasis::Folder, "Current folder total", "Each item's share of this folder."),
            (22, UsageBasis::DriveUsed, "Used drive space", "Each item's share of all used space on the drive."),
            (23, UsageBasis::DriveTotal, "Total drive capacity", "Each item's share of the entire drive capacity."),
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

        // --- 10. THEME & APPEARANCE ---
        ui.label(RichText::new("Theme & Appearance").size(13.5).strong());
        let theme_items = [
            (24, AppTheme::Dark, "Dark Mode"),
            (25, AppTheme::Light, "Light Mode"),
            (26, AppTheme::System, "System (Auto)"),
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

        // --- 11. APPLICATION PREFERENCES ---
        ui.label(RichText::new("Application Preferences").size(13.5).strong());
        let is_f_27 = is_panel_focused && self.settings_nav_idx == 27;
        let pill_rem = setting_pill(ui, self.config.remember_last_path, is_f_27, "Remember last visited path", None, false, false, is_dark);
        if pill_rem.clicked() || (is_f_27 && key_activate) {
            self.config.remember_last_path = !self.config.remember_last_path;
            if !self.config.remember_last_path {
                self.config.last_path = None;
            }
            self.save_config();
        }

        ui.add_space(6.0);
        ui.separator();

        // --- 12. SHORTCUTS & FACTORY RESET ---
        ui.label(RichText::new("Settings Options").size(13.5).strong());
        let is_f_28 = is_panel_focused && self.settings_nav_idx == 28;
        let pill_sc_opt = setting_pill(ui, self.config.show_shortcuts_in_settings, is_f_28, "Show shortcuts in settings", None, false, false, is_dark);
        if pill_sc_opt.clicked() || (is_f_28 && key_activate) {
            self.config.show_shortcuts_in_settings = !self.config.show_shortcuts_in_settings;
            self.save_config();
        }

        ui.add_space(4.0);
        let is_f_29 = is_panel_focused && self.settings_nav_idx == 29;
        let btn_sc = ui.button("Configure Shortcuts & Hotkeys (F1)");
        if is_f_29 {
            ui.painter().rect_stroke(
                btn_sc.rect.expand(2.0),
                3.0,
                Stroke::new(1.5, Color32::from_rgb(100, 185, 255)),
                egui::StrokeKind::Outside,
            );
        }
        if btn_sc.clicked() || (is_f_29 && key_activate) {
            self.show_shortcuts_modal = true;
            self.shortcuts_just_opened = true;
            self.shortcuts_selected_idx = 0;
        }

        ui.add_space(4.0);
        let is_f_30 = is_panel_focused && self.settings_nav_idx == 30;
        if !self.confirm_factory_reset {
            let btn_reset = ui.button(RichText::new("Reset All Settings to Defaults").color(Color32::from_rgb(255, 120, 120)));
            if is_f_30 {
                ui.painter().rect_stroke(
                    btn_reset.rect.expand(2.0),
                    3.0,
                    Stroke::new(1.5, Color32::from_rgb(100, 185, 255)),
                    egui::StrokeKind::Outside,
                );
            }
            if btn_reset.clicked() || (is_f_30 && key_activate) {
                self.confirm_factory_reset = true;
            }
        } else {
            ui.label(RichText::new("Confirm resetting all preferences to defaults?").small().strong());
            ui.horizontal(|ui| {
                if ui.button(RichText::new("Yes, Reset").color(Color32::RED)).clicked() {
                    self.config = AppConfig::default();
                    self.ignore_dev_caches = self.config.ignore_dev_caches;
                    self.ignore_media_caches = self.config.ignore_media_caches;
                    self.ignore_temp_caches = self.config.ignore_temp_caches;
                    self.custom_excludes = self.config.custom_excludes.clone();
                    self.confirm_factory_reset = false;
                    ui.ctx().set_pixels_per_point(1.0);
                    self.save_config();
                    *trigger_rescan = true;
                }
                if ui.button("Cancel").clicked() {
                    self.confirm_factory_reset = false;
                }
            });
        }

        ui.add_space(10.0);
    }
}
