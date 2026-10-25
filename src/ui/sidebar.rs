use std::path::{Path, PathBuf};
use eframe::egui::{self, Color32, Rect, RichText, ScrollArea, Sense, Stroke, Vec2};

use crate::config::{
    default_excluded_patterns, AppConfig, AppTheme, QuickSearchMode, UsageBasis,
};
use crate::locations::LocationManager;
use crate::model::{format_size, DriveInfo};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SidebarTab {
    Explorer,
    Settings,
}

pub struct SidebarAction {
    pub navigate_to: Option<PathBuf>,
    pub add_favorite: bool,
    pub rescan_needed: bool,
    pub recompute_duplicates: bool,
    pub open_shortcuts_modal: bool,
}

pub fn render_sidebar(
    ui: &mut egui::Ui,
    config: &mut AppConfig,
    active_tab: &mut SidebarTab,
    drives: &[DriveInfo],
    current_path: &Path,
    location_mgr: &LocationManager,
    is_dark: bool,
) -> SidebarAction {
    let mut action = SidebarAction {
        navigate_to: None,
        add_favorite: false,
        rescan_needed: false,
        recompute_duplicates: false,
        open_shortcuts_modal: false,
    };

    ui.set_width(260.0);

    // Tab Header
    ui.add_space(4.0);
    ui.horizontal(|ui| {
        let exp_selected = *active_tab == SidebarTab::Explorer;
        let set_selected = *active_tab == SidebarTab::Settings;

        let exp_btn = ui.selectable_label(exp_selected, RichText::new("Explorer").strong());
        if exp_btn.clicked() {
            *active_tab = SidebarTab::Explorer;
        }

        let set_btn = ui.selectable_label(set_selected, RichText::new("Settings").strong());
        if set_btn.clicked() {
            *active_tab = SidebarTab::Settings;
        }
    });
    ui.add_space(4.0);
    ui.separator();

    ScrollArea::vertical()
        .auto_shrink([false, false])
        .show(ui, |ui| {
            match active_tab {
                SidebarTab::Explorer => {
                    render_explorer_tab(
                        ui,
                        config,
                        drives,
                        current_path,
                        location_mgr,
                        &mut action,
                        is_dark,
                    );
                }
                SidebarTab::Settings => {
                    render_settings_tab(
                        ui,
                        config,
                        &mut action,
                        is_dark,
                    );
                }
            }
        });

    action
}

fn render_explorer_tab(
    ui: &mut egui::Ui,
    config: &mut AppConfig,
    drives: &[DriveInfo],
    current_path: &Path,
    location_mgr: &LocationManager,
    action: &mut SidebarAction,
    is_dark: bool,
) {
    ui.add_space(4.0);

    // --- DRIVES SECTION ---
    ui.label(RichText::new("DRIVES").strong().small().color(Color32::GRAY));
    ui.add_space(4.0);

    for drive in drives {
        let is_current = current_path.starts_with(&drive.path);
        let free_str = format_size(drive.available_space);
        let total_str = format_size(drive.total_space);
        let used_ratio = if drive.total_space > 0 {
            (drive.total_space.saturating_sub(drive.available_space)) as f32 / drive.total_space as f32
        } else {
            0.0
        };

        let card_bg = if is_current {
            if is_dark {
                Color32::from_rgb(32, 52, 78)
            } else {
                Color32::from_rgb(215, 235, 255)
            }
        } else if is_dark {
            Color32::from_gray(30)
        } else {
            Color32::from_gray(242)
        };

        let card_stroke = if is_current {
            Stroke::new(1.0, Color32::from_rgb(80, 160, 240))
        } else {
            Stroke::new(1.0, if is_dark { Color32::from_gray(45) } else { Color32::from_gray(225) })
        };

        let card_frame = egui::Frame::new()
            .fill(card_bg)
            .stroke(card_stroke)
            .corner_radius(4.0)
            .inner_margin(egui::Margin::symmetric(8, 6));

        let response = card_frame.show(ui, |ui| {
            ui.set_width(ui.available_width());

            // Header line
            ui.horizontal(|ui| {
                let name_color = if is_current {
                    if is_dark { Color32::from_rgb(120, 215, 255) } else { Color32::from_rgb(0, 95, 195) }
                } else if is_dark {
                    Color32::WHITE
                } else {
                    Color32::BLACK
                };

                ui.label(RichText::new(&drive.name).strong().color(name_color));

                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    ui.label(RichText::new(format!("{free_str} free of {total_str}")).small().color(Color32::GRAY));
                });
            });

            ui.add_space(4.0);

            // Usage bar
            let bar_height = 4.0;
            let bar_rect = Rect::from_min_size(
                ui.cursor().min,
                Vec2::new(ui.available_width(), bar_height),
            );

            ui.painter().rect_filled(
                bar_rect,
                2.0,
                if is_dark { Color32::from_gray(50) } else { Color32::from_gray(215) },
            );

            let bar_color = if used_ratio > 0.90 {
                Color32::from_rgb(255, 90, 90)
            } else if used_ratio > 0.75 {
                Color32::from_rgb(255, 180, 60)
            } else {
                Color32::from_rgb(80, 180, 255)
            };

            let filled_width = (bar_rect.width() * used_ratio.clamp(0.0, 1.0)).max(1.0);
            let filled_rect = Rect::from_min_size(bar_rect.min, Vec2::new(filled_width, bar_height));
            ui.painter().rect_filled(filled_rect, 2.0, bar_color);

            ui.advance_cursor_after_rect(bar_rect);
        }).response;

        // Make entire card interactive and clickable!
        let card_click = ui.interact(response.rect, ui.scope_id().with(&drive.path), Sense::click());
        let card_click = card_click.on_hover_text(format!(
            "Click to navigate to {} ({} free of {})",
            drive.path.to_string_lossy(),
            free_str,
            total_str
        ));

        if card_click.clicked() {
            action.navigate_to = Some(drive.path.clone());
        }

        ui.add_space(4.0);
    }

    ui.add_space(10.0);
    ui.separator();
    ui.add_space(6.0);

    // --- FAVORITES SECTION ---
    ui.horizontal(|ui| {
        ui.label(RichText::new("FAVORITES").strong().small().color(Color32::GRAY));
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            if ui.small_button("+ Add").on_hover_text("Bookmark current folder to favorites (Ctrl+B)").clicked() {
                action.add_favorite = true;
            }
        });
    });
    ui.add_space(4.0);

    let mut remove_path = None;
    let mut move_op = None;

    for (i, loc) in config.saved_locations.iter().enumerate() {
        let is_active = current_path == loc.path;
        let is_accessible = location_mgr.is_accessible(&loc.path);
        let is_startup = i == 0;

        let row_frame = egui::Frame::new()
            .fill(if is_active {
                if is_dark { Color32::from_rgb(32, 52, 78) } else { Color32::from_rgb(215, 235, 255) }
            } else {
                Color32::TRANSPARENT
            })
            .corner_radius(3.0)
            .inner_margin(egui::Margin::symmetric(4, 3));

        let row_resp = row_frame.show(ui, |ui| {
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
                    Color32::from_rgb(200, 100, 100)
                } else if is_active {
                    if is_dark { Color32::from_rgb(120, 215, 255) } else { Color32::from_rgb(0, 95, 195) }
                } else if is_dark {
                    Color32::LIGHT_GRAY
                } else {
                    Color32::BLACK
                };

                ui.label(RichText::new(&loc.name).color(name_color).strong());

                if !is_accessible {
                    ui.label(RichText::new("[Offline]").small().color(Color32::from_rgb(220, 100, 100)));
                }
            });
        }).response;

        let item_click = ui.interact(row_resp.rect, ui.scope_id().with(&loc.path), Sense::click());
        let item_click = if is_accessible {
            item_click.on_hover_text(loc.path.to_string_lossy().to_string())
        } else {
            item_click.on_hover_text("Location currently offline or unreachable")
        };

        if item_click.clicked() && is_accessible {
            action.navigate_to = Some(loc.path.clone());
        }

        item_click.context_menu(|ui| {
            if ui.button("Remove from Favorites").clicked() {
                remove_path = Some(loc.path.clone());
                ui.close();
            }
            if i > 0 && ui.button("Set as Startup Folder").clicked() {
                move_op = Some(i);
                ui.close();
            }
        });

        ui.add_space(2.0);
    }

    if let Some(path) = remove_path {
        LocationManager::remove_location(config, &path);
    }
    if let Some(idx) = move_op {
        LocationManager::set_as_startup(config, idx);
    }

    ui.add_space(10.0);
    ui.separator();
    ui.add_space(6.0);

    // --- QUICK VIEW OPTIONS ---
    ui.label(RichText::new("VIEW OPTIONS").strong().small().color(Color32::GRAY));
    ui.add_space(4.0);

    let mut dup_only = config.show_duplicates_only;
    if ui.checkbox(&mut dup_only, "Duplicates Only").on_hover_text("Filter to show duplicate groups and wasted space").changed() {
        config.show_duplicates_only = dup_only;
        let _ = config.save();
        action.rescan_needed = true;
    }

    let mut flatten = config.flatten_subfolders;
    if ui.checkbox(&mut flatten, "Flatten Subfolders (F6)").on_hover_text("Recursively flatten subfolder contents into a single list").changed() {
        config.flatten_subfolders = flatten;
        let _ = config.save();
        action.rescan_needed = true;
    }
}

fn render_settings_tab(
    ui: &mut egui::Ui,
    config: &mut AppConfig,
    action: &mut SidebarAction,
    _is_dark: bool,
) {
    ui.add_space(4.0);

    // --- THEME & APPEARANCE ---
    ui.label(RichText::new("THEME & APPEARANCE").strong().small().color(Color32::GRAY));
    ui.add_space(4.0);

    ui.horizontal(|ui| {
        if ui.selectable_value(&mut config.theme, AppTheme::Dark, "Dark").clicked() {
            let _ = config.save();
        }
        if ui.selectable_value(&mut config.theme, AppTheme::Light, "Light").clicked() {
            let _ = config.save();
        }
        if ui.selectable_value(&mut config.theme, AppTheme::System, "System").clicked() {
            let _ = config.save();
        }
    });

    ui.add_space(6.0);
    ui.label("UI Scale:");
    ui.horizontal(|ui| {
        let mut scale = config.ui_scale;
        if ui.add(egui::Slider::new(&mut scale, 0.8..=1.6).step_by(0.05)).changed() {
            config.ui_scale = scale;
            ui.ctx().set_pixels_per_point(scale);
            let _ = config.save();
        }
        if ui.button("100%").on_hover_text("Reset scale to 1.0").clicked() {
            config.ui_scale = 1.0;
            ui.ctx().set_pixels_per_point(1.0);
            let _ = config.save();
        }
    });

    ui.add_space(10.0);
    ui.separator();
    ui.add_space(6.0);

    // --- USAGE BAR SCALING ---
    ui.label(RichText::new("USAGE BAR BASIS").strong().small().color(Color32::GRAY));
    ui.add_space(4.0);

    if ui.radio_value(&mut config.usage_basis, UsageBasis::Folder, "Current folder total").clicked() {
        let _ = config.save();
    }
    if ui.radio_value(&mut config.usage_basis, UsageBasis::LargestItem, "Relative to largest item").clicked() {
        let _ = config.save();
    }
    if ui.radio_value(&mut config.usage_basis, UsageBasis::DriveUsed, "Used disk space").clicked() {
        let _ = config.save();
    }
    if ui.radio_value(&mut config.usage_basis, UsageBasis::DriveTotal, "Total disk capacity").clicked() {
        let _ = config.save();
    }

    ui.add_space(10.0);
    ui.separator();
    ui.add_space(6.0);

    // --- FILE VISIBILITY ---
    ui.label(RichText::new("FILE VISIBILITY").strong().small().color(Color32::GRAY));
    ui.add_space(4.0);

    if ui.checkbox(&mut config.show_hidden_files, "Show hidden files").changed() {
        let _ = config.save();
    }
    if ui.checkbox(&mut config.show_system_files, "Show system files").changed() {
        let _ = config.save();
    }
    if ui.checkbox(&mut config.show_locked_folders, "Show locked folders").changed() {
        let _ = config.save();
    }

    ui.add_space(10.0);
    ui.separator();
    ui.add_space(6.0);

    // --- CATEGORY FILTERS ---
    ui.horizontal(|ui| {
        ui.label(RichText::new("WHAT'S SHOWN").strong().small().color(Color32::GRAY));
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            if ui.small_button("All").on_hover_text("Enable all categories").clicked() {
                config.category_filters.show_images = true;
                config.category_filters.show_videos = true;
                config.category_filters.show_audio = true;
                config.category_filters.show_documents = true;
                config.category_filters.show_archives = true;
                let _ = config.save();
            }
        });
    });
    ui.add_space(4.0);

    if ui.checkbox(&mut config.category_filters.show_images, "Images").changed() {
        let _ = config.save();
    }
    if ui.checkbox(&mut config.category_filters.show_videos, "Videos").changed() {
        let _ = config.save();
    }
    if ui.checkbox(&mut config.category_filters.show_audio, "Audio").changed() {
        let _ = config.save();
    }
    if ui.checkbox(&mut config.category_filters.show_documents, "Documents").changed() {
        let _ = config.save();
    }
    if ui.checkbox(&mut config.category_filters.show_archives, "Archives").changed() {
        let _ = config.save();
    }

    ui.add_space(10.0);
    ui.separator();
    ui.add_space(6.0);

    // --- QUICK SEARCH MODE ---
    ui.label(RichText::new("KEYBOARD SEARCH").strong().small().color(Color32::GRAY));
    ui.add_space(4.0);

    if ui.radio_value(&mut config.quick_search_mode, QuickSearchMode::JumpToPrefix, "Jump-to-Name (Explorer)").clicked() {
        let _ = config.save();
    }
    if ui.radio_value(&mut config.quick_search_mode, QuickSearchMode::LiveFilter, "Live Filter (Type to filter)").clicked() {
        let _ = config.save();
    }

    ui.add_space(10.0);
    ui.separator();
    ui.add_space(6.0);

    // --- DUPLICATE CRITERIA ---
    ui.label(RichText::new("DUPLICATE CRITERIA").strong().small().color(Color32::GRAY));
    ui.add_space(4.0);

    let mut dup_changed = false;
    if ui.checkbox(&mut config.duplicate_criteria.match_size, "Same size").changed() {
        dup_changed = true;
    }
    if ui.checkbox(&mut config.duplicate_criteria.match_name, "Same file name").changed() {
        dup_changed = true;
    }
    if ui.checkbox(&mut config.duplicate_criteria.match_timestamp, "Same timestamp").changed() {
        dup_changed = true;
    }
    if ui.checkbox(&mut config.duplicate_criteria.verify_hash, "Bit-exact SHA-256").changed() {
        dup_changed = true;
    }

    if dup_changed {
        let _ = config.save();
        action.recompute_duplicates = true;
    }

    ui.add_space(10.0);
    ui.separator();
    ui.add_space(6.0);

    // --- EXCLUSIONS ---
    ui.label(RichText::new("EXCLUDED PATTERNS").strong().small().color(Color32::GRAY));
    ui.add_space(4.0);

    let mut pattern_changed = false;
    for p in &mut config.excluded_patterns {
        if ui.checkbox(&mut p.enabled, &p.pattern).changed() {
            pattern_changed = true;
        }
    }

    if ui.small_button("Restore Default Exclusions").clicked() {
        config.excluded_patterns = default_excluded_patterns();
        pattern_changed = true;
    }

    if pattern_changed {
        let _ = config.save();
        action.rescan_needed = true;
    }

    ui.add_space(10.0);
    ui.separator();
    ui.add_space(6.0);

    // --- SHORTCUTS & FACTORY RESET ---
    ui.label(RichText::new("SYSTEM & MAINTENANCE").strong().small().color(Color32::GRAY));
    ui.add_space(4.0);

    if ui.button("Keyboard Shortcuts (F1)").clicked() {
        action.open_shortcuts_modal = true;
    }

    ui.add_space(4.0);
    if ui.button(RichText::new("Reset to Factory Defaults").color(Color32::from_rgb(255, 120, 120))).clicked() {
        *config = AppConfig::default();
        let _ = config.save();
        ui.ctx().set_pixels_per_point(1.0);
        action.rescan_needed = true;
    }

    ui.add_space(10.0);
}
