use std::time::Instant;
use eframe::egui;

use crate::config::QuickSearchMode;
use super::modals::DeleteConfirmState;
use super::{ActivePanelFocus, FreeDataDietApp};

impl FreeDataDietApp {
    pub(super) fn handle_global_shortcuts(&mut self, ui: &mut egui::Ui) {
        // Global F1 shortcut for Shortcuts Dialog
        let f1_pressed = ui.input(|i| i.key_pressed(egui::Key::F1));
        if f1_pressed {
            self.show_shortcuts_modal = !self.show_shortcuts_modal;
            if self.show_shortcuts_modal {
                self.shortcuts_just_opened = true;
                self.shortcuts_selected_idx = 0;
            }
        }

        // Global F12 / F4 / Ctrl+, shortcut for Settings & Locations Panel
        let f12_pressed = ui.input(|i| i.key_pressed(egui::Key::F12));
        let f4_pressed = ui.input(|i| i.key_pressed(egui::Key::F4));
        let ctrl_comma_pressed = ui.input(|i| (i.modifiers.ctrl || i.modifiers.command) && i.key_pressed(egui::Key::Comma));
        if !self.show_shortcuts_modal
            && ((self.config.shortcuts.f12_settings && f12_pressed)
                || f4_pressed
                || (self.config.shortcuts.ctrl_comma_settings && ctrl_comma_pressed))
        {
            self.config.show_sidebar = !self.config.show_sidebar;
            self.save_config();
        }

        // Global F5 shortcut for Refresh
        let f5_pressed = ui.input(|i| i.key_pressed(egui::Key::F5));
        if self.config.shortcuts.f5_refresh
            && f5_pressed
            && !self.show_shortcuts_modal
            && self.delete_modal == DeleteConfirmState::None
        {
            self.trigger_scan();
        }

        // Global F6 shortcut for Include Subfolders
        let f6_pressed = ui.input(|i| i.key_pressed(egui::Key::F6));
        if f6_pressed
            && !self.show_shortcuts_modal
            && self.delete_modal == DeleteConfirmState::None
        {
            self.config.flatten_subfolders = !self.config.flatten_subfolders;
            let msg = if self.config.flatten_subfolders {
                "Include Subfolders enabled"
            } else {
                "Include Subfolders disabled"
            };
            self.status_toast = Some((msg.to_string(), Instant::now()));
            self.save_config();
            self.trigger_scan();
        }

        // Global F3 shortcut for Live Filter Search Bar (activate / toggle)
        let f3_pressed = ui.input(|i| i.key_pressed(egui::Key::F3));
        if self.config.shortcuts.f3_search_filter
            && f3_pressed
            && !self.show_shortcuts_modal
            && self.delete_modal == DeleteConfirmState::None
        {
            if self.config.show_duplicates_only {
                self.focus_duplicate_filter = true;
            } else {
                self.config.quick_search_mode = QuickSearchMode::LiveFilter;
                self.filter_bar_active = !self.filter_bar_active;
                if !self.filter_bar_active {
                    self.quick_search_buffer.clear();
                }
                self.save_config();
            }
        }

        if !ui.ctx().egui_wants_keyboard_input()
            && self.delete_modal == DeleteConfirmState::None
            && self.active_context_menu.is_none()
            && !self.show_shortcuts_modal
        {
            let ctrl = ui.input(|i| i.modifiers.ctrl || i.modifiers.command);
            let shift = ui.input(|i| i.modifiers.shift);
            let h_pressed = ui.input(|i| i.key_pressed(egui::Key::H));
            let d_pressed = ui.input(|i| i.key_pressed(egui::Key::D));
            let b_pressed = ui.input(|i| i.key_pressed(egui::Key::B));
            let f_pressed = ui.input(|i| i.key_pressed(egui::Key::F));
            let f2_pressed = ui.input(|i| i.key_pressed(egui::Key::F2));

            if self.config.shortcuts.ctrl_f_filter && ctrl && !shift && f_pressed && self.config.show_duplicates_only {
                self.focus_duplicate_filter = true;
            } else if self.config.shortcuts.ctrl_f_filter && ctrl && !shift && f_pressed {
                self.config.quick_search_mode = QuickSearchMode::LiveFilter;
                self.filter_bar_active = true;
                self.quick_search_buffer.clear();
                self.panel_focus = ActivePanelFocus::MainView;
                self.save_config();
            } else if self.config.shortcuts.f2_search_mode && f2_pressed {
                self.config.quick_search_mode = match self.config.quick_search_mode {
                    QuickSearchMode::JumpToPrefix => QuickSearchMode::LiveFilter,
                    _ => QuickSearchMode::JumpToPrefix,
                };
                let mode_str = match self.config.quick_search_mode {
                    QuickSearchMode::LiveFilter => "Quick Search: Type to Filter",
                    QuickSearchMode::JumpToPrefix => "Quick Search: Type to Select",
                    QuickSearchMode::Disabled => "Quick Search: Disabled",
                };
                self.status_toast = Some((mode_str.to_string(), Instant::now()));
                self.quick_search_buffer.clear();
                self.save_config();
            } else if self.config.shortcuts.ctrl_shift_h_system && ctrl && shift && h_pressed {
                self.config.show_system_files = !self.config.show_system_files;
                self.save_config();
                self.sort_entries();
            } else if self.config.shortcuts.ctrl_h_hidden && ctrl && !shift && h_pressed {
                self.config.show_hidden_files = !self.config.show_hidden_files;
                self.save_config();
                self.sort_entries();
            } else if ctrl && shift && d_pressed {
                if let Some(sugg) = &self.active_suggestion {
                    let candidate_paths = sugg.candidate_paths.clone();
                    let sel_val = sugg.action == crate::duplicate_suggestion::SuggestionAction::Select;
                    for cluster in &mut self.duplicate_clusters {
                        for occ in &mut cluster.occurrences {
                            if candidate_paths.contains(&occ.path) && !occ.is_system_protected {
                                occ.is_selected = sel_val;
                            }
                        }
                    }
                    self.active_suggestion = None;
                    self.recompute_duplicate_stats();
                }
            } else if ctrl && !shift && d_pressed {
                self.config.show_duplicates_only = !self.config.show_duplicates_only;
                self.save_config();
                self.trigger_scan();
            } else if ctrl && !shift && b_pressed {
                let added = crate::locations::LocationManager::toggle_favorite(&mut self.config, &self.current_path);
                let msg = if added {
                    "Added current folder to Favorites"
                } else {
                    "Removed current folder from Favorites"
                };
                self.status_toast = Some((msg.to_string(), Instant::now()));
            }
        }
    }
}
