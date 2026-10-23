use std::path::PathBuf;
use std::time::SystemTime;

use eframe::egui::{self, Color32, Panel, ProgressBar, RichText, ScrollArea, Vec2};

use crate::config::{AppConfig, ColumnId};
use crate::model::{format_size, DriveInfo, FileEntry, SortBy, SortDirection};
use crate::platform;
use crate::scanner::{start_scan, ScanMessage, ScanSummary, ScannerHandle};

#[derive(Clone, PartialEq)]
enum DeleteConfirmState {
    None,
    ConfirmPermanent(PathBuf, String, bool),
    ConfirmTrash(PathBuf, String, bool),
}

pub struct FreeDataDietApp {
    config: AppConfig,
    dragged_column_idx: Option<usize>,

    current_path: PathBuf,
    path_input: String,
    drives: Vec<DriveInfo>,
    entries: Vec<FileEntry>,
    selected_index: Option<usize>,

    sort_by: SortBy,
    sort_dir: SortDirection,

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
            sort_by: config.sort_by,
            sort_dir: config.sort_direction,
            ignore_dev_caches: config.ignore_dev_caches,
            custom_excludes: config.custom_excludes.clone(),
            config,
            dragged_column_idx: None,
            path_input: start_path.to_string_lossy().to_string(),
            current_path: start_path,
            drives,
            entries: Vec::new(),
            selected_index: None,
            show_exclude_bar: false,
            scanner: None,
            is_scanning: false,
            scanned_counter: 0,
            summary: None,
            error_message: None,
            delete_modal: DeleteConfirmState::None,
            is_admin,
        };

        app.trigger_scan();
        app
    }

    fn save_config(&mut self) {
        self.config.sort_by = self.sort_by;
        self.config.sort_direction = self.sort_dir;
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
        self.selected_index = None;
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
                ScanMessage::EntryReady(entry) => {
                    self.entries.push(entry);
                    self.sort_entries();
                }
                ScanMessage::Completed(sum) => {
                    self.summary = Some(sum);
                    self.is_scanning = false;
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
        let sort_by = self.sort_by;
        let sort_dir = self.sort_dir;

        self.entries.sort_by(|a, b| {
            let cmp = match sort_by {
                SortBy::Size => a.size.cmp(&b.size),
                SortBy::AllocatedSize => a.allocated_size.cmp(&b.allocated_size),
                SortBy::Name => a.name.to_lowercase().cmp(&b.name.to_lowercase()),
                SortBy::Modified => a.modified.cmp(&b.modified),
                SortBy::ItemCount => a.item_count.cmp(&b.item_count),
            };

            match sort_dir {
                SortDirection::Ascending => cmp,
                SortDirection::Descending => cmp.reverse(),
            }
        });
    }

    fn toggle_sort(&mut self, target: SortBy) {
        if self.sort_by == target {
            self.sort_dir = match self.sort_dir {
                SortDirection::Ascending => SortDirection::Descending,
                SortDirection::Descending => SortDirection::Ascending,
            };
        } else {
            self.sort_by = target;
            self.sort_dir = SortDirection::Descending;
        }
        self.sort_entries();
        self.save_config();
    }
}

fn get_type_badge(entry: &FileEntry) -> (String, Color32) {
    if entry.is_system_protected {
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

                if let Some(idx) = self.selected_index {
                    if let Some(entry) = self.entries.get(idx) {
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
                                ui.label(if entry.is_dir { "Folder" } else { "File" });
                                ui.end_row();

                                ui.label("File Size:");
                                ui.label(format_size(entry.size));
                                ui.end_row();

                                ui.label("Size on Disk:");
                                ui.label(format_size(entry.allocated_size));
                                ui.end_row();

                                if entry.is_dir {
                                    ui.label("Items Inside:");
                                    ui.label(format!("{}", entry.item_count));
                                    ui.end_row();
                                }

                                ui.label("Modified:");
                                let date_str = entry
                                    .modified
                                    .and_then(|t| t.duration_since(SystemTime::UNIX_EPOCH).ok())
                                    .map(|d| {
                                        let secs = d.as_secs();
                                        format!("{secs}")
                                    })
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
                            self.delete_modal = DeleteConfirmState::ConfirmTrash(
                                entry.path.clone(),
                                entry.name.clone(),
                                entry.is_system_protected,
                            );
                        }

                        let perm_btn = ui.button(
                            RichText::new("Delete Permanently")
                                .color(Color32::from_rgb(255, 100, 100)),
                        );
                        if perm_btn.clicked() {
                            self.delete_modal = DeleteConfirmState::ConfirmPermanent(
                                entry.path.clone(),
                                entry.name.clone(),
                                entry.is_system_protected,
                            );
                        }
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

            // Interactive Drag-and-Drop & Sortable Header Row
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = col_spacing;

                for (idx, &col) in self.config.column_order.iter().enumerate() {
                    let w = if col == ColumnId::Name { name_width } else { col.fixed_width() };
                    let is_dragging = self.dragged_column_idx == Some(idx);
                    let is_active_sort = col.sort_by() == Some(self.sort_by);

                    let arrow = if is_active_sort {
                        if self.sort_dir == SortDirection::Ascending { " ^" } else { " v" }
                    } else {
                        ""
                    };
                    let label_text = format!("{}{}", col.title(), arrow);

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
                        .on_hover_text("Click to sort, drag left/right to reorder columns");

                    if header_resp.clicked() {
                        if let Some(target) = col.sort_by() {
                            sort_column = Some(target);
                        }
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

                    let (align, text_pos) = if is_right_aligned {
                        (
                            egui::Align2::RIGHT_CENTER,
                            egui::pos2(header_rect.max.x - 8.0, header_rect.center().y),
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
                        egui::FontId::proportional(12.5),
                        text_color,
                    );
                }
            });

            if let Some((from, to)) = swap_columns {
                self.config.column_order.swap(from, to);
                self.dragged_column_idx = Some(to);
                let _ = self.config.save();
            }

            if let Some(target) = sort_column {
                self.toggle_sort(target);
            }

            ui.add_space(2.0);
            ui.separator();

            let max_size = self
                .entries
                .first()
                .map(|e| e.size)
                .unwrap_or(1)
                .max(1) as f32;

            let mut navigate_target = None;
            let mut scroll_to_selected = false;
            let mut context_delete = None;

            // Keyboard navigation (active when not typing in text input)
            if !ui.ctx().egui_wants_keyboard_input() && self.delete_modal == DeleteConfirmState::None {
                let down = ui.input(|i| i.key_pressed(egui::Key::ArrowDown) || i.key_pressed(egui::Key::J));
                let up = ui.input(|i| i.key_pressed(egui::Key::ArrowUp) || i.key_pressed(egui::Key::K));
                let enter = ui.input(|i| i.key_pressed(egui::Key::Enter) || i.key_pressed(egui::Key::ArrowRight));
                let back = ui.input(|i| i.key_pressed(egui::Key::Backspace) || i.key_pressed(egui::Key::ArrowLeft));
                let del = ui.input(|i| i.key_pressed(egui::Key::Delete));
                let shift = ui.input(|i| i.modifiers.shift);
                let refresh = ui.input(|i| i.key_pressed(egui::Key::F5));

                if down && !self.entries.is_empty() {
                    let next = match self.selected_index {
                        Some(idx) => (idx + 1).min(self.entries.len().saturating_sub(1)),
                        None => 0,
                    };
                    self.selected_index = Some(next);
                    scroll_to_selected = true;
                }

                if up && !self.entries.is_empty() {
                    let prev = match self.selected_index {
                        Some(idx) => idx.saturating_sub(1),
                        None => 0,
                    };
                    self.selected_index = Some(prev);
                    scroll_to_selected = true;
                }

                if enter {
                    if let Some(idx) = self.selected_index {
                        if let Some(entry) = self.entries.get(idx) {
                            if entry.is_dir {
                                navigate_target = Some(entry.path.clone());
                            } else {
                                let _ = platform::open_file(&entry.path);
                            }
                        }
                    }
                }

                if back {
                    self.navigate_up();
                }

                if del {
                    if let Some(idx) = self.selected_index {
                        if let Some(entry) = self.entries.get(idx) {
                            if shift {
                                self.delete_modal = DeleteConfirmState::ConfirmPermanent(
                                    entry.path.clone(),
                                    entry.name.clone(),
                                    entry.is_system_protected,
                                );
                            } else {
                                self.delete_modal = DeleteConfirmState::ConfirmTrash(
                                    entry.path.clone(),
                                    entry.name.clone(),
                                    entry.is_system_protected,
                                );
                            }
                        }
                    }
                }

                if refresh {
                    self.trigger_scan();
                }
            }

            ScrollArea::vertical()
                .auto_shrink([false, false])
                .show(ui, |ui| {
                    for (i, entry) in self.entries.iter().enumerate() {
                        let is_selected = self.selected_index == Some(i);
                        let (badge, badge_color) = get_type_badge(entry);

                        // Reserve background shape slot before drawing row text
                        let bg_idx = ui.painter().add(egui::Shape::Noop);

                        let row_resp = ui.horizontal(|ui| {
                            ui.spacing_mut().item_spacing.x = col_spacing;

                            for &col in &self.config.column_order {
                                let w = if col == ColumnId::Name { name_width } else { col.fixed_width() };
                                let (cell_rect, _) = ui.allocate_exact_size(Vec2::new(w, 20.0), egui::Sense::hover());

                                match col {
                                    ColumnId::Name => {
                                        let mut child_ui = ui.new_child(egui::UiBuilder::new().max_rect(cell_rect));
                                        child_ui.with_layout(egui::Layout::left_to_right(egui::Align::Center), |ui| {
                                            // Type Badge
                                            ui.add_sized(
                                                [44.0, 20.0],
                                                egui::Label::new(
                                                    RichText::new(&badge).monospace().color(badge_color).strong(),
                                                ),
                                            );

                                            // Name Label (highlighted if selected)
                                            let name_color = if is_selected {
                                                Color32::from_rgb(120, 215, 255)
                                            } else {
                                                Color32::WHITE
                                            };
                                            ui.add(
                                                egui::Label::new(
                                                    RichText::new(&entry.name).color(name_color).strong(),
                                                ).truncate(),
                                            );
                                        });
                                    }
                                    ColumnId::Size => {
                                        ui.painter().text(
                                            egui::pos2(cell_rect.max.x - 8.0, cell_rect.center().y),
                                            egui::Align2::RIGHT_CENTER,
                                            format_size(entry.size),
                                            egui::FontId::monospace(12.5),
                                            Color32::from_rgb(220, 220, 220),
                                        );
                                    }
                                    ColumnId::AllocatedSize => {
                                        ui.painter().text(
                                            egui::pos2(cell_rect.max.x - 8.0, cell_rect.center().y),
                                            egui::Align2::RIGHT_CENTER,
                                            format_size(entry.allocated_size),
                                            egui::FontId::monospace(12.5),
                                            Color32::GRAY,
                                        );
                                    }
                                    ColumnId::Usage => {
                                        let bar_rect = egui::Rect::from_center_size(cell_rect.center(), Vec2::new(w - 10.0, 16.0));
                                        let mut child_ui = ui.new_child(egui::UiBuilder::new().max_rect(bar_rect));
                                        let ratio = (entry.size as f32 / max_size).clamp(0.0, 1.0);
                                        let bar = ProgressBar::new(ratio)
                                            .text(format!("{:.1}%", ratio * 100.0))
                                            .desired_width(w - 10.0);
                                        child_ui.add(bar);
                                    }
                                    ColumnId::Items => {
                                        let txt = if entry.is_dir {
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
                            ui.scope_id().with(i),
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
                                context_delete = Some(DeleteConfirmState::ConfirmTrash(
                                    entry.path.clone(),
                                    entry.name.clone(),
                                    entry.is_system_protected,
                                ));
                                ui.close();
                            }

                            if ui.button(
                                RichText::new("Delete Permanently")
                                    .color(Color32::from_rgb(255, 100, 100)),
                            ).clicked() {
                                context_delete = Some(DeleteConfirmState::ConfirmPermanent(
                                    entry.path.clone(),
                                    entry.name.clone(),
                                    entry.is_system_protected,
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
                            self.selected_index = Some(i);
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

            if let Some(modal) = context_delete {
                self.delete_modal = modal;
            }

            if let Some(target) = navigate_target {
                self.navigate_to(target);
            }
        });

        // --- CONFIRMATION DIALOG MODAL ---
        let mut trigger_rescan = false;
        let mut close_modal = false;
        let mut modal_error = None;

        let current_modal = self.delete_modal.clone();
        match current_modal {
            DeleteConfirmState::ConfirmTrash(path, name, is_system) => {
                egui::Window::new("Move to Trash?")
                    .collapsible(false)
                    .resizable(false)
                    .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
                    .show(ui.ctx(), |ui| {
                        if is_system {
                            ui.label(RichText::new("[!] WARNING: Protected system path!").color(Color32::RED).strong());
                        }
                        ui.label(format!("Do you want to move \"{}\" to the Recycle Bin?", name));
                        ui.add_space(8.0);
                        ui.horizontal(|ui| {
                            if ui.button("Yes, Move to Trash").clicked() {
                                if let Err(e) = trash::delete(&path) {
                                    modal_error = Some(format!("Trash Error: {}", e));
                                } else {
                                    trigger_rescan = true;
                                }
                                close_modal = true;
                            }
                            if ui.button("Cancel").clicked() {
                                close_modal = true;
                            }
                        });
                    });
            }
            DeleteConfirmState::ConfirmPermanent(path, name, is_system) => {
                egui::Window::new("Delete Permanently?")
                    .collapsible(false)
                    .resizable(false)
                    .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
                    .show(ui.ctx(), |ui| {
                        if is_system {
                            ui.label(RichText::new("[CRITICAL WARNING] This is a SYSTEM PATH! Deleting it may break your operating system!").color(Color32::RED).strong());
                            ui.add_space(4.0);
                        }
                        ui.label(RichText::new(format!(
                            "[DANGER] \"{}\" will be permanently deleted from disk!",
                            name
                        )).color(Color32::RED).strong());
                        ui.label("This action cannot be undone.");
                        ui.add_space(8.0);
                        ui.horizontal(|ui| {
                            if ui.button(RichText::new("Yes, Delete Permanently").color(Color32::RED)).clicked() {
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
                            }
                            if ui.button("Cancel").clicked() {
                                close_modal = true;
                            }
                        });
                    });
            }
            DeleteConfirmState::None => {}
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
