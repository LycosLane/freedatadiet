use std::path::PathBuf;

use eframe::egui::{self, Color32, RichText, ScrollArea, Sense, Stroke};

use crate::model::{format_size, format_system_time};
use crate::platform;

use super::modals::{ActiveContextMenu, DeleteConfirmState, DeleteModalFocus};
use super::FreeDataDietApp;

#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum DuplicateRowItem {
    Header(usize),
    Occurrence(usize, usize),
}

impl FreeDataDietApp {
    pub(super) fn render_duplicates_view(&mut self, ui: &mut egui::Ui, is_dark: bool) {
        let total_clusters = self.duplicate_clusters.len();

        // --- TOP DUPLICATES METRICS & ACTION BAR ---
        ui.horizontal(|ui| {
            ui.label(
                RichText::new(format!(
                    "Duplicate Groups: {}  |  Wasted Space: {}  |  Selected: {} ({})",
                    total_clusters,
                    format_size(self.duplicate_total_wasted),
                    self.duplicate_eligible_count,
                    format_size(self.duplicate_eligible_bytes)
                ))
                .strong(),
            );

            if self.duplicate_blocked_count > 0 {
                ui.label(
                    RichText::new(format!(
                        "({} skipped: check group box to delete all copies)",
                        self.duplicate_blocked_count
                    ))
                    .small()
                    .color(Color32::from_rgb(255, 180, 60)),
                );
            }

            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if ui.button("Exit Duplicates View").clicked() {
                    self.config.show_duplicates_only = false;
                    self.save_config();
                    self.trigger_scan();
                    return;
                }
            });
        });

        ui.add_space(2.0);

        // Action controls toolbar row
        ui.horizontal(|ui| {
            // --- DUPLICATE SORT BUTTONS ---
            ui.label(RichText::new("Sort:").weak());
            let sort_columns = [
                (crate::config::DuplicateSortColumn::WastedSpace, "Wasted Space"),
                (crate::config::DuplicateSortColumn::FileSize, "File Size"),
                (crate::config::DuplicateSortColumn::Name, "Name"),
                (crate::config::DuplicateSortColumn::Count, "Copies"),
            ];

            for (col, label) in sort_columns {
                let is_active = self.config.duplicate_sort_column == col;
                let arrow = if is_active {
                    match self.config.duplicate_sort_direction {
                        crate::model::SortDirection::Ascending => " ^",
                        crate::model::SortDirection::Descending => " v",
                    }
                } else {
                    ""
                };

                let btn_text = format!("{}{}", label, arrow);
                let btn = if is_active {
                    egui::Button::new(RichText::new(btn_text).strong().color(Color32::WHITE))
                        .fill(Color32::from_rgb(225, 120, 20))
                } else {
                    egui::Button::new(RichText::new(btn_text).color(if is_dark { Color32::from_gray(200) } else { Color32::from_gray(50) }))
                };

                if ui.add(btn).clicked() {
                    if is_active {
                        self.config.duplicate_sort_direction = match self.config.duplicate_sort_direction {
                            crate::model::SortDirection::Ascending => crate::model::SortDirection::Descending,
                            crate::model::SortDirection::Descending => crate::model::SortDirection::Ascending,
                        };
                    } else {
                        self.config.duplicate_sort_column = col;
                        self.config.duplicate_sort_direction = match col {
                            crate::config::DuplicateSortColumn::Name => crate::model::SortDirection::Ascending,
                            _ => crate::model::SortDirection::Descending,
                        };
                    }
                    self.save_config();
                    self.sort_duplicate_clusters();
                }
            }

            ui.separator();

            if ui
                .button("Keep First Copy in All")
                .on_hover_text("Marks all redundant copies for deletion while keeping the first occurrence in each group")
                .clicked()
            {
                for cluster in &mut self.duplicate_clusters {
                    cluster.is_group_selected = false;
                    for (i, occ) in cluster.occurrences.iter_mut().enumerate() {
                        occ.is_selected = i > 0 && !occ.is_system_protected;
                    }
                }
                self.safety_warning = None;
                self.active_suggestion = None;
                self.recompute_duplicate_stats();
            }

            if self.config.enable_duplicate_suggestions {
                let auto_rules_btn = egui::Button::new(
                    RichText::new("Apply Category Rules").color(Color32::from_rgb(255, 175, 50)),
                );
                if ui
                    .add(auto_rules_btn)
                    .on_hover_text("Auto-mark copies based on category duplicate rules (Keep Oldest/Newest)")
                    .clicked()
                {
                    self.apply_category_duplicate_rules();
                }
            }

            if ui.button("Clear Selection").clicked() {
                for cluster in &mut self.duplicate_clusters {
                    cluster.set_group_selected(false);
                }
                self.safety_warning = None;
                self.active_suggestion = None;
                self.recompute_duplicate_stats();
            }

            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                let has_eligible = self.duplicate_eligible_count > 0;
                let perm_btn = egui::Button::new(
                    RichText::new("Delete Permanently").color(if has_eligible {
                        Color32::from_rgb(255, 110, 110)
                    } else {
                        Color32::GRAY
                    }),
                );
                if ui.add_enabled(has_eligible, perm_btn).clicked() {
                    let mut paths = Vec::new();
                    let mut total_size = 0u64;
                    for cluster in &self.duplicate_clusters {
                        if !cluster.are_all_copies_selected() || cluster.is_group_selected {
                            let p = cluster.deletable_paths();
                            total_size += p.len() as u64 * cluster.file_size;
                            paths.extend(p);
                        }
                    }
                    self.delete_modal = DeleteConfirmState::Batch {
                        paths,
                        total_size,
                        has_system: false,
                    };
                    self.modal_focus = DeleteModalFocus::Permanent;
                }

                let trash_btn = egui::Button::new(
                    RichText::new("Move to Trash").color(if has_eligible {
                        Color32::from_rgb(100, 200, 255)
                    } else {
                        Color32::GRAY
                    }),
                );
                if ui.add_enabled(has_eligible, trash_btn).clicked() {
                    let mut paths = Vec::new();
                    let mut total_size = 0u64;
                    for cluster in &self.duplicate_clusters {
                        if !cluster.are_all_copies_selected() || cluster.is_group_selected {
                            let p = cluster.deletable_paths();
                            total_size += p.len() as u64 * cluster.file_size;
                            paths.extend(p);
                        }
                    }
                    self.delete_modal = DeleteConfirmState::Batch {
                        paths,
                        total_size,
                        has_system: false,
                    };
                    self.modal_focus = DeleteModalFocus::Trash;
                }
            });
        });

        ui.add_space(4.0);

        // --- DUPLICATE ASSISTANT BANNER ---
        let mut apply_sugg = false;
        let mut dismiss_sugg = false;
        if let Some(sugg) = &self.active_suggestion {
            egui::Frame::new()
                .fill(if is_dark {
                    Color32::from_rgb(28, 48, 68)
                } else {
                    Color32::from_rgb(220, 238, 255)
                })
                .stroke(Stroke::new(1.0, Color32::from_rgb(50, 130, 210)))
                .corner_radius(4.0)
                .inner_margin(egui::Margin::symmetric(10, 6))
                .show(ui, |ui| {
                    ui.set_max_width(ui.available_width());
                    ui.horizontal_wrapped(|ui| {
                        ui.label(
                            RichText::new(&sugg.message)
                                .strong()
                                .color(if is_dark {
                                    Color32::from_rgb(170, 215, 255)
                                } else {
                                    Color32::from_rgb(10, 70, 130)
                                }),
                        );
                        ui.add_space(8.0);
                        let folder_display = if sugg.folder_name.len() > 30 {
                            format!("{}...", &sugg.folder_name[..27])
                        } else {
                            sugg.folder_name.clone()
                        };
                        let (btn_action_label, is_unsel) = match sugg.action {
                            crate::duplicate::SuggestionAction::Select => (
                                format!("Select Copies in \"{}\"", folder_display),
                                false,
                            ),
                            crate::duplicate::SuggestionAction::Unselect => (
                                format!("Unselect Copies in \"{}\"", folder_display),
                                true,
                            ),
                        };
                        let action_btn = egui::Button::new(RichText::new(btn_action_label).strong()).fill(
                            if is_unsel {
                                Color32::from_rgb(80, 85, 105)
                            } else {
                                Color32::from_rgb(225, 120, 20)
                            },
                        );
                        if ui.add(action_btn).clicked() {
                            apply_sugg = true;
                        }
                        if ui.button("Dismiss").clicked() {
                            dismiss_sugg = true;
                        }
                    });
                });
            ui.add_space(4.0);
        }
        if apply_sugg {
            if let Some(sugg) = &self.active_suggestion {
                let candidates = sugg.candidate_paths.clone();
                let sel_val = sugg.action == crate::duplicate::SuggestionAction::Select;
                for cluster in &mut self.duplicate_clusters {
                    for occ in &mut cluster.occurrences {
                        if candidates.contains(&occ.path) && !occ.is_system_protected {
                            occ.is_selected = sel_val;
                        }
                    }
                }
            }
            self.active_suggestion = None;
            self.recompute_duplicate_stats();
        }
        if dismiss_sugg {
            self.active_suggestion = None;
        }

        // --- SAFETY WARNING BANNER ---
        let mut jump_to_first: Option<PathBuf> = None;
        let mut clear_warn = false;
        if let Some((warn_msg, first_path)) = &self.safety_warning {
            egui::Frame::new()
                .fill(if is_dark {
                    Color32::from_rgb(65, 38, 20)
                } else {
                    Color32::from_rgb(255, 235, 210)
                })
                .stroke(Stroke::new(1.0, Color32::from_rgb(230, 140, 40)))
                .corner_radius(4.0)
                .inner_margin(egui::Margin::symmetric(10, 6))
                .show(ui, |ui| {
                    ui.set_max_width(ui.available_width());
                    ui.horizontal_wrapped(|ui| {
                        ui.label(RichText::new("[!]").color(Color32::from_rgb(255, 175, 50)).strong());
                        ui.label(
                            RichText::new(warn_msg).color(if is_dark {
                                Color32::from_rgb(255, 220, 175)
                            } else {
                                Color32::from_rgb(120, 60, 10)
                            }),
                        );
                        ui.add_space(8.0);
                        if let Some(fp) = first_path {
                            if ui.button("Jump to First Occurrence").clicked() {
                                jump_to_first = Some(fp.clone());
                            }
                        }
                        if ui.button("Dismiss").clicked() {
                            clear_warn = true;
                        }
                    });
                });
            ui.add_space(4.0);
        }
        if let Some(target) = jump_to_first {
            self.selected_path = Some(target);
            self.force_scroll_to_selected = true;
        }
        if clear_warn {
            self.safety_warning = None;
        }

        ui.separator();

        if self.is_scanning || self.is_finding_duplicates {
            ui.add_space(30.0);
            ui.vertical_centered(|ui| {
                ui.spinner();
                ui.add_space(6.0);
                let msg = if self.is_finding_duplicates {
                    if self.duplicate_progress_text.is_empty() {
                        "Analyzing and grouping duplicate files...".to_string()
                    } else {
                        self.duplicate_progress_text.clone()
                    }
                } else {
                    format!("Scanning subfolders for duplicates... {} items analyzed", self.scanned_counter)
                };
                ui.label(RichText::new(msg).italics().weak());
                if self.is_finding_duplicates {
                    ui.add_space(8.0);
                    if ui.button("Cancel Duplicate Search").clicked() {
                        self.cancel_duplicate_hashing.store(true, std::sync::atomic::Ordering::Relaxed);
                        self.is_finding_duplicates = false;
                        self.duplicate_receiver = None;
                    }
                }
            });
            return;
        }

        if self.duplicate_clusters.is_empty() {
            ui.add_space(40.0);
            ui.vertical_centered(|ui| {
                ui.label(
                    RichText::new("No duplicate files found matching current criteria.")
                        .size(15.0)
                        .weak(),
                );
                ui.add_space(4.0);
                ui.label(
                    RichText::new("Adjust duplicate matching criteria in the Settings & Menu panel (F12).")
                        .small()
                        .weak(),
                );
            });
            return;
        }

        // Build or reuse cached flat row mapping for virtualized rendering (skipping ignored clusters)
        if self.duplicate_flat_rows_dirty || self.cached_duplicate_flat_rows.is_empty() {
            self.cached_duplicate_flat_rows.clear();
            for c_idx in 0..self.duplicate_clusters.len() {
                if self.ignored_cluster_ids.contains(&self.duplicate_clusters[c_idx].cluster_id) {
                    continue;
                }
                self.cached_duplicate_flat_rows.push(DuplicateRowItem::Header(c_idx));
                for o_idx in 0..self.duplicate_clusters[c_idx].occurrences.len() {
                    self.cached_duplicate_flat_rows.push(DuplicateRowItem::Occurrence(c_idx, o_idx));
                }
            }
            self.duplicate_flat_rows_dirty = false;
        }
        let flat_rows = self.cached_duplicate_flat_rows.clone();
        let total_rows = flat_rows.len();

        let mut scroll_to_selected = self.force_scroll_to_selected;
        self.force_scroll_to_selected = false;
        let mut open_context_menu: Option<ActiveContextMenu> = None;
        let mut stats_dirty = false;
        let mut manual_toggled: Option<(PathBuf, bool)> = None;
        let mut ignore_cluster_id: Option<String> = None;
        let mut new_warning: Option<(String, Option<PathBuf>)> = None;

        let is_main_focus = self.panel_focus == super::ActivePanelFocus::MainView;

        if is_main_focus
            && !ui.ctx().egui_wants_keyboard_input()
            && self.delete_modal == DeleteConfirmState::None
            && self.active_context_menu.is_none()
            && !self.show_shortcuts_modal
        {
            let down = ui.input(|i| i.key_pressed(egui::Key::ArrowDown));
            let up = ui.input(|i| i.key_pressed(egui::Key::ArrowUp));
            let space = ui.input(|i| i.key_pressed(egui::Key::Space));
            let enter = ui.input(|i| i.key_pressed(egui::Key::Enter));
            let del = ui.input(|i| i.key_pressed(egui::Key::Delete));
            let shift = ui.input(|i| i.modifiers.shift);

            if down && total_rows > 0 {
                self.selected_duplicate_row_idx = (self.selected_duplicate_row_idx + 1).min(total_rows - 1);
                scroll_to_selected = true;
            }
            if up && total_rows > 0 {
                self.selected_duplicate_row_idx = self.selected_duplicate_row_idx.saturating_sub(1);
                scroll_to_selected = true;
            }

            if space && self.selected_duplicate_row_idx < total_rows {
                match flat_rows[self.selected_duplicate_row_idx] {
                    DuplicateRowItem::Header(c_idx) => {
                        let cluster = &mut self.duplicate_clusters[c_idx];
                        let new_val = !cluster.is_group_selected;
                        cluster.set_group_selected(new_val);
                        stats_dirty = true;
            self.duplicate_flat_rows_dirty = true;
                    }
                    DuplicateRowItem::Occurrence(c_idx, o_idx) => {
                        let cluster = &mut self.duplicate_clusters[c_idx];
                        let occ = &mut cluster.occurrences[o_idx];
                        if !occ.is_system_protected {
                            let new_val = !occ.is_selected;
                            let occ_path = occ.path.clone();
                            cluster.set_copy_selected(&occ_path, new_val);
                            stats_dirty = true;
                            manual_toggled = Some((occ_path.clone(), new_val));
                            self.duplicate_flat_rows_dirty = true;
                            if new_val && cluster.are_all_copies_selected() && !cluster.is_group_selected {
                                let first_p = cluster.occurrences.first().map(|o| o.path.clone());
                                new_warning = Some((
                                    format!(
                                        "All copies of \"{}\" are marked, but the file name checkbox is not checked. None of its copies will be deleted unless you check the file name box.",
                                        cluster.primary_name
                                    ),
                                    first_p,
                                ));
                            }
                        }
                    }
                }
            }

            if enter && self.selected_duplicate_row_idx < total_rows {
                if let DuplicateRowItem::Occurrence(c_idx, o_idx) = flat_rows[self.selected_duplicate_row_idx] {
                    let occ = &self.duplicate_clusters[c_idx].occurrences[o_idx];
                    let _ = platform::open_in_file_manager(&occ.path);
                }
            }

            if del && self.duplicate_eligible_count > 0 {
                let can_delete = if shift {
                    self.config.shortcuts.shift_del_permanent
                } else {
                    self.config.shortcuts.del_trash
                };
                if can_delete {
                    let mut paths = Vec::new();
                    let mut total_size = 0u64;
                    for cluster in &self.duplicate_clusters {
                        if !cluster.are_all_copies_selected() || cluster.is_group_selected {
                            let p = cluster.deletable_paths();
                            total_size += p.len() as u64 * cluster.file_size;
                            paths.extend(p);
                        }
                    }
                    self.delete_modal = DeleteConfirmState::Batch {
                        paths,
                        total_size,
                        has_system: false,
                    };
                    self.modal_focus = if shift {
                        DeleteModalFocus::Permanent
                    } else {
                        DeleteModalFocus::Trash
                    };
                }
            }
        }

        // --- VIRTUALIZED SCROLLABLE DUPLICATE ROWS ---
        let row_height = 25.0;
        let mut scroll = ScrollArea::vertical().auto_shrink([false, false]);
        if scroll_to_selected {
            let offset = (self.selected_duplicate_row_idx as f32 * row_height - 100.0).max(0.0);
            scroll = scroll.vertical_scroll_offset(offset);
        }

        scroll.show_rows(ui, row_height, total_rows, |ui, row_range| {
            for r_idx in row_range {
                let is_cursor_row = r_idx == self.selected_duplicate_row_idx;
                match flat_rows[r_idx] {
                    DuplicateRowItem::Header(c_idx) => {
                        let cluster = &mut self.duplicate_clusters[c_idx];
                        let primary_name = cluster.primary_name.clone();
                        let copies_count = cluster.occurrences.len();
                        let file_size = cluster.file_size;
                        let wasted = cluster.wasted_bytes();
                        let all_copies_checked_unconfirmed =
                            cluster.are_all_copies_selected() && !cluster.is_group_selected;

                        let header_bg = if all_copies_checked_unconfirmed {
                            if is_dark { Color32::from_rgb(65, 42, 22) } else { Color32::from_rgb(255, 232, 205) }
                        } else if is_cursor_row {
                            if is_dark { Color32::from_rgb(38, 55, 78) } else { Color32::from_rgb(215, 232, 252) }
                        } else if is_dark {
                            Color32::from_rgb(28, 34, 44)
                        } else {
                            Color32::from_rgb(232, 238, 246)
                        };

                        egui::Frame::new()
                            .fill(header_bg)
                            .corner_radius(3.0)
                            .inner_margin(egui::Margin::symmetric(8, 3))
                            .show(ui, |ui| {
                                ui.horizontal(|ui| {
                                    let mut group_sel = cluster.is_group_selected;
                                    if ui
                                        .checkbox(&mut group_sel, "")
                                        .on_hover_text("Select file name to authorize deleting all occurrences of this file")
                                        .changed()
                                    {
                                        cluster.set_group_selected(group_sel);
                                        stats_dirty = true;
            self.duplicate_flat_rows_dirty = true;
                                    }

                                    ui.label(
                                        RichText::new(&primary_name)
                                            .strong()
                                            .color(if is_dark { Color32::from_rgb(110, 205, 255) } else { Color32::from_rgb(0, 95, 190) }),
                                    );

                                    ui.label(
                                        RichText::new(format!(
                                            "({} copies, {} each - {} wasted)",
                                            copies_count,
                                            format_size(file_size),
                                            format_size(wasted)
                                        ))
                                        .small()
                                        .weak(),
                                    );

                                    if all_copies_checked_unconfirmed {
                                        ui.label(
                                            RichText::new("[!] Check file name box to delete all copies")
                                                .small()
                                                .strong()
                                                .color(Color32::from_rgb(255, 175, 50)),
                                        );
                                    }

                                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                                        if ui
                                            .small_button("Ignore for Now")
                                            .on_hover_text("Hide this duplicate group for the current session")
                                            .clicked()
                                        {
                                            ignore_cluster_id = Some(cluster.cluster_id.clone());
                                        }

                                        if ui
                                            .small_button("Keep First Copy")
                                            .on_hover_text("Selects all copies for deletion except the first occurrence")
                                            .clicked()
                                        {
                                            cluster.is_group_selected = false;
                                            for (i, occ) in cluster.occurrences.iter_mut().enumerate() {
                                                occ.is_selected = i > 0 && !occ.is_system_protected;
                                            }
                                            stats_dirty = true;
            self.duplicate_flat_rows_dirty = true;
                                        }
                                    });
                                });
                            });
                    }
                    DuplicateRowItem::Occurrence(c_idx, o_idx) => {
                        let cluster = &mut self.duplicate_clusters[c_idx];
                        let all_copies_checked_unconfirmed =
                            cluster.are_all_copies_selected() && !cluster.is_group_selected;
                        let primary_name = cluster.primary_name.clone();
                        let occ = &mut cluster.occurrences[o_idx];

                        let occ_path = occ.path.clone();
                        let occ_name = occ.name.clone();
                        let is_hardlink = occ.is_hardlink;
                        let is_sys = occ.is_system_protected;
                        let modified = occ.modified;
                        let mut copy_checked = occ.is_selected;

                        let row_bg = if is_cursor_row {
                            if is_dark { Color32::from_rgb(32, 54, 82) } else { Color32::from_rgb(205, 228, 252) }
                        } else {
                            Color32::TRANSPARENT
                        };

                        egui::Frame::new()
                            .fill(row_bg)
                            .corner_radius(2.0)
                            .inner_margin(egui::Margin::symmetric(6, 2))
                            .show(ui, |ui| {
                                ui.horizontal(|ui| {
                                    ui.add_space(18.0);

                                    let cb = ui.add_enabled(!is_sys, egui::Checkbox::new(&mut copy_checked, ""));
                                    if cb.changed() {
                                        cluster.set_copy_selected(&occ_path, copy_checked);
                                        manual_toggled = Some((occ_path.clone(), copy_checked));
                                        stats_dirty = true;
            self.duplicate_flat_rows_dirty = true;
                                        if copy_checked && cluster.are_all_copies_selected() && !cluster.is_group_selected {
                                            let first_p = cluster.occurrences.first().map(|o| o.path.clone());
                                            new_warning = Some((
                                                format!(
                                                    "All copies of \"{}\" are marked, but the file name checkbox is not checked. None of its copies will be deleted unless you check the file name box.",
                                                    primary_name
                                                ),
                                                first_p,
                                            ));
                                        }
                                    }

                                    if is_sys {
                                        ui.label(RichText::new("[SYS]").monospace().small().color(Color32::from_rgb(255, 175, 40)));
                                    } else if is_hardlink {
                                        ui.label(RichText::new("[LINK]").monospace().small().color(Color32::from_rgb(200, 160, 255)));
                                    }

                                    let path_color = if copy_checked && !all_copies_checked_unconfirmed {
                                        Color32::from_rgb(255, 130, 130)
                                    } else if is_cursor_row {
                                        if is_dark { Color32::from_rgb(120, 215, 255) } else { Color32::from_rgb(0, 90, 180) }
                                    } else if is_dark {
                                        Color32::LIGHT_GRAY
                                    } else {
                                        Color32::from_rgb(25, 30, 40)
                                    };

                                    let lbl = ui.add(
                                        egui::Label::new(
                                            RichText::new(occ_path.to_string_lossy().as_ref()).color(path_color),
                                        )
                                        .sense(Sense::click()),
                                    );

                                    if lbl.clicked() {
                                        self.selected_duplicate_row_idx = r_idx;
                                    }
                                    if lbl.double_clicked() {
                                        let _ = platform::open_in_file_manager(&occ_path);
                                    }
                                    if lbl.secondary_clicked() {
                                        self.selected_duplicate_row_idx = r_idx;
                                        let pointer_pos = ui.input(|i| i.pointer.interact_pos());
                                        open_context_menu = Some(ActiveContextMenu {
                                            path: occ_path.clone(),
                                            name: occ_name,
                                            is_dir: false,
                                            is_system_protected: is_sys,
                                            pos: pointer_pos,
                                            selected_idx: 0,
                                            just_opened: true,
                                        });
                                    }

                                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                                        if let Some(m) = modified {
                                            ui.label(
                                                RichText::new(format_system_time(m))
                                                    .monospace()
                                                    .small()
                                                    .weak(),
                                            );
                                        }
                                    });
                                });
                            });
                    }
                }
            }
        });

        if let Some(cid) = ignore_cluster_id {
            self.ignored_cluster_ids.insert(cid);
            self.status_toast = Some((
                "Duplicate group ignored for this session".to_string(),
                std::time::Instant::now(),
            ));
            stats_dirty = true;
            self.duplicate_flat_rows_dirty = true;
        }

        if let Some((p, is_sel)) = manual_toggled {
            self.on_manual_duplicate_toggled(&p, is_sel);
        }

        if stats_dirty {
            self.recompute_duplicate_stats();
        }

        if let Some((msg, fp)) = new_warning {
            self.safety_warning = Some((msg, fp));
        }

        if let Some(menu) = open_context_menu {
            self.active_context_menu = Some(menu);
        }
    }
}
