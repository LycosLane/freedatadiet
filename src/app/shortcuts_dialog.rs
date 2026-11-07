use eframe::egui::{self, Color32, RichText};

use super::FreeDataDietApp;

impl FreeDataDietApp {
    pub(super) fn render_shortcuts_modal(&mut self, ui: &mut egui::Ui, is_dark: bool) {
        let esc_pressed = ui.input(|i| i.key_pressed(egui::Key::Escape));
        let mut close_shortcuts = esc_pressed;

        let nav_down = ui.input(|i| i.key_pressed(egui::Key::ArrowDown));
        let nav_up = ui.input(|i| i.key_pressed(egui::Key::ArrowUp));
        let tab_forward = ui.input(|i| !i.modifiers.shift && i.key_pressed(egui::Key::Tab));
        let tab_backward = ui.input(|i| i.modifiers.shift && i.key_pressed(egui::Key::Tab));
        let activate = ui.input(|i| i.key_pressed(egui::Key::Space) || i.key_pressed(egui::Key::Enter));

        const TOTAL_SHORTCUT_ITEMS: usize = 12; // 11 toggles (0..10) + Close button (11)

        if tab_forward || nav_down {
            self.shortcuts_selected_idx = (self.shortcuts_selected_idx + 1) % TOTAL_SHORTCUT_ITEMS;
        } else if tab_backward || nav_up {
            self.shortcuts_selected_idx = (self.shortcuts_selected_idx + TOTAL_SHORTCUT_ITEMS - 1) % TOTAL_SHORTCUT_ITEMS;
        }
        let sel_sc_idx = self.shortcuts_selected_idx;

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
                        let mut toggle_idx = 0usize;
                        let mut add_toggle = |ui: &mut egui::Ui, key: &str, desc: &str, opt: &mut bool| {
                            let cur_idx = toggle_idx;
                            toggle_idx += 1;
                            let is_focused = sel_sc_idx == cur_idx;
                            let key_resp = ui.label(RichText::new(key).strong().monospace().color(if is_dark { Color32::from_rgb(100, 190, 255) } else { Color32::from_rgb(0, 100, 200) }));
                            if is_focused {
                                ui.painter().rect_stroke(
                                    key_resp.rect.expand(2.0),
                                    2.0,
                                    egui::Stroke::new(1.5, Color32::from_rgb(100, 180, 255)),
                                    egui::StrokeKind::Outside,
                                );
                            }
                            ui.label(desc);
                            ui.push_id(key, |ui| {
                                let resp = ui.checkbox(opt, "");
                                if is_focused {
                                    ui.painter().rect_stroke(
                                        resp.rect.expand(2.0),
                                        3.0,
                                        egui::Stroke::new(1.5, Color32::from_rgb(100, 180, 255)),
                                        egui::StrokeKind::Outside,
                                    );
                                }
                                if resp.changed() || (is_focused && activate) {
                                    if is_focused && activate {
                                        *opt = !*opt;
                                    }
                                    shortcuts_changed = true;
                                }
                            });
                            ui.end_row();
                        };
                        let add_fixed = |ui: &mut egui::Ui, key: &str, desc: &str| {
                            ui.label(RichText::new(key).strong().monospace().color(if is_dark { Color32::from_rgb(100, 190, 255) } else { Color32::from_rgb(0, 100, 200) }));
                            ui.label(desc);
                            ui.label(RichText::new("Always").weak().small());
                            ui.end_row();
                        };

                        add_sec(ui, "Navigation");
                        add_fixed(ui, "Up / Down", "Move selection through folder rows");
                        add_fixed(ui, "Left", "Navigate up to parent directory");
                        add_fixed(ui, "Right", "Enter selected folder");
                        add_fixed(ui, "Enter", "Enter selected folder or open selected file");
                        add_fixed(ui, "Backspace", "Navigate up to parent directory");
                        add_fixed(ui, "Home / End", "Jump to first / last item in current folder");
                        add_fixed(ui, "PageUp / PageDown", "Scroll up / down by 15 items");

                        add_sec(ui, "Quick Search");
                        add_toggle(ui, "Ctrl + F", "Activate the type-to-filter bar", &mut self.config.shortcuts.ctrl_f_filter);
                        add_toggle(ui, "F2", "Cycle search mode (type to select vs type to filter)", &mut self.config.shortcuts.f2_search_mode);
                        add_toggle(ui, "F3", "Activate the type-to-filter bar", &mut self.config.shortcuts.f3_search_filter);
                        add_fixed(ui, "A-Z, 0-9", "Type to select an item or to filter the list");
                        add_fixed(ui, "Esc", "Clear active search filter query or dismiss popup");

                        add_sec(ui, "File Operations & Modes");
                        add_toggle(ui, "Del", "Move selected item to Recycle Bin / Trash", &mut self.config.shortcuts.del_trash);
                        add_toggle(ui, "Shift + Del", "Permanently delete selected item (with confirmation)", &mut self.config.shortcuts.shift_del_permanent);
                        add_fixed(ui, "Menu / F10", "Open right-click context menu using keyboard");
                        add_toggle(ui, "F5", "Refresh and rescan current directory", &mut self.config.shortcuts.f5_refresh);
                        add_fixed(ui, "F6", "Toggle Include Subfolders scan");
                        add_fixed(ui, "Ctrl + D", "Toggle Duplicate Finder view");
                        add_fixed(ui, "Ctrl + B", "Bookmark current folder to Favorites");

                        add_sec(ui, "Preferences & View");
                        add_fixed(ui, "F1", "Toggle this Keyboard Shortcuts cheat sheet (open / close)");
                        add_toggle(ui, "F11", "Toggle fullscreen mode", &mut self.config.shortcuts.f11_fullscreen);
                        add_toggle(ui, "F12", "Toggle Settings & Menu panel", &mut self.config.shortcuts.f12_settings);
                        add_toggle(ui, "Ctrl + ,", "Toggle Settings & Menu panel", &mut self.config.shortcuts.ctrl_comma_settings);
                        add_toggle(ui, "Ctrl + H", "Toggle hidden files and folders", &mut self.config.shortcuts.ctrl_h_hidden);
                        add_toggle(ui, "Ctrl + Shift + H", "Toggle protected system files & links", &mut self.config.shortcuts.ctrl_shift_h_system);
                    });

                if shortcuts_changed {
                    self.save_config();
                }

                ui.add_space(8.0);
                ui.separator();
                ui.horizontal(|ui| {
                    ui.label(RichText::new("Press Esc / F1 or click outside to return").weak().small());
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        let resp_close = ui.button("Close (Esc / F1)");
                        if sel_sc_idx == 11 {
                            ui.painter().rect_stroke(
                                resp_close.rect.expand(2.0),
                                3.0,
                                egui::Stroke::new(1.5, Color32::from_rgb(100, 180, 255)),
                                egui::StrokeKind::Outside,
                            );
                        }
                        if resp_close.clicked() || (sel_sc_idx == 11 && activate) {
                            close_shortcuts = true;
                        }
                    });
                });
            });

        if let Some(resp) = win_resp
            && !self.shortcuts_just_opened && ui.input(|i| i.pointer.any_pressed())
                && let Some(pos) = ui.input(|i| i.pointer.interact_pos())
                    && !resp.response.rect.contains(pos) {
                        close_shortcuts = true;
                    }

        if self.shortcuts_just_opened {
            self.shortcuts_just_opened = false;
        }

        if close_shortcuts {
            self.show_shortcuts_modal = false;
        }
    }
}
