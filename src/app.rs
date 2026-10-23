use std::path::PathBuf;

use eframe::egui::{self, Color32, Panel, ProgressBar, RichText, ScrollArea, Vec2};

use crate::config::{AppConfig, ColumnId, SortRule, UsageBasis};
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

pub struct FreeDataDietApp {
    config: AppConfig,
    dragged_column_idx: Option<usize>,

    current_path: PathBuf,
    path_input: String,
    drives: Vec<DriveInfo>,
    entries: Vec<FileEntry>,
    selected_path: Option<PathBuf>,

    // Exclude filters
    ignore_dev_caches: bool,
    custom_excludes: String,
    show_exclude_bar: bool,

    scanner: Option<ScannerHandle>,
    is_scanning: bool,
    scanned_counter: usize,
    summary: Option<ScanSummary>,
    error_message: Option<String>,

    delete_modal: DeleteConfirmState,
    modal_focus: DeleteModalFocus,
    keyboard_context_menu_open: bool,
    is_admin: bool,
}

impl FreeDataDietApp {
    pub fn new(_cc: &eframe::CreationContext<'_>) -> Self {
        let config = AppConfig::load();
        let drives = platform::get_drives();
        let is_admin = platform::is_admin();

        let start_path = if let Some(ref last) = config.last_path {
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
            show_exclude_bar: false,
            scanner: None,
            is_scanning: false,
            scanned_counter: 0,
            summary: None,
            error_message: None,
            delete_modal: DeleteConfirmState::None,
            modal_focus: DeleteModalFocus::Trash,
            keyboard_context_menu_open: false,
            is_admin,
        };

        app.trigger_scan();
        app
    }

    fn save_config(&mut self) {
        self.config.ignore_dev_caches = self.ignore_dev_caches;
        self.config.custom_excludes = self.custom_excludes.clone();
        self.config.last_path = Some(self.current_path.clone());
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
            match self.config.sort_rules[pos].direction {
                SortDirection::Descending => {
                    self.config.sort_rules[pos].direction = SortDirection::Ascending;
                }
                SortDirection::Ascending => {
                    self.config.sort_rules.remove(pos);
                }
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

fn get_type_badge(entry: &FileEntry) -> (String, Color32) {
    if entry.is_reparse_point {
        ("[JUNC]".to_string(), Color32::from_rgb(200, 160, 255))
    } else if entry.is_system_protected {
        ("[SYS]".to_string(), Color32::from_rgb(255, 175, 40))
    } else if entry.is_access_denied {
        ("[LOCK]".to_string(), Color32::from_rgb(255, 90, 90))
    } else if entry.is_dir {
        ("[DIR]".to_string(), Color32::from_rgb(80, 200, 255))
    } else if let Some(ext) = entry.path.extension().and_then(|e| e.to_str()) {
        let ext_upper = ext.to_uppercase();
        if ext_upper.len() <= 4 {
            (format!("[{}]", ext_upper), Color32::from_rgb(160, 185, 205))
        } else {
            ("[FILE]".to_string(), Color32::from_rgb(150, 160, 175))
        }
    } else {
        ("[FILE]".to_string(), Color32::from_rgb(150, 160, 175))
    }
}

impl eframe::App for FreeDataDietApp {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        self.poll_scanner();

        if self.is_scanning {
            ui.ctx().request_repaint();
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
                            Color32::from_rgb(100, 200, 255)
                        } else {
                            Color32::LIGHT_GRAY
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

                let text_edit = ui.add_sized(
                    Vec2::new(ui.available_width() - 300.0, 24.0),
                    egui::TextEdit::singleline(&mut self.path_input),
                );

                if text_edit.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)) {
                    let candidate = PathBuf::from(&self.path_input);
                    if candidate.exists() {
                        self.navigate_to(candidate);
                    }
                }

                if ui.button("Browse...").clicked() {
                    if let Some(folder) = rfd::FileDialog::new()
                        .set_directory(&self.current_path)
                        .pick_folder()
                    {
                        self.navigate_to(folder);
                    }
                }

                if ui.button("Refresh").clicked() {
                    self.trigger_scan();
                }

                let filter_label = if self.show_exclude_bar {
                    "Filter [-]"
                } else {
                    "Filter [+]"
                };
                if ui.button(filter_label).clicked() {
                    self.show_exclude_bar = !self.show_exclude_bar;
                }

                ui.menu_button(format!("%: {}", self.config.usage_basis.label()), |ui| {
                    if ui.selectable_label(self.config.usage_basis == UsageBasis::Folder, "Current Folder").clicked() {
                        self.config.usage_basis = UsageBasis::Folder;
                        self.save_config();
                        ui.close();
                    }
                    if ui.selectable_label(self.config.usage_basis == UsageBasis::DriveUsed, "Drive (Used Space)").clicked() {
                        self.config.usage_basis = UsageBasis::DriveUsed;
                        self.save_config();
                        ui.close();
                    }
                    if ui.selectable_label(self.config.usage_basis == UsageBasis::DriveTotal, "Drive (Total Capacity)").clicked() {
                        self.config.usage_basis = UsageBasis::DriveTotal;
                        self.save_config();
                        ui.close();
                    }
                });

                if ui.checkbox(&mut self.config.show_hidden_files, "Hidden").on_hover_text("Show hidden files and dotfiles").changed() {
                    self.save_config();
                }
                if ui.checkbox(&mut self.config.show_system_files, "System").on_hover_text("Show protected system files and junctions (e.g. Documents and Settings)").changed() {
                    self.save_config();
                }
            });

            // Exclude Filter Bar
            if self.show_exclude_bar {
                ui.add_space(4.0);
                ui.horizontal(|ui| {
                    let prev_ignore = self.ignore_dev_caches;
                    ui.checkbox(&mut self.ignore_dev_caches, "Ignore dev caches:");
                    let edit = ui.add(
                        egui::TextEdit::singleline(&mut self.custom_excludes)
                            .hint_text(".git, node_modules, target, .cache")
                            .desired_width(ui.available_width() - 90.0),
                    );

                    let apply_btn = ui.button("Apply").clicked();
                    if self.ignore_dev_caches != prev_ignore
                        || apply_btn
                        || (edit.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)))
                    {
                        self.save_config();
                        self.trigger_scan();
                    }
                });
            }

            ui.add_space(6.0);
        });

        if let Some(drive_path) = target_drive {
            self.navigate_to(drive_path);
        }

        // --- BOTTOM PANEL ---
        Panel::bottom("bottom_panel").show(ui, |ui| {
            ui.add_space(4.0);
            ui.horizontal(|ui| {
                if self.is_scanning {
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
                        RichText::new("[↑↓ / jk] Navigate  |  [Enter / →] Open  |  [Backspace / ←] Up  |  [Del] Trash  |  [Shift+Del] Delete")
                            .small()
                            .color(Color32::DARK_GRAY),
                    );
                });
            });
            ui.add_space(4.0);
        });

        // --- RIGHT DETAILS PANEL ---
        Panel::right("details_panel")
            .min_size(270.0)
            .max_size(340.0)
            .show(ui, |ui| {
                ui.add_space(8.0);
                ui.heading(RichText::new("Item Details").size(16.0));
                ui.separator();

                let selected_entry = self.selected_path.as_ref().and_then(|p| {
                    self.entries.iter().find(|e| &e.path == p)
                });

                if let Some(entry) = selected_entry {
                    let (badge, badge_color) = get_type_badge(entry);
                    ui.horizontal(|ui| {
                        ui.label(RichText::new(badge).monospace().color(badge_color).strong());
                        ui.label(RichText::new(&entry.name).strong());
                    });
                    ui.add_space(4.0);

                    egui::Grid::new("detail_grid")
                        .num_columns(2)
                        .spacing([8.0, 6.0])
                        .show(ui, |ui| {
                            ui.label("Type:");
                            ui.label(if entry.is_reparse_point { "Junction / Link" } else if entry.is_dir { "Folder" } else { "File" });
                            ui.end_row();

                            ui.label("File Size:");
                            if entry.is_scanning {
                                ui.label(RichText::new("Calculating...").italics().color(Color32::from_rgb(200, 160, 80)));
                            } else {
                                ui.label(format_size(entry.size));
                            }
                            ui.end_row();

                            ui.label("Size on Disk:");
                            if entry.is_scanning {
                                ui.label(RichText::new("Calculating...").italics().color(Color32::from_rgb(200, 160, 80)));
                            } else {
                                ui.label(format_size(entry.allocated_size));
                            }
                            ui.end_row();

                            if entry.is_dir {
                                ui.label("Items Inside:");
                                if entry.is_scanning {
                                    ui.label("...");
                                } else {
                                    ui.label(format!("{}", entry.item_count));
                                }
                                ui.end_row();
                            }

                            ui.label("Modified:");
                            let date_str = entry
                                .modified
                                .map(crate::model::format_system_time)
                                .unwrap_or_else(|| "-".to_string());
                            ui.label(date_str);
                            ui.end_row();

                                if entry.is_system_protected {
                                    ui.label(RichText::new("Protection:").color(Color32::from_rgb(255, 180, 50)));
                                    ui.label(
                                        RichText::new("[SYS] System Path")
                                            .color(Color32::from_rgb(255, 180, 50)),
                                    );
                                    ui.end_row();
                                }

                                if entry.is_access_denied {
                                    ui.label(RichText::new("Status:").color(Color32::RED));
                                    ui.label(
                                        RichText::new("[LOCKED] Access Denied")
                                            .color(Color32::from_rgb(255, 120, 120)),
                                    );
                                    ui.end_row();
                                }
                            });

                        ui.add_space(14.0);
                        ui.separator();
                        ui.label(RichText::new("Actions").strong());
                        ui.add_space(4.0);

                        if ui
                            .button("Show in File Manager")
                            .on_hover_text("Opens the native Explorer / file manager")
                            .clicked()
                        {
                            let _ = platform::open_in_file_manager(&entry.path);
                        }

                        if !entry.is_dir && ui.button("Open File").clicked() {
                            let _ = platform::open_file(&entry.path);
                        }

                        if ui.button("Copy Path").clicked() {
                            ui.ctx().copy_text(entry.path.to_string_lossy().to_string());
                        }

                        #[cfg(windows)]
                        if ui
                            .button("Properties (Windows)")
                            .on_hover_text("Opens the native Windows properties dialog")
                            .clicked()
                        {
                            let _ = platform::show_properties(&entry.path);
                        }

                        ui.add_space(8.0);

                        let trash_btn = ui.button(
                            RichText::new("Move to Trash")
                                .color(Color32::from_rgb(255, 200, 100)),
                        );
                        if trash_btn.clicked() {
                            self.delete_modal = DeleteConfirmState::Confirm {
                                path: entry.path.clone(),
                                name: entry.name.clone(),
                                is_system: entry.is_system_protected,
                            };
                            self.modal_focus = DeleteModalFocus::Trash;
                        }

                        let perm_btn = ui.button(
                            RichText::new("Delete Permanently")
                                .color(Color32::from_rgb(255, 100, 100)),
                        );
                        if perm_btn.clicked() {
                            self.delete_modal = DeleteConfirmState::Confirm {
                                path: entry.path.clone(),
                                name: entry.name.clone(),
                                is_system: entry.is_system_protected,
                            };
                            self.modal_focus = DeleteModalFocus::Permanent;
                        }
                } else {
                    ui.label(
                        RichText::new("Select a folder or file to view details.")
                            .italics()
                            .color(Color32::GRAY),
                    );
                }
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

                    let indicator = if let Some(pos) = sort_pos {
                        let arrow = if self.config.sort_rules[pos].direction == SortDirection::Ascending { "^" } else { "v" };
                        if self.config.sort_rules.len() > 1 {
                            format!(" {}{}", pos + 1, arrow)
                        } else {
                            format!(" {}", arrow)
                        }
                    } else {
                        "".to_string()
                    };

                    let col_title = if col == ColumnId::Usage {
                        match current_basis {
                            UsageBasis::Folder => "Usage (Dir)",
                            UsageBasis::DriveUsed => "Usage (Drive)",
                            UsageBasis::DriveTotal => "Usage (Total)",
                        }
                    } else {
                        col.title()
                    };
                    let label_text = format!("{}{}", col_title, indicator);

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
                        .on_hover_text("Click to sort (multi-column: 1st=desc, 2nd=asc, 3rd=remove), drag to reorder");

                    if col == ColumnId::Usage {
                        header_resp.context_menu(|ui| {
                            ui.label(RichText::new("Usage Percentage Basis").strong());
                            ui.separator();
                            if ui.selectable_label(current_basis == UsageBasis::Folder, "Relative to Current Folder").clicked() {
                                change_basis = Some(UsageBasis::Folder);
                                ui.close();
                            }
                            if ui.selectable_label(current_basis == UsageBasis::DriveUsed, "Relative to Drive Used Space").clicked() {
                                change_basis = Some(UsageBasis::DriveUsed);
                                ui.close();
                            }
                            if ui.selectable_label(current_basis == UsageBasis::DriveTotal, "Relative to Drive Total Capacity").clicked() {
                                change_basis = Some(UsageBasis::DriveTotal);
                                ui.close();
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
                        Color32::from_rgb(45, 80, 125)
                    } else if header_resp.hovered() {
                        Color32::from_rgb(38, 48, 62)
                    } else {
                        Color32::from_rgb(26, 33, 44)
                    };

                    let stroke_color = if is_dragging {
                        Color32::from_rgb(100, 180, 255)
                    } else if is_active_sort {
                        Color32::from_rgb(60, 120, 180)
                    } else {
                        Color32::from_rgb(48, 58, 72)
                    };

                    ui.painter().rect(
                        header_rect,
                        3.0,
                        bg_color,
                        egui::Stroke::new(1.0, stroke_color),
                        egui::StrokeKind::Inside,
                    );

                    let text_color = if is_active_sort || is_dragging {
                        Color32::WHITE
                    } else {
                        Color32::from_rgb(200, 210, 220)
                    };

                    let is_right_aligned = matches!(
                        col,
                        ColumnId::Size | ColumnId::AllocatedSize | ColumnId::Items | ColumnId::Modified
                    );

                    let is_centered = matches!(col, ColumnId::Type);

                    let (align, text_pos) = if is_right_aligned {
                        (
                            egui::Align2::RIGHT_CENTER,
                            egui::pos2(header_rect.max.x - 8.0, header_rect.center().y),
                        )
                    } else if is_centered {
                        (
                            egui::Align2::CENTER_CENTER,
                            header_rect.center(),
                        )
                    } else {
                        (
                            egui::Align2::LEFT_CENTER,
                            egui::pos2(header_rect.min.x + 8.0, header_rect.center().y),
                        )
                    };

                    ui.painter().text(
                        text_pos,
                        align,
                        &label_text,
                        egui::FontId::proportional(12.0),
                        text_color,
                    );
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
            let mut context_delete: Option<(DeleteConfirmState, DeleteModalFocus)> = None;
            let mut do_navigate_up = false;
            let mut do_trigger_rescan = false;

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
                    true
                })
                .collect();

            let selected_visible_idx = self.selected_path.as_ref().and_then(|p| {
                visible_entries.iter().position(|(_, e)| &e.path == p)
            });

            // Keyboard navigation (active when not typing in text input)
            if !ui.ctx().egui_wants_keyboard_input() && self.delete_modal == DeleteConfirmState::None {
                let down = ui.input(|i| i.key_pressed(egui::Key::ArrowDown) || i.key_pressed(egui::Key::J));
                let up = ui.input(|i| i.key_pressed(egui::Key::ArrowUp) || i.key_pressed(egui::Key::K));
                let enter = ui.input(|i| i.key_pressed(egui::Key::Enter));
                let right = ui.input(|i| i.key_pressed(egui::Key::ArrowRight));
                let back = ui.input(|i| i.key_pressed(egui::Key::Backspace) || i.key_pressed(egui::Key::ArrowLeft));
                let home = ui.input(|i| i.key_pressed(egui::Key::Home));
                let end = ui.input(|i| i.key_pressed(egui::Key::End));
                let page_up = ui.input(|i| i.key_pressed(egui::Key::PageUp));
                let page_down = ui.input(|i| i.key_pressed(egui::Key::PageDown));
                let del = ui.input(|i| i.key_pressed(egui::Key::Delete));
                let shift = ui.input(|i| i.modifiers.shift);
                let refresh = ui.input(|i| i.key_pressed(egui::Key::F5));
                let context_key = ui.input(|i| i.key_pressed(egui::Key::Menu) || (i.modifiers.shift && i.key_pressed(egui::Key::F10)));

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

                if back {
                    do_navigate_up = true;
                }

                if del {
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
                    self.keyboard_context_menu_open = true;
                }

                if refresh {
                    do_trigger_rescan = true;
                }
            }

            ScrollArea::vertical()
                .auto_shrink([false, false])
                .show(ui, |ui| {
                    for (_vis_idx, &(orig_idx, entry)) in visible_entries.iter().enumerate() {
                        let is_selected = self.selected_path.as_ref() == Some(&entry.path);
                        let (badge, badge_color) = get_type_badge(entry);

                        // Reserve background shape slot before drawing row text
                        let bg_idx = ui.painter().add(egui::Shape::Noop);

                        let row_resp = ui.horizontal(|ui| {
                            ui.spacing_mut().item_spacing.x = col_spacing;

                            for &col in &self.config.column_order {
                                let w = if col == ColumnId::Name { name_width } else { col.fixed_width() };
                                let (cell_rect, _) = ui.allocate_exact_size(Vec2::new(w, 20.0), egui::Sense::hover());

                                match col {
                                    ColumnId::Type => {
                                        ui.painter().text(
                                            cell_rect.center(),
                                            egui::Align2::CENTER_CENTER,
                                            &badge,
                                            egui::FontId::monospace(12.0),
                                            badge_color,
                                        );
                                    }
                                    ColumnId::Name => {
                                        let mut child_ui = ui.new_child(egui::UiBuilder::new().max_rect(cell_rect));
                                        child_ui.with_layout(egui::Layout::left_to_right(egui::Align::Center), |ui| {
                                            let name_color = if is_selected {
                                                Color32::from_rgb(120, 215, 255)
                                            } else {
                                                Color32::WHITE
                                            };
                                            ui.add_space(4.0);
                                            ui.add(
                                                egui::Label::new(
                                                    RichText::new(&entry.name).color(name_color).strong(),
                                                ).truncate(),
                                            );
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
                                                Color32::from_rgb(220, 220, 220),
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
                                                Color32::GRAY,
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
                                            Color32::DARK_GRAY,
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
                                            Color32::GRAY,
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

                        // Right-Click Context Menu
                        row_click.context_menu(|ui| {
                            ui.set_min_width(210.0);
                            ui.horizontal(|ui| {
                                ui.label(RichText::new(&badge).monospace().color(badge_color).strong());
                                ui.label(RichText::new(&entry.name).strong());
                            });
                            ui.separator();

                            if entry.is_dir {
                                if ui.button("Open Folder in FreeDataDiet").clicked() {
                                    navigate_target = Some(entry.path.clone());
                                    ui.close();
                                }
                            } else if ui.button("Open / Launch File").clicked() {
                                let _ = platform::open_file(&entry.path);
                                ui.close();
                            }

                            if ui.button("Show in Windows Explorer").clicked() {
                                let _ = platform::open_in_file_manager(&entry.path);
                                ui.close();
                            }

                            if ui.button("Copy Full Path").clicked() {
                                ui.ctx().copy_text(entry.path.to_string_lossy().to_string());
                                ui.close();
                            }

                            #[cfg(windows)]
                            if ui.button("Windows Properties...").clicked() {
                                let _ = platform::show_properties(&entry.path);
                                ui.close();
                            }

                            ui.separator();

                            if ui.button(
                                RichText::new("Move to Recycle Bin")
                                    .color(Color32::from_rgb(255, 200, 100)),
                            ).clicked() {
                                context_delete = Some((
                                    DeleteConfirmState::Confirm {
                                        path: entry.path.clone(),
                                        name: entry.name.clone(),
                                        is_system: entry.is_system_protected,
                                    },
                                    DeleteModalFocus::Trash,
                                ));
                                ui.close();
                            }

                            if ui.button(
                                RichText::new("Delete Permanently")
                                    .color(Color32::from_rgb(255, 100, 100)),
                            ).clicked() {
                                context_delete = Some((
                                    DeleteConfirmState::Confirm {
                                        path: entry.path.clone(),
                                        name: entry.name.clone(),
                                        is_system: entry.is_system_protected,
                                    },
                                    DeleteModalFocus::Permanent,
                                ));
                                ui.close();
                            }
                        });

                        // Paint full-row selection or hover background behind text
                        if is_selected {
                            ui.painter().set(
                                bg_idx,
                                egui::epaint::RectShape::new(
                                    full_row_rect,
                                    3.0,
                                    Color32::from_rgb(32, 54, 82),
                                    egui::Stroke::new(1.0, Color32::from_rgb(60, 140, 220)),
                                    egui::StrokeKind::Inside,
                                ),
                            );
                        } else if row_click.hovered() {
                            ui.painter().set(
                                bg_idx,
                                egui::epaint::RectShape::filled(
                                    full_row_rect,
                                    3.0,
                                    Color32::from_rgb(26, 32, 42),
                                ),
                            );
                        }

                        if row_click.clicked() || row_click.secondary_clicked() {
                            self.selected_path = Some(entry.path.clone());
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

            if let Some((modal, focus)) = context_delete {
                self.delete_modal = modal;
                self.modal_focus = focus;
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
        });

        // --- KEYBOARD CONTEXT MENU WINDOW ---
        if self.keyboard_context_menu_open {
            let selected_entry = self.selected_path.as_ref().and_then(|p| {
                self.entries.iter().find(|e| &e.path == p).cloned()
            });

            if let Some(entry) = selected_entry {
                let mut close_menu = false;
                let mut menu_navigate = None;
                let mut menu_delete: Option<(DeleteConfirmState, DeleteModalFocus)> = None;

                egui::Window::new("Entry Actions")
                    .collapsible(false)
                    .resizable(false)
                    .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
                    .show(ui.ctx(), |ui| {
                        ui.label(RichText::new(&entry.name).strong());
                        ui.separator();

                        if entry.is_dir {
                            if ui.button("Open Folder").clicked() {
                                menu_navigate = Some(entry.path.clone());
                                close_menu = true;
                            }
                        } else if ui.button("Open / Launch File").clicked() {
                            let _ = platform::open_file(&entry.path);
                            close_menu = true;
                        }

                        if ui.button("Show in Windows Explorer").clicked() {
                            let _ = platform::open_in_file_manager(&entry.path);
                            close_menu = true;
                        }

                        if ui.button("Copy Full Path").clicked() {
                            ui.ctx().copy_text(entry.path.to_string_lossy().to_string());
                            close_menu = true;
                        }

                        #[cfg(windows)]
                        if ui.button("Windows Properties...").clicked() {
                            let _ = platform::show_properties(&entry.path);
                            close_menu = true;
                        }

                        ui.separator();

                        if ui.button(
                            RichText::new("Move to Recycle Bin")
                                .color(Color32::from_rgb(255, 200, 100)),
                        ).clicked() {
                            menu_delete = Some((
                                DeleteConfirmState::Confirm {
                                    path: entry.path.clone(),
                                    name: entry.name.clone(),
                                    is_system: entry.is_system_protected,
                                },
                                DeleteModalFocus::Trash,
                            ));
                            close_menu = true;
                        }

                        if ui.button(
                            RichText::new("Delete Permanently")
                                .color(Color32::from_rgb(255, 100, 100)),
                        ).clicked() {
                            menu_delete = Some((
                                DeleteConfirmState::Confirm {
                                    path: entry.path.clone(),
                                    name: entry.name.clone(),
                                    is_system: entry.is_system_protected,
                                },
                                DeleteModalFocus::Permanent,
                            ));
                            close_menu = true;
                        }

                        ui.separator();
                        if ui.button("Close Menu (Esc)").clicked() {
                            close_menu = true;
                        }
                    });

                if ui.input(|i| i.key_pressed(egui::Key::Escape)) {
                    close_menu = true;
                }

                if let Some(target) = menu_navigate {
                    self.navigate_to(target);
                }
                if let Some((modal, focus)) = menu_delete {
                    self.delete_modal = modal;
                    self.modal_focus = focus;
                }
                if close_menu {
                    self.keyboard_context_menu_open = false;
                }
            } else {
                self.keyboard_context_menu_open = false;
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
                            RichText::new("▶ Move to Trash ◀").color(Color32::WHITE).strong()
                        } else {
                            RichText::new("Move to Trash")
                        };
                        let trash_btn = egui::Button::new(trash_text)
                            .fill(if is_trash { Color32::from_rgb(30, 110, 200) } else { Color32::from_rgb(50, 50, 50) });

                        // 2. Delete Permanently
                        let is_perm = self.modal_focus == DeleteModalFocus::Permanent;
                        let perm_text = if is_perm {
                            RichText::new("▶ Delete Permanently ◀").color(Color32::WHITE).strong()
                        } else {
                            RichText::new("Delete Permanently").color(Color32::from_rgb(255, 120, 120))
                        };
                        let perm_btn = egui::Button::new(perm_text)
                            .fill(if is_perm { Color32::from_rgb(180, 40, 40) } else { Color32::from_rgb(50, 50, 50) });

                        // 3. Cancel
                        let is_cancel = self.modal_focus == DeleteModalFocus::Cancel;
                        let cancel_text = if is_cancel {
                            RichText::new("▶ Cancel ◀").color(Color32::WHITE).strong()
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
                                std::fs::remove_dir_all(&path)
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
    }
}
