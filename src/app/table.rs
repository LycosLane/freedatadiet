use std::time::{Duration, Instant};

use eframe::egui::{self, Color32, ProgressBar, RichText, ScrollArea, Vec2};

use crate::config::{ColumnId, QuickSearchMode, UsageBasis};
use crate::model::{format_size, SortDirection};
use crate::platform;

use super::modals::{ActiveContextMenu, DeleteConfirmState, DeleteModalFocus};
use super::{get_type_badge, FreeDataDietApp};

impl FreeDataDietApp {
    pub(super) fn render_standard_table(&mut self, ui: &mut egui::Ui, is_dark: bool) {
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

                if is_dragging && header_resp.dragged()
                    && let Some(mouse_pos) = ui.input(|i| i.pointer.interact_pos()) {
                        if mouse_pos.x < header_rect.min.x - 8.0 && idx > 0 {
                            swap_columns = Some((idx, idx - 1));
                        } else if mouse_pos.x > header_rect.max.x + 8.0 && idx + 1 < self.config.column_order.len() {
                            swap_columns = Some((idx, idx + 1));
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
                } else if is_dark {
                    Color32::from_rgb(26, 33, 44)
                } else {
                    Color32::from_rgb(235, 240, 248)
                };

                let stroke_color = if is_dragging {
                    Color32::from_rgb(100, 180, 255)
                } else if is_active_sort {
                    Color32::from_rgb(60, 120, 180)
                } else if is_dark {
                    Color32::from_rgb(48, 58, 72)
                } else {
                    Color32::from_rgb(205, 215, 225)
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
                } else if is_dark {
                    Color32::from_rgb(200, 210, 220)
                } else {
                    Color32::from_rgb(50, 60, 75)
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
        let mut scroll_to_selected = self.force_scroll_to_selected;
        self.force_scroll_to_selected = false;
        let mut open_context_menu: Option<ActiveContextMenu> = None;
        let mut do_navigate_up = false;

        let entries_len = self.entries.len();
        let indices_invalid = self.visible_indices_dirty
            || self.visible_indices.len() > entries_len
            || self.visible_indices.iter().any(|&idx| idx >= entries_len);

        if indices_invalid {
            self.update_visible_indices();
        }
        let num_visible = self.visible_indices.len();

        let mut selected_visible_idx = self.selected_path.as_ref().and_then(|p| {
            self.visible_indices.iter().position(|&idx| &self.entries[idx].path == p)
        });

        if selected_visible_idx.is_none() && num_visible > 0 && self.config.quick_search_mode == QuickSearchMode::LiveFilter && !self.quick_search_buffer.is_empty() {
            self.selected_path = Some(self.entries[self.visible_indices[0]].path.clone());
            selected_visible_idx = Some(0);
        }

        // Keyboard navigation (active when not typing in text input, not in modal, not in context menu)
        if !ui.ctx().egui_wants_keyboard_input()
            && self.delete_modal == DeleteConfirmState::None
            && self.active_context_menu.is_none()
            && !self.show_shortcuts_modal
        {
            let (typed_text, key_backspace, key_escape) = ui.input(|i| {
                let mut text = String::new();
                let mut bs = false;
                let mut esc = false;
                for event in &i.events {
                    match event {
                        egui::Event::Text(t) => {
                            if !i.modifiers.ctrl && !i.modifiers.alt && !i.modifiers.command {
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
            let context_key = ui.input(|i| i.key_pressed(egui::Key::Menu) || i.key_pressed(egui::Key::F10));

            // Quick Search handling
            match self.config.quick_search_mode {
                QuickSearchMode::LiveFilter => {
                    if key_escape {
                        self.quick_search_buffer.clear();
                        self.filter_bar_active = false;
                        self.visible_indices_dirty = true;
                    } else if key_backspace {
                        if self.filter_bar_active {
                            if !self.quick_search_buffer.is_empty() {
                                self.quick_search_buffer.pop();
                                self.visible_indices_dirty = true;
                            }
                        } else {
                            do_navigate_up = true;
                        }
                    } else if !typed_text.is_empty() {
                        self.quick_search_buffer.push_str(&typed_text);
                        self.filter_bar_active = true;
                        self.visible_indices_dirty = true;
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
                                    Some(curr) => (curr + 1) % num_visible.max(1),
                                    None => 0,
                                };
                                let found = self.visible_indices.iter().skip(start_idx)
                                    .chain(self.visible_indices.iter().take(start_idx))
                                    .find(|&&idx| self.entries[idx].name.to_lowercase().starts_with(&ch_lower));
                                if let Some(&idx) = found {
                                    self.selected_path = Some(self.entries[idx].path.clone());
                                    scroll_to_selected = true;
                                }
                            } else {
                                self.quick_search_buffer.push(ch);
                                let query = self.quick_search_buffer.to_lowercase();
                                if let Some(&idx) = self.visible_indices.iter().find(|&&idx| self.entries[idx].name.to_lowercase().starts_with(&query)) {
                                    self.selected_path = Some(self.entries[idx].path.clone());
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

            let is_main_focus = self.panel_focus == super::ActivePanelFocus::MainView;

            if is_main_focus && down && num_visible > 0 {
                let next = match selected_visible_idx {
                    Some(idx) => (idx + 1).min(num_visible.saturating_sub(1)),
                    None => 0,
                };
                self.selected_path = Some(self.entries[self.visible_indices[next]].path.clone());
                scroll_to_selected = true;
            }

            if is_main_focus && up && num_visible > 0 {
                let prev = match selected_visible_idx {
                    Some(idx) => idx.saturating_sub(1),
                    None => 0,
                };
                self.selected_path = Some(self.entries[self.visible_indices[prev]].path.clone());
                scroll_to_selected = true;
            }

            if is_main_focus && home && num_visible > 0 {
                self.selected_path = Some(self.entries[self.visible_indices[0]].path.clone());
                scroll_to_selected = true;
            }

            if is_main_focus && end && num_visible > 0 {
                let last_idx = self.visible_indices[num_visible - 1];
                self.selected_path = Some(self.entries[last_idx].path.clone());
                scroll_to_selected = true;
            }

            if is_main_focus && page_up && num_visible > 0 {
                let curr = selected_visible_idx.unwrap_or(0);
                let target = curr.saturating_sub(15);
                self.selected_path = Some(self.entries[self.visible_indices[target]].path.clone());
                scroll_to_selected = true;
            }

            if is_main_focus && page_down && num_visible > 0 {
                let curr = selected_visible_idx.unwrap_or(0);
                let target = (curr + 15).min(num_visible.saturating_sub(1));
                self.selected_path = Some(self.entries[self.visible_indices[target]].path.clone());
                scroll_to_selected = true;
            }

            if is_main_focus && enter
                && let Some(vidx) = selected_visible_idx {
                    let entry = &self.entries[self.visible_indices[vidx]];
                    if entry.is_dir {
                        navigate_target = Some(entry.path.clone());
                    } else {
                        let _ = platform::open_file(&entry.path);
                    }
                }

            if is_main_focus && right
                && let Some(vidx) = selected_visible_idx {
                    let entry = &self.entries[self.visible_indices[vidx]];
                    if entry.is_dir {
                        navigate_target = Some(entry.path.clone());
                    }
                }

            if is_main_focus && del
                && let Some(vidx) = selected_visible_idx {
                    let can_delete = if shift {
                        self.config.shortcuts.shift_del_permanent
                    } else {
                        self.config.shortcuts.del_trash
                    };

                    if can_delete {
                        let entry = &self.entries[self.visible_indices[vidx]];
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

            if let (true, true, Some(sel)) = (is_main_focus, context_key, selected_visible_idx) {
                let entry = &self.entries[self.visible_indices[sel]];
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
        }

        let mut clear_search_buffer = false;
        let matches_count = num_visible;

        if self.config.quick_search_mode == QuickSearchMode::LiveFilter && (self.filter_bar_active || !self.quick_search_buffer.is_empty()) {
            ui.add_space(2.0);
            ui.horizontal(|ui| {
                ui.label(RichText::new("Filter:").color(if is_dark { Color32::from_rgb(255, 200, 100) } else { Color32::from_rgb(200, 110, 0) }).strong().small());
                if self.quick_search_buffer.is_empty() {
                    ui.label(RichText::new("Type letters to filter view...").weak().small());
                    if ui.button("Close (Esc)").clicked() {
                        self.filter_bar_active = false;
                    }
                } else {
                    ui.label(RichText::new(format!("\"{}\"", self.quick_search_buffer)).strong().small());
                    ui.label(RichText::new(format!("({} matches)", matches_count)).weak().small());
                    if ui.button("Clear (Esc)").clicked() {
                        clear_search_buffer = true;
                    }
                }
            });
        }

        if clear_search_buffer {
            self.quick_search_buffer.clear();
            self.filter_bar_active = false;
        }

        ui.add_space(2.0);

        let row_height = 20.0;
        let mut scroll_area = ScrollArea::vertical().auto_shrink([false, false]);
        if scroll_to_selected
            && let Some(target_idx) = selected_visible_idx {
                let approx_viewport_height = ui.available_height();
                let target_y = (target_idx as f32 * row_height - approx_viewport_height / 2.0).max(0.0);
                scroll_area = scroll_area.vertical_scroll_offset(target_y);
            }

        let flatten_active = self.config.flatten_subfolders;
        let current_root = self.current_path.clone();

        scroll_area.show_rows(ui, row_height, num_visible, |ui, row_range| {
            if num_visible == 0 {
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
            for vis_idx in row_range {
                let orig_idx = self.visible_indices[vis_idx];
                let entry = &self.entries[orig_idx];
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
                                    } else if is_dark {
                                        Color32::WHITE
                                    } else {
                                        Color32::from_rgb(20, 25, 35)
                                    };
                                    let display_name = if flatten_active {
                                        entry
                                            .path
                                            .strip_prefix(&current_root)
                                            .map(|rel| rel.to_string_lossy().to_string())
                                            .unwrap_or_else(|_| entry.name.clone())
                                    } else {
                                        entry.name.clone()
                                    };
                                    ui.add_space(4.0);
                                    ui.add(
                                        egui::Label::new(
                                            RichText::new(display_name).color(name_color).strong(),
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
                                    let ratio = if base_size > 0.0 && !base_size.is_nan() {
                                        (entry.size as f64 / base_size).clamp(0.0, 1.0) as f32
                                    } else {
                                        0.0
                                    };
                                    let percent = if base_size > 0.0 && !base_size.is_nan() {
                                        entry.size as f64 / base_size * 100.0
                                    } else {
                                        0.0
                                    };
                                    let text = if percent >= 0.1 {
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
    }
}
