use std::path::{Path, PathBuf};
use eframe::egui::{self, Color32, RichText, ScrollArea, Vec2};

use crate::config::AppConfig;
use crate::locations::LocationManager;
use crate::model::{format_size, DriveInfo};

pub struct SidebarAction {
    pub navigate_to: Option<PathBuf>,
    pub add_favorite: bool,
    pub rescan_needed: bool,
}

pub fn render_sidebar(
    ui: &mut egui::Ui,
    config: &mut AppConfig,
    drives: &[DriveInfo],
    current_path: &Path,
    location_mgr: &LocationManager,
    is_dark: bool,
) -> SidebarAction {
    let mut action = SidebarAction {
        navigate_to: None,
        add_favorite: false,
        rescan_needed: false,
    };

    ui.set_width(220.0);

    ScrollArea::vertical().show(ui, |ui| {
        ui.add_space(6.0);

        // --- FAVORITES SECTION ---
        ui.horizontal(|ui| {
            ui.label(RichText::new("FAVORITES").strong().small());
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if ui.small_button("+ Add").on_hover_text("Bookmark current folder to favorites (Ctrl+B)").clicked() {
                    action.add_favorite = true;
                }
            });
        });
        ui.add_space(2.0);

        let mut remove_path = None;
        let mut move_op = None;

        for (i, loc) in config.saved_locations.iter().enumerate() {
            let is_active = current_path == loc.path;
            let is_accessible = location_mgr.is_accessible(&loc.path);
            let is_startup = i == 0;

            ui.horizontal(|ui| {
                let star_text = if is_startup { "★" } else { "•" };
                let star_color = if is_startup {
                    Color32::from_rgb(255, 200, 80)
                } else if is_dark {
                    Color32::GRAY
                } else {
                    Color32::DARK_GRAY
                };

                ui.label(RichText::new(star_text).color(star_color));

                let name_color = if !is_accessible {
                    Color32::from_rgb(180, 100, 100)
                } else if is_active {
                    if is_dark {
                        Color32::from_rgb(100, 200, 255)
                    } else {
                        Color32::from_rgb(0, 110, 220)
                    }
                } else if is_dark {
                    Color32::LIGHT_GRAY
                } else {
                    Color32::BLACK
                };

                let btn_text = RichText::new(&loc.name).color(name_color);
                let btn = ui.add(egui::Button::new(btn_text).frame(false));
                let btn = if !is_accessible {
                    btn.on_hover_text("Location currently offline or inaccessible")
                } else {
                    btn.on_hover_text(loc.path.to_string_lossy())
                };

                if btn.clicked() && is_accessible {
                    action.navigate_to = Some(loc.path.clone());
                }

                if !is_accessible {
                    ui.label(RichText::new("[Offline]").small().color(Color32::from_rgb(220, 100, 100)));
                }

                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |_ui| {
                    btn.context_menu(|ui| {
                        if ui.button("Remove from Favorites").clicked() {
                            remove_path = Some(loc.path.clone());
                            ui.close();
                        }
                        if i > 0 && ui.button("Set as Startup Folder").clicked() {
                            move_op = Some((i, 0));
                            ui.close();
                        }
                    });
                });
            });
        }

        if let Some(path) = remove_path {
            LocationManager::remove_location(config, &path);
        }
        if let Some((from, to)) = move_op {
            if to == 0 {
                LocationManager::set_as_startup(config, from);
            }
        }

        ui.add_space(14.0);
        ui.separator();
        ui.add_space(6.0);

        // --- DRIVES SECTION ---
        ui.label(RichText::new("DRIVES").strong().small());
        ui.add_space(2.0);

        for drive in drives {
            let is_current = current_path.starts_with(&drive.path);
            let free_str = format_size(drive.available_space);
            let total_str = format_size(drive.total_space);
            let used_ratio = if drive.total_space > 0 {
                (drive.total_space.saturating_sub(drive.available_space)) as f32 / drive.total_space as f32
            } else {
                0.0
            };

            ui.vertical(|ui| {
                ui.horizontal(|ui| {
                    let drive_name_color = if is_current {
                        if is_dark {
                            Color32::from_rgb(100, 200, 255)
                        } else {
                            Color32::from_rgb(0, 110, 220)
                        }
                    } else if is_dark {
                        Color32::WHITE
                    } else {
                        Color32::BLACK
                    };

                    if ui.add(egui::Button::new(RichText::new(&drive.name).strong().color(drive_name_color)).frame(false)).clicked() {
                        action.navigate_to = Some(drive.path.clone());
                    }

                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        ui.label(RichText::new(format!("{free_str} free of {total_str}")).small().color(Color32::GRAY));
                    });
                });

                let bar_color = if used_ratio > 0.9 {
                    Color32::from_rgb(255, 90, 90)
                } else if used_ratio > 0.75 {
                    Color32::from_rgb(255, 180, 60)
                } else {
                    Color32::from_rgb(80, 180, 255)
                };

                let (rect, _) = ui.allocate_exact_size(Vec2::new(ui.available_width() - 8.0, 4.0), egui::Sense::hover());
                ui.painter().rect_filled(rect, 2.0, if is_dark { Color32::from_gray(40) } else { Color32::from_gray(210) });
                let filled_width = (rect.width() * used_ratio).max(1.0);
                let filled_rect = egui::Rect::from_min_size(rect.min, Vec2::new(filled_width, rect.height()));
                ui.painter().rect_filled(filled_rect, 2.0, bar_color);
            });
            ui.add_space(4.0);
        }

        ui.add_space(14.0);
        ui.separator();
        ui.add_space(6.0);

        // --- QUICK VIEWS ---
        ui.label(RichText::new("VIEWS & MODES").strong().small());
        ui.add_space(2.0);

        let mut dup_active = config.show_duplicates_only;
        if ui.checkbox(&mut dup_active, "Duplicates Only").changed() {
            config.show_duplicates_only = dup_active;
            action.rescan_needed = true;
        }

        let mut flat_active = config.flatten_subfolders;
        if ui.checkbox(&mut flat_active, "Flatten Subfolders (F6)").changed() {
            config.flatten_subfolders = flat_active;
            action.rescan_needed = true;
        }
    });

    action
}
