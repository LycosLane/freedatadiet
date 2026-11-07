use std::time::{Duration, Instant};

use eframe::egui;

use crate::config::QuickSearchMode;
use crate::platform;

use super::context_menu::ActiveContextMenu;
use super::modals::{DeleteConfirmState, DeleteModalFocus};
use super::FreeDataDietApp;

impl FreeDataDietApp {
    /// Handles keyboard navigation, quick search (live filter or prefix jump),
    /// row selection movements, and key shortcuts for the main files table.
    #[allow(clippy::too_many_arguments)]
    pub(super) fn handle_table_keyboard_navigation(
        &mut self,
        ui: &mut egui::Ui,
        num_visible: usize,
        selected_visible_idx: Option<usize>,
        scroll_to_selected: &mut bool,
        open_context_menu: &mut Option<ActiveContextMenu>,
        navigate_target: &mut Option<std::path::PathBuf>,
        do_navigate_up: &mut bool,
    ) {
        if ui.ctx().egui_wants_keyboard_input()
            || self.delete_modal != DeleteConfirmState::None
            || self.active_context_menu.is_some()
            || self.show_shortcuts_modal
        {
            return;
        }

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
                        *do_navigate_up = true;
                    }
                } else if !typed_text.is_empty() {
                    if self.quick_search_buffer.len() < 128 {
                        self.quick_search_buffer.push_str(&typed_text);
                    }
                    self.filter_bar_active = true;
                    self.visible_indices_dirty = true;
                    *scroll_to_selected = true;
                }

                if arrow_left {
                    *do_navigate_up = true;
                }
            }
            QuickSearchMode::JumpToPrefix => {
                if key_escape {
                    self.quick_search_buffer.clear();
                    self.quick_search_time = None;
                } else if key_backspace || arrow_left {
                    *do_navigate_up = true;
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
                                *scroll_to_selected = true;
                            }
                        } else {
                            if self.quick_search_buffer.len() < 128 {
                                self.quick_search_buffer.push(ch);
                            }
                            let query = self.quick_search_buffer.to_lowercase();
                            if let Some(&idx) = self.visible_indices.iter().find(|&&idx| self.entries[idx].name.to_lowercase().starts_with(&query)) {
                                self.selected_path = Some(self.entries[idx].path.clone());
                                *scroll_to_selected = true;
                            }
                        }
                    }
                }
            }
            QuickSearchMode::Disabled => {
                if key_backspace || arrow_left {
                    *do_navigate_up = true;
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
            *scroll_to_selected = true;
        }

        if is_main_focus && up && num_visible > 0 {
            let prev = match selected_visible_idx {
                Some(idx) => idx.saturating_sub(1),
                None => 0,
            };
            self.selected_path = Some(self.entries[self.visible_indices[prev]].path.clone());
            *scroll_to_selected = true;
        }

        if is_main_focus && home && num_visible > 0 {
            self.selected_path = Some(self.entries[self.visible_indices[0]].path.clone());
            *scroll_to_selected = true;
        }

        if is_main_focus && end && num_visible > 0 {
            let last_idx = self.visible_indices[num_visible - 1];
            self.selected_path = Some(self.entries[last_idx].path.clone());
            *scroll_to_selected = true;
        }

        if is_main_focus && page_up && num_visible > 0 {
            let curr = selected_visible_idx.unwrap_or(0);
            let target = curr.saturating_sub(15);
            self.selected_path = Some(self.entries[self.visible_indices[target]].path.clone());
            *scroll_to_selected = true;
        }

        if is_main_focus && page_down && num_visible > 0 {
            let curr = selected_visible_idx.unwrap_or(0);
            let target = (curr + 15).min(num_visible.saturating_sub(1));
            self.selected_path = Some(self.entries[self.visible_indices[target]].path.clone());
            *scroll_to_selected = true;
        }

        if is_main_focus && enter
            && let Some(vidx) = selected_visible_idx {
                let entry = &self.entries[self.visible_indices[vidx]];
                if entry.is_dir {
                    *navigate_target = Some(entry.path.clone());
                } else {
                    let _ = platform::open_file(&entry.path);
                }
            }

        if is_main_focus && right
            && let Some(vidx) = selected_visible_idx {
                let entry = &self.entries[self.visible_indices[vidx]];
                if entry.is_dir {
                    *navigate_target = Some(entry.path.clone());
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
            *open_context_menu = Some(ActiveContextMenu {
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
}
