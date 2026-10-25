use std::collections::HashSet;
use std::path::PathBuf;
use eframe::egui::{self, Color32, RichText};

use crate::duplicate::DuplicateSuggestion;
use crate::model::format_size;

pub struct BatchBarAction {
    pub trigger_trash: bool,
    pub trigger_permanent: bool,
    pub clear_selection: bool,
    pub apply_suggestion: Option<Vec<PathBuf>>,
    pub dismiss_suggestion: bool,
    pub clear_safety_warning: bool,
}

pub fn render_batch_and_assistant_bars(
    ui: &mut egui::Ui,
    selected_paths: &HashSet<PathBuf>,
    total_selected_bytes: u64,
    suggestion: Option<&DuplicateSuggestion>,
    safety_warning: Option<&str>,
    is_dark: bool,
) -> BatchBarAction {
    let mut action = BatchBarAction {
        trigger_trash: false,
        trigger_permanent: false,
        clear_selection: false,
        apply_suggestion: None,
        dismiss_suggestion: false,
        clear_safety_warning: false,
    };

    // 1. Safety Gate Warning Banner
    if let Some(warning) = safety_warning {
        ui.horizontal(|ui| {
            ui.label(RichText::new("Notice:").color(Color32::from_rgb(255, 100, 100)).strong());
            ui.label(RichText::new(warning).color(Color32::from_rgb(255, 200, 80)));
            if ui.small_button("Dismiss").clicked() {
                action.clear_safety_warning = true;
            }
        });
        ui.add_space(2.0);
    }

    // 2. Duplicate Assistant Suggestion Banner
    if let Some(sugg) = suggestion {
        ui.horizontal(|ui| {
            let banner_bg = if is_dark {
                Color32::from_rgb(30, 45, 60)
            } else {
                Color32::from_rgb(230, 240, 255)
            };

            egui::Frame::new()
                .fill(banner_bg)
                .inner_margin(egui::Margin::symmetric(8, 4))
                .corner_radius(4.0)
                .show(ui, |ui| {
                    ui.horizontal(|ui| {
                        ui.label(RichText::new("Duplicate Assistant:").color(Color32::from_rgb(100, 200, 255)).strong().small());
                        let count = sugg.candidate_paths.len();
                        let size_str = format_size(sugg.reclaimable_bytes);
                        ui.label(
                            RichText::new(format!(
                                "{} sibling duplicates found in \"{}\" ({}) whose originals exist in other folders.",
                                count, sugg.folder_name, size_str
                            )).small(),
                        );

                        let btn = ui.add(
                            egui::Button::new(
                                RichText::new(format!("Select All {} Duplicates (Ctrl+Shift+D)", count))
                                    .color(Color32::from_rgb(60, 220, 120))
                                    .strong()
                                    .small(),
                            ),
                        );
                        if btn.clicked() {
                            action.apply_suggestion = Some(sugg.candidate_paths.clone());
                        }

                        if ui.small_button("Dismiss").clicked() {
                            action.dismiss_suggestion = true;
                        }
                    });
                });
        });
        ui.add_space(2.0);
    }

    // 3. Floating Batch Action Bar
    if !selected_paths.is_empty() {
        ui.horizontal(|ui| {
            let count = selected_paths.len();
            let size_str = format_size(total_selected_bytes);

            ui.label(RichText::new(format!("{} items selected ({})", count, size_str)).strong());

            if ui
                .add(
                    egui::Button::new(
                        RichText::new("Move to Recycle Bin (Del)")
                            .color(Color32::from_rgb(100, 220, 150))
                            .small(),
                    ),
                )
                .clicked()
            {
                action.trigger_trash = true;
            }

            if ui
                .add(
                    egui::Button::new(
                        RichText::new("Delete Permanently (Shift+Del)")
                            .color(Color32::from_rgb(255, 120, 120))
                            .small(),
                    ),
                )
                .clicked()
            {
                action.trigger_permanent = true;
            }

            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if ui.small_button("Clear (Esc)").clicked() {
                    action.clear_selection = true;
                }
            });
        });
        ui.add_space(2.0);
    }

    action
}
