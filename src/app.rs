use std::path::PathBuf;
use std::time::{Duration, Instant};

use eframe::egui::{self, Color32, Panel, ProgressBar, RichText, ScrollArea, Vec2};

use crate::config::{AppConfig, AppTheme, ColumnId, QuickSearchMode, SortRule, UsageBasis};
use crate::model::{format_size, DriveInfo, FileEntry, SortDirection};
use crate::platform;
use crate::scanner::{start_scan, ScanMessage, ScanSummary, ScannerHandle};

#[derive(Clone, PartialEq)]
enum DeleteConfirmState {
    None,
    Confirm {
        path: PathBuf,
        name: String,
        is_system: bool,
    },
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum DeleteModalFocus {
    Trash,
    Permanent,
    Cancel,
}

#[derive(Clone)]
struct ActiveContextMenu {
    path: PathBuf,
    name: String,
    is_dir: bool,
    is_system_protected: bool,
    pos: Option<egui::Pos2>,
    selected_idx: usize,
    just_opened: bool,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum ContextAction {
    Open,
    ShowInExplorer,
    CopyName,
    CopyFolderPath,
    CopyFullPath,
    #[cfg(windows)]
    Properties,
    MoveToTrash,
    DeletePermanently,
}

struct ContextMenuItem {
    action: ContextAction,
    label: String,
    color: Option<Color32>,
}

fn get_context_menu_items(entry: &ActiveContextMenu) -> Vec<ContextMenuItem> {
    let mut items = Vec::new();
    items.push(ContextMenuItem {
        action: ContextAction::Open,
        label: "Open".to_string(),
        color: None,
    });
    items.push(ContextMenuItem {
        action: ContextAction::ShowInExplorer,
        label: "Show in Explorer".to_string(),
        color: None,
    });
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
        label: "Move to Recycle Bin".to_string(),
        color: Some(Color32::from_rgb(255, 200, 100)),
    });
    items.push(ContextMenuItem {
        action: ContextAction::DeletePermanently,
        label: "Delete Permanently".to_string(),
        color: Some(Color32::from_rgb(255, 100, 100)),
    });
    items
}

pub struct FreeDataDietApp {
    config: AppConfig,
    dragged_column_idx: Option<usize>,

    current_path: PathBuf,
    path_input: String,
    drives: Vec<DriveInfo>,
    entries: Vec<FileEntry>,
    selected_path: Option<PathBuf>,

    // Quick Search
    quick_search_buffer: String,
    quick_search_time: Option<Instant>,

    // Exclude filters
    ignore_dev_caches: bool,
    custom_excludes: String,

    scanner: Option<ScannerHandle>,
    is_scanning: bool,
    scanned_counter: usize,
    summary: Option<ScanSummary>,
    error_message: Option<String>,

    delete_modal: DeleteConfirmState,
    modal_focus: DeleteModalFocus,
    active_context_menu: Option<ActiveContextMenu>,
    show_settings_modal: bool,
    settings_just_opened: bool,
    settings_selected_idx: usize,
    settings_scroll_to_focus: bool,
    show_shortcuts_modal: bool,
    shortcuts_just_opened: bool,
    status_toast: Option<(String, Instant)>,
    is_admin: bool,
}

impl FreeDataDietApp {
    pub fn new(_cc: &eframe::CreationContext<'_>) -> Self {
        let config = AppConfig::load();
        let drives = platform::get_drives();
        let is_admin = platform::is_admin();

        let start_path = if config.remember_last_path {
            if let Some(ref last) = config.last_path {
                if last.is_dir() {
                    last.clone()
                } else if let Some(first_drive) = drives.first() {
                    first_drive.path.clone()
                } else {
                    std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."))
                }
            } else if let Some(first_drive) = drives.first() {
                first_drive.path.clone()
            } else {
                std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."))
            }
        } else if let Some(first_drive) = drives.first() {
            first_drive.path.clone()
        } else {
            std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."))
        };

        let mut app = Self {
            ignore_dev_caches: config.ignore_dev_caches,
            custom_excludes: config.custom_excludes.clone(),
            config,
            dragged_column_idx: None,
            path_input: start_path.to_string_lossy().to_string(),
            current_path: start_path,
            drives,
            entries: Vec::new(),
            selected_path: None,
            quick_search_buffer: String::new(),
            quick_search_time: None,
            scanner: None,
            is_scanning: false,
            scanned_counter: 0,
            summary: None,
            error_message: None,
            delete_modal: DeleteConfirmState::None,
            modal_focus: DeleteModalFocus::Trash,
            active_context_menu: None,
            show_settings_modal: false,
            settings_just_opened: false,
            settings_selected_idx: 0,
            settings_scroll_to_focus: false,
            show_shortcuts_modal: false,
            shortcuts_just_opened: false,
            status_toast: None,
            is_admin,
        };

        app.trigger_scan();
        app
    }

    fn save_config(&mut self) {
        self.config.ignore_dev_caches = self.ignore_dev_caches;
        self.config.custom_excludes = self.custom_excludes.clone();
        if self.config.remember_last_path {
            self.config.last_path = Some(self.current_path.clone());
        } else {
            self.config.last_path = None;
        }
        let _ = self.config.save();
    }

    fn collect_excludes(&self) -> Vec<String> {
        let mut list = Vec::new();
        if self.ignore_dev_caches {
            for item in self.custom_excludes.split(',') {
                let trimmed = item.trim();
                if !trimmed.is_empty() {
                    list.push(trimmed.to_string());
                }
            }
        }
        list
    }

    fn trigger_scan(&mut self) {
        if let Some(scanner) = &self.scanner {
            scanner.cancel();
        }

        self.entries.clear();
        self.selected_path = None;
        self.quick_search_buffer.clear();
        self.quick_search_time = None;
        self.summary = None;
        self.error_message = None;
        self.is_scanning = true;
        self.scanned_counter = 0;

        self.path_input = self.current_path.to_string_lossy().to_string();
        let excludes = self.collect_excludes();
        self.scanner = Some(start_scan(self.current_path.clone(), excludes));
    }

    fn navigate_to(&mut self, path: PathBuf) {
        if path.is_dir() {
            self.quick_search_buffer.clear();
            self.quick_search_time = None;
            self.current_path = path;
            self.save_config();
            self.trigger_scan();
        }
    }

    fn navigate_up(&mut self) {
        if let Some(parent) = self.current_path.parent() {
            self.navigate_to(parent.to_path_buf());
        }
    }

    fn poll_scanner(&mut self) {
        let mut messages = Vec::new();
        if let Some(scanner) = &self.scanner {
            while let Ok(msg) = scanner.receiver.try_recv() {
                messages.push(msg);
            }
        }

        for msg in messages {
            match msg {
                ScanMessage::Progress { scanned_items } => {
                    self.scanned_counter = scanned_items;
                }
                ScanMessage::InitialEntries(entries) => {
                    self.entries = entries;
                    self.sort_entries();
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
                    self.sort_entries();
                }
                ScanMessage::Completed(sum) => {
                    self.summary = Some(sum);
                    self.is_scanning = false;
                    for e in &mut self.entries {
                        e.is_scanning = false;
                    }
                    self.sort_entries();
                }
                ScanMessage::Failed(err) => {
                    self.error_message = Some(err);
                    self.is_scanning = false;
                }
            }
        }
    }

    fn sort_entries(&mut self) {
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
                        let cmp = a.name.to_lowercase().cmp(&b.name.to_lowercase());
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

            a.name.to_lowercase().cmp(&b.name.to_lowercase())
        });
    }

    fn toggle_sort_column(&mut self, col: ColumnId) {
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

fn get_type_sort_key(e: &FileEntry) -> (u8, String) {
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

fn get_type_badge(entry: &FileEntry, is_dark: bool) -> (String, Color32, &'static str) {
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
            "System: Protected Windows system directory or file",
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
            AppTheme::System => platform::is_system_dark_mode(),
        };
        if is_dark {
            ui.ctx().set_visuals(egui::Visuals::dark());
        } else {
            ui.ctx().set_visuals(egui::Visuals::light());
        }

        self.poll_scanner();

        if self.is_scanning {
            ui.ctx().request_repaint();
        }

        // Global F1 / ? shortcut for Shortcuts Help (always active everywhere)
        let f1_pressed = ui.input(|i| {
            i.key_pressed(egui::Key::F1)
                || (i.modifiers.shift && i.key_pressed(egui::Key::Slash) && !ui.ctx().egui_wants_keyboard_input())
        });
        if f1_pressed {
            self.show_shortcuts_modal = !self.show_shortcuts_modal;
            self.shortcuts_just_opened = self.show_shortcuts_modal;
        }

        // Global Shortcuts (F-keys & Ctrl shortcuts) - active when enabled per-key
        let f11_pressed = ui.input(|i| i.key_pressed(egui::Key::F11));
        if self.config.shortcuts.f11_fullscreen && f11_pressed {
            let is_fs = ui.ctx().input(|i| i.viewport().fullscreen.unwrap_or(false));
            ui.ctx().send_viewport_cmd(egui::ViewportCommand::Fullscreen(!is_fs));
        }

        if !ui.ctx().egui_wants_keyboard_input()
            && self.delete_modal == DeleteConfirmState::None
            && self.active_context_menu.is_none()
            && !self.show_shortcuts_modal
            && !self.show_settings_modal
        {
            let ctrl = ui.input(|i| i.modifiers.ctrl);
            let shift = ui.input(|i| i.modifiers.shift);
            let h_pressed = ui.input(|i| i.key_pressed(egui::Key::H));
            let comma_pressed = ui.input(|i| i.key_pressed(egui::Key::Comma));
            let f12_pressed = ui.input(|i| i.key_pressed(egui::Key::F12));
            let f5_pressed = ui.input(|i| i.key_pressed(egui::Key::F5));
            let f2_pressed = ui.input(|i| i.key_pressed(egui::Key::F2));
            let f3_pressed = ui.input(|i| i.key_pressed(egui::Key::F3));

            let open_settings = (self.config.shortcuts.ctrl_comma_settings && ctrl && comma_pressed)
                || (self.config.shortcuts.f12_settings && f12_pressed);

            if open_settings {
                self.show_settings_modal = !self.show_settings_modal;
                self.settings_just_opened = self.show_settings_modal;
            } else if self.config.shortcuts.f5_refresh && f5_pressed {
                self.trigger_scan();
            } else if self.config.shortcuts.f2_search_mode && f2_pressed {
                self.config.quick_search_mode = match self.config.quick_search_mode {
                    QuickSearchMode::JumpToPrefix => QuickSearchMode::LiveFilter,
                    _ => QuickSearchMode::JumpToPrefix,
                };
                self.quick_search_buffer.clear();
                self.save_config();
            } else if self.config.shortcuts.f3_search_filter && f3_pressed {
                self.config.quick_search_mode = QuickSearchMode::LiveFilter;
                self.save_config();
            } else if self.config.shortcuts.ctrl_shift_h_system && ctrl && shift && h_pressed {
                self.config.show_system_files = !self.config.show_system_files;
                self.save_config();
                self.sort_entries();
            } else if self.config.shortcuts.ctrl_h_hidden && ctrl && h_pressed {
                self.config.show_hidden_files = !self.config.show_hidden_files;
                self.save_config();
                self.sort_entries();
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
                        } else {
                            if is_dark { Color32::LIGHT_GRAY } else { Color32::DARK_GRAY }
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
                        let _ = platform::restart_as_admin(Some(&self.current_path));
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

                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if ui.button("Settings (Ctrl+,)").clicked() {
                        self.show_settings_modal = true;
                        self.settings_just_opened = true;
                    }

                    if ui.button("Refresh (F5)").clicked() {
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
                if do_refresh {
                    self.trigger_scan();
                }
            });

            ui.add_space(6.0);
        });

        if let Some(drive_path) = target_drive {
            self.navigate_to(drive_path);
        }

        // --- BOTTOM PANEL ---
        Panel::bottom("bottom_panel").show(ui, |ui| {
            ui.add_space(4.0);
            ui.horizontal(|ui| {
                if let Some((toast, time)) = &self.status_toast {
                    if time.elapsed() < Duration::from_secs(3) {
                        ui.label(RichText::new(toast).color(Color32::from_rgb(100, 220, 150)).strong());
                    } else if self.is_scanning {
                        ui.spinner();
                        ui.label(format!(
                            "Scanning storage... {} items analyzed",
                            self.scanned_counter
                        ));
                    } else if let Some(summary) = &self.summary {
                        ui.label(RichText::new(format!(
                            "Scanned in {:.2}s  |  {} items  |  Total: {} (On Disk: {})",
                            summary.duration.as_secs_f32(),
                            summary.total_files,
                            format_size(summary.total_size),
                            format_size(summary.total_allocated)
                        )).small());
                    } else if let Some(err) = &self.error_message {
                        ui.label(RichText::new(err).color(Color32::from_rgb(255, 100, 100)));
                    } else {
                        ui.label("Ready");
                    }
                } else if self.is_scanning {
                    ui.spinner();
                    ui.label(format!(
                        "Scanning storage... {} items analyzed",
                        self.scanned_counter
                    ));
                    if ui.button("Cancel").clicked() {
                        if let Some(scanner) = &self.scanner {
                            scanner.cancel();
                            self.is_scanning = false;
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
                } else if let Some(err) = &self.error_message {
                    ui.label(RichText::new(err).color(Color32::from_rgb(255, 100, 100)));
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

        // --- CENTRAL DATA TABLE PANEL ---
        egui::CentralPanel::default().show(ui, |ui| {
            let total_width = ui.available_width() - 14.0;
            let col_spacing = 6.0;

            let fixed_sum: f32 = self
                .config
                .column_order
                .iter()
                .filter(|&&c| c != ColumnId::Name)
                .map(|c| c.fixed_width())
                .sum();
            let spacing_sum = (self.config.column_order.len().saturating_sub(1) as f32) * col_spacing;
            let name_width = (total_width - fixed_sum - spacing_sum).max(160.0);

            let mut swap_columns = None;
            let mut sort_column = None;
            let mut change_basis = None;
            let current_basis = self.config.usage_basis;

            // Interactive Drag-and-Drop & Sortable Header Row
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = col_spacing;

                for (idx, &col) in self.config.column_order.iter().enumerate() {
                    let w = if col == ColumnId::Name { name_width } else { col.fixed_width() };
                    let is_dragging = self.dragged_column_idx == Some(idx);

                    let sort_pos = self.config.sort_rules.iter().position(|r| r.column == col);
                    let is_active_sort = sort_pos.is_some();

                    let priority_suffix = if let Some(pos) = sort_pos {
                        if self.config.sort_rules.len() > 1 {
                            format!(" {}", pos + 1)
                        } else {
                            "".to_string()
                        }
                    } else {
                        "".to_string()
                    };

                    let col_title = if col == ColumnId::Usage {
                        current_basis.title()
                    } else {
                        col.title()
                    };
                    let label_text = format!("{}{}", col_title, priority_suffix);

                    let (header_rect, header_resp) = ui.allocate_exact_size(
                        Vec2::new(w, 24.0),
                        egui::Sense::click_and_drag(),
                    );

                    let header_resp = header_resp
                        .on_hover_cursor(if is_dragging {
                            egui::CursorIcon::Grabbing
                        } else {
                            egui::CursorIcon::Grab
                        })
                        .on_hover_text("Click to sort, drag to reorder");

                    if col == ColumnId::Usage {
                        header_resp.context_menu(|ui| {
                            ui.label(RichText::new("Usage Bar Scaling").strong());
                            ui.separator();
                            for basis in [
                                UsageBasis::LargestItem,
                                UsageBasis::Folder,
                                UsageBasis::DriveUsed,
                                UsageBasis::DriveTotal,
                            ] {
                                if ui.selectable_label(current_basis == basis, basis.display_name())
                                    .on_hover_text(basis.description())
                                    .clicked()
                                {
                                    change_basis = Some(basis);
                                    ui.close();
                                }
                            }
                        });
                    }

                    if header_resp.clicked() {
                        sort_column = Some(col);
                    }

                    if header_resp.drag_started() {
                        self.dragged_column_idx = Some(idx);
                    }

                    if is_dragging && header_resp.dragged() {
                        if let Some(mouse_pos) = ui.input(|i| i.pointer.interact_pos()) {
                            if mouse_pos.x < header_rect.min.x - 8.0 && idx > 0 {
                                swap_columns = Some((idx, idx - 1));
                            } else if mouse_pos.x > header_rect.max.x + 8.0 && idx + 1 < self.config.column_order.len() {
                                swap_columns = Some((idx, idx + 1));
                            }
                        }
                    }

                    if header_resp.drag_stopped() {
                        self.dragged_column_idx = None;
                        let _ = self.config.save();
                    }

                    // Background color
                    let bg_color = if is_dragging {
                        if is_dark { Color32::from_rgb(45, 80, 125) } else { Color32::from_rgb(180, 210, 245) }
                    } else if header_resp.hovered() {
                        if is_dark { Color32::from_rgb(38, 48, 62) } else { Color32::from_rgb(220, 228, 238) }
                    } else {
                        if is_dark { Color32::from_rgb(26, 33, 44) } else { Color32::from_rgb(235, 240, 248) }
                    };

                    let stroke_color = if is_dragging {
                        Color32::from_rgb(100, 180, 255)
                    } else if is_active_sort {
                        Color32::from_rgb(60, 120, 180)
                    } else {
                        if is_dark { Color32::from_rgb(48, 58, 72) } else { Color32::from_rgb(205, 215, 225) }
                    };

                    ui.painter().rect(
                        header_rect,
                        3.0,
                        bg_color,
                        egui::Stroke::new(1.0, stroke_color),
                        egui::StrokeKind::Inside,
                    );

                    let text_color = if is_active_sort || is_dragging {
                        if is_dark { Color32::WHITE } else { Color32::from_rgb(10, 50, 110) }
                    } else {
                        if is_dark { Color32::from_rgb(200, 210, 220) } else { Color32::from_rgb(50, 60, 75) }
                    };

                    let is_right_aligned = matches!(
                        col,
                        ColumnId::Size | ColumnId::AllocatedSize | ColumnId::Items | ColumnId::Modified
                    );

                    let is_centered = matches!(col, ColumnId::Type);

                    let font_id = egui::FontId::proportional(12.0);
                    let galley = ui.painter().layout_no_wrap(label_text.clone(), font_id.clone(), text_color);
                    let text_w = galley.size().x;
                    let arrow_w = 7.0;
                    let gap = 4.0;
                    let cy = header_rect.center().y;

                    if let Some(pos) = sort_pos {
                        let dir = self.config.sort_rules[pos].direction;
                        let arrow_color = if is_dark { Color32::from_rgb(100, 185, 255) } else { Color32::from_rgb(0, 110, 220) };

                        let (text_pos, align, arrow_cx) = if is_right_aligned {
                            let right_pad = 8.0;
                            let acx = header_rect.max.x - right_pad - (arrow_w / 2.0);
                            let tpos = egui::pos2(header_rect.max.x - right_pad - arrow_w - gap, cy);
                            (tpos, egui::Align2::RIGHT_CENTER, acx)
                        } else if is_centered {
                            let total_w = text_w + gap + arrow_w;
                            let start_x = header_rect.center().x - (total_w / 2.0);
                            let tpos = egui::pos2(start_x, cy);
                            let acx = start_x + text_w + gap + (arrow_w / 2.0);
                            (tpos, egui::Align2::LEFT_CENTER, acx)
                        } else {
                            let left_pad = 8.0;
                            let start_x = header_rect.min.x + left_pad;
                            let tpos = egui::pos2(start_x, cy);
                            let acx = start_x + text_w + gap + (arrow_w / 2.0);
                            (tpos, egui::Align2::LEFT_CENTER, acx)
                        };

                        ui.painter().text(text_pos, align, &label_text, font_id, text_color);

                        let points = match dir {
                            SortDirection::Ascending => vec![
                                egui::pos2(arrow_cx, cy - 2.5),
                                egui::pos2(arrow_cx + 3.5, cy + 2.5),
                                egui::pos2(arrow_cx - 3.5, cy + 2.5),
                            ],
                            SortDirection::Descending => vec![
                                egui::pos2(arrow_cx - 3.5, cy - 2.5),
                                egui::pos2(arrow_cx + 3.5, cy - 2.5),
                                egui::pos2(arrow_cx, cy + 2.5),
                            ],
                        };
                        ui.painter().add(egui::Shape::convex_polygon(
                            points,
                            arrow_color,
                            egui::Stroke::NONE,
                        ));
                    } else {
                        let (text_pos, align) = if is_right_aligned {
                            (egui::pos2(header_rect.max.x - 8.0, cy), egui::Align2::RIGHT_CENTER)
                        } else if is_centered {
                            (header_rect.center(), egui::Align2::CENTER_CENTER)
                        } else {
                            (egui::pos2(header_rect.min.x + 8.0, cy), egui::Align2::LEFT_CENTER)
                        };

                        ui.painter().text(text_pos, align, &label_text, font_id, text_color);
                    }
                }
            });

            if let Some((from, to)) = swap_columns {
                self.config.column_order.swap(from, to);
                self.dragged_column_idx = Some(to);
                let _ = self.config.save();
            }

            if let Some(target_col) = sort_column {
                self.toggle_sort_column(target_col);
            }

            if let Some(b) = change_basis {
                self.config.usage_basis = b;
                self.save_config();
            }

            ui.add_space(2.0);
            ui.separator();

            let base_size: f64 = match self.config.usage_basis {
                UsageBasis::LargestItem => {
                    let max_size = self.entries.iter().map(|e| e.size).max().unwrap_or(0);
                    if max_size > 0 {
                        max_size as f64
                    } else {
                        1.0
                    }
                }
                UsageBasis::Folder => {
                    let sum: u64 = self.entries.iter().map(|e| e.size).sum();
                    if sum > 0 {
                        sum as f64
                    } else {
                        1.0
                    }
                }
                UsageBasis::DriveUsed => {
                    let drive = self.drives.iter().find(|d| self.current_path.starts_with(&d.path));
                    let used = drive.map(|d| d.used_space()).unwrap_or(0);
                    if used > 0 {
                        used as f64
                    } else {
                        let sum: u64 = self.entries.iter().map(|e| e.size).sum();
                        sum.max(1) as f64
                    }
                }
                UsageBasis::DriveTotal => {
                    let drive = self.drives.iter().find(|d| self.current_path.starts_with(&d.path));
                    let total = drive.map(|d| d.total_space).unwrap_or(0);
                    if total > 0 {
                        total as f64
                    } else {
                        let sum: u64 = self.entries.iter().map(|e| e.size).sum();
                        sum.max(1) as f64
                    }
                }
            };

            let mut navigate_target = None;
            let mut scroll_to_selected = false;
            let mut open_context_menu: Option<ActiveContextMenu> = None;
            let mut do_navigate_up = false;
            let mut do_trigger_rescan = false;
            let mut do_cycle_search_mode = false;

            let visible_entries: Vec<(usize, &FileEntry)> = self.entries
                .iter()
                .enumerate()
                .filter(|(_, e)| {
                    if !self.config.show_hidden_files && (e.is_hidden || (e.is_reparse_point && e.is_system_protected)) {
                        return false;
                    }
                    if !self.config.show_system_files && e.is_system_protected {
                        return false;
                    }
                    if !self.config.show_locked_folders && e.is_access_denied {
                        return false;
                    }
                    if self.config.quick_search_mode == QuickSearchMode::LiveFilter && !self.quick_search_buffer.is_empty() {
                        if !e.name.to_lowercase().contains(&self.quick_search_buffer.to_lowercase()) {
                            return false;
                        }
                    }
                    true
                })
                .collect();

            let mut selected_visible_idx = self.selected_path.as_ref().and_then(|p| {
                visible_entries.iter().position(|(_, e)| &e.path == p)
            });

            if selected_visible_idx.is_none() && !visible_entries.is_empty() && self.config.quick_search_mode == QuickSearchMode::LiveFilter && !self.quick_search_buffer.is_empty() {
                self.selected_path = Some(visible_entries[0].1.path.clone());
                selected_visible_idx = Some(0);
            }

            // Keyboard navigation (active when not typing in text input, not in modal, not in context menu)
            if self.config.shortcuts.arrow_navigation
                && !ui.ctx().egui_wants_keyboard_input()
                && self.delete_modal == DeleteConfirmState::None
                && self.active_context_menu.is_none()
                && !self.show_shortcuts_modal
                && !self.show_settings_modal
            {
                let (typed_text, key_backspace, key_escape) = ui.input(|i| {
                    let mut text = String::new();
                    let mut bs = false;
                    let mut esc = false;
                    for event in &i.events {
                        match event {
                            egui::Event::Text(t) => {
                                if !i.modifiers.ctrl && !i.modifiers.alt {
                                    for ch in t.chars() {
                                        if !ch.is_control() {
                                            text.push(ch);
                                        }
                                    }
                                }
                            }
                            egui::Event::Key { key: egui::Key::Backspace, pressed: true, .. } => {
                                bs = true;
                            }
                            egui::Event::Key { key: egui::Key::Escape, pressed: true, .. } => {
                                esc = true;
                            }
                            _ => {}
                        }
                    }
                    (text, bs, esc)
                });

                let down = ui.input(|i| i.key_pressed(egui::Key::ArrowDown));
                let up = ui.input(|i| i.key_pressed(egui::Key::ArrowUp));
                let enter = ui.input(|i| i.key_pressed(egui::Key::Enter));
                let right = ui.input(|i| i.key_pressed(egui::Key::ArrowRight));
                let arrow_left = ui.input(|i| i.key_pressed(egui::Key::ArrowLeft));
                let home = ui.input(|i| i.key_pressed(egui::Key::Home));
                let end = ui.input(|i| i.key_pressed(egui::Key::End));
                let page_up = ui.input(|i| i.key_pressed(egui::Key::PageUp));
                let page_down = ui.input(|i| i.key_pressed(egui::Key::PageDown));
                let del = ui.input(|i| i.key_pressed(egui::Key::Delete));
                let shift = ui.input(|i| i.modifiers.shift);
                let refresh = ui.input(|i| i.key_pressed(egui::Key::F5));
                let context_key = ui.input(|i| i.key_pressed(egui::Key::Menu) || (i.modifiers.shift && i.key_pressed(egui::Key::F10)));

                // Quick Search handling
                match self.config.quick_search_mode {
                    QuickSearchMode::LiveFilter => {
                        if key_escape {
                            self.quick_search_buffer.clear();
                        } else if key_backspace {
                            if !self.quick_search_buffer.is_empty() {
                                self.quick_search_buffer.pop();
                            } else {
                                do_navigate_up = true;
                            }
                        } else if !typed_text.is_empty() {
                            self.quick_search_buffer.push_str(&typed_text);
                            scroll_to_selected = true;
                        }

                        if arrow_left {
                            do_navigate_up = true;
                        }
                    }
                    QuickSearchMode::JumpToPrefix => {
                        if key_escape {
                            self.quick_search_buffer.clear();
                            self.quick_search_time = None;
                        } else if key_backspace || arrow_left {
                            do_navigate_up = true;
                        } else if !typed_text.is_empty() {
                            let now = Instant::now();
                            let timed_out = match self.quick_search_time {
                                Some(t) => now.duration_since(t) > Duration::from_millis(1000),
                                None => true,
                            };
                            if timed_out {
                                self.quick_search_buffer.clear();
                            }
                            self.quick_search_time = Some(now);

                            for ch in typed_text.chars() {
                                let ch_lower = ch.to_lowercase().to_string();
                                if self.quick_search_buffer.len() == 1 && self.quick_search_buffer.to_lowercase() == ch_lower {
                                    let start_idx = match selected_visible_idx {
                                        Some(curr) => (curr + 1) % visible_entries.len().max(1),
                                        None => 0,
                                    };
                                    let found = visible_entries.iter().skip(start_idx)
                                        .chain(visible_entries.iter().take(start_idx))
                                        .find(|(_, e)| e.name.to_lowercase().starts_with(&ch_lower));
                                    if let Some((_, entry)) = found {
                                        self.selected_path = Some(entry.path.clone());
                                        scroll_to_selected = true;
                                    }
                                } else {
                                    self.quick_search_buffer.push(ch);
                                    let query = self.quick_search_buffer.to_lowercase();
                                    if let Some((_, entry)) = visible_entries.iter().find(|(_, e)| e.name.to_lowercase().starts_with(&query)) {
                                        self.selected_path = Some(entry.path.clone());
                                        scroll_to_selected = true;
                                    }
                                }
                            }
                        }
                    }
                    QuickSearchMode::Disabled => {
                        if key_backspace || arrow_left {
                            do_navigate_up = true;
                        }
                    }
                }

                if down && !visible_entries.is_empty() {
                    let next = match selected_visible_idx {
                        Some(idx) => (idx + 1).min(visible_entries.len().saturating_sub(1)),
                        None => 0,
                    };
                    self.selected_path = Some(visible_entries[next].1.path.clone());
                    scroll_to_selected = true;
                }

                if up && !visible_entries.is_empty() {
                    let prev = match selected_visible_idx {
                        Some(idx) => idx.saturating_sub(1),
                        None => 0,
                    };
                    self.selected_path = Some(visible_entries[prev].1.path.clone());
                    scroll_to_selected = true;
                }

                if home && !visible_entries.is_empty() {
                    self.selected_path = Some(visible_entries[0].1.path.clone());
                    scroll_to_selected = true;
                }

                if end && !visible_entries.is_empty() {
                    self.selected_path = Some(visible_entries.last().unwrap().1.path.clone());
                    scroll_to_selected = true;
                }

                if page_up && !visible_entries.is_empty() {
                    let curr = selected_visible_idx.unwrap_or(0);
                    let target = curr.saturating_sub(15);
                    self.selected_path = Some(visible_entries[target].1.path.clone());
                    scroll_to_selected = true;
                }

                if page_down && !visible_entries.is_empty() {
                    let curr = selected_visible_idx.unwrap_or(0);
                    let target = (curr + 15).min(visible_entries.len().saturating_sub(1));
                    self.selected_path = Some(visible_entries[target].1.path.clone());
                    scroll_to_selected = true;
                }

                if enter {
                    if let Some(vidx) = selected_visible_idx {
                        let entry = visible_entries[vidx].1;
                        if entry.is_dir {
                            navigate_target = Some(entry.path.clone());
                        } else {
                            let _ = platform::open_file(&entry.path);
                        }
                    }
                }

                if right {
                    if let Some(vidx) = selected_visible_idx {
                        let entry = visible_entries[vidx].1;
                        if entry.is_dir {
                            navigate_target = Some(entry.path.clone());
                        }
                    }
                }

                if self.config.shortcuts.delete_keys && del {
                    if let Some(vidx) = selected_visible_idx {
                        let entry = visible_entries[vidx].1;
                        self.delete_modal = DeleteConfirmState::Confirm {
                            path: entry.path.clone(),
                            name: entry.name.clone(),
                            is_system: entry.is_system_protected,
                        };
                        self.modal_focus = if shift {
                            DeleteModalFocus::Permanent
                        } else {
                            DeleteModalFocus::Trash
                        };
                    }
                }

                if context_key && selected_visible_idx.is_some() {
                    let entry = visible_entries[selected_visible_idx.unwrap()].1;
                    open_context_menu = Some(ActiveContextMenu {
                        path: entry.path.clone(),
                        name: entry.name.clone(),
                        is_dir: entry.is_dir,
                        is_system_protected: entry.is_system_protected,
                        pos: None,
                        selected_idx: 0,
                        just_opened: true,
                    });
                }

                if refresh {
                    do_trigger_rescan = true;
                }
            }

            let mut clear_search_buffer = false;
            let matches_count = visible_entries.len();

            ui.add_space(2.0);
            ui.horizontal(|ui| {
                let (mode_badge, mode_color, mode_tip) = match self.config.quick_search_mode {
                    QuickSearchMode::LiveFilter => ("[Live Filter (F2)]", Color32::from_rgb(100, 200, 255), "Filter as you type. Click or press F2 to cycle mode."),
                    QuickSearchMode::JumpToPrefix => ("[Jump to Name (F2)]", Color32::from_rgb(255, 200, 100), "Jump to item on keypress (Explorer style). Click or press F2 to cycle mode."),
                    QuickSearchMode::Disabled => ("[Search: Disabled (F2)]", Color32::GRAY, "Keyboard search disabled. Click or press F2 to cycle mode."),
                };
                if ui.button(RichText::new(mode_badge).small().color(mode_color)).on_hover_text(mode_tip).clicked() {
                    do_cycle_search_mode = true;
                }

                match self.config.quick_search_mode {
                    QuickSearchMode::LiveFilter => {
                        if self.quick_search_buffer.is_empty() {
                            ui.label(RichText::new("Type any letters to filter current view...").weak().small());
                        } else {
                            ui.label(RichText::new("Filter:").color(Color32::from_rgb(255, 200, 100)).strong().small());
                            ui.label(RichText::new(format!("\"{}\"", self.quick_search_buffer)).color(Color32::WHITE).strong().small());
                            ui.label(RichText::new(format!("({} matches)", matches_count)).weak().small());
                            if ui.button("Clear (Esc)").clicked() {
                                clear_search_buffer = true;
                            }
                        }
                    }
                    QuickSearchMode::JumpToPrefix => {
                        if self.quick_search_buffer.is_empty() {
                            ui.label(RichText::new("Type first letter to jump directly to item...").weak().small());
                        } else {
                            ui.label(RichText::new(format!("Jumped to: \"{}\"", self.quick_search_buffer)).color(Color32::from_rgb(255, 200, 100)).small());
                        }
                    }
                    QuickSearchMode::Disabled => {
                        ui.label(RichText::new("Press F2 or click to enable quick search").weak().small());
                    }
                }
            });

            if clear_search_buffer {
                self.quick_search_buffer.clear();
            }

            ui.add_space(2.0);

            ScrollArea::vertical()
                .auto_shrink([false, false])
                .show(ui, |ui| {
                    if visible_entries.is_empty() {
                        ui.add_space(24.0);
                        ui.vertical_centered(|ui| {
                            if self.config.quick_search_mode == QuickSearchMode::LiveFilter && !self.quick_search_buffer.is_empty() {
                                ui.label(RichText::new(format!("No entries matching \"{}\"", self.quick_search_buffer)).weak());
                            } else if self.is_scanning {
                                ui.label(RichText::new("Discovering directory contents...").italics().weak());
                            } else {
                                ui.label(RichText::new("Folder is empty").weak());
                            }
                        });
                    }
                    for (_vis_idx, &(orig_idx, entry)) in visible_entries.iter().enumerate() {
                        let is_selected = self.selected_path.as_ref() == Some(&entry.path);
                        let (badge, badge_color, badge_tooltip) = get_type_badge(entry, is_dark);

                        // Reserve background shape slot before drawing row text
                        let bg_idx = ui.painter().add(egui::Shape::Noop);

                        let row_resp = ui.horizontal(|ui| {
                            ui.spacing_mut().item_spacing.x = col_spacing;

                            for &col in &self.config.column_order {
                                let w = if col == ColumnId::Name { name_width } else { col.fixed_width() };
                                let (cell_rect, _) = ui.allocate_exact_size(Vec2::new(w, 20.0), egui::Sense::hover());

                                match col {
                                    ColumnId::Type => {
                                        let mut child_ui = ui.new_child(egui::UiBuilder::new().max_rect(cell_rect));
                                        child_ui.with_layout(egui::Layout::centered_and_justified(egui::Direction::LeftToRight), |ui| {
                                            ui.add(egui::Label::new(RichText::new(&badge).monospace().size(12.0).color(badge_color)))
                                                .on_hover_text(badge_tooltip);
                                        });
                                    }
                                    ColumnId::Name => {
                                        let mut child_ui = ui.new_child(egui::UiBuilder::new().max_rect(cell_rect));
                                        child_ui.with_layout(egui::Layout::left_to_right(egui::Align::Center), |ui| {
                                            let name_color = if is_selected {
                                                if is_dark { Color32::from_rgb(120, 215, 255) } else { Color32::from_rgb(0, 90, 180) }
                                            } else {
                                                if is_dark { Color32::WHITE } else { Color32::from_rgb(20, 25, 35) }
                                            };
                                            ui.add_space(4.0);
                                            ui.add(
                                                egui::Label::new(
                                                    RichText::new(&entry.name).color(name_color).strong(),
                                                ).truncate(),
                                            ).on_hover_text(entry.path.to_string_lossy().as_ref());
                                        });
                                    }
                                    ColumnId::Size => {
                                        if entry.is_scanning {
                                            ui.painter().text(
                                                egui::pos2(cell_rect.max.x - 8.0, cell_rect.center().y),
                                                egui::Align2::RIGHT_CENTER,
                                                "[calculating]",
                                                egui::FontId::monospace(11.0),
                                                Color32::from_rgb(200, 160, 80),
                                            );
                                        } else {
                                            ui.painter().text(
                                                egui::pos2(cell_rect.max.x - 8.0, cell_rect.center().y),
                                                egui::Align2::RIGHT_CENTER,
                                                format_size(entry.size),
                                                egui::FontId::monospace(12.5),
                                                if is_dark { Color32::from_rgb(220, 220, 220) } else { Color32::from_rgb(30, 35, 45) },
                                            );
                                        }
                                    }
                                    ColumnId::AllocatedSize => {
                                        if entry.is_scanning {
                                            ui.painter().text(
                                                egui::pos2(cell_rect.max.x - 8.0, cell_rect.center().y),
                                                egui::Align2::RIGHT_CENTER,
                                                "-",
                                                egui::FontId::monospace(12.0),
                                                Color32::DARK_GRAY,
                                            );
                                        } else {
                                            ui.painter().text(
                                                egui::pos2(cell_rect.max.x - 8.0, cell_rect.center().y),
                                                egui::Align2::RIGHT_CENTER,
                                                format_size(entry.allocated_size),
                                                egui::FontId::monospace(12.5),
                                                if is_dark { Color32::GRAY } else { Color32::from_rgb(90, 95, 105) },
                                            );
                                        }
                                    }
                                    ColumnId::Usage => {
                                        let bar_rect = egui::Rect::from_center_size(cell_rect.center(), Vec2::new(w - 10.0, 16.0));
                                        let mut child_ui = ui.new_child(egui::UiBuilder::new().max_rect(bar_rect));
                                        if entry.is_scanning {
                                            let bar = ProgressBar::new(0.0)
                                                .text("Scanning...")
                                                .desired_width(w - 10.0);
                                            child_ui.add(bar);
                                        } else {
                                            let ratio = (entry.size as f64 / base_size).clamp(0.0, 1.0) as f32;
                                            let percent = entry.size as f64 / base_size * 100.0;
                                            let text = if percent >= 10.0 {
                                                format!("{:.1}%", percent)
                                            } else if percent >= 0.1 {
                                                format!("{:.1}%", percent)
                                            } else if percent > 0.0 {
                                                format!("{:.2}%", percent)
                                            } else {
                                                "0.0%".to_string()
                                            };
                                            let bar = ProgressBar::new(ratio)
                                                .text(text)
                                                .desired_width(w - 10.0);
                                            child_ui.add(bar);
                                        }
                                    }
                                    ColumnId::Items => {
                                        let txt = if entry.is_scanning {
                                            "...".to_string()
                                        } else if entry.is_dir {
                                            format!("{}", entry.item_count)
                                        } else {
                                            "-".to_string()
                                        };
                                        ui.painter().text(
                                            egui::pos2(cell_rect.max.x - 8.0, cell_rect.center().y),
                                            egui::Align2::RIGHT_CENTER,
                                            txt,
                                            egui::FontId::monospace(12.0),
                                            if is_dark { Color32::DARK_GRAY } else { Color32::from_rgb(120, 125, 135) },
                                        );
                                    }
                                    ColumnId::Modified => {
                                        let mod_str = entry
                                            .modified
                                            .map(crate::model::format_system_time)
                                            .unwrap_or_else(|| "-".to_string());
                                        ui.painter().text(
                                            egui::pos2(cell_rect.max.x - 8.0, cell_rect.center().y),
                                            egui::Align2::RIGHT_CENTER,
                                            mod_str,
                                            egui::FontId::monospace(11.5),
                                            if is_dark { Color32::GRAY } else { Color32::from_rgb(90, 95, 105) },
                                        );
                                    }
                                }
                            }
                        }).response;

                        // Extend rect to span full width of the table
                        let mut full_row_rect = row_resp.rect;
                        full_row_rect.max.x = full_row_rect.min.x + total_width;

                        // Click anywhere on the row (sense clicks over full row width)
                        let row_click = ui.interact(
                            full_row_rect,
                            ui.scope_id().with(orig_idx),
                            egui::Sense::click(),
                        );

                        // Paint full-row selection or hover background behind text
                        if is_selected {
                            ui.painter().set(
                                bg_idx,
                                egui::epaint::RectShape::new(
                                    full_row_rect,
                                    3.0,
                                    if is_dark { Color32::from_rgb(32, 54, 82) } else { Color32::from_rgb(205, 228, 252) },
                                    egui::Stroke::new(1.0, if is_dark { Color32::from_rgb(60, 140, 220) } else { Color32::from_rgb(100, 160, 230) }),
                                    egui::StrokeKind::Inside,
                                ),
                            );
                        } else if row_click.hovered() {
                            ui.painter().set(
                                bg_idx,
                                egui::epaint::RectShape::filled(
                                    full_row_rect,
                                    3.0,
                                    if is_dark { Color32::from_rgb(26, 32, 42) } else { Color32::from_rgb(238, 243, 250) },
                                ),
                            );
                        }

                        if row_click.clicked() {
                            self.selected_path = Some(entry.path.clone());
                        }
                        if row_click.secondary_clicked() {
                            self.selected_path = Some(entry.path.clone());
                            let pointer_pos = ui.input(|i| i.pointer.interact_pos());
                            open_context_menu = Some(ActiveContextMenu {
                                path: entry.path.clone(),
                                name: entry.name.clone(),
                                is_dir: entry.is_dir,
                                is_system_protected: entry.is_system_protected,
                                pos: pointer_pos,
                                selected_idx: 0,
                                just_opened: true,
                            });
                        }
                        if row_click.double_clicked() {
                            if entry.is_dir {
                                navigate_target = Some(entry.path.clone());
                            } else {
                                let _ = platform::open_file(&entry.path);
                            }
                        }

                        if is_selected && scroll_to_selected {
                            row_click.scroll_to_me(Some(egui::Align::Center));
                        }
                    }
                });

            if let Some(menu) = open_context_menu {
                self.active_context_menu = Some(menu);
            }

            if let Some(target) = navigate_target {
                self.navigate_to(target);
            }
            if do_navigate_up {
                self.navigate_up();
            }
            if do_trigger_rescan {
                self.trigger_scan();
            }
            if do_cycle_search_mode {
                self.config.quick_search_mode = match self.config.quick_search_mode {
                    QuickSearchMode::JumpToPrefix => QuickSearchMode::LiveFilter,
                    QuickSearchMode::LiveFilter => QuickSearchMode::Disabled,
                    QuickSearchMode::Disabled => QuickSearchMode::JumpToPrefix,
                };
                self.quick_search_buffer.clear();
                self.save_config();
            }
        });

        // --- UNIFIED CONTEXT MENU (KEYBOARD & MOUSE RIGHT-CLICK) ---
        if let Some(mut menu) = self.active_context_menu.take() {
            let items = get_context_menu_items(&menu);
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
                    if item.action == ContextAction::CopyName || item.action == ContextAction::MoveToTrash {
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

        if let DeleteConfirmState::Confirm { path, name, is_system } = &self.delete_modal {
            let path = path.clone();
            let name = name.clone();
            let is_system = *is_system;

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

            if modal_escape {
                close_modal = true;
            }

            egui::Window::new("Confirm Deletion")
                .collapsible(false)
                .resizable(false)
                .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
                .show(ui.ctx(), |ui| {
                    if is_system {
                        ui.label(RichText::new("[!] WARNING: Protected system path! Deleting may break your operating system.").color(Color32::RED).strong());
                        ui.add_space(4.0);
                    }
                    ui.label(RichText::new(format!("Do you want to delete \"{}\"?", name)).size(14.0));
                    ui.add_space(10.0);

                    ui.horizontal(|ui| {
                        // 1. Move to Trash (default selection)
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

                        let do_trash = ui.add(trash_btn).clicked() || (is_trash && modal_enter);
                        let do_perm = ui.add(perm_btn).clicked() || (is_perm && modal_enter);
                        let do_cancel = ui.add(cancel_btn).clicked() || (is_cancel && modal_enter);

                        if do_trash {
                            if let Err(e) = trash::delete(&path) {
                                modal_error = Some(format!("Trash Error: {}", e));
                            } else {
                                trigger_rescan = true;
                            }
                            close_modal = true;
                        } else if do_perm {
                            let res = if path.is_dir() {
                                let is_link = std::fs::symlink_metadata(&path).map(|m| {
                                    m.file_type().is_symlink() || {
                                        #[cfg(windows)]
                                        {
                                            use std::os::windows::fs::MetadataExt;
                                            (m.file_attributes() & 0x0000_0400) != 0
                                        }
                                        #[cfg(not(windows))]
                                        false
                                    }
                                }).unwrap_or(false);

                                if is_link {
                                    std::fs::remove_dir(&path)
                                } else {
                                    std::fs::remove_dir_all(&path)
                                }
                            } else {
                                std::fs::remove_file(&path)
                            };
                            if let Err(e) = res {
                                modal_error = Some(format!("Delete Error: {}", e));
                            } else {
                                trigger_rescan = true;
                            }
                            close_modal = true;
                        } else if do_cancel {
                            close_modal = true;
                        }
                    });
                });
        }

        if let Some(err) = modal_error {
            self.error_message = Some(err);
        }
        if close_modal {
            self.delete_modal = DeleteConfirmState::None;
        }
        if trigger_rescan {
            self.trigger_scan();
        }

        // --- KEYBOARD SHORTCUTS POPUP ---
        if self.show_shortcuts_modal {
            let esc_pressed = ui.input(|i| i.key_pressed(egui::Key::Escape));
            let mut close_shortcuts = esc_pressed;

            let win_resp = egui::Window::new("Keyboard Shortcuts")
                .collapsible(false)
                .resizable(false)
                .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
                .show(ui.ctx(), |ui| {
                    ui.set_min_width(560.0);
                    ui.add_space(4.0);

                    let mut shortcuts_changed = false;

                    ui.label(RichText::new("Keyboard Shortcuts & Hotkey Configuration").heading());
                    ui.add_space(2.0);
                    ui.separator();

                    egui::Grid::new("shortcuts_grid")
                        .num_columns(3)
                        .spacing([18.0, 6.0])
                        .striped(true)
                        .show(ui, |ui| {
                            let add_sec = |ui: &mut egui::Ui, title: &str| {
                                ui.label(RichText::new(title).strong().color(if is_dark { Color32::from_rgb(255, 200, 100) } else { Color32::from_rgb(180, 100, 0) }));
                                ui.label("");
                                ui.label(RichText::new("Active").weak().small());
                                ui.end_row();
                            };
                            let mut add_toggle = |ui: &mut egui::Ui, key: &str, desc: &str, opt: &mut bool| {
                                ui.label(RichText::new(key).strong().monospace().color(if is_dark { Color32::from_rgb(100, 190, 255) } else { Color32::from_rgb(0, 100, 200) }));
                                ui.label(desc);
                                if ui.checkbox(opt, "").changed() {
                                    shortcuts_changed = true;
                                }
                                ui.end_row();
                            };
                            let add_fixed = |ui: &mut egui::Ui, key: &str, desc: &str| {
                                ui.label(RichText::new(key).strong().monospace().color(if is_dark { Color32::from_rgb(100, 190, 255) } else { Color32::from_rgb(0, 100, 200) }));
                                ui.label(desc);
                                ui.label(RichText::new("Always").weak().small());
                                ui.end_row();
                            };

                            add_sec(ui, "Navigation");
                            add_toggle(ui, "Up / Down", "Move selection through folder rows", &mut self.config.shortcuts.arrow_navigation);
                            add_fixed(ui, "Right / Enter", "Enter selected folder or launch file");
                            add_fixed(ui, "Left / Backspace", "Navigate up to parent directory");
                            add_fixed(ui, "Home / End", "Jump to first / last item in current folder");
                            add_fixed(ui, "PageUp / PageDown", "Scroll up / down by 15 items");

                            add_sec(ui, "Quick Search");
                            add_toggle(ui, "F2", "Cycle search mode (Jump to Name vs Live Filter)", &mut self.config.shortcuts.f2_search_mode);
                            add_toggle(ui, "F3", "Activate Live Filter search bar", &mut self.config.shortcuts.f3_search_filter);
                            add_fixed(ui, "A-Z, 0-9", "Jump to prefix (Explorer) or Live filter list (FilePilot)");
                            add_fixed(ui, "Esc", "Clear active search filter query or dismiss popup");

                            add_sec(ui, "File Operations");
                            add_toggle(ui, "Del", "Move selected item to Recycle Bin / Trash", &mut self.config.shortcuts.delete_keys);
                            add_toggle(ui, "Shift + Del", "Permanently delete selected item (with confirmation)", &mut self.config.shortcuts.delete_keys);
                            add_fixed(ui, "Menu / Shift+F10", "Open right-click context menu using keyboard");
                            add_toggle(ui, "F5", "Refresh and rescan current directory", &mut self.config.shortcuts.f5_refresh);

                            add_sec(ui, "Preferences & View");
                            add_fixed(ui, "F1", "Toggle this Keyboard Shortcuts cheat sheet");
                            add_toggle(ui, "F11", "Toggle fullscreen mode", &mut self.config.shortcuts.f11_fullscreen);
                            add_toggle(ui, "F12", "Open Settings & Preferences dialog", &mut self.config.shortcuts.f12_settings);
                            add_toggle(ui, "Ctrl + ,", "Open Settings & Preferences dialog", &mut self.config.shortcuts.ctrl_comma_settings);
                            add_toggle(ui, "Ctrl + H", "Toggle hidden files and folders", &mut self.config.shortcuts.ctrl_h_hidden);
                            add_toggle(ui, "Ctrl + Shift + H", "Toggle protected system files & links", &mut self.config.shortcuts.ctrl_shift_h_system);

                            add_sec(ui, "Dialog & Menu Controls");
                            add_fixed(ui, "Up / Down / Tab", "Navigate options inside Settings dialog");
                            add_fixed(ui, "Left / Right", "Switch options in horizontal button rows");
                            add_fixed(ui, "Space / Enter", "Toggle selected checkbox or click button");
                            add_fixed(ui, "Esc / Click Outside", "Close dialogs, menus, or search filter");
                        });

                    if shortcuts_changed {
                        self.save_config();
                    }

                    ui.add_space(8.0);
                    ui.separator();
                    ui.horizontal(|ui| {
                        ui.label(RichText::new("Press Esc or click outside to return").weak().small());
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            if ui.button("Close (Esc)").clicked() {
                                close_shortcuts = true;
                            }
                        });
                    });
                });

            if let Some(resp) = win_resp {
                if !self.shortcuts_just_opened && ui.input(|i| i.pointer.any_pressed()) {
                    if let Some(pos) = ui.input(|i| i.pointer.interact_pos()) {
                        if !resp.response.rect.contains(pos) {
                            close_shortcuts = true;
                        }
                    }
                }
            }

            if self.shortcuts_just_opened {
                self.shortcuts_just_opened = false;
            }

            if close_shortcuts {
                self.show_shortcuts_modal = false;
            }
        }

        // --- SETTINGS MODAL ---
        if self.show_settings_modal {
            let mut close_settings = ui.input(|i| i.key_pressed(egui::Key::Escape));
            let mut trigger_rescan = false;

            let nav_down = ui.input(|i| i.key_pressed(egui::Key::ArrowDown));
            let nav_up = ui.input(|i| i.key_pressed(egui::Key::ArrowUp));
            let nav_left = ui.input(|i| i.key_pressed(egui::Key::ArrowLeft));
            let nav_right = ui.input(|i| i.key_pressed(egui::Key::ArrowRight));
            let tab_forward = ui.input(|i| !i.modifiers.shift && i.key_pressed(egui::Key::Tab));
            let tab_backward = ui.input(|i| i.modifiers.shift && i.key_pressed(egui::Key::Tab));
            let activate = !ui.ctx().egui_wants_keyboard_input() && ui.input(|i| i.key_pressed(egui::Key::Space) || i.key_pressed(egui::Key::Enter));

            const TOTAL_SETTINGS_ITEMS: usize = 18;
            let mut key_navigated = false;

            if tab_forward {
                self.settings_selected_idx = (self.settings_selected_idx + 1) % TOTAL_SETTINGS_ITEMS;
                key_navigated = true;
            } else if tab_backward {
                self.settings_selected_idx = (self.settings_selected_idx + TOTAL_SETTINGS_ITEMS - 1) % TOTAL_SETTINGS_ITEMS;
                key_navigated = true;
            } else if nav_left {
                if self.settings_selected_idx > 0 && self.settings_selected_idx <= 2 {
                    self.settings_selected_idx -= 1;
                    key_navigated = true;
                }
            } else if nav_right {
                if self.settings_selected_idx < 2 {
                    self.settings_selected_idx += 1;
                    key_navigated = true;
                }
            } else if nav_down {
                if self.settings_selected_idx <= 2 {
                    self.settings_selected_idx = 3;
                } else if self.settings_selected_idx >= TOTAL_SETTINGS_ITEMS - 1 {
                    self.settings_selected_idx = 0;
                } else {
                    self.settings_selected_idx += 1;
                }
                key_navigated = true;
            } else if nav_up {
                if self.settings_selected_idx <= 2 {
                    self.settings_selected_idx = TOTAL_SETTINGS_ITEMS - 1;
                } else if self.settings_selected_idx == 3 {
                    self.settings_selected_idx = 0;
                } else {
                    self.settings_selected_idx -= 1;
                }
                key_navigated = true;
            }

            if key_navigated {
                self.settings_scroll_to_focus = true;
            }

            let sel_idx = self.settings_selected_idx;
            let scroll_to_focus = self.settings_scroll_to_focus;

            let win_resp = egui::Window::new("Settings & Preferences")
                .collapsible(false)
                .resizable(false)
                .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
                .show(ui.ctx(), |ui| {
                    ui.set_min_width(540.0);
                    ui.add_space(4.0);

                    egui::ScrollArea::vertical().max_height(480.0).show(ui, |ui| {
                        // Focus drawing helper (only scrolls into view when navigating via keyboard!)
                        let draw_focus = |ui: &mut egui::Ui, resp: &egui::Response, item_idx: usize| {
                            if sel_idx == item_idx {
                                if scroll_to_focus {
                                    resp.scroll_to_me(Some(egui::Align::Center));
                                }
                                ui.painter().rect_stroke(
                                    resp.rect.expand(2.0),
                                    3.0,
                                    egui::Stroke::new(1.5, Color32::from_rgb(100, 180, 255)),
                                    egui::StrokeKind::Outside,
                                );
                            }
                        };

                        // 1. Theme
                        ui.label(RichText::new("Theme & Appearance").size(14.0).strong());
                        ui.horizontal(|ui| {
                            let themes = [(0, AppTheme::Dark), (1, AppTheme::Light), (2, AppTheme::System)];
                            for (t_idx, theme) in themes {
                                let label_text = if sel_idx == t_idx {
                                    format!("> {}", theme.label())
                                } else {
                                    theme.label().to_string()
                                };
                                let resp = ui.selectable_label(self.config.theme == theme, label_text);
                                draw_focus(ui, &resp, t_idx);
                                if resp.clicked() || (sel_idx == t_idx && activate) {
                                    self.config.theme = theme;
                                    self.save_config();
                                }
                            }
                        });

                        ui.add_space(6.0);
                        ui.separator();

                        // 2. Usage Bar Scaling
                        ui.label(RichText::new("Usage Bar Scaling").size(14.0).strong());
                        ui.label(RichText::new("Scale usage bars relative to:").weak().small());
                        ui.add_space(2.0);

                        let bases = [
                            (3, UsageBasis::LargestItem),
                            (4, UsageBasis::Folder),
                            (5, UsageBasis::DriveUsed),
                            (6, UsageBasis::DriveTotal),
                        ];
                        for (b_idx, basis) in bases {
                            let is_chosen = self.config.usage_basis == basis;
                            let title = if sel_idx == b_idx {
                                format!("> {}", basis.display_name())
                            } else {
                                basis.display_name().to_string()
                            };
                            let resp = ui.selectable_label(is_chosen, RichText::new(title).strong());
                            draw_focus(ui, &resp, b_idx);
                            if resp.clicked() || (sel_idx == b_idx && activate) {
                                self.config.usage_basis = basis;
                                self.save_config();
                            }
                            ui.indent(format!("desc_{:?}", basis), |ui| {
                                ui.label(RichText::new(basis.description()).weak().small());
                            });
                            ui.add_space(2.0);
                        }

                        ui.add_space(6.0);
                        ui.separator();

                        // 3. Quick Search
                        ui.label(RichText::new("Quick Search").size(14.0).strong());
                        ui.add_space(2.0);

                        let s_modes = [
                            (7, QuickSearchMode::JumpToPrefix, "Jump to name (Explorer style)"),
                            (8, QuickSearchMode::LiveFilter, "Filter list as you type (FilePilot style)"),
                            (9, QuickSearchMode::Disabled, "Disabled"),
                        ];
                        for (m_idx, mode, label) in s_modes {
                            let title = if sel_idx == m_idx {
                                format!("> {}", label)
                            } else {
                                label.to_string()
                            };
                            let resp = ui.selectable_label(self.config.quick_search_mode == mode, title);
                            draw_focus(ui, &resp, m_idx);
                            if resp.clicked() || (sel_idx == m_idx && activate) {
                                self.config.quick_search_mode = mode;
                                self.quick_search_buffer.clear();
                                self.save_config();
                            }
                        }

                        ui.add_space(6.0);
                        ui.separator();

                        // 4. File Visibility
                        ui.label(RichText::new("File Visibility").size(14.0).strong());
                        let resp_h = ui.checkbox(&mut self.config.show_hidden_files, "Show Hidden Files (Ctrl+H)").on_hover_text("Show hidden files and dotfiles");
                        draw_focus(ui, &resp_h, 10);
                        if resp_h.changed() || (sel_idx == 10 && activate) {
                            if sel_idx == 10 && activate {
                                self.config.show_hidden_files = !self.config.show_hidden_files;
                            }
                            self.save_config();
                            self.sort_entries();
                        }

                        let resp_s = ui.checkbox(&mut self.config.show_system_files, "Show System Files & Links (Ctrl+Shift+H)").on_hover_text("Show protected system files and directory links");
                        draw_focus(ui, &resp_s, 11);
                        if resp_s.changed() || (sel_idx == 11 && activate) {
                            if sel_idx == 11 && activate {
                                self.config.show_system_files = !self.config.show_system_files;
                            }
                            self.save_config();
                            self.sort_entries();
                        }

                        let resp_l = ui.checkbox(&mut self.config.show_locked_folders, "Show Locked Folders").on_hover_text("Show folders that cannot be scanned due to permission restrictions");
                        draw_focus(ui, &resp_l, 12);
                        if resp_l.changed() || (sel_idx == 12 && activate) {
                            if sel_idx == 12 && activate {
                                self.config.show_locked_folders = !self.config.show_locked_folders;
                            }
                            self.save_config();
                            self.sort_entries();
                        }

                        ui.add_space(6.0);
                        ui.separator();

                        // 5. Application Preferences
                        ui.label(RichText::new("Application Preferences").size(14.0).strong());
                        let resp_r = ui.checkbox(&mut self.config.remember_last_path, "Remember last visited path on startup");
                        draw_focus(ui, &resp_r, 13);
                        if resp_r.changed() || (sel_idx == 13 && activate) {
                            if sel_idx == 13 && activate {
                                self.config.remember_last_path = !self.config.remember_last_path;
                            }
                            if !self.config.remember_last_path {
                                self.config.last_path = None;
                            }
                            self.save_config();
                        }

                        ui.add_space(6.0);
                        ui.separator();

                        // 6. Developer Cache Filter
                        ui.label(RichText::new("Developer Cache Filter").size(14.0).strong());
                        let resp_d = ui.checkbox(&mut self.ignore_dev_caches, "Ignore dev caches (.git, node_modules, target, .cache, .venv)");
                        draw_focus(ui, &resp_d, 14);
                        if resp_d.changed() || (sel_idx == 14 && activate) {
                            if sel_idx == 14 && activate {
                                self.ignore_dev_caches = !self.ignore_dev_caches;
                            }
                            self.config.ignore_dev_caches = self.ignore_dev_caches;
                            self.save_config();
                            trigger_rescan = true;
                        }

                        ui.label(RichText::new("Custom Exclude Names (comma-separated):").weak().small());
                        let edit = ui.add(
                            egui::TextEdit::singleline(&mut self.custom_excludes)
                                .hint_text(".git, node_modules, target, .cache, .venv")
                        );
                        if edit.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)) {
                            self.config.custom_excludes = self.custom_excludes.clone();
                            self.save_config();
                            trigger_rescan = true;
                        }

                        let resp_apply = ui.button("Apply Filter Changes");
                        draw_focus(ui, &resp_apply, 15);
                        if resp_apply.clicked() || (sel_idx == 15 && activate) {
                            self.config.custom_excludes = self.custom_excludes.clone();
                            self.save_config();
                            trigger_rescan = true;
                        }

                        ui.add_space(6.0);
                        ui.separator();

                        // 7. Shortcuts Sheet
                        let resp_sc = ui.button("Configure Shortcuts & Hotkeys (F1)");
                        draw_focus(ui, &resp_sc, 16);
                        if resp_sc.clicked() || (sel_idx == 16 && activate) {
                            self.show_shortcuts_modal = true;
                            self.shortcuts_just_opened = true;
                        }
                    });

                    ui.add_space(8.0);
                    ui.separator();
                    ui.horizontal(|ui| {
                        ui.label(RichText::new("Press Esc or click outside to return").weak().small());
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            let resp_done = ui.button("Done (Esc)");
                            if sel_idx == 17 {
                                ui.painter().rect_stroke(
                                    resp_done.rect.expand(2.0),
                                    3.0,
                                    egui::Stroke::new(1.5, Color32::from_rgb(100, 180, 255)),
                                    egui::StrokeKind::Outside,
                                );
                            }
                            if resp_done.clicked() || (sel_idx == 17 && activate) {
                                close_settings = true;
                            }
                        });
                    });
                });

            self.settings_scroll_to_focus = false;

            if let Some(resp) = win_resp {
                if !self.settings_just_opened && ui.input(|i| i.pointer.any_pressed()) {
                    if let Some(pos) = ui.input(|i| i.pointer.interact_pos()) {
                        if !resp.response.rect.contains(pos) {
                            close_settings = true;
                        }
                    }
                }
            }

            if self.settings_just_opened {
                self.settings_just_opened = false;
            }

            if close_settings {
                self.show_settings_modal = false;
            }
            if trigger_rescan {
                self.trigger_scan();
            }
        }
    }
}
