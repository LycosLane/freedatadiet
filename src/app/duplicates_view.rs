use std::path::PathBuf;

use eframe::egui::{self, Color32, RichText, ScrollArea, Sense};

use crate::model::{format_size, format_system_time};
use crate::platform;

use super::modals::{ActiveContextMenu, DeleteConfirmState, DeleteModalFocus};
use super::FreeDataDietApp;

impl FreeDataDietApp {
    pub(super) fn render_duplicates_view(&mut self, ui: &mut egui::Ui, is_dark: bool) {
        let total_clusters = self.duplicate_clusters.len();
        let total_wasted: u64 = self.duplicate_clusters.iter().map(|c| c.wasted_bytes()).sum();

        let mut eligible_paths: Vec<PathBuf> = Vec::new();
        let mut eligible_bytes: u64 = 0;
        let mut blocked_clusters_count = 0usize;

        for cluster in &self.duplicate_clusters {
            if cluster.are_all_copies_selected() && !cluster.is_group_selected {
                blocked_clusters_count += 1;
            } else {
                let paths = cluster.deletable_paths();
                eligible_bytes += paths.len() as u64 * cluster.file_size;
                eligible_paths.extend(paths);
            }
        }

        // --- TOP DUPLICATES ACTION BAR ---
        ui.horizontal(|ui| {
            ui.label(
                RichText::new(format!(
                    "Duplicate Groups: {}  |  Wasted Space: {}  |  Selected to Delete: {} ({})",
                    total_clusters,
                    format_size(total_wasted),
                    eligible_paths.len(),
                    format_size(eligible_bytes)
                ))
                .strong(),
            );

            if blocked_clusters_count > 0 {
                ui.label(
                    RichText::new(format!(
                        "({} group(s) skipped: check file name box to delete all copies)",
                        blocked_clusters_count
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

                let has_eligible = !eligible_paths.is_empty();
                let perm_btn = egui::Button::new(
                    RichText::new("Delete Permanently").color(if has_eligible {
                        Color32::from_rgb(255, 110, 110)
                    } else {
                        Color32::GRAY
                    }),
                );
                if ui.add_enabled(has_eligible, perm_btn).clicked() {
                    self.delete_modal = DeleteConfirmState::Batch {
                        paths: eligible_paths.clone(),
                        total_size: eligible_bytes,
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
                    self.delete_modal = DeleteConfirmState::Batch {
                        paths: eligible_paths.clone(),
                        total_size: eligible_bytes,
                        has_system: false,
                    };
                    self.modal_focus = DeleteModalFocus::Trash;
                }

                if ui.button("Clear Selection").clicked() {
                    for cluster in &mut self.duplicate_clusters {
                        cluster.set_group_selected(false);
                    }
                    self.safety_warning = None;
                    self.update_duplicate_assistant();
                }

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
                    self.update_duplicate_assistant();
                }
            });
        });

        ui.add_space(2.0);

        // --- DUPLICATE ASSISTANT BANNER ---
        let mut apply_sugg = false;
        let mut dismiss_sugg = false;
        if let Some(sugg) = &self.active_suggestion {
            egui::Frame::new()
                .fill(if is_dark { Color32::from_rgb(28, 48, 68) } else { Color32::from_rgb(220, 238, 255) })
                .corner_radius(3.0)
                .inner_margin(egui::Margin::symmetric(8, 4))
                .show(ui, |ui| {
                    ui.horizontal(|ui| {
                        ui.label(
                            RichText::new(format!(
                                "Duplicate Assistant: Found {} sibling duplicate copies in folder \"{}\" ({} reclaimable).",
                                sugg.candidate_paths.len(),
                                sugg.folder_name,
                                format_size(sugg.reclaimable_bytes)
                            ))
                            .strong()
                            .small(),
                        );
                        if ui.small_button("Select Copies in Folder (Ctrl+Shift+D)").clicked() {
                            apply_sugg = true;
                        }
                        if ui.small_button("Dismiss").clicked() {
                            dismiss_sugg = true;
                        }
                    });
                });
            ui.add_space(2.0);
        }
        if apply_sugg {
            if let Some(sugg) = &self.active_suggestion {
                let candidates = sugg.candidate_paths.clone();
                for cluster in &mut self.duplicate_clusters {
                    for occ in &mut cluster.occurrences {
                        if candidates.contains(&occ.path) && !occ.is_system_protected {
                            occ.is_selected = true;
                        }
                    }
                }
            }
            self.update_duplicate_assistant();
        }
        if dismiss_sugg {
            self.active_suggestion = None;
        }

        // --- SAFETY WARNING BANNER ---
        let mut jump_to_first: Option<PathBuf> = None;
        let mut clear_warn = false;
        if let Some((warn_msg, first_path)) = &self.safety_warning {
            egui::Frame::new()
                .fill(if is_dark { Color32::from_rgb(65, 38, 20) } else { Color32::from_rgb(255, 235, 210) })
                .stroke(egui::Stroke::new(1.0, Color32::from_rgb(230, 140, 40)))
                .corner_radius(3.0)
                .inner_margin(egui::Margin::symmetric(8, 4))
                .show(ui, |ui| {
                    ui.horizontal(|ui| {
                        ui.label(
                            RichText::new(warn_msg)
                                .color(if is_dark { Color32::from_rgb(255, 200, 110) } else { Color32::from_rgb(160, 70, 0) })
                                .strong()
                                .small(),
                        );
                        if let Some(fp) = first_path {
                            if ui.small_button("Jump to First Copy").clicked() {
                                jump_to_first = Some(fp.clone());
                            }
                        }
                        if ui.small_button("Dismiss").clicked() {
                            clear_warn = true;
                        }
                    });
                });
            ui.add_space(2.0);
        }
        if let Some(fp) = jump_to_first {
            self.selected_path = Some(fp);
            self.force_scroll_to_selected = true;
        }
        if clear_warn {
            self.safety_warning = None;
        }

        ui.separator();

        if self.is_scanning {
            ui.add_space(30.0);
            ui.vertical_centered(|ui| {
                ui.spinner();
                ui.add_space(6.0);
                ui.label(
                    RichText::new(format!(
                        "Scanning subfolders for duplicates... {} items analyzed",
                        self.scanned_counter
                    ))
                    .italics()
                    .weak(),
                );
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

        // Flatten all occurrences for keyboard navigation
        let all_occ_paths: Vec<PathBuf> = self
            .duplicate_clusters
            .iter()
            .flat_map(|c| c.occurrences.iter().map(|o| o.path.clone()))
            .collect();

        let mut scroll_to_selected = self.force_scroll_to_selected;
        self.force_scroll_to_selected = false;
        let mut open_context_menu: Option<ActiveContextMenu> = None;

        if !ui.ctx().egui_wants_keyboard_input()
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
            let curr_idx = self
                .selected_path
                .as_ref()
                .and_then(|p| all_occ_paths.iter().position(|op| op == p));

            if down && !all_occ_paths.is_empty() {
                let next = match curr_idx {
                    Some(idx) => (idx + 1).min(all_occ_paths.len().saturating_sub(1)),
                    None => 0,
                };
                self.selected_path = Some(all_occ_paths[next].clone());
                scroll_to_selected = true;
            }
            if up && !all_occ_paths.is_empty() {
                let prev = match curr_idx {
                    Some(idx) => idx.saturating_sub(1),
                    None => 0,
                };
                self.selected_path = Some(all_occ_paths[prev].clone());
                scroll_to_selected = true;
            }
            if space {
                if let Some(sel_p) = self.selected_path.clone() {
                    let mut triggered_warn = None;
                    for cluster in &mut self.duplicate_clusters {
                        if let Some(occ) = cluster.occurrences.iter().find(|o| o.path == sel_p) {
                            let new_sel = !occ.is_selected;
                            cluster.set_copy_selected(&sel_p, new_sel);
                            if new_sel && cluster.are_all_copies_selected() && !cluster.is_group_selected {
                                let first_p = cluster.occurrences.first().map(|o| o.path.clone());
                                triggered_warn = Some((
                                    format!(
                                        "All copies of \"{}\" are marked, but the file name checkbox is not checked. None of its copies will be deleted unless you check the file name box.",
                                        cluster.primary_name
                                    ),
                                    first_p,
                                ));
                            }
                            break;
                        }
                    }
                    if let Some((msg, fp)) = triggered_warn {
                        if let Some(first_p) = &fp {
                            self.selected_path = Some(first_p.clone());
                            scroll_to_selected = true;
                        }
                        self.safety_warning = Some((msg, fp));
                    }
                    self.update_duplicate_assistant();
                }
            }
            if enter {
                if let Some(sel_p) = &self.selected_path {
                    let _ = platform::open_in_file_manager(sel_p);
                }
            }
            if del && !eligible_paths.is_empty() {
                let can_delete = if shift {
                    self.config.shortcuts.shift_del_permanent
                } else {
                    self.config.shortcuts.del_trash
                };
                if can_delete {
                    self.delete_modal = DeleteConfirmState::Batch {
                        paths: eligible_paths.clone(),
                        total_size: eligible_bytes,
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

        // --- SCROLLABLE DUPLICATE CLUSTERS ---
        let mut assistant_dirty = false;
        let mut new_warning: Option<(String, Option<PathBuf>)> = None;

        ScrollArea::vertical()
            .auto_shrink([false, false])
            .show(ui, |ui| {
                for cluster in &mut self.duplicate_clusters {
                    let cluster_id = cluster.cluster_id.clone();
                    let primary_name = cluster.primary_name.clone();
                    let copies_count = cluster.occurrences.len();
                    let file_size = cluster.file_size;
                    let wasted = cluster.wasted_bytes();
                    let all_copies_checked_unconfirmed =
                        cluster.are_all_copies_selected() && !cluster.is_group_selected;

                    ui.push_id(&cluster_id, |ui| {
                        let header_bg = if all_copies_checked_unconfirmed {
                            if is_dark { Color32::from_rgb(65, 42, 22) } else { Color32::from_rgb(255, 232, 205) }
                        } else if is_dark {
                            Color32::from_rgb(30, 38, 50)
                        } else {
                            Color32::from_rgb(230, 236, 246)
                        };

                        egui::Frame::new()
                            .fill(header_bg)
                            .corner_radius(3.0)
                            .inner_margin(egui::Margin::symmetric(8, 4))
                            .show(ui, |ui| {
                                ui.horizontal(|ui| {
                                    // Master checkbox in front of the file name (never checked automatically)
                                    let mut group_sel = cluster.is_group_selected;
                                    if ui
                                        .checkbox(&mut group_sel, "")
                                        .on_hover_text("Select file name to automatically select all copies and authorize deleting all occurrences of this file")
                                        .changed()
                                    {
                                        cluster.set_group_selected(group_sel);
                                        assistant_dirty = true;
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
                                            .small_button("Keep First Copy")
                                            .on_hover_text("Selects all copies for deletion except the first occurrence")
                                            .clicked()
                                        {
                                            cluster.is_group_selected = false;
                                            for (i, occ) in cluster.occurrences.iter_mut().enumerate() {
                                                occ.is_selected = i > 0 && !occ.is_system_protected;
                                            }
                                            assistant_dirty = true;
                                        }
                                    });
                                });
                            });

                        let num_occ = cluster.occurrences.len();
                        for occ_idx in 0..num_occ {
                            let (occ_path, occ_name, is_hardlink, is_sys, modified, is_sel) = {
                                let o = &cluster.occurrences[occ_idx];
                                (
                                    o.path.clone(),
                                    o.name.clone(),
                                    o.is_hardlink,
                                    o.is_system_protected,
                                    o.modified,
                                    o.is_selected,
                                )
                            };

                            let is_cursor = self.selected_path.as_ref() == Some(&occ_path);
                            let row_bg = if is_cursor {
                                if is_dark { Color32::from_rgb(32, 54, 82) } else { Color32::from_rgb(205, 228, 252) }
                            } else {
                                Color32::TRANSPARENT
                            };

                            let row_resp = egui::Frame::new()
                                .fill(row_bg)
                                .corner_radius(2.0)
                                .inner_margin(egui::Margin::symmetric(6, 2))
                                .show(ui, |ui| {
                                    ui.horizontal(|ui| {
                                        ui.add_space(18.0);

                                        let mut copy_checked = is_sel;
                                        let cb = ui.add_enabled(!is_sys, egui::Checkbox::new(&mut copy_checked, ""));
                                        if cb.changed() {
                                            cluster.set_copy_selected(&occ_path, copy_checked);
                                            assistant_dirty = true;
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
                                        } else if is_cursor {
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
                                            self.selected_path = Some(occ_path.clone());
                                        }
                                        if lbl.double_clicked() {
                                            let _ = platform::open_in_file_manager(&occ_path);
                                        }
                                        if lbl.secondary_clicked() {
                                            self.selected_path = Some(occ_path.clone());
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
                                })
                                .response;

                            if is_cursor && scroll_to_selected {
                                row_resp.scroll_to_me(Some(egui::Align::Center));
                            }
                        }

                        ui.add_space(4.0);
                    });
                }
            });

        if let Some((msg, fp)) = new_warning {
            if let Some(first_p) = &fp {
                self.selected_path = Some(first_p.clone());
                self.force_scroll_to_selected = true;
            }
            self.safety_warning = Some((msg, fp));
        }
        if assistant_dirty {
            self.update_duplicate_assistant();
        }
        if let Some(menu) = open_context_menu {
            self.active_context_menu = Some(menu);
        }
    }
}
