use std::sync::atomic::Ordering;
use std::time::Duration;

use eframe::egui::{self, Color32, Panel, RichText};

use crate::model::format_size;

use super::FreeDataDietApp;

impl FreeDataDietApp {
    pub(super) fn render_status_bar(&mut self, ui: &mut egui::Ui, is_dark: bool) {
        Panel::bottom("bottom_panel").show(ui, |ui| {
            ui.add_space(4.0);
            ui.horizontal(|ui| {
                if let Some((_, time)) = &self.status_toast {
                    if time.elapsed() >= Duration::from_secs(3) {
                        self.status_toast = None;
                    } else {
                        ui.ctx().request_repaint_after(Duration::from_millis(250));
                    }
                }

                if let Some((toast, _)) = &self.status_toast {
                    ui.label(RichText::new(toast).color(Color32::from_rgb(100, 220, 150)).strong());
                } else if let Some(err) = &self.error_message {
                    ui.label(RichText::new(err).color(Color32::from_rgb(255, 100, 100)));
                } else if self.is_finding_duplicates {
                    ui.spinner();
                    ui.label(RichText::new(&self.duplicate_progress_text).color(Color32::from_rgb(255, 180, 60)).strong());
                    if ui.small_button("Cancel").clicked() {
                        self.cancel_duplicate_hashing.store(true, Ordering::Relaxed);
                        self.is_finding_duplicates = false;
                        self.duplicate_receiver = None;
                    }
                } else if self.is_scanning {
                    ui.spinner();
                    ui.label(format!(
                        "Scanning storage... {} items analyzed",
                        self.scanned_counter
                    ));
                    if ui.button("Cancel").clicked() {
                        if let Some(scanner) = &self.scanner {
                            scanner.cancel();
                        }
                        self.scanner = None;
                        self.is_scanning = false;
                        for e in &mut self.entries {
                            e.is_scanning = false;
                        }
                    }
                } else if self.config.show_duplicates_only {
                    let active_clusters_count = self
                        .duplicate_clusters
                        .iter()
                        .filter(|c| !self.ignored_cluster_ids.contains(&c.cluster_id))
                        .count();
                    ui.label(RichText::new(format!(
                        "Duplicate Search Complete: {} groups  |  Wasted Space: {}",
                        active_clusters_count,
                        format_size(self.duplicate_total_wasted)
                    )).small());
                } else if let Some(summary) = &self.summary {
                    ui.label(RichText::new(format!(
                        "Scanned in {:.2}s  |  {} items  |  Total: {} (On Disk: {})",
                        summary.duration.as_secs_f32(),
                        summary.total_files,
                        format_size(summary.total_size),
                        format_size(summary.total_allocated)
                    )).small());
                } else {
                    ui.label("Ready");
                }

                let avail_width = ui.available_width();
                if avail_width > 200.0 {
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        let hint = if avail_width > 550.0 {
                            "[Up/Down] Navigate  |  [Enter] Open  |  [Backspace] Up  |  [Del] Trash  |  [F1] Help  |  [F5] Rescan  |  [F12] Settings"
                        } else if avail_width > 350.0 {
                            "[Enter] Open  |  [Backspace] Up  |  [Del] Trash  |  [F12] Settings"
                        } else {
                            "[F1] Help  |  [F12] Settings"
                        };
                        ui.label(
                            RichText::new(hint)
                                .small()
                                .color(if is_dark { Color32::DARK_GRAY } else { Color32::from_rgb(120, 120, 120) }),
                        );
                    });
                }
            });
            ui.add_space(4.0);
        });
    }
}
