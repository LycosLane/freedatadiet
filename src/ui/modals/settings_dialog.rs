use eframe::egui::{self, Color32, RichText, ScrollArea, Vec2, Window};

use crate::config::{
    default_excluded_patterns, AppConfig, AppTheme, ColumnId, ExcludePattern, QuickSearchMode,
    StartupLocationMode, UsageBasis,
};
use crate::locations::LocationManager;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SettingsTab {
    General,
    Appearance,
    Favorites,
    Duplicates,
    Exclusions,
    Shortcuts,
    Reset,
}

pub struct SettingsDialogState {
    pub active_tab: SettingsTab,
    pub new_exclude_input: String,
    pub confirm_reset: bool,
}

impl Default for SettingsDialogState {
    fn default() -> Self {
        Self {
            active_tab: SettingsTab::General,
            new_exclude_input: String::new(),
            confirm_reset: false,
        }
    }
}

pub fn render_settings_dialog(
    ctx: &egui::Context,
    config: &mut AppConfig,
    state: &mut SettingsDialogState,
    is_open: &mut bool,
) -> bool {
    let mut changed = false;
    let mut close_requested = false;

    Window::new("Settings & Preferences")
        .open(is_open)
        .collapsible(false)
        .resizable(true)
        .default_size(Vec2::new(680.0, 480.0))
        .show(ctx, |ui| {
            ui.horizontal(|ui| {
                // Left Tab Rail
                ui.vertical(|ui| {
                    ui.set_width(130.0);
                    let tabs = [
                        (SettingsTab::General, "General"),
                        (SettingsTab::Appearance, "Appearance"),
                        (SettingsTab::Favorites, "Favorites"),
                        (SettingsTab::Duplicates, "Duplicates"),
                        (SettingsTab::Exclusions, "Exclusions"),
                        (SettingsTab::Shortcuts, "Shortcuts"),
                        (SettingsTab::Reset, "Reset & Defaults"),
                    ];

                    for (tab, label) in tabs {
                        let selected = state.active_tab == tab;
                        if ui
                            .selectable_label(selected, RichText::new(label).strong())
                            .clicked()
                        {
                            state.active_tab = tab;
                        }
                    }
                });

                ui.separator();

                // Right Content Area
                ScrollArea::vertical().show(ui, |ui| {
                    match state.active_tab {
                        SettingsTab::General => {
                            ui.heading("General Settings");
                            ui.add_space(8.0);

                            ui.label(RichText::new("Theme Mode").strong());
                            ui.horizontal(|ui| {
                                if ui.selectable_value(&mut config.theme, AppTheme::Dark, "Dark Mode").clicked() {
                                    changed = true;
                                }
                                if ui.selectable_value(&mut config.theme, AppTheme::Light, "Light Mode").clicked() {
                                    changed = true;
                                }
                                if ui.selectable_value(&mut config.theme, AppTheme::System, "System (Auto)").clicked() {
                                    changed = true;
                                }
                            });
                            ui.add_space(12.0);

                            ui.label(RichText::new("Usage Bar Scaling").strong());
                            ui.radio_value(&mut config.usage_basis, UsageBasis::Folder, "Current folder total")
                                .on_hover_text("Shows what percentage of this folder's total size each subfolder or file consumes.");
                            ui.radio_value(&mut config.usage_basis, UsageBasis::LargestItem, "Relative to largest item")
                                .on_hover_text("Scales bars relative to the heaviest item in view, highlighting peak space consumers immediately.");
                            ui.radio_value(&mut config.usage_basis, UsageBasis::DriveUsed, "Used drive space")
                                .on_hover_text("Shows what portion of all used disk space on this drive belongs to each item.");
                            ui.radio_value(&mut config.usage_basis, UsageBasis::DriveTotal, "Total drive capacity")
                                .on_hover_text("Shows each item's consumption against the full physical capacity of the drive.");
                            ui.add_space(12.0);

                            if ui.checkbox(&mut config.remember_last_path, "Remember last visited folder on startup").changed() {
                                changed = true;
                            }
                            if ui.checkbox(&mut config.show_hidden_files, "Show hidden files and dotfiles").changed() {
                                changed = true;
                            }
                            if ui.checkbox(&mut config.show_system_files, "Show system files (protected from deletion)").changed() {
                                changed = true;
                            }
                            if ui.checkbox(&mut config.show_locked_folders, "Show locked or permission-denied folders").changed() {
                                changed = true;
                            }
                            ui.add_space(12.0);

                            ui.label(RichText::new("Quick Search Behavior").strong());
                            ui.radio_value(&mut config.quick_search_mode, QuickSearchMode::JumpToPrefix, "Jump-to-Name (Type letters to jump cursor)")
                                .on_hover_text("Type letters to jump focus to matching file names, matching standard Explorer behavior.");
                            ui.radio_value(&mut config.quick_search_mode, QuickSearchMode::LiveFilter, "Live Filter (Type to filter visible rows)")
                                .on_hover_text("Filters the file list in real time as you type, narrowing the view to specific extensions or names.");
                        }
                        SettingsTab::Appearance => {
                            ui.heading("Appearance & Scale");
                            ui.add_space(8.0);

                            ui.label(RichText::new("Font and UI Scale").strong());
                            ui.horizontal(|ui| {
                                let mut scale = config.ui_scale;
                                if ui.add(egui::Slider::new(&mut scale, 0.8..=1.6).text("Scale").step_by(0.05)).changed() {
                                    config.ui_scale = scale;
                                    ctx.set_pixels_per_point(scale);
                                    changed = true;
                                }
                                if ui.button("100% (Default)").clicked() {
                                    config.ui_scale = 1.0;
                                    ctx.set_pixels_per_point(1.0);
                                    changed = true;
                                }
                            });
                            ui.add_space(12.0);

                            if ui.checkbox(&mut config.show_sidebar, "Show left sidebar (Favorites & Drives)").changed() {
                                changed = true;
                            }
                            ui.add_space(12.0);

                            ui.label(RichText::new("Visible Columns").strong());
                            for col in ColumnId::all() {
                                let mut visible = config.column_order.contains(col);
                                if ui.checkbox(&mut visible, col.title()).changed() {
                                    if visible {
                                        config.column_order.push(*col);
                                    } else {
                                        config.column_order.retain(|c| c != col);
                                    }
                                    changed = true;
                                }
                            }
                        }
                        SettingsTab::Favorites => {
                            ui.heading("Saved Locations & Favorites");
                            ui.add_space(8.0);

                            ui.label(RichText::new("Startup Folder Priority").strong());
                            ui.radio_value(&mut config.startup_mode, StartupLocationMode::FirstFavorite, "Top favorite folder (★ Startup)");
                            ui.radio_value(&mut config.startup_mode, StartupLocationMode::LastVisitedPath, "Last visited folder");
                            ui.radio_value(&mut config.startup_mode, StartupLocationMode::PrimaryDrive, "Primary drive root (C:\\)");
                            ui.add_space(12.0);

                            ui.label(RichText::new("Pinned Locations:").strong());
                            let mut to_remove = None;
                            let mut move_op = None;

                            for (i, loc) in config.saved_locations.iter().enumerate() {
                                ui.horizontal(|ui| {
                                    if i == 0 {
                                        ui.label(RichText::new("★").color(Color32::from_rgb(100, 200, 255)));
                                    } else {
                                        ui.label("  ");
                                    }

                                    ui.label(RichText::new(&loc.name).strong());
                                    ui.label(RichText::new(loc.path.to_string_lossy().to_string()).small().color(Color32::GRAY));

                                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                                        if ui.button("Remove").clicked() {
                                            to_remove = Some(loc.path.clone());
                                        }
                                        if i + 1 < config.saved_locations.len() && ui.button("Down").clicked() {
                                            move_op = Some((i, false));
                                        }
                                        if i > 0 && ui.button("Up").clicked() {
                                            move_op = Some((i, true));
                                        }
                                    });
                                });
                            }

                            if let Some(path) = to_remove {
                                LocationManager::remove_location(config, &path);
                                changed = true;
                            }
                            if let Some((idx, up)) = move_op {
                                LocationManager::move_location(config, idx, up);
                                changed = true;
                            }
                        }
                        SettingsTab::Duplicates => {
                            ui.heading("Duplicate Detection Settings");
                            ui.add_space(8.0);

                            ui.label(RichText::new("Matching Criteria").strong());
                            ui.checkbox(&mut config.duplicate_criteria.match_size, "Match same file size")
                                .on_hover_text("Identifies files with identical byte lengths (detects renamed copies).");
                            ui.checkbox(&mut config.duplicate_criteria.match_name, "Match same file name")
                                .on_hover_text("Requires identical file names across folders.");
                            ui.checkbox(&mut config.duplicate_criteria.match_timestamp, "Match same modification timestamp")
                                .on_hover_text("Matches identical last-modified timestamps (useful for backup syncs).");
                            ui.checkbox(&mut config.duplicate_criteria.verify_hash, "Verify bit-exact SHA-256 hash")
                                .on_hover_text("Performs two-pass hash check (4KB prefix -> 64KB streaming full hash) for 100% collision-proof verification.");
                        }
                        SettingsTab::Exclusions => {
                            ui.heading("Excluded Folders & Patterns");
                            ui.add_space(4.0);
                            ui.label(RichText::new("Files and directories matching checked patterns will be skipped during scan:").small());
                            ui.add_space(8.0);

                            let mut to_remove_idx = None;
                            for (i, p) in config.excluded_patterns.iter_mut().enumerate() {
                                ui.horizontal(|ui| {
                                    if ui.checkbox(&mut p.enabled, &p.pattern).changed() {
                                        changed = true;
                                    }
                                    if p.is_preset {
                                        ui.label(RichText::new("(Preset)").small().color(Color32::DARK_GRAY));
                                    } else {
                                        if ui.small_button("Delete").clicked() {
                                            to_remove_idx = Some(i);
                                        }
                                    }
                                });
                            }

                            if let Some(idx) = to_remove_idx {
                                config.excluded_patterns.remove(idx);
                                changed = true;
                            }

                            ui.add_space(10.0);
                            ui.horizontal(|ui| {
                                ui.add(egui::TextEdit::singleline(&mut state.new_exclude_input).hint_text("Add custom pattern (e.g. *.tmp, build)"));
                                if ui.button("+ Add Pattern").clicked() {
                                    let trimmed = state.new_exclude_input.trim();
                                    if !trimmed.is_empty() && !config.excluded_patterns.iter().any(|p| p.pattern.eq_ignore_ascii_case(trimmed)) {
                                        config.excluded_patterns.push(ExcludePattern {
                                            pattern: trimmed.to_string(),
                                            enabled: true,
                                            is_preset: false,
                                        });
                                        state.new_exclude_input.clear();
                                        changed = true;
                                    }
                                }
                            });

                            ui.add_space(12.0);
                            if ui.button("Restore Default Exclusions").clicked() {
                                config.excluded_patterns = default_excluded_patterns();
                                changed = true;
                            }
                        }
                        SettingsTab::Shortcuts => {
                            ui.heading("Keyboard Shortcuts");
                            ui.add_space(8.0);
                            ui.checkbox(&mut config.shortcuts.f2_search_mode, "F2: Toggle search mode (Jump vs Filter)");
                            ui.checkbox(&mut config.shortcuts.f3_search_filter, "F3: Focus search filter input");
                            ui.checkbox(&mut config.shortcuts.f5_refresh, "F5: Rescan current directory");
                            ui.checkbox(&mut config.shortcuts.f11_fullscreen, "F11: Toggle fullscreen window");
                            ui.checkbox(&mut config.shortcuts.f12_settings, "F12 / Ctrl+,: Open settings dialog");
                            ui.checkbox(&mut config.shortcuts.ctrl_h_hidden, "Ctrl+H: Toggle hidden dotfiles");
                            ui.checkbox(&mut config.shortcuts.ctrl_shift_h_system, "Ctrl+Shift+H: Toggle system files");
                            ui.checkbox(&mut config.shortcuts.del_trash, "Del: Move selection to Recycle Bin");
                            ui.checkbox(&mut config.shortcuts.shift_del_permanent, "Shift+Del: Delete selection permanently");
                        }
                        SettingsTab::Reset => {
                            ui.heading("Maintenance & Factory Reset");
                            ui.add_space(8.0);

                            ui.label("Restore all application preferences, filter toggles, theme, and exclusions to initial defaults.");
                            ui.label("Your files and disks are never modified by resetting settings.");
                            ui.add_space(16.0);

                            if !state.confirm_reset {
                                if ui.add(egui::Button::new(RichText::new("Reset All Settings to Factory Defaults").color(Color32::from_rgb(255, 120, 120)))).clicked() {
                                    state.confirm_reset = true;
                                }
                            } else {
                                ui.label(RichText::new("Are you sure? All custom settings will be cleared.").color(Color32::from_rgb(255, 100, 100)).strong());
                                ui.horizontal(|ui| {
                                    if ui.button(RichText::new("Yes, Reset to Factory Defaults").color(Color32::RED).strong()).clicked() {
                                        *config = AppConfig::default();
                                        let _ = config.save();
                                        ctx.set_pixels_per_point(1.0);
                                        state.confirm_reset = false;
                                        changed = true;
                                    }
                                    if ui.button("Cancel").clicked() {
                                        state.confirm_reset = false;
                                    }
                                });
                            }
                        }
                    }
                });
            });

            ui.add_space(8.0);
            ui.separator();
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if ui.button("Done").clicked() {
                    close_requested = true;
                }
            });
        });

    if close_requested {
        *is_open = false;
    }
    if changed {
        let _ = config.save();
    }
    changed
}
