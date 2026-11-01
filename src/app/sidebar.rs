use std::path::PathBuf;
use std::time::Instant;

use eframe::egui::{self, Color32, Rect, RichText, ScrollArea, Sense, Stroke, Vec2};

use crate::config::StartupLocationMode;
use crate::locations::LocationManager;
use crate::model::format_size;

use super::{ActivePanelFocus, FreeDataDietApp};

impl FreeDataDietApp {
    pub(super) fn render_sidebar(&mut self, ui: &mut egui::Ui, is_dark: bool) {
        ui.set_width(300.0);
        ui.add_space(4.0);

        let is_panel_focused = self.panel_focus == ActivePanelFocus::SettingsPanel;

        ui.horizontal(|ui| {
            ui.label(RichText::new("Settings & Locations").size(14.0).strong());
            if is_panel_focused {
                ui.label(RichText::new("[Focused - Tab to view]").small().color(Color32::from_rgb(100, 185, 255)));
            }
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if ui
                    .small_button("Hide (F12)")
                    .on_hover_text("Collapse settings panel (F12 / F4 / Ctrl+,)")
                    .clicked()
                {
                    self.config.show_sidebar = false;
                    self.save_config();
                }
            });
        });
        ui.add_space(2.0);
        ui.separator();

        let mut nav_target: Option<PathBuf> = None;
        let mut trigger_rescan = false;
        let mut recompute_dups = false;
        let show_sc = self.config.show_shortcuts_in_settings;

        const TOTAL_SETTINGS_ITEMS: usize = 32;

        let nav_down = is_panel_focused && ui.input(|i| i.key_pressed(egui::Key::ArrowDown));
        let nav_up = is_panel_focused && ui.input(|i| i.key_pressed(egui::Key::ArrowUp));
        let key_activate = is_panel_focused && ui.input(|i| i.key_pressed(egui::Key::Enter) || i.key_pressed(egui::Key::Space));

        if nav_down {
            self.settings_nav_idx = (self.settings_nav_idx + 1) % TOTAL_SETTINGS_ITEMS;
        } else if nav_up {
            self.settings_nav_idx = (self.settings_nav_idx + TOTAL_SETTINGS_ITEMS - 1) % TOTAL_SETTINGS_ITEMS;
        }

        ScrollArea::vertical()
            .auto_shrink([false, false])
            .show(ui, |ui| {
                // --- 1. DRIVES ---
                ui.add_space(4.0);
                ui.label(RichText::new("Drives").size(13.5).strong());
                ui.add_space(2.0);

                for drive in &self.drives {
                    let is_current = self.current_path.starts_with(&drive.path);
                    let free_str = format_size(drive.available_space);
                    let total_str = format_size(drive.total_space);
                    let used_ratio = if drive.total_space > 0 {
                        (drive.total_space.saturating_sub(drive.available_space)) as f32
                            / drive.total_space as f32
                    } else {
                        0.0
                    };

                    let card_bg = if is_current {
                        if is_dark {
                            Color32::from_rgb(25, 45, 70)
                        } else {
                            Color32::from_rgb(220, 235, 252)
                        }
                    } else if is_dark {
                        Color32::from_rgb(32, 38, 48)
                    } else {
                        Color32::from_rgb(238, 242, 248)
                    };

                    let card_stroke = if is_current {
                        Stroke::new(1.0, Color32::from_rgb(70, 150, 230))
                    } else {
                        Stroke::new(1.0, if is_dark { Color32::from_rgb(48, 58, 72) } else { Color32::from_rgb(210, 218, 228) })
                    };

                    let resp = egui::Frame::new()
                        .fill(card_bg)
                        .stroke(card_stroke)
                        .corner_radius(3.0)
                        .inner_margin(egui::Margin::symmetric(8, 5))
                        .show(ui, |ui| {
                            ui.set_width(ui.available_width());
                            ui.horizontal(|ui| {
                                let name_col = if is_current {
                                    if is_dark { Color32::from_rgb(120, 215, 255) } else { Color32::from_rgb(0, 90, 180) }
                                } else if is_dark {
                                    Color32::WHITE
                                } else {
                                    Color32::from_rgb(20, 25, 35)
                                };
                                ui.label(RichText::new(&drive.name).strong().color(name_col));
                                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                                    ui.label(RichText::new(format!("{free_str} free of {total_str}")).small().weak());
                                });
                            });
                            ui.add_space(3.0);
                            let bar_h = 4.0;
                            let bar_rect = Rect::from_min_size(ui.cursor().min, Vec2::new(ui.available_width(), bar_h));
                            ui.painter().rect_filled(bar_rect, 2.0, if is_dark { Color32::from_gray(45) } else { Color32::from_gray(210) });
                            let bar_col = if used_ratio > 0.90 {
                                Color32::from_rgb(255, 90, 90)
                            } else if used_ratio > 0.75 {
                                Color32::from_rgb(255, 180, 60)
                            } else {
                                Color32::from_rgb(80, 180, 255)
                            };
                            let fill_w = (bar_rect.width() * used_ratio.clamp(0.0, 1.0)).max(1.0);
                            ui.painter().rect_filled(Rect::from_min_size(bar_rect.min, Vec2::new(fill_w, bar_h)), 2.0, bar_col);
                            ui.advance_cursor_after_rect(bar_rect);
                        })
                        .response;

                    let click = ui
                        .interact(resp.rect, ui.scope_id().with(&drive.path), Sense::click())
                        .on_hover_cursor(egui::CursorIcon::PointingHand)
                        .on_hover_text(format!("Click to scan {}", drive.path.to_string_lossy()));

                    if click.clicked() {
                        nav_target = Some(drive.path.clone());
                    }
                    ui.add_space(3.0);
                }

                ui.add_space(6.0);
                ui.separator();

                // --- 2. FAVORITES (WITH CTRL+CLICK MULTI-SELECT) ---
                ui.horizontal(|ui| {
                    ui.label(RichText::new("Favorites").size(13.5).strong());
                    if self.selected_favorites.len() > 1
                        && ui.small_button(format!("Clear ({})", self.selected_favorites.len())).clicked() {
                            self.selected_favorites.clear();
                            trigger_rescan = true;
                        }
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if ui
                            .small_button("+ Add Current")
                            .on_hover_text("Bookmark current folder to Favorites (Ctrl+B)")
                            .clicked()
                        {
                            LocationManager::add_location(&mut self.config, self.current_path.clone(), None);
                            self.status_toast = Some(("Added current folder to Favorites".to_string(), Instant::now()));
                        }
                    });
                });
                ui.label(RichText::new("Hold Ctrl to select multiple folders to search across").weak().small());
                ui.add_space(2.0);

                let mut remove_fav = None;
                let mut startup_fav = None;
                let mut move_fav = None;

                for (i, loc) in self.config.saved_locations.iter().enumerate() {
                    let is_multi_selected = self.selected_favorites.contains(&loc.path);
                    let is_active = self.current_path == loc.path || is_multi_selected;
                    let is_accessible = self.location_mgr.is_accessible(&loc.path);
                    let is_startup = i == 0 && self.config.startup_mode == StartupLocationMode::FirstFavorite;

                    let row_bg = if is_multi_selected {
                        if is_dark { Color32::from_rgb(20, 60, 110) } else { Color32::from_rgb(190, 220, 255) }
                    } else if is_active {
                        if is_dark { Color32::from_rgb(32, 54, 82) } else { Color32::from_rgb(205, 228, 252) }
                    } else {
                        Color32::TRANSPARENT
                    };

                    let row_resp = egui::Frame::new()
                        .fill(row_bg)
                        .corner_radius(3.0)
                        .inner_margin(egui::Margin::symmetric(6, 3))
                        .show(ui, |ui| {
                            ui.set_width(ui.available_width());
                            ui.horizontal(|ui| {
                                let prefix = if is_multi_selected {
                                    "[V]"
                                } else if is_startup {
                                    "[Startup]"
                                } else {
                                    "*"
                                };
                                ui.label(RichText::new(prefix).monospace().small().color(if is_multi_selected {
                                    Color32::from_rgb(100, 200, 255)
                                } else if is_startup {
                                    Color32::from_rgb(255, 200, 80)
                                } else {
                                    Color32::GRAY
                                }));
                                let col = if !is_accessible {
                                    Color32::from_rgb(200, 100, 100)
                                } else if is_active {
                                    if is_dark { Color32::from_rgb(120, 215, 255) } else { Color32::from_rgb(0, 90, 180) }
                                } else if is_dark {
                                    Color32::LIGHT_GRAY
                                } else {
                                    Color32::from_rgb(20, 25, 35)
                                };
                                ui.label(RichText::new(&loc.name).strong().color(col));
                                if !is_accessible {
                                    ui.label(RichText::new("[Offline]").small().color(Color32::from_rgb(220, 100, 100)));
                                }
                            });
                        })
                        .response;

                    let fav_click = ui
                        .interact(row_resp.rect, ui.scope_id().with(&loc.path), Sense::click())
                        .on_hover_cursor(egui::CursorIcon::PointingHand)
                        .on_hover_text(format!("{} (Ctrl+click to multi-select, Right-click for options)", loc.path.to_string_lossy()));

                    if fav_click.clicked() && is_accessible {
                        let ctrl = ui.input(|inp| inp.modifiers.ctrl || inp.modifiers.command);
                        if ctrl {
                            if self.selected_favorites.contains(&loc.path) {
                                self.selected_favorites.remove(&loc.path);
                            } else {
                                self.selected_favorites.insert(loc.path.clone());
                            }
                            if self.selected_favorites.is_empty() {
                                self.selected_favorites.insert(self.current_path.clone());
                            }
                            trigger_rescan = true;
                        } else {
                            self.selected_favorites.clear();
                            self.selected_favorites.insert(loc.path.clone());
                            nav_target = Some(loc.path.clone());
                        }
                    }

                    fav_click.context_menu(|ui| {
                        if ui.button("Set as Startup Folder").clicked() {
                            startup_fav = Some(i);
                            ui.close();
                        }
                        if i > 0 && ui.button("Move Up").clicked() {
                            move_fav = Some((i, true));
                            ui.close();
                        }
                        if i + 1 < self.config.saved_locations.len() && ui.button("Move Down").clicked() {
                            move_fav = Some((i, false));
                            ui.close();
                        }
                        ui.separator();
                        if ui.button("Remove from Favorites").clicked() {
                            remove_fav = Some(loc.path.clone());
                            ui.close();
                        }
                    });
                }

                if let Some(idx) = startup_fav {
                    LocationManager::set_as_startup(&mut self.config, idx);
                }
                if let Some((idx, up)) = move_fav {
                    LocationManager::move_location(&mut self.config, idx, up);
                }
                if let Some(p) = remove_fav {
                    LocationManager::remove_location(&mut self.config, &p);
                }

                ui.add_space(6.0);
                ui.separator();

                self.render_sidebar_settings(
                    ui,
                    is_dark,
                    is_panel_focused,
                    key_activate,
                    show_sc,
                    &mut trigger_rescan,
                    &mut recompute_dups,
                );
            });

        if let Some(target) = nav_target {
            self.navigate_to(target);
        }
        if trigger_rescan {
            self.trigger_scan();
        }
        if recompute_dups {
            self.compute_duplicates();
        }
    }
}
