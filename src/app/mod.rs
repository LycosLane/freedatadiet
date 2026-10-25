pub mod duplicates_view;
pub mod modals;
pub mod sidebar;
pub mod table;

use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

use eframe::egui::{self, Color32, Panel, RichText, Vec2};

use crate::config::{AppConfig, AppTheme, ColumnId, QuickSearchMode, SortRule};
use crate::duplicate::{DuplicateCluster, DuplicateEngine, DuplicateSuggestion};
use crate::locations::LocationManager;
use crate::model::{format_size, DriveInfo, FileEntry, SortDirection};
use crate::platform;
use crate::scanner::{start_scan, ScanMessage, ScanSummary, ScannerHandle};

use self::modals::{ActiveContextMenu, DeleteConfirmState, DeleteModalFocus};

pub struct FreeDataDietApp {
    pub(super) config: AppConfig,
    pub(super) location_mgr: LocationManager,
    pub(super) dragged_column_idx: Option<usize>,

    pub(super) current_path: PathBuf,
    pub(super) path_input: String,
    pub(super) drives: Vec<DriveInfo>,
    pub(super) entries: Vec<FileEntry>,
    pub(super) selected_path: Option<PathBuf>,
    pub(super) force_scroll_to_selected: bool,

    // Duplicates & Assistant
    pub(super) duplicate_clusters: Vec<DuplicateCluster>,
    pub(super) active_suggestion: Option<DuplicateSuggestion>,
    pub(super) safety_warning: Option<(String, Option<PathBuf>)>,
    pub(super) cancel_duplicate_hashing: Arc<AtomicBool>,

    // Quick Search
    pub(super) quick_search_buffer: String,
    pub(super) quick_search_time: Option<Instant>,
    pub(super) filter_bar_active: bool,
    pub(super) cached_system_dark: bool,
    pub(super) last_theme_check: Instant,

    // Exclude filters
    pub(super) ignore_dev_caches: bool,
    pub(super) custom_excludes: String,

    pub(super) scanner: Option<ScannerHandle>,
    pub(super) is_scanning: bool,
    pub(super) scanned_counter: usize,
    pub(super) summary: Option<ScanSummary>,
    pub(super) error_message: Option<String>,

    pub(super) delete_modal: DeleteConfirmState,
    pub(super) modal_focus: DeleteModalFocus,
    pub(super) active_context_menu: Option<ActiveContextMenu>,
    pub(super) show_shortcuts_modal: bool,
    pub(super) shortcuts_just_opened: bool,
    pub(super) shortcuts_selected_idx: usize,
    pub(super) confirm_factory_reset: bool,
    pub(super) status_toast: Option<(String, Instant)>,
    pub(super) is_admin: bool,
    pub(super) updater: crate::updater::UpdateManager,
    pub(super) show_update_modal: bool,
}

impl FreeDataDietApp {
    pub fn new(cc: &eframe::CreationContext<'_>) -> Self {
        let config = AppConfig::load();
        cc.egui_ctx.set_pixels_per_point(config.ui_scale);

        let drives = platform::get_drives();
        let is_admin = platform::is_admin();
        let location_mgr = LocationManager::new();

        let fallback = drives
            .first()
            .map(|d| d.path.clone())
            .unwrap_or_else(|| std::env::current_dir().unwrap_or_else(|_| PathBuf::from(".")));

        let start_path = location_mgr.resolve_startup_path(&config, &fallback);

        let mut app = Self {
            ignore_dev_caches: config.ignore_dev_caches,
            custom_excludes: config.custom_excludes.clone(),
            config,
            location_mgr,
            dragged_column_idx: None,
            path_input: start_path.to_string_lossy().to_string(),
            current_path: start_path,
            drives,
            entries: Vec::new(),
            selected_path: None,
            force_scroll_to_selected: false,
            duplicate_clusters: Vec::new(),
            active_suggestion: None,
            safety_warning: None,
            cancel_duplicate_hashing: Arc::new(AtomicBool::new(false)),
            quick_search_buffer: String::new(),
            quick_search_time: None,
            filter_bar_active: false,
            cached_system_dark: platform::is_system_dark_mode(),
            last_theme_check: Instant::now(),
            scanner: None,
            is_scanning: false,
            scanned_counter: 0,
            summary: None,
            error_message: None,
            delete_modal: DeleteConfirmState::None,
            modal_focus: DeleteModalFocus::Trash,
            active_context_menu: None,
            show_shortcuts_modal: false,
            shortcuts_just_opened: false,
            shortcuts_selected_idx: 0,
            confirm_factory_reset: false,
            status_toast: None,
            is_admin,
            updater: crate::updater::UpdateManager::new(),
            show_update_modal: false,
        };

        app.trigger_scan();
        app
    }

    pub(super) fn save_config(&mut self) {
        self.config.ignore_dev_caches = self.ignore_dev_caches;
        self.config.custom_excludes = self.custom_excludes.clone();
        if self.config.remember_last_path {
            self.config.last_path = Some(self.current_path.clone());
        } else {
            self.config.last_path = None;
        }
        let _ = self.config.save();
    }

    pub(super) fn collect_excludes(&self) -> Vec<String> {
        let mut list = Vec::new();
        if self.ignore_dev_caches {
            for item in self.custom_excludes.split(',') {
                let trimmed = item.trim();
                if !trimmed.is_empty() {
                    list.push(trimmed.to_string());
                }
            }
        }
        for p in &self.config.excluded_patterns {
            if p.enabled && !list.iter().any(|existing| existing.eq_ignore_ascii_case(&p.pattern)) {
                list.push(p.pattern.clone());
            }
        }
        list
    }

    pub(super) fn trigger_scan(&mut self) {
        if let Some(scanner) = &self.scanner {
            scanner.cancel();
        }
        self.cancel_duplicate_hashing.store(true, Ordering::SeqCst);
        self.cancel_duplicate_hashing = Arc::new(AtomicBool::new(false));

        self.entries.clear();
        self.duplicate_clusters.clear();
        self.active_suggestion = None;
        self.safety_warning = None;
        self.selected_path = None;
        self.quick_search_buffer.clear();
        self.quick_search_time = None;
        self.summary = None;
        self.error_message = None;
        self.is_scanning = true;
        self.scanned_counter = 0;

        self.path_input = self.current_path.to_string_lossy().to_string();
        let excludes = self.collect_excludes();
        let flatten = self.config.flatten_subfolders || self.config.show_duplicates_only;
        self.scanner = Some(start_scan(self.current_path.clone(), excludes, flatten));
    }

    pub(super) fn navigate_to(&mut self, path: PathBuf) {
        if path.is_dir() {
            self.quick_search_buffer.clear();
            self.quick_search_time = None;
            self.filter_bar_active = false;
            self.current_path = path;
            self.save_config();
            self.trigger_scan();
        }
    }

    pub(super) fn navigate_up(&mut self) {
        if let Some(parent) = self.current_path.parent() {
            self.navigate_to(parent.to_path_buf());
        }
    }

    pub(super) fn compute_duplicates(&mut self) {
        self.duplicate_clusters = DuplicateEngine::find_duplicates(
            &self.entries,
            self.config.duplicate_criteria,
            &self.cancel_duplicate_hashing,
        );
        self.update_duplicate_assistant();
    }

    pub(super) fn update_duplicate_assistant(&mut self) {
        let mut selected_set = std::collections::HashSet::new();
        for cluster in &self.duplicate_clusters {
            for occ in &cluster.occurrences {
                if occ.is_selected {
                    selected_set.insert(occ.path.clone());
                }
            }
        }
        self.active_suggestion = DuplicateEngine::find_folder_suggestion(
            &self.duplicate_clusters,
            &selected_set,
        );
    }

    fn poll_scanner(&mut self) {
        let mut messages = Vec::new();
        if let Some(scanner) = &self.scanner {
            while let Ok(msg) = scanner.receiver.try_recv() {
                messages.push(msg);
            }
        }

        let mut needs_sort = false;
        let mut needs_duplicates = false;
        for msg in messages {
            match msg {
                ScanMessage::Progress { scanned_items } => {
                    self.scanned_counter = scanned_items;
                }
                ScanMessage::InitialEntries(entries) => {
                    if self.config.flatten_subfolders || self.config.show_duplicates_only {
                        self.entries.extend(entries);
                    } else {
                        self.entries = entries;
                    }
                    needs_sort = true;
                }
                ScanMessage::EntryUpdated(entry) => {
                    if let Some(existing) = self.entries.iter_mut().find(|e| e.path == entry.path) {
                        existing.size = entry.size;
                        existing.allocated_size = entry.allocated_size;
                        existing.item_count = entry.item_count;
                        existing.modified = entry.modified;
                        existing.is_access_denied = entry.is_access_denied;
                        existing.is_partially_locked = entry.is_partially_locked;
                        existing.is_scanning = false;
                    } else {
                        self.entries.push(entry);
                    }
                    needs_sort = true;
                }
                ScanMessage::Completed(sum) => {
                    self.summary = Some(sum);
                    self.is_scanning = false;
                    self.scanner = None;
                    for e in &mut self.entries {
                        e.is_scanning = false;
                    }
                    needs_sort = true;
                    if self.config.show_duplicates_only {
                        needs_duplicates = true;
                    }
                }
                ScanMessage::Failed(err) => {
                    self.error_message = Some(err);
                    self.is_scanning = false;
                    self.scanner = None;
                }
            }
        }
        // Sort once per frame instead of once per message
        if needs_sort {
            self.sort_entries();
        }
        if needs_duplicates {
            self.compute_duplicates();
        }
    }

    pub(super) fn sort_entries(&mut self) {
        let rules = self.config.sort_rules.clone();

        self.entries.sort_by(|a, b| {
            for rule in &rules {
                let directed = match rule.column {
                    ColumnId::Type => {
                        let a_key = get_type_sort_key(a);
                        let b_key = get_type_sort_key(b);
                        let cmp = a_key.cmp(&b_key);
                        match rule.direction {
                            SortDirection::Ascending => cmp,
                            SortDirection::Descending => cmp.reverse(),
                        }
                    }
                    ColumnId::Name => {
                        let cmp = a.name.chars().flat_map(char::to_lowercase).cmp(b.name.chars().flat_map(char::to_lowercase));
                        match rule.direction {
                            SortDirection::Ascending => cmp,
                            SortDirection::Descending => cmp.reverse(),
                        }
                    }
                    ColumnId::Size | ColumnId::Usage => {
                        if a.is_scanning != b.is_scanning {
                            match rule.direction {
                                SortDirection::Descending => {
                                    if a.is_scanning {
                                        std::cmp::Ordering::Less
                                    } else {
                                        std::cmp::Ordering::Greater
                                    }
                                }
                                SortDirection::Ascending => {
                                    if a.is_scanning {
                                        std::cmp::Ordering::Greater
                                    } else {
                                        std::cmp::Ordering::Less
                                    }
                                }
                            }
                        } else {
                            let cmp = a.size.cmp(&b.size);
                            match rule.direction {
                                SortDirection::Ascending => cmp,
                                SortDirection::Descending => cmp.reverse(),
                            }
                        }
                    }
                    ColumnId::AllocatedSize => {
                        if a.is_scanning != b.is_scanning {
                            match rule.direction {
                                SortDirection::Descending => {
                                    if a.is_scanning {
                                        std::cmp::Ordering::Less
                                    } else {
                                        std::cmp::Ordering::Greater
                                    }
                                }
                                SortDirection::Ascending => {
                                    if a.is_scanning {
                                        std::cmp::Ordering::Greater
                                    } else {
                                        std::cmp::Ordering::Less
                                    }
                                }
                            }
                        } else {
                            let cmp = a.allocated_size.cmp(&b.allocated_size);
                            match rule.direction {
                                SortDirection::Ascending => cmp,
                                SortDirection::Descending => cmp.reverse(),
                            }
                        }
                    }
                    ColumnId::Items => {
                        if a.is_scanning != b.is_scanning {
                            match rule.direction {
                                SortDirection::Descending => {
                                    if a.is_scanning {
                                        std::cmp::Ordering::Less
                                    } else {
                                        std::cmp::Ordering::Greater
                                    }
                                }
                                SortDirection::Ascending => {
                                    if a.is_scanning {
                                        std::cmp::Ordering::Greater
                                    } else {
                                        std::cmp::Ordering::Less
                                    }
                                }
                            }
                        } else {
                            let cmp = a.item_count.cmp(&b.item_count);
                            match rule.direction {
                                SortDirection::Ascending => cmp,
                                SortDirection::Descending => cmp.reverse(),
                            }
                        }
                    }
                    ColumnId::Modified => {
                        let cmp = a.modified.cmp(&b.modified);
                        match rule.direction {
                            SortDirection::Ascending => cmp,
                            SortDirection::Descending => cmp.reverse(),
                        }
                    }
                };

                if directed != std::cmp::Ordering::Equal {
                    return directed;
                }
            }

            a.name.chars().flat_map(char::to_lowercase).cmp(b.name.chars().flat_map(char::to_lowercase))
        });
    }

    pub(super) fn toggle_sort_column(&mut self, col: ColumnId) {
        if let Some(pos) = self.config.sort_rules.iter().position(|r| r.column == col) {
            let default_dir = match col {
                ColumnId::Name | ColumnId::Type => SortDirection::Ascending,
                _ => SortDirection::Descending,
            };
            if self.config.sort_rules[pos].direction == default_dir {
                // Second click: invert direction
                self.config.sort_rules[pos].direction = match default_dir {
                    SortDirection::Ascending => SortDirection::Descending,
                    SortDirection::Descending => SortDirection::Ascending,
                };
            } else {
                // Third click: remove sort rule
                self.config.sort_rules.remove(pos);
            }
        } else {
            let default_dir = match col {
                ColumnId::Name | ColumnId::Type => SortDirection::Ascending,
                _ => SortDirection::Descending,
            };
            self.config.sort_rules.push(SortRule {
                column: col,
                direction: default_dir,
            });
        }
        self.sort_entries();
        self.save_config();
    }
}

pub(super) fn get_type_sort_key(e: &FileEntry) -> (u8, String) {
    if e.is_dir {
        if e.is_reparse_point {
            (1, e.name.to_lowercase())
        } else {
            (0, e.name.to_lowercase())
        }
    } else if let Some(ext) = e.path.extension().and_then(|s| s.to_str()) {
        (2, ext.to_lowercase())
    } else {
        (3, e.name.to_lowercase())
    }
}

pub(super) fn get_type_badge(entry: &FileEntry, is_dark: bool) -> (String, Color32, &'static str) {
    if entry.is_reparse_point {
        (
            "[LINK]".to_string(),
            if is_dark { Color32::from_rgb(200, 160, 255) } else { Color32::from_rgb(130, 70, 200) },
            "Link: Directory junction or symbolic link to another folder",
        )
    } else if entry.is_system_protected {
        (
            "[SYS]".to_string(),
            if is_dark { Color32::from_rgb(255, 175, 40) } else { Color32::from_rgb(200, 110, 0) },
            "System: Protected system directory or file",
        )
    } else if entry.is_access_denied {
        (
            "[LOCK]".to_string(),
            if is_dark { Color32::from_rgb(255, 90, 90) } else { Color32::from_rgb(200, 30, 30) },
            "Locked: Permission denied, folder cannot be scanned",
        )
    } else if entry.is_partially_locked {
        (
            "[PART]".to_string(),
            if is_dark { Color32::from_rgb(255, 190, 70) } else { Color32::from_rgb(210, 130, 10) },
            "Partially Locked: Accessible, but some subfolders were restricted",
        )
    } else if entry.is_dir {
        (
            "[DIR]".to_string(),
            if is_dark { Color32::from_rgb(80, 200, 255) } else { Color32::from_rgb(0, 120, 210) },
            "Directory: Standard folder",
        )
    } else if let Some(ext) = entry.path.extension().and_then(|e| e.to_str()) {
        let ext_upper = ext.to_uppercase();
        if ext_upper.len() <= 4 {
            (
                format!("[{}]", ext_upper),
                if is_dark { Color32::from_rgb(160, 185, 205) } else { Color32::from_rgb(60, 90, 120) },
                "File",
            )
        } else {
            (
                "[FILE]".to_string(),
                if is_dark { Color32::from_rgb(150, 160, 175) } else { Color32::from_rgb(90, 100, 115) },
                "File",
            )
        }
    } else {
        (
            "[FILE]".to_string(),
            if is_dark { Color32::from_rgb(150, 160, 175) } else { Color32::from_rgb(90, 100, 115) },
            "File",
        )
    }
}

impl eframe::App for FreeDataDietApp {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        let is_dark = match self.config.theme {
            AppTheme::Dark => true,
            AppTheme::Light => false,
            AppTheme::System => {
                // The OS theme rarely changes: poll it every couple of seconds instead of every frame
                if self.last_theme_check.elapsed() > Duration::from_secs(2) {
                    self.cached_system_dark = platform::is_system_dark_mode();
                    self.last_theme_check = Instant::now();
                }
                self.cached_system_dark
            }
        };
        if is_dark {
            ui.ctx().set_visuals(egui::Visuals::dark());
        } else {
            ui.ctx().set_visuals(egui::Visuals::light());
        }

        self.poll_scanner();

        if self.is_scanning {
            ui.ctx().request_repaint_after(Duration::from_millis(33));
        }

        // Global F1 / ? shortcut for Shortcuts Help (always active everywhere)
        let f1_pressed = ui.input(|i| {
            i.key_pressed(egui::Key::F1)
                || (i.modifiers.shift && i.key_pressed(egui::Key::Slash) && !ui.ctx().egui_wants_keyboard_input())
        });
        if f1_pressed {
            self.show_shortcuts_modal = !self.show_shortcuts_modal;
            self.shortcuts_just_opened = self.show_shortcuts_modal;
            if self.show_shortcuts_modal {
                self.shortcuts_selected_idx = 0;
            }
        }

        // Global Shortcuts (F-keys & Ctrl shortcuts) - active when enabled per-key
        let f11_pressed = ui.input(|i| i.key_pressed(egui::Key::F11));
        if self.config.shortcuts.f11_fullscreen && f11_pressed {
            let is_fs = ui.ctx().input(|i| i.viewport().fullscreen.unwrap_or(false));
            ui.ctx().send_viewport_cmd(egui::ViewportCommand::Fullscreen(!is_fs));
        }

        // Global F12, F4 & Ctrl+, for Live Settings & Favorites Left Panel
        let f12_pressed = ui.input(|i| i.key_pressed(egui::Key::F12));
        let f4_pressed = ui.input(|i| i.key_pressed(egui::Key::F4));
        let ctrl_comma_pressed = ui.input(|i| (i.modifiers.ctrl || i.modifiers.command) && i.key_pressed(egui::Key::Comma));
        if !self.show_shortcuts_modal
            && ((self.config.shortcuts.f12_settings && f12_pressed)
                || f4_pressed
                || (self.config.shortcuts.ctrl_comma_settings && ctrl_comma_pressed))
        {
            self.config.show_sidebar = !self.config.show_sidebar;
            self.save_config();
        }

        // Global F5 shortcut for Refresh
        let f5_pressed = ui.input(|i| i.key_pressed(egui::Key::F5));
        if self.config.shortcuts.f5_refresh
            && f5_pressed
            && !self.show_shortcuts_modal
            && self.delete_modal == DeleteConfirmState::None
        {
            self.trigger_scan();
        }

        // Global F6 shortcut for Recursive / Flatten Subfolders
        let f6_pressed = ui.input(|i| i.key_pressed(egui::Key::F6));
        if f6_pressed
            && !self.show_shortcuts_modal
            && self.delete_modal == DeleteConfirmState::None
        {
            self.config.flatten_subfolders = !self.config.flatten_subfolders;
            let msg = if self.config.flatten_subfolders {
                "Recursive subfolder scan enabled"
            } else {
                "Standard single-folder scan enabled"
            };
            self.status_toast = Some((msg.to_string(), Instant::now()));
            self.save_config();
            self.trigger_scan();
        }

        // Global F3 shortcut for Live Filter Search Bar (activate / toggle)
        let f3_pressed = ui.input(|i| i.key_pressed(egui::Key::F3));
        if self.config.shortcuts.f3_search_filter
            && f3_pressed
            && !self.show_shortcuts_modal
            && self.delete_modal == DeleteConfirmState::None
        {
            self.config.quick_search_mode = QuickSearchMode::LiveFilter;
            self.filter_bar_active = !self.filter_bar_active;
            if !self.filter_bar_active {
                self.quick_search_buffer.clear();
            }
            self.save_config();
        }

        if !ui.ctx().egui_wants_keyboard_input()
            && self.delete_modal == DeleteConfirmState::None
            && self.active_context_menu.is_none()
            && !self.show_shortcuts_modal
        {
            let ctrl = ui.input(|i| i.modifiers.ctrl || i.modifiers.command);
            let shift = ui.input(|i| i.modifiers.shift);
            let h_pressed = ui.input(|i| i.key_pressed(egui::Key::H));
            let d_pressed = ui.input(|i| i.key_pressed(egui::Key::D));
            let b_pressed = ui.input(|i| i.key_pressed(egui::Key::B));
            let f2_pressed = ui.input(|i| i.key_pressed(egui::Key::F2));

            if self.config.shortcuts.f2_search_mode && f2_pressed {
                self.config.quick_search_mode = match self.config.quick_search_mode {
                    QuickSearchMode::JumpToPrefix => QuickSearchMode::LiveFilter,
                    _ => QuickSearchMode::JumpToPrefix,
                };
                let mode_str = match self.config.quick_search_mode {
                    QuickSearchMode::LiveFilter => "Quick Search: Live Filter",
                    QuickSearchMode::JumpToPrefix => "Quick Search: Jump to Name",
                    QuickSearchMode::Disabled => "Quick Search: Disabled",
                };
                self.status_toast = Some((mode_str.to_string(), Instant::now()));
                self.quick_search_buffer.clear();
                self.save_config();
            } else if self.config.shortcuts.ctrl_shift_h_system && ctrl && shift && h_pressed {
                self.config.show_system_files = !self.config.show_system_files;
                self.save_config();
                self.sort_entries();
            } else if self.config.shortcuts.ctrl_h_hidden && ctrl && !shift && h_pressed {
                self.config.show_hidden_files = !self.config.show_hidden_files;
                self.save_config();
                self.sort_entries();
            } else if ctrl && shift && d_pressed {
                if let Some(sugg) = &self.active_suggestion {
                    let candidate_paths = sugg.candidate_paths.clone();
                    for cluster in &mut self.duplicate_clusters {
                        for occ in &mut cluster.occurrences {
                            if candidate_paths.contains(&occ.path) && !occ.is_system_protected {
                                occ.is_selected = true;
                            }
                        }
                    }
                    self.update_duplicate_assistant();
                }
            } else if ctrl && !shift && d_pressed {
                self.config.show_duplicates_only = !self.config.show_duplicates_only;
                self.save_config();
                self.trigger_scan();
            } else if ctrl && !shift && b_pressed {
                let added = LocationManager::toggle_favorite(&mut self.config, &self.current_path);
                let msg = if added {
                    "Added current folder to Favorites"
                } else {
                    "Removed current folder from Favorites"
                };
                self.status_toast = Some((msg.to_string(), Instant::now()));
            }
        }

        let mut target_drive = None;

        // --- TOP PANEL ---
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
                    {
                        if let Err(msg) = platform::restart_as_admin(Some(&self.current_path)) {
                            self.status_toast = Some((msg, Instant::now()));
                        }
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

                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if ui
                        .selectable_label(self.config.show_sidebar, RichText::new("Settings & Menu").strong())
                        .on_hover_text("Toggle live Settings, Filters & Favorites sidebar (F12 / Ctrl+,)")
                        .clicked()
                    {
                        self.config.show_sidebar = !self.config.show_sidebar;
                        self.save_config();
                    }

                    if ui
                        .selectable_label(
                            self.config.show_duplicates_only,
                            RichText::new("Duplicates").color(if self.config.show_duplicates_only {
                                Color32::from_rgb(255, 190, 60)
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
                            RichText::new("Recursive"),
                        )
                        .on_hover_text("Flatten and list all files across all subfolders (F6)")
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
                });

                if do_browse {
                    if let Some(folder) = rfd::FileDialog::new()
                        .set_directory(&self.current_path)
                        .pick_folder()
                    {
                        self.navigate_to(folder);
                    }
                }
                if toggle_duplicates {
                    self.config.show_duplicates_only = !self.config.show_duplicates_only;
                    self.save_config();
                    self.trigger_scan();
                }
                if toggle_flatten {
                    self.config.flatten_subfolders = !self.config.flatten_subfolders;
                    self.save_config();
                    self.trigger_scan();
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

        // --- LEFT PANEL: LIVE SETTINGS, FILTERS, FAVORITES & DRIVES ---
        if self.config.show_sidebar {
            Panel::left("left_settings_panel").show(ui, |ui| {
                self.render_left_panel(ui, is_dark);
            });
        }

        // --- BOTTOM PANEL ---
        Panel::bottom("bottom_panel").show(ui, |ui| {
            ui.add_space(4.0);
            ui.horizontal(|ui| {
                if let Some((_, time)) = &self.status_toast {
                    if time.elapsed() >= Duration::from_secs(3) {
                        self.status_toast = None;
                    } else {
                        ui.ctx().request_repaint_after(Duration::from_millis(250));
                    }
                }

                if let Some((toast, _)) = &self.status_toast {
                    ui.label(RichText::new(toast).color(Color32::from_rgb(100, 220, 150)).strong());
                } else if let Some(err) = &self.error_message {
                    ui.label(RichText::new(err).color(Color32::from_rgb(255, 100, 100)));
                } else if self.is_scanning {
                    ui.spinner();
                    ui.label(format!(
                        "Scanning storage... {} items analyzed",
                        self.scanned_counter
                    ));
                    if ui.button("Cancel").clicked() {
                        if let Some(scanner) = &self.scanner {
                            scanner.cancel();
                        }
                        self.scanner = None;
                        self.is_scanning = false;
                        for e in &mut self.entries {
                            e.is_scanning = false;
                        }
                    }
                } else if let Some(summary) = &self.summary {
                    ui.label(RichText::new(format!(
                        "Scanned in {:.2}s  |  {} items  |  Total: {} (On Disk: {})",
                        summary.duration.as_secs_f32(),
                        summary.total_files,
                        format_size(summary.total_size),
                        format_size(summary.total_allocated)
                    )).small());
                } else {
                    ui.label("Ready");
                }

                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    ui.label(
                        RichText::new("[Up/Down] Navigate  |  [Enter] Open  |  [Backspace] Up  |  [Del] Trash  |  [F1] Help  |  [F5] Rescan  |  [F12] Settings")
                            .small()
                            .color(if is_dark { Color32::DARK_GRAY } else { Color32::from_rgb(120, 120, 120) }),
                    );
                });
            });
            ui.add_space(4.0);
        });

        // --- CENTRAL DATA TABLE OR DUPLICATES PANEL ---
        egui::CentralPanel::default().show(ui, |ui| {
            if self.config.show_duplicates_only {
                self.render_duplicates_view(ui, is_dark);
            } else {
                self.render_standard_table(ui, is_dark);
            }
        });

        // --- MODALS (Context Menu, Delete Confirmation, Shortcuts F1, Update) ---
        self.render_modals(ui, is_dark);
    }
}
