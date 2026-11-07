pub mod context_menu;
pub mod duplicates_view;
pub mod modals;
pub mod shortcuts_dialog;
pub mod shortcuts_handler;
pub mod sidebar;
pub mod sidebar_settings;
pub mod sorting;
pub mod status_bar;
pub mod table;
pub mod table_nav;
pub mod top_bar;

pub use self::context_menu::ActiveContextMenu;
pub use self::sorting::get_type_badge;

use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

use eframe::egui::{self, Panel};

use crate::config::{AppConfig, AppTheme, QuickSearchMode};
use crate::duplicate::{filter_terms, DuplicateCluster, DuplicateEngine};
use crate::duplicate_suggestion::DuplicateSuggestion;
use crate::locations::LocationManager;
use crate::model::{DriveInfo, FileEntry};
use crate::platform;
use crate::scanner::{start_scan, ScanMessage, ScanSummary, ScannerHandle};

use self::modals::{DeleteConfirmState, DeleteModalFocus};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ActivePanelFocus {
    #[default]
    MainView,
    SettingsPanel,
}

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
    pub(super) selected_favorites: std::collections::HashSet<PathBuf>,
    pub(super) panel_focus: ActivePanelFocus,
    pub(super) settings_nav_idx: usize,

    // Duplicates & Assistant
    pub(super) duplicate_clusters: Vec<DuplicateCluster>,
    pub(super) active_suggestion: Option<DuplicateSuggestion>,
    pub(super) safety_warning: Option<(String, Option<PathBuf>)>,
    pub(super) cancel_duplicate_hashing: Arc<AtomicBool>,
    pub(super) is_finding_duplicates: bool,
    pub(super) duplicate_started: Instant,
    /// Text filter for the duplicate list; matching groups are shown with all their copies.
    pub(super) duplicate_filter: String,
    pub(super) focus_duplicate_filter: bool,
    /// Minimum file size (KB) the current duplicate results were computed with.
    pub(super) duplicate_computed_min_kb: u64,
    /// Live minimum size (bytes) shared with the running duplicate search.
    pub(super) duplicate_min_size_live: Arc<AtomicU64>,
    pub(super) duplicate_duration: Option<Duration>,
    pub(super) duplicate_progress_text: String,
    pub(super) duplicate_receiver: Option<std::sync::mpsc::Receiver<crate::duplicate::DuplicateMessage>>,
    pub(super) duplicate_total_wasted: u64,
    pub(super) duplicate_eligible_count: usize,
    pub(super) duplicate_eligible_bytes: u64,
    pub(super) duplicate_blocked_count: usize,
    pub(super) selected_duplicate_row_idx: usize,
    pub(super) ignored_cluster_ids: std::collections::HashSet<String>,
    pub(super) cached_duplicate_flat_rows: Vec<duplicates_view::DuplicateRowItem>,
    pub(super) duplicate_flat_rows_dirty: bool,
    pub(super) empty_folders_prompt: Option<Vec<PathBuf>>,

    // Drive root confirmation modal
    pub(super) show_root_subfolder_confirm: bool,

    // Throttled scan sorting & table cache
    pub(super) last_scan_sort: Instant,
    pub(super) visible_indices: Vec<usize>,
    pub(super) visible_indices_dirty: bool,

    // Quick Search
    pub(super) quick_search_buffer: String,
    pub(super) quick_search_time: Option<Instant>,
    pub(super) filter_bar_active: bool,
    pub(super) cached_system_dark: bool,
    pub(super) last_theme_check: Instant,

    // Exclude filters
    pub(super) ignore_dev_caches: bool,
    pub(super) ignore_media_caches: bool,
    pub(super) ignore_temp_caches: bool,
    pub(super) show_custom_excludes_input: bool,
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

/// A group is listed, counted and acted on when it is not ignored and matches the filter.
pub(super) fn cluster_visible(
    cluster: &DuplicateCluster,
    ignored: &std::collections::HashSet<String>,
    terms: &[crate::duplicate::DuplicateFilterTerm],
) -> bool {
    !ignored.contains(&cluster.cluster_id) && cluster.matches_filter(terms)
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
            ignore_media_caches: config.ignore_media_caches,
            ignore_temp_caches: config.ignore_temp_caches,
            show_custom_excludes_input: false,
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
            selected_favorites: std::collections::HashSet::new(),
            panel_focus: ActivePanelFocus::MainView,
            settings_nav_idx: 0,
            duplicate_clusters: Vec::new(),
            active_suggestion: None,
            safety_warning: None,
            cancel_duplicate_hashing: Arc::new(AtomicBool::new(false)),
            is_finding_duplicates: false,
            duplicate_started: Instant::now(),
            duplicate_filter: String::new(),
            focus_duplicate_filter: false,
            duplicate_computed_min_kb: 0,
            duplicate_min_size_live: Arc::new(AtomicU64::new(0)),
            duplicate_duration: None,
            duplicate_progress_text: String::new(),
            duplicate_receiver: None,
            duplicate_total_wasted: 0,
            duplicate_eligible_count: 0,
            duplicate_eligible_bytes: 0,
            duplicate_blocked_count: 0,
            selected_duplicate_row_idx: 0,
            ignored_cluster_ids: std::collections::HashSet::new(),
            cached_duplicate_flat_rows: Vec::new(),
            duplicate_flat_rows_dirty: true,
            empty_folders_prompt: None,
            show_root_subfolder_confirm: false,
            last_scan_sort: Instant::now(),
            visible_indices: Vec::new(),
            visible_indices_dirty: true,
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
        self.config.ignore_media_caches = self.ignore_media_caches;
        self.config.ignore_temp_caches = self.ignore_temp_caches;
        self.config.custom_excludes = self.custom_excludes.clone();
        if self.config.remember_last_path {
            self.config.last_path = Some(self.current_path.clone());
        } else {
            self.config.last_path = None;
        }
        let _ = self.config.save();
    }

    pub(super) fn collect_excludes(&self) -> Vec<String> {
        let mut list: Vec<String> = Vec::new();
        let mut add = |item: &str| {
            if !list.iter().any(|existing| existing.eq_ignore_ascii_case(item)) {
                list.push(item.to_string());
            }
        };

        let presets = [
            (self.ignore_dev_caches, &self.config.preset_dev_patterns),
            (self.ignore_media_caches, &self.config.preset_media_patterns),
            (self.ignore_temp_caches, &self.config.preset_temp_patterns),
        ];
        for (_, patterns) in presets.iter().filter(|(enabled, _)| *enabled) {
            patterns.split(',').map(str::trim).filter(|s| !s.is_empty()).for_each(&mut add);
        }        for item in self.custom_excludes.split(',').map(str::trim).filter(|s| !s.is_empty()) {
            add(item);
        }
        for p in self.config.excluded_patterns.iter().filter(|p| p.enabled) {
            add(&p.pattern);
        }
        for loc in self.config.excluded_locations.iter().filter(|l| {
            l.enabled && l.scope == crate::config::ExcludeScope::HideFromScan
        }) {
            add(&loc.path.to_string_lossy());
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
        self.duplicate_receiver = None;
        self.is_finding_duplicates = false;
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
        let flatten = self.config.flatten_subfolders;
        let scan_roots = if self.selected_favorites.len() > 1 {
            self.selected_favorites.iter().cloned().collect()
        } else {
            vec![self.current_path.clone()]
        };
        self.scanner = Some(start_scan(scan_roots, excludes, flatten, self.config.show_duplicates_only && !flatten));
    }

    pub(super) fn navigate_to(&mut self, path: PathBuf) {
        if path.is_dir() {
            self.quick_search_buffer.clear();
            self.quick_search_time = None;
            self.filter_bar_active = false;
            self.duplicate_filter.clear();
            self.selected_favorites.clear();
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
        self.cancel_duplicate_hashing.store(true, Ordering::SeqCst);
        self.cancel_duplicate_hashing = Arc::new(AtomicBool::new(false));

        self.duplicate_clusters.clear();
        self.active_suggestion = None;
        self.safety_warning = None;
        self.is_finding_duplicates = true;
        self.duplicate_started = Instant::now();
        self.duplicate_duration = None;
        self.duplicate_progress_text = "Starting duplicate scan...".to_string();

        let ignore_dup_paths: Vec<PathBuf> = self
            .config
            .excluded_locations
            .iter()
            .filter(|l| l.enabled && l.scope == crate::config::ExcludeScope::IgnoreForDuplicates)
            .map(|l| l.path.clone())
            .collect();

        self.duplicate_computed_min_kb = self.config.duplicate_min_size_kb;
        let min_size = self.config.duplicate_min_size_kb * 1024;
        self.duplicate_min_size_live = Arc::new(AtomicU64::new(min_size));
        let live_min = Arc::clone(&self.duplicate_min_size_live);
        let entries_clone: Vec<FileEntry> = self
            .entries
            .iter()
            .filter(|e| e.size >= min_size && !ignore_dup_paths.iter().any(|ignored| e.path.starts_with(ignored)))
            .cloned()
            .collect();

        let criteria = self.config.duplicate_criteria;
        let cancel = Arc::clone(&self.cancel_duplicate_hashing);
        let (tx, rx) = std::sync::mpsc::channel();

        std::thread::spawn(move || {
            let _ = DuplicateEngine::find_duplicates(&entries_clone, criteria, &cancel, &live_min, Some(&tx));
        });

        self.duplicate_receiver = Some(rx);
    }

    /// Reacts to a changed minimum size. Raising it only trims existing results (no
    /// re-hashing, a running search skips the remaining small files); lowering it needs new
    /// files, so it restarts.
    pub(super) fn on_min_size_changed(&mut self) {
        if self.is_scanning {
            return; // the search has not started yet and will read the new value
        }
        let raised = self.config.duplicate_min_size_kb > self.duplicate_computed_min_kb;
        if raised && self.config.duplicate_criteria.match_size {
            if self.is_finding_duplicates {
                // Remaining small files are skipped by the running search; results are trimmed on completion.
                self.duplicate_min_size_live
                    .store(self.config.duplicate_min_size_kb * 1024, Ordering::Relaxed);
            } else {
                self.trim_clusters_to_min_size();
                self.recompute_duplicate_stats();
                self.duplicate_flat_rows_dirty = true;
            }
        } else {
            self.compute_duplicates();
        }
    }

    /// Drops groups whose files are below the current minimum size.
    fn trim_clusters_to_min_size(&mut self) {
        if self.config.duplicate_criteria.match_size {
            let min_bytes = self.config.duplicate_min_size_kb * 1024;
            self.duplicate_clusters.retain(|c| c.file_size >= min_bytes);
        }
        self.duplicate_computed_min_kb = self.config.duplicate_min_size_kb;
    }
    pub(super) fn recompute_duplicate_stats(&mut self) {
        let mut total_wasted = 0u64;
        let mut eligible_count = 0usize;
        let mut eligible_bytes = 0u64;
        let mut blocked_count = 0usize;

        let terms = filter_terms(&self.duplicate_filter);
        for cluster in &self.duplicate_clusters {
            if !cluster_visible(cluster, &self.ignored_cluster_ids, &terms) {
                continue;
            }
            total_wasted += cluster.filtered_wasted_bytes(&terms);
            let paths = cluster.filtered_deletable_paths(&terms);
            if paths.is_empty() {
                if let Some(valid_indices) = cluster.filtered_occurrences(&terms) {
                    let all_valid_selected = !valid_indices.is_empty()
                        && valid_indices.iter().all(|&idx| cluster.occurrences[idx].is_selected);
                    if all_valid_selected && !cluster.is_group_selected {
                        blocked_count += 1;
                    }
                }
            } else {
                eligible_count += paths.len();
                eligible_bytes += paths.len() as u64 * cluster.file_size;
            }
        }

        self.duplicate_total_wasted = total_wasted;
        self.duplicate_eligible_count = eligible_count;
        self.duplicate_eligible_bytes = eligible_bytes;
        self.duplicate_blocked_count = blocked_count;
    }

    pub(super) fn apply_category_duplicate_rules(&mut self) {
        if !self.config.enable_duplicate_suggestions {
            return;
        }
        let rules = self.config.category_duplicate_rules;
        let terms = filter_terms(&self.duplicate_filter);
        for cluster in &mut self.duplicate_clusters {
            if !cluster_visible(cluster, &self.ignored_cluster_ids, &terms) {
                continue;
            }
            let cat = crate::model::FileCategory::from_path_and_dir(
                std::path::Path::new(&cluster.primary_name),
                false,
            );
            let pref = match cat {
                crate::model::FileCategory::Image => rules.images,
                crate::model::FileCategory::Video => rules.videos,
                crate::model::FileCategory::Audio => rules.audio,
                crate::model::FileCategory::Document => rules.documents,
                crate::model::FileCategory::Archive => rules.archives,
                crate::model::FileCategory::Other | crate::model::FileCategory::Folder => {
                    crate::config::DuplicateRulePreference::Off
                }
            };

            if pref == crate::config::DuplicateRulePreference::Off {
                continue;
            }

            cluster.keep_one_filtered(pref, &terms);
        }
        self.safety_warning = None;
        self.active_suggestion = None;
        self.recompute_duplicate_stats();
    }

    pub(super) fn update_visible_indices(&mut self) {
        let cats = self.config.category_filters;
        let search_filter = self.config.quick_search_mode == QuickSearchMode::LiveFilter
            && !self.quick_search_buffer.is_empty();
        let search_query = self.quick_search_buffer.to_lowercase();

        self.visible_indices = self
            .entries
            .iter()
            .enumerate()
            .filter(|(_, e)| {
                if !self.config.show_hidden_files
                    && (e.is_hidden || (e.is_reparse_point && e.is_system_file))
                {
                    return false;
                }
                if !self.config.show_system_files && e.is_system_file {
                    return false;
                }
                if !self.config.show_locked_folders && e.is_access_denied {
                    return false;
                }
                if !e.is_dir {
                    match e.category {
                        crate::model::FileCategory::Image if !cats.show_images => return false,
                        crate::model::FileCategory::Video if !cats.show_videos => return false,
                        crate::model::FileCategory::Audio if !cats.show_audio => return false,
                        crate::model::FileCategory::Document if !cats.show_documents => {
                            return false
                        }
                        crate::model::FileCategory::Archive if !cats.show_archives => {
                            return false
                        }
                        _ => {}
                    }
                }
                if search_filter {
                    let name_lower = e.name.to_lowercase();
                    for token in search_query.split_whitespace() {
                        if token.is_empty() {
                            continue;
                        }
                        if let Some(rest) = token.strip_prefix('-') {
                            if rest.is_empty() {
                                // Lone '-': literal '-' search
                                if !name_lower.contains('-') {
                                    return false;
                                }
                            } else if let Some(escaped) = rest.strip_prefix('\\') {
                                // '-\-something': must NOT contain '-something'
                                if !escaped.is_empty() && name_lower.contains(escaped) {
                                    return false;
                                }
                            } else {
                                // '-something': must NOT contain 'something'
                                if name_lower.contains(rest) {
                                    return false;
                                }
                            }
                        } else if let Some(escaped) = token.strip_prefix('\\') {
                            if !escaped.is_empty() && !name_lower.contains(escaped) {
                                return false;
                            }
                        } else if !name_lower.contains(token) {
                            return false;
                        }
                    }
                }
                true
            })
            .map(|(idx, _)| idx)
            .collect();

        self.visible_indices_dirty = false;
    }

    pub(super) fn sort_duplicate_clusters(&mut self) {
        let col = self.config.duplicate_sort_column;
        let dir = self.config.duplicate_sort_direction;

        self.duplicate_clusters.sort_by(|a, b| {
            let cmp = match col {
                crate::config::DuplicateSortColumn::WastedSpace => a.wasted_bytes().cmp(&b.wasted_bytes()),
                crate::config::DuplicateSortColumn::FileSize => a.file_size.cmp(&b.file_size),
                crate::config::DuplicateSortColumn::Name => {
                    a.primary_name.to_lowercase().cmp(&b.primary_name.to_lowercase())
                }
                crate::config::DuplicateSortColumn::Count => a.occurrences.len().cmp(&b.occurrences.len()),
            };
            match dir {
                crate::model::SortDirection::Ascending => cmp,
                crate::model::SortDirection::Descending => cmp.reverse(),
            }
        });

        self.duplicate_flat_rows_dirty = true;
    }

    pub(super) fn on_manual_duplicate_toggled(
        &mut self,
        toggled_path: &std::path::Path,
        is_now_selected: bool,
    ) {
        if !self.config.enable_duplicate_suggestions {
            self.active_suggestion = None;
            return;
        }

        let terms = filter_terms(&self.duplicate_filter);
        let active_clusters: Vec<DuplicateCluster> = self
            .duplicate_clusters
            .iter()
            .filter(|c| cluster_visible(c, &self.ignored_cluster_ids, &terms))
            .cloned()
            .collect();

        self.active_suggestion = crate::duplicate_suggestion::find_manual_folder_suggestion(
            &active_clusters,
            toggled_path,
            is_now_selected,
        );
    }

    pub(super) fn update_duplicate_assistant(&mut self) {
        // Suggestions are intentionally triggered only by direct manual item toggles
    }

    fn poll_scanner(&mut self) {
        let mut messages = Vec::new();
        let mut scanner_disconnected = false;
        if let Some(scanner) = &self.scanner {
            loop {
                match scanner.receiver.try_recv() {
                    Ok(msg) => messages.push(msg),
                    Err(std::sync::mpsc::TryRecvError::Empty) => break,
                    Err(std::sync::mpsc::TryRecvError::Disconnected) => {
                        scanner_disconnected = true;
                        break;
                    }
                }
            }
        }
        if scanner_disconnected && self.is_scanning {
            self.is_scanning = false;
            self.scanner = None;
            for e in &mut self.entries {
                e.is_scanning = false;
            }
        }

        let mut needs_sort = false;
        let mut force_immediate_sort = false;
        let mut completed = false;
        let mut needs_duplicates = false;

        for msg in messages {
            match msg {
                ScanMessage::Progress { scanned_items } => {
                    self.scanned_counter = scanned_items;
                }
                ScanMessage::InitialEntries(entries) => {
                    if self.config.flatten_subfolders {
                        self.entries.extend(entries);
                    } else {
                        self.entries = entries;
                        // Always sort and show directory entries immediately upon entering a folder
                        force_immediate_sort = true;
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
                    completed = true;
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

        // Sort immediately for initial directory entries or completion, otherwise throttle to 1000ms
        if completed
            || force_immediate_sort
            || (needs_sort && self.last_scan_sort.elapsed() > Duration::from_millis(1000))
        {
            self.sort_entries();
            self.last_scan_sort = Instant::now();
            self.visible_indices_dirty = true;
        }

        if needs_duplicates {
            self.compute_duplicates();
        }

        if self.is_finding_duplicates
            && let Some(rx) = &self.duplicate_receiver {
                loop {
                    match rx.try_recv() {
                        Ok(msg) => {
                            match msg {
                                crate::duplicate::DuplicateMessage::Progress { stage, done, total } => {
                                    if total > 0 {
                                        self.duplicate_progress_text = format!("{} {} / {}", stage, done, total);
                                    } else {
                                        self.duplicate_progress_text = stage;
                                    }
                                }
                                crate::duplicate::DuplicateMessage::Completed(clusters) => {
                                    self.duplicate_clusters = clusters;
                                    self.trim_clusters_to_min_size();
                                    self.sort_duplicate_clusters();
                                    self.is_finding_duplicates = false;
                                    self.duplicate_duration = Some(self.duplicate_started.elapsed());
                                    self.duplicate_receiver = None;
                                    self.duplicate_flat_rows_dirty = true;
                                    self.active_suggestion = None;
                                    if self.config.enable_duplicate_suggestions {
                                        self.apply_category_duplicate_rules();
                                    }
                                    self.recompute_duplicate_stats();
                                    break;
                                }
                            }
                        }
                        Err(std::sync::mpsc::TryRecvError::Empty) => break,
                        Err(std::sync::mpsc::TryRecvError::Disconnected) => {
                            self.is_finding_duplicates = false;
                            self.duplicate_receiver = None;
                            break;
                        }
                    }
                }
            }
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

        if self.is_scanning || self.is_finding_duplicates {
            ui.ctx().request_repaint_after(Duration::from_millis(33));
        }

        // Global F1 shortcut for Shortcuts Help (always active everywhere)
        let f1_pressed = ui.input(|i| i.key_pressed(egui::Key::F1));
        if f1_pressed {
            self.show_shortcuts_modal = !self.show_shortcuts_modal;
            self.shortcuts_just_opened = self.show_shortcuts_modal;
            if self.show_shortcuts_modal {
                self.shortcuts_selected_idx = 0;
            }
        }

        // Tab switches focus between Main View and Settings Panel
        let tab_pressed = ui.input(|i| i.key_pressed(egui::Key::Tab));
        if tab_pressed
            && !ui.ctx().egui_wants_keyboard_input()
            && self.delete_modal == DeleteConfirmState::None
            && self.active_context_menu.is_none()
            && !self.show_shortcuts_modal
            && self.config.show_sidebar {
                self.panel_focus = match self.panel_focus {
                    ActivePanelFocus::MainView => ActivePanelFocus::SettingsPanel,
                    ActivePanelFocus::SettingsPanel => ActivePanelFocus::MainView,
                };
            }

        // Global Shortcuts (F-keys & Ctrl shortcuts) - active when enabled per-key
        let f11_pressed = ui.input(|i| i.key_pressed(egui::Key::F11));
        if self.config.shortcuts.f11_fullscreen && f11_pressed {
            let is_fs = ui.ctx().input(|i| i.viewport().fullscreen.unwrap_or(false));
            ui.ctx().send_viewport_cmd(egui::ViewportCommand::Fullscreen(!is_fs));
        }

        self.handle_global_shortcuts(ui);

        // --- TOP PANEL ---
        self.render_top_bar(ui, is_dark);

        // --- RIGHT PANEL: LIVE SETTINGS, FILTERS, FAVORITES & DRIVES ---
        if self.config.show_sidebar {
            Panel::right("right_settings_panel").show(ui, |ui| {
                self.render_sidebar(ui, is_dark);
            });
        }

        // --- BOTTOM PANEL ---
        self.render_status_bar(ui, is_dark);

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
