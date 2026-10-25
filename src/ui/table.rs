use std::collections::HashSet;
use std::path::PathBuf;
use eframe::egui::{self, Color32, Rect, RichText, ScrollArea, Sense, Stroke, Vec2};

use crate::config::{AppConfig, ColumnId, QuickSearchMode, SortRule};
use crate::duplicate::DuplicateCluster;
use crate::model::{format_size, format_system_time, FileCategory, FileEntry, SortDirection};

pub struct TableAction {
    pub navigate_to: Option<PathBuf>,
    pub open_path: Option<PathBuf>,
    pub open_in_explorer: Option<PathBuf>,
    pub toggle_favorite: Option<PathBuf>,
    pub request_delete_single: Option<FileEntry>,
    pub safety_warning: Option<String>,
    pub clear_live_filter: bool,
}

pub fn render_file_table(
    ui: &mut egui::Ui,
    config: &mut AppConfig,
    entries: &[FileEntry],
    duplicate_clusters: &mut [DuplicateCluster],
    selected_paths: &mut HashSet<PathBuf>,
    selected_path: &mut Option<PathBuf>,
    scroll_to_selected: bool,
    quick_search_buffer: &str,
    filter_bar_active: bool,
    base_size: f64,
    is_dark: bool,
) -> TableAction {
    let mut action = TableAction {
        navigate_to: None,
        open_path: None,
        open_in_explorer: None,
        toggle_favorite: None,
        request_delete_single: None,
        safety_warning: None,
        clear_live_filter: false,
    };

    if config.show_duplicates_only {
        render_duplicate_clusters_table(
            ui,
            duplicate_clusters,
            selected_paths,
            selected_path,
            scroll_to_selected,
            &mut action,
            is_dark,
        );
    } else {
        render_standard_file_table(
            ui,
            config,
            entries,
            selected_paths,
            selected_path,
            scroll_to_selected,
            quick_search_buffer,
            filter_bar_active,
            base_size,
            &mut action,
            is_dark,
        );
    }

    action
}

fn render_duplicate_clusters_table(
    ui: &mut egui::Ui,
    clusters: &mut [DuplicateCluster],
    selected_paths: &mut HashSet<PathBuf>,
    selected_path: &mut Option<PathBuf>,
    scroll_to_selected: bool,
    action: &mut TableAction,
    is_dark: bool,
) {
    if clusters.is_empty() {
        ui.vertical_centered(|ui| {
            ui.add_space(60.0);
            ui.label(RichText::new("No duplicates detected matching current criteria.").heading().color(Color32::GRAY));
            ui.label("Try adjusting matching criteria in the left Settings menu or toggling Flatten Subfolders.");
        });
        return;
    }

    ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
        for cluster in clusters.iter_mut() {
            let cluster_id = cluster.cluster_id.clone();
            let copies_count = cluster.occurrences.len();
            let wasted = cluster.wasted_bytes();
            let primary_name = cluster.primary_name.clone();
            let file_size = cluster.file_size;

            ui.push_id(&cluster_id, |ui| {
                // --- GROUP HEADER ROW ---
                let header_bg = if is_dark {
                    Color32::from_rgb(35, 42, 54)
                } else {
                    Color32::from_rgb(228, 235, 246)
                };

                egui::Frame::new()
                    .fill(header_bg)
                    .inner_margin(egui::Margin::symmetric(8, 5))
                    .corner_radius(4.0)
                    .show(ui, |ui| {
                        ui.horizontal(|ui| {
                            // Checkbox in front of the file name
                            let mut group_sel = cluster.is_group_selected;
                            if ui
                                .checkbox(&mut group_sel, "")
                                .on_hover_text("Select all copies for complete purge (User authorization required)")
                                .changed()
                            {
                                cluster.set_group_selected(group_sel);
                                for occ in &cluster.occurrences {
                                    if group_sel {
                                        selected_paths.insert(occ.path.clone());
                                    } else {
                                        selected_paths.remove(&occ.path);
                                    }
                                }
                            }

                            ui.label(RichText::new(&primary_name).strong().color(
                                if is_dark { Color32::from_rgb(120, 215, 255) } else { Color32::from_rgb(0, 95, 195) }
                            ));
                            ui.label(RichText::new(format!(
                                "({} copies, {} each - {} wasted)",
                                copies_count,
                                format_size(file_size),
                                format_size(wasted)
                            )).small().color(Color32::GRAY));

                            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                                if ui
                                    .small_button("Keep First Copy")
                                    .on_hover_text("Selects all copies for deletion EXCEPT the first one")
                                    .clicked()
                                {
                                    cluster.is_group_selected = false;
                                    for (i, occ) in cluster.occurrences.iter_mut().enumerate() {
                                        if i == 0 {
                                            occ.is_selected = false;
                                            selected_paths.remove(&occ.path);
                                        } else if !occ.is_system_protected {
                                            occ.is_selected = true;
                                            selected_paths.insert(occ.path.clone());
                                        }
                                    }
                                }
                            });
                        });
                    });

                let mut check_all_selected = false;
                let num_occurrences = cluster.occurrences.len();

                // --- OCCURRENCE ROWS ---
                for occ_idx in 0..num_occurrences {
                    let (path, is_hardlink, modified) = {
                        let occ = &cluster.occurrences[occ_idx];
                        (occ.path.clone(), occ.is_hardlink, occ.modified)
                    };

                    let is_cursor = selected_path.as_ref() == Some(&path);

                    let row_resp = ui.horizontal(|ui| {
                        ui.add_space(24.0);

                        let mut copy_sel = cluster.occurrences[occ_idx].is_selected;
                        if ui.checkbox(&mut copy_sel, "").changed() {
                            cluster.occurrences[occ_idx].is_selected = copy_sel;
                            if copy_sel {
                                selected_paths.insert(path.clone());
                                check_all_selected = true;
                            } else {
                                selected_paths.remove(&path);
                                cluster.is_group_selected = false;
                            }
                        }

                        let path_color = if copy_sel {
                            Color32::from_rgb(255, 120, 120)
                        } else if is_dark {
                            Color32::LIGHT_GRAY
                        } else {
                            Color32::BLACK
                        };

                        let label = ui.add(
                            egui::Label::new(
                                RichText::new(path.to_string_lossy().to_string()).color(path_color),
                            )
                            .sense(Sense::click()),
                        );

                        if label.clicked() {
                            *selected_path = Some(path.clone());
                        }

                        if label.double_clicked() {
                            action.open_in_explorer = Some(path.clone());
                        }

                        if is_hardlink {
                            ui.label(RichText::new("[Hardlink Copy]").small().color(Color32::from_rgb(180, 180, 80)));
                        }

                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            if let Some(m) = modified {
                                ui.label(RichText::new(format_system_time(m)).small().color(Color32::GRAY));
                            }
                        });
                    }).response;

                    if is_cursor && scroll_to_selected {
                        row_resp.scroll_to_me(Some(egui::Align::Center));
                    }
                }

                if check_all_selected && cluster.are_all_copies_selected() && !cluster.is_group_selected {
                    action.safety_warning = Some(format!(
                        "Safety notice: All copies of \"{}\" selected without file header confirmation. Check the file header box to confirm deleting all copies.",
                        primary_name
                    ));
                    if let Some(first) = cluster.occurrences.first() {
                        *selected_path = Some(first.path.clone());
                    }
                }

                ui.add_space(4.0);
            });
        }
    });
}

fn render_standard_file_table(
    ui: &mut egui::Ui,
    config: &mut AppConfig,
    entries: &[FileEntry],
    selected_paths: &mut HashSet<PathBuf>,
    selected_path: &mut Option<PathBuf>,
    scroll_to_selected: bool,
    quick_search_buffer: &str,
    filter_bar_active: bool,
    base_size: f64,
    action: &mut TableAction,
    is_dark: bool,
) {
    // --- LIVE FILTER BANNER (If Active) ---
    if config.quick_search_mode == QuickSearchMode::LiveFilter && (filter_bar_active || !quick_search_buffer.is_empty()) {
        ui.horizontal(|ui| {
            ui.label(RichText::new("Live Filter:").color(if is_dark { Color32::from_rgb(255, 200, 100) } else { Color32::from_rgb(200, 110, 0) }).strong().small());
            if quick_search_buffer.is_empty() {
                ui.label(RichText::new("Type letters on keyboard to filter view...").weak().small());
                if ui.button("Close (Esc)").clicked() {
                    action.clear_live_filter = true;
                }
            } else {
                ui.label(RichText::new(format!("\"{}\"", quick_search_buffer)).strong().small());
                ui.label(RichText::new(format!("({} matches)", entries.len())).weak().small());
                if ui.button("Clear (Esc)").clicked() {
                    action.clear_live_filter = true;
                }
            }
        });
        ui.add_space(2.0);
    }

    // --- TABLE HEADER ---
    render_table_header(ui, config, is_dark);

    let row_height = 22.0;
    let total_rows = entries.len();

    if entries.is_empty() {
        ui.vertical_centered(|ui| {
            ui.add_space(40.0);
            if config.quick_search_mode == QuickSearchMode::LiveFilter && !quick_search_buffer.is_empty() {
                ui.label(RichText::new(format!("No entries matching \"{}\"", quick_search_buffer)).weak());
            } else {
                ui.label(RichText::new("Folder is empty").weak());
            }
        });
        return;
    }

    // Virtualized Table Body
    ScrollArea::vertical()
        .auto_shrink([false, false])
        .show_rows(ui, row_height, total_rows, |ui, row_range| {
            for idx in row_range {
                let entry = &entries[idx];
                let is_checked = selected_paths.contains(&entry.path);
                let is_cursor = selected_path.as_ref() == Some(&entry.path);

                let row_bg = if is_checked {
                    if is_dark {
                        Color32::from_rgb(40, 65, 95)
                    } else {
                        Color32::from_rgb(205, 228, 252)
                    }
                } else if is_cursor {
                    if is_dark {
                        Color32::from_rgb(32, 45, 60)
                    } else {
                        Color32::from_rgb(225, 238, 252)
                    }
                } else {
                    Color32::TRANSPARENT
                };

                let (rect, row_resp) = ui.allocate_exact_size(
                    Vec2::new(ui.available_width(), row_height),
                    Sense::click(),
                );

                if row_bg != Color32::TRANSPARENT {
                    ui.painter().rect_filled(rect, 2.0, row_bg);
                } else if row_resp.hovered() {
                    ui.painter().rect_filled(
                        rect,
                        2.0,
                        if is_dark { Color32::from_gray(35) } else { Color32::from_gray(245) },
                    );
                }

                if is_cursor {
                    ui.painter().rect_stroke(
                        rect,
                        2.0,
                        Stroke::new(1.0, if is_dark { Color32::from_rgb(70, 150, 230) } else { Color32::from_rgb(100, 160, 230) }),
                        egui::StrokeKind::Inside,
                    );
                }

                // Click Handling
                if row_resp.clicked() {
                    let modifiers = ui.input(|i| i.modifiers);
                    if modifiers.shift {
                        if let Some(prev_path) = selected_path.as_ref() {
                            if let Some(prev_idx) = entries.iter().position(|e| &e.path == prev_path) {
                                let start = prev_idx.min(idx);
                                let end = prev_idx.max(idx);
                                for i in start..=end {
                                    selected_paths.insert(entries[i].path.clone());
                                }
                            }
                        } else {
                            selected_paths.insert(entry.path.clone());
                        }
                    } else if modifiers.command || modifiers.ctrl {
                        if is_checked {
                            selected_paths.remove(&entry.path);
                        } else {
                            selected_paths.insert(entry.path.clone());
                        }
                    } else {
                        selected_paths.clear();
                        selected_paths.insert(entry.path.clone());
                    }
                    *selected_path = Some(entry.path.clone());
                }

                if row_resp.secondary_clicked() {
                    *selected_path = Some(entry.path.clone());
                    if !is_checked {
                        selected_paths.clear();
                        selected_paths.insert(entry.path.clone());
                    }
                }

                if row_resp.double_clicked() {
                    if entry.is_dir {
                        action.navigate_to = Some(entry.path.clone());
                    } else {
                        action.open_path = Some(entry.path.clone());
                    }
                }

                // Scroll to keyboard selection if requested
                if is_cursor && scroll_to_selected {
                    row_resp.scroll_to_me(Some(egui::Align::Center));
                }

                // Context Menu
                row_resp.context_menu(|ui| {
                    if entry.is_dir {
                        if ui.button("Open Folder").clicked() {
                            action.navigate_to = Some(entry.path.clone());
                            ui.close();
                        }
                    } else if ui.button("Open File").clicked() {
                        action.open_path = Some(entry.path.clone());
                        ui.close();
                    }

                    if ui.button("Show in Explorer / File Manager").clicked() {
                        action.open_in_explorer = Some(entry.path.clone());
                        ui.close();
                    }

                    if ui.button("Copy Path").clicked() {
                        ui.ctx().copy_text(entry.path.to_string_lossy().to_string());
                        ui.close();
                    }

                    ui.separator();

                    if entry.is_dir {
                        let is_fav = config.saved_locations.iter().any(|l| l.path == entry.path);
                        let fav_label = if is_fav {
                            "Remove from Favorites"
                        } else {
                            "Add to Favorites (Ctrl+B)"
                        };
                        if ui.button(fav_label).clicked() {
                            action.toggle_favorite = Some(entry.path.clone());
                            ui.close();
                        }
                    }

                    ui.separator();

                    if ui.button(RichText::new("Delete...").color(Color32::from_rgb(255, 120, 120))).clicked() {
                        action.request_delete_single = Some(entry.clone());
                        ui.close();
                    }
                });

                // Render Columns in Row
                let mut x_offset = rect.min.x;
                for col in &config.column_order {
                    let col_width = col.fixed_width();
                    let cell_width = if col_width > 0.0 {
                        col_width
                    } else {
                        (rect.width() - 480.0).max(120.0)
                    };

                    let cell_rect = Rect::from_min_size(
                        egui::pos2(x_offset, rect.min.y),
                        Vec2::new(cell_width, row_height),
                    );

                    render_cell(ui, col, entry, cell_rect, base_size, is_cursor, is_dark);
                    x_offset += cell_width;
                }
            }
        });
}

fn render_table_header(ui: &mut egui::Ui, config: &mut AppConfig, _is_dark: bool) {
    ui.horizontal(|ui| {
        for col in &config.column_order {
            let width = col.fixed_width();
            let label = col.title();

            let btn = if width > 0.0 {
                ui.add_sized([width, 22.0], egui::Button::new(RichText::new(label).strong().small()))
            } else {
                ui.add_sized([(ui.available_width() - 480.0).max(120.0), 22.0], egui::Button::new(RichText::new(label).strong().small()))
            };

            if btn.clicked() {
                if let Some(rule) = config.sort_rules.first_mut() {
                    if rule.column == *col {
                        rule.direction = match rule.direction {
                            SortDirection::Ascending => SortDirection::Descending,
                            SortDirection::Descending => SortDirection::Ascending,
                        };
                    } else {
                        *rule = SortRule {
                            column: *col,
                            direction: SortDirection::Descending,
                        };
                    }
                }
                let _ = config.save();
            }
        }
    });
    ui.separator();
}

fn render_cell(
    ui: &egui::Ui,
    col: &ColumnId,
    entry: &FileEntry,
    rect: Rect,
    base_size: f64,
    is_cursor: bool,
    is_dark: bool,
) {
    let painter = ui.painter();
    let text_color = if is_cursor {
        if is_dark { Color32::from_rgb(120, 215, 255) } else { Color32::from_rgb(0, 95, 195) }
    } else if is_dark {
        Color32::LIGHT_GRAY
    } else {
        Color32::BLACK
    };

    match col {
        ColumnId::Type => {
            let badge = entry.category.label();
            let badge_color = match entry.category {
                FileCategory::Folder => Color32::from_rgb(100, 200, 255),
                FileCategory::Image => Color32::from_rgb(255, 140, 180),
                FileCategory::Video => Color32::from_rgb(255, 180, 60),
                FileCategory::Audio => Color32::from_rgb(180, 140, 255),
                FileCategory::Document => Color32::from_rgb(100, 220, 150),
                FileCategory::Archive => Color32::from_rgb(220, 180, 100),
                FileCategory::Other => Color32::GRAY,
            };
            painter.text(
                rect.center(),
                egui::Align2::CENTER_CENTER,
                badge,
                egui::FontId::monospace(11.0),
                badge_color,
            );
        }
        ColumnId::Name => {
            let label = if entry.is_dir {
                format!("{}/", entry.name)
            } else {
                entry.name.clone()
            };
            let text_rect = Rect::from_min_size(
                egui::pos2(rect.min.x + 6.0, rect.min.y),
                Vec2::new(rect.width() - 12.0, rect.height()),
            );
            painter.text(
                egui::pos2(text_rect.min.x, text_rect.center().y),
                egui::Align2::LEFT_CENTER,
                label,
                egui::FontId::proportional(12.5),
                text_color,
            );
        }
        ColumnId::Size => {
            let size_str = format_size(entry.size);
            painter.text(
                egui::pos2(rect.max.x - 8.0, rect.center().y),
                egui::Align2::RIGHT_CENTER,
                size_str,
                egui::FontId::monospace(12.0),
                text_color,
            );
        }
        ColumnId::AllocatedSize => {
            let alloc_str = format_size(entry.allocated_size);
            painter.text(
                egui::pos2(rect.max.x - 8.0, rect.center().y),
                egui::Align2::RIGHT_CENTER,
                alloc_str,
                egui::FontId::monospace(11.5),
                Color32::GRAY,
            );
        }
        ColumnId::Usage => {
            let bar_height = 10.0;
            let bar_rect = Rect::from_min_size(
                egui::pos2(rect.min.x + 6.0, rect.center().y - (bar_height / 2.0)),
                Vec2::new(rect.width() - 12.0, bar_height),
            );

            painter.rect_filled(
                bar_rect,
                2.0,
                if is_dark { Color32::from_gray(40) } else { Color32::from_gray(215) },
            );

            if base_size > 0.0 && entry.size > 0 {
                let ratio = (entry.size as f64 / base_size).clamp(0.0, 1.0) as f32;
                let filled_width = (bar_rect.width() * ratio).max(1.0);
                let filled_rect = Rect::from_min_size(bar_rect.min, Vec2::new(filled_width, bar_height));

                let bar_color = if ratio > 0.5 {
                    Color32::from_rgb(255, 140, 60)
                } else if ratio > 0.25 {
                    Color32::from_rgb(255, 200, 80)
                } else {
                    Color32::from_rgb(80, 180, 255)
                };

                painter.rect_filled(filled_rect, 2.0, bar_color);
            }
        }
        ColumnId::Items => {
            let item_str = if entry.is_dir {
                format!("{}", entry.item_count)
            } else {
                "-".to_string()
            };
            painter.text(
                egui::pos2(rect.max.x - 8.0, rect.center().y),
                egui::Align2::RIGHT_CENTER,
                item_str,
                egui::FontId::monospace(11.5),
                Color32::GRAY,
            );
        }
        ColumnId::Modified => {
            let mod_str = entry
                .modified
                .map(format_system_time)
                .unwrap_or_else(|| "-".to_string());
            painter.text(
                egui::pos2(rect.min.x + 6.0, rect.center().y),
                egui::Align2::LEFT_CENTER,
                mod_str,
                egui::FontId::monospace(11.0),
                Color32::GRAY,
            );
        }
    }
}
