pub mod batch_bar;
pub mod filter_strip;
pub mod modals;
pub mod sidebar;
pub mod status_bar;
pub mod table;
pub mod top_bar;

use std::collections::HashSet;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Instant;

use eframe::egui::{self, Panel};

use crate::config::{AppConfig, AppTheme, ColumnId, QuickSearchMode};
use crate::duplicate::{DuplicateCluster, DuplicateEngine, DuplicateSuggestion};
use crate::locations::LocationManager;
use crate::model::{DriveInfo, FileCategory, FileEntry, SortDirection};
use crate::platform;
use crate::scanner::{start_scan, ScanMessage, ScanSummary, ScannerHandle};
use crate::updater::UpdateManager;

use self::batch_bar::render_batch_and_assistant_bars;
use self::filter_strip::render_filter_strip;
use self::modals::delete_dialog::{
    render_delete_dialog, DeleteConfirmState, DeleteModalFocus,
};
use self::modals::settings_dialog::{render_settings_dialog, SettingsDialogState};
use self::modals::shortcuts_dialog::render_shortcuts_dialog;
use self::modals::update_dialog::render_update_dialog;
use self::sidebar::render_sidebar;
use self::status_bar::render_status_bar;
use self::table::render_file_table;
use self::top_bar::render_top_bar;

pub struct FreeDataDietApp {
    pub config: AppConfig,
    pub location_mgr: LocationManager,
    pub current_path: PathBuf,
    pub path_input: String,
    pub drives: Vec<DriveInfo>,
    pub entries: Vec<FileEntry>,
    pub duplicate_clusters: Vec<DuplicateCluster>,
    pub selected_paths: HashSet<PathBuf>,
    pub focused_idx: Option<usize>,

    // Search and Filters
    pub search_query: String,
    pub filter_bar_active: bool,
    pub active_suggestion: Option<DuplicateSuggestion>,
    pub safety_warning: Option<String>,

    // Scanner
    pub scanner: Option<ScannerHandle>,
    pub is_scanning: bool,
    pub scanned_counter: usize,
    pub summary: Option<ScanSummary>,
    pub error_message: Option<String>,

    // Modals
    pub delete_modal: DeleteConfirmState,
    pub delete_focus: DeleteModalFocus,
    pub show_settings_modal: bool,
    pub settings_state: SettingsDialogState,
    pub show_shortcuts_modal: bool,
    pub show_update_modal: bool,

    // Status & System
    pub status_toast: Option<(String, Instant)>,
    pub is_admin: bool,
    pub updater: UpdateManager,
    pub cancel_duplicate_hashing: Arc<AtomicBool>,
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
            path_input: start_path.to_string_lossy().to_string(),
            current_path: start_path,
            config,
            location_mgr,
            drives,
            entries: Vec::new(),
            duplicate_clusters: Vec::new(),
            selected_paths: HashSet::new(),
            focused_idx: None,
            search_query: String::new(),
            filter_bar_active: false,
            active_suggestion: None,
            safety_warning: None,
            scanner: None,
            is_scanning: false,
            scanned_counter: 0,
            summary: None,
            error_message: None,
            delete_modal: DeleteConfirmState::None,
            delete_focus: DeleteModalFocus::Trash,
            show_settings_modal: false,
            settings_state: SettingsDialogState::default(),
            show_shortcuts_modal: false,
            show_update_modal: false,
            status_toast: None,
            is_admin,
            updater: UpdateManager::new(),
            cancel_duplicate_hashing: Arc::new(AtomicBool::new(false)),
        };

        app.trigger_scan();
        app
    }

    pub fn trigger_scan(&mut self) {
        if let Some(scanner) = &self.scanner {
            scanner.cancel();
        }
        self.cancel_duplicate_hashing.store(true, Ordering::SeqCst);
        self.cancel_duplicate_hashing = Arc::new(AtomicBool::new(false));

        self.entries.clear();
        self.duplicate_clusters.clear();
        self.selected_paths.clear();
        self.focused_idx = None;
        self.active_suggestion = None;
        self.safety_warning = None;
        self.summary = None;
        self.error_message = None;
        self.is_scanning = true;
        self.scanned_counter = 0;

        self.path_input = self.current_path.to_string_lossy().to_string();
        let excludes = self.collect_active_excludes();
        let flatten = self.config.flatten_subfolders;

        self.scanner = Some(start_scan(self.current_path.clone(), excludes, flatten));
    }

    fn collect_active_excludes(&self) -> Vec<String> {
        let mut list = Vec::new();
        for p in &self.config.excluded_patterns {
            if p.enabled {
                list.push(p.pattern.clone());
            }
        }
        list
    }

    pub fn navigate_to(&mut self, path: PathBuf) {
        if path.is_dir() {
            self.search_query.clear();
            self.filter_bar_active = false;
            self.current_path = path;
            if self.config.remember_last_path {
                self.config.last_path = Some(self.current_path.clone());
                let _ = self.config.save();
            }
            self.trigger_scan();
        }
    }

    pub fn navigate_up(&mut self) {
        if let Some(parent) = self.current_path.parent() {
            self.navigate_to(parent.to_path_buf());
        }
    }

    fn check_scanner_messages(&mut self) {
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
                ScanMessage::InitialEntries(new_entries) => {
                    self.entries.extend(new_entries);
                    needs_sort = true;
                }
                ScanMessage::EntryUpdated(updated) => {
                    if let Some(idx) = self.entries.iter().position(|e| e.path == updated.path) {
                        self.entries[idx] = updated;
                    } else {
                        self.entries.push(updated);
                    }
                }
                ScanMessage::Completed(sum) => {
                    self.summary = Some(sum);
                    self.is_scanning = false;
                    needs_sort = true;
                    if self.config.show_duplicates_only {
                        needs_duplicates = true;
                    }
                }
                ScanMessage::Failed(err) => {
                    self.error_message = Some(err);
                    self.is_scanning = false;
                }
            }
        }

        if needs_sort {
            self.sort_entries();
        }
        if needs_duplicates {
            self.compute_duplicates();
        }
    }

    pub fn compute_duplicates(&mut self) {
        self.duplicate_clusters = DuplicateEngine::find_duplicates(
            &self.entries,
            self.config.duplicate_criteria,
            &self.cancel_duplicate_hashing,
        );
        self.update_duplicate_assistant();
    }

    pub fn update_duplicate_assistant(&mut self) {
        self.active_suggestion = DuplicateEngine::find_folder_suggestion(
            &self.duplicate_clusters,
            &self.selected_paths,
        );
    }

    fn sort_entries(&mut self) {
        if let Some(rule) = self.config.sort_rules.first() {
            let desc = rule.direction == SortDirection::Descending;
            match rule.column {
                ColumnId::Name => {
                    self.entries.sort_by(|a, b| {
                        let cmp = a.name.to_lowercase().cmp(&b.name.to_lowercase());
                        if desc { cmp.reverse() } else { cmp }
                    });
                }
                ColumnId::Size => {
                    self.entries.sort_by(|a, b| {
                        let cmp = a.size.cmp(&b.size);
                        if desc { cmp.reverse() } else { cmp }
                    });
                }
                ColumnId::AllocatedSize => {
                    self.entries.sort_by(|a, b| {
                        let cmp = a.allocated_size.cmp(&b.allocated_size);
                        if desc { cmp.reverse() } else { cmp }
                    });
                }
                ColumnId::Items => {
                    self.entries.sort_by(|a, b| {
                        let cmp = a.item_count.cmp(&b.item_count);
                        if desc { cmp.reverse() } else { cmp }
                    });
                }
                ColumnId::Modified => {
                    self.entries.sort_by(|a, b| {
                        let epoch = std::time::SystemTime::UNIX_EPOCH;
                        let ta = a.modified.unwrap_or(epoch);
                        let tb = b.modified.unwrap_or(epoch);
                        let cmp = ta.cmp(&tb);
                        if desc { cmp.reverse() } else { cmp }
                    });
                }
                _ => {}
            }
        }
    }

    fn filter_visible_entries(&self) -> Vec<FileEntry> {
        let query = self.search_query.trim().to_lowercase();
        let cats = &self.config.category_filters;

        self.entries
            .iter()
            .filter(|e| {
                if !self.config.show_hidden_files && e.is_hidden {
                    return false;
                }
                if !self.config.show_system_files && e.is_system_protected {
                    return false;
                }
                if !self.config.show_locked_folders && e.is_access_denied {
                    return false;
                }

                if !e.is_dir {
                    match e.category {
                        FileCategory::Image if !cats.show_images => return false,
                        FileCategory::Video if !cats.show_videos => return false,
                        FileCategory::Audio if !cats.show_audio => return false,
                        FileCategory::Document if !cats.show_documents => return false,
                        FileCategory::Archive if !cats.show_archives => return false,
                        _ => {}
                    }
                }

                if !query.is_empty() {
                    let matches_name = e.name.to_lowercase().contains(&query);
                    let matches_ext = e
                        .path
                        .extension()
                        .map(|ext| ext.to_string_lossy().to_lowercase().contains(&query))
                        .unwrap_or(false);
                    if !matches_name && !matches_ext {
                        return false;
                    }
                }

                true
            })
            .cloned()
            .collect()
    }

    fn execute_batch_delete(&mut self, paths: Vec<PathBuf>, permanent: bool) {
        let mut deleted = 0;
        let mut skipped = 0;

        for path in paths {
            if crate::model::is_root_or_system(&path) {
                skipped += 1;
                continue;
            }

            let res = if permanent {
                if path.is_dir() {
                    std::fs::remove_dir_all(&path).map_err(|e| e.to_string())
                } else {
                    std::fs::remove_file(&path).map_err(|e| e.to_string())
                }
            } else {
                trash::delete(&path).map_err(|e| e.to_string())
            };

            match res {
                Ok(_) => deleted += 1,
                Err(_) => skipped += 1,
            }
        }

        self.selected_paths.clear();
        self.status_toast = Some((
            format!("Deleted {} items ({} skipped)", deleted, skipped),
            Instant::now(),
        ));
        self.trigger_scan();
    }

    fn handle_keyboard_shortcuts(&mut self, ctx: &egui::Context) {
        let input = ctx.input(|i| i.clone());

        if input.key_pressed(egui::Key::F1) {
            self.show_shortcuts_modal = true;
        }
        if input.key_pressed(egui::Key::F2) {
            self.config.quick_search_mode = match self.config.quick_search_mode {
                QuickSearchMode::JumpToPrefix => QuickSearchMode::LiveFilter,
                QuickSearchMode::LiveFilter => QuickSearchMode::Disabled,
                QuickSearchMode::Disabled => QuickSearchMode::JumpToPrefix,
            };
            let _ = self.config.save();
        }
        if input.key_pressed(egui::Key::F4) {
            self.config.show_sidebar = !self.config.show_sidebar;
            let _ = self.config.save();
        }
        if input.key_pressed(egui::Key::F5) {
            self.trigger_scan();
        }
        if input.key_pressed(egui::Key::F6) {
            self.config.flatten_subfolders = !self.config.flatten_subfolders;
            let _ = self.config.save();
            self.trigger_scan();
        }
        if input.key_pressed(egui::Key::F12) || (input.modifiers.command && input.key_pressed(egui::Key::Comma)) {
            self.show_settings_modal = true;
        }

        if input.modifiers.command && input.key_pressed(egui::Key::D) {
            if input.modifiers.shift {
                // Ctrl+Shift+D: Apply duplicate assistant suggestion
                if let Some(sugg) = &self.active_suggestion {
                    for p in &sugg.candidate_paths {
                        self.selected_paths.insert(p.clone());
                    }
                    self.update_duplicate_assistant();
                }
            } else {
                // Ctrl+D: Toggle duplicates mode
                self.config.show_duplicates_only = !self.config.show_duplicates_only;
                let _ = self.config.save();
                self.trigger_scan();
            }
        }

        if input.modifiers.command && input.key_pressed(egui::Key::B) {
            LocationManager::add_location(&mut self.config, self.current_path.clone(), None);
            self.status_toast = Some(("Added current folder to Favorites".into(), Instant::now()));
        }

        if input.modifiers.command && input.key_pressed(egui::Key::A) {
            for e in self.filter_visible_entries() {
                self.selected_paths.insert(e.path);
            }
        }

        if input.key_pressed(egui::Key::Escape) {
            if !self.selected_paths.is_empty() {
                self.selected_paths.clear();
            }
            self.delete_modal = DeleteConfirmState::None;
            self.show_settings_modal = false;
            self.show_shortcuts_modal = false;
            self.show_update_modal = false;
        }

        if input.key_pressed(egui::Key::Delete) {
            if !self.selected_paths.is_empty() {
                let paths: Vec<PathBuf> = self.selected_paths.iter().cloned().collect();
                let total_size: u64 = self
                    .entries
                    .iter()
                    .filter(|e| self.selected_paths.contains(&e.path))
                    .map(|e| e.size)
                    .sum();
                let has_protected = self
                    .entries
                    .iter()
                    .any(|e| self.selected_paths.contains(&e.path) && e.is_system_protected);

                self.delete_modal = DeleteConfirmState::Batch {
                    paths,
                    total_size,
                    total_count: self.selected_paths.len(),
                    has_protected,
                };
            }
        }
    }
}

impl eframe::App for FreeDataDietApp {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        let ctx = ui.ctx().clone();
        self.check_scanner_messages();
        self.handle_keyboard_shortcuts(&ctx);

        let is_dark = match self.config.theme {
            AppTheme::Dark => true,
            AppTheme::Light => false,
            AppTheme::System => platform::is_system_dark_mode(),
        };

        if is_dark {
            ctx.set_visuals(egui::Visuals::dark());
        } else {
            ctx.set_visuals(egui::Visuals::light());
        }

        // --- TOP PANEL ---
        Panel::top("top_panel").show(ui, |ui| {
            ui.add_space(4.0);

            let top_action = render_top_bar(
                ui,
                &self.current_path,
                &mut self.path_input,
                self.is_admin,
                &self.updater,
                is_dark,
            );

            if let Some(target) = top_action.navigate_to {
                self.navigate_to(target);
            }
            if top_action.navigate_up {
                self.navigate_up();
            }
            if top_action.trigger_browse {
                if let Some(folder) = rfd::FileDialog::new().pick_folder() {
                    self.navigate_to(folder);
                }
            }
            if top_action.trigger_rescan {
                self.trigger_scan();
            }
            if top_action.open_settings {
                self.show_settings_modal = true;
            }
            if top_action.open_update_modal {
                self.show_update_modal = true;
            }
            if let Some(new_exe) = top_action.restart_and_apply_update {
                self.updater.restart_and_apply(new_exe);
            }
            if let Some(err) = top_action.restart_admin_error {
                self.status_toast = Some((err, Instant::now()));
            }

            ui.add_space(4.0);

            // Filter Strip Pills
            let filter_action = render_filter_strip(
                ui,
                &mut self.config,
                &mut self.search_query,
                &mut self.filter_bar_active,
                is_dark,
            );
            if filter_action.rescan_needed {
                self.trigger_scan();
            }

            ui.add_space(4.0);
        });

        // --- LEFT SIDEBAR (Collapsible F4) ---
        if self.config.show_sidebar {
            Panel::left("left_sidebar").show(ui, |ui| {
                let side_action = render_sidebar(
                    ui,
                    &mut self.config,
                    &self.drives,
                    &self.current_path,
                    &self.location_mgr,
                    is_dark,
                );

                if let Some(path) = side_action.navigate_to {
                    self.navigate_to(path);
                }
                if side_action.add_favorite {
                    LocationManager::add_location(&mut self.config, self.current_path.clone(), None);
                    self.status_toast = Some(("Added current folder to Favorites".into(), Instant::now()));
                }
                if side_action.rescan_needed {
                    self.trigger_scan();
                }
            });
        }

        // --- BOTTOM PANEL ---
        Panel::bottom("bottom_panel").show(ui, |ui| {
            ui.add_space(4.0);

            let total_selected_bytes: u64 = self
                .entries
                .iter()
                .filter(|e| self.selected_paths.contains(&e.path))
                .map(|e| e.size)
                .sum();

            let batch_action = render_batch_and_assistant_bars(
                ui,
                &self.selected_paths,
                total_selected_bytes,
                self.active_suggestion.as_ref(),
                self.safety_warning.as_deref(),
                is_dark,
            );

            if batch_action.trigger_trash {
                let paths: Vec<PathBuf> = self.selected_paths.iter().cloned().collect();
                self.delete_modal = DeleteConfirmState::Batch {
                    paths,
                    total_size: total_selected_bytes,
                    total_count: self.selected_paths.len(),
                    has_protected: false,
                };
            }
            if batch_action.trigger_permanent {
                let paths: Vec<PathBuf> = self.selected_paths.iter().cloned().collect();
                self.execute_batch_delete(paths, true);
            }
            if batch_action.clear_selection {
                self.selected_paths.clear();
                self.update_duplicate_assistant();
            }
            if let Some(paths) = batch_action.apply_suggestion {
                for p in paths {
                    self.selected_paths.insert(p);
                }
                self.update_duplicate_assistant();
            }
            if batch_action.dismiss_suggestion {
                self.active_suggestion = None;
            }
            if batch_action.clear_safety_warning {
                self.safety_warning = None;
            }

            let status_action = render_status_bar(
                ui,
                &mut self.status_toast,
                self.error_message.as_deref(),
                self.is_scanning,
                self.scanned_counter,
                self.summary.as_ref(),
                is_dark,
            );

            if status_action.cancel_scan {
                if let Some(s) = &self.scanner {
                    s.cancel();
                }
                self.is_scanning = false;
            }

            ui.add_space(4.0);
        });

        // --- CENTRAL PANEL: TABLE ---
        egui::CentralPanel::default().show(ui, |ui| {
            let visible_entries = self.filter_visible_entries();
            let table_action = render_file_table(
                ui,
                &mut self.config,
                &visible_entries,
                &mut self.duplicate_clusters,
                &mut self.selected_paths,
                &mut self.focused_idx,
                is_dark,
            );

            if let Some(p) = table_action.navigate_to {
                self.navigate_to(p);
            }
            if let Some(p) = table_action.open_path {
                let _ = platform::open_file(&p);
            }
            if let Some(p) = table_action.open_in_explorer {
                let _ = platform::open_in_file_manager(&p);
            }
            if let Some(p) = table_action.toggle_favorite {
                let added = LocationManager::toggle_favorite(&mut self.config, &p);
                let msg = if added {
                    "Added folder to Favorites"
                } else {
                    "Removed folder from Favorites"
                };
                self.status_toast = Some((msg.to_string(), Instant::now()));
            }
            if let Some(entry) = table_action.request_delete_single {
                self.delete_modal = DeleteConfirmState::Single {
                    path: entry.path,
                    name: entry.name,
                    is_dir: entry.is_dir,
                    size: entry.size,
                    allocated_size: entry.allocated_size,
                    item_count: entry.item_count,
                    is_protected: entry.is_system_protected,
                };
            }
            if let Some(warn) = table_action.safety_warning {
                self.safety_warning = Some(warn);
            }
        });

        // --- MODALS ---
        if let Some(act) = render_delete_dialog(&ctx, &self.delete_modal, &mut self.delete_focus) {
            if act.confirmed_trash {
                match &self.delete_modal {
                    DeleteConfirmState::Single { path, .. } => {
                        let p = path.clone();
                        self.delete_modal = DeleteConfirmState::None;
                        self.execute_batch_delete(vec![p], false);
                    }
                    DeleteConfirmState::Batch { paths, .. } => {
                        let p = paths.clone();
                        self.delete_modal = DeleteConfirmState::None;
                        self.execute_batch_delete(p, false);
                    }
                    DeleteConfirmState::None => {}
                }
            } else if act.confirmed_permanent {
                match &self.delete_modal {
                    DeleteConfirmState::Single { path, .. } => {
                        let p = path.clone();
                        self.delete_modal = DeleteConfirmState::None;
                        self.execute_batch_delete(vec![p], true);
                    }
                    DeleteConfirmState::Batch { paths, .. } => {
                        let p = paths.clone();
                        self.delete_modal = DeleteConfirmState::None;
                        self.execute_batch_delete(p, true);
                    }
                    DeleteConfirmState::None => {}
                }
            } else if act.cancelled {
                self.delete_modal = DeleteConfirmState::None;
            }
        }

        render_settings_dialog(
            &ctx,
            &mut self.config,
            &mut self.settings_state,
            &mut self.show_settings_modal,
        );

        render_shortcuts_dialog(&ctx, &mut self.show_shortcuts_modal);
        render_update_dialog(&ctx, &mut self.updater, &mut self.show_update_modal);
    }
}
