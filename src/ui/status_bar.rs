use std::time::{Duration, Instant};
use eframe::egui::{self, Color32, RichText};

use crate::model::format_size;
use crate::scanner::ScanSummary;

pub struct StatusBarAction {
    pub cancel_scan: bool,
}

pub fn render_status_bar(
    ui: &mut egui::Ui,
    status_toast: &mut Option<(String, Instant)>,
    error_message: Option<&str>,
    is_scanning: bool,
    scanned_counter: usize,
    summary: Option<&ScanSummary>,
    is_dark: bool,
) -> StatusBarAction {
    let mut action = StatusBarAction { cancel_scan: false };

    ui.horizontal(|ui| {
        // Auto-expire status toast
        if let Some((_, time)) = status_toast {
            if time.elapsed() >= Duration::from_secs(3) {
                *status_toast = None;
            } else {
                ui.ctx().request_repaint_after(Duration::from_millis(250));
            }
        }

        if let Some((toast, _)) = status_toast {
            ui.label(RichText::new(&*toast).color(Color32::from_rgb(100, 220, 150)).strong());
        } else if let Some(err) = error_message {
            ui.label(RichText::new(err).color(Color32::from_rgb(255, 100, 100)));
        } else if is_scanning {
            ui.spinner();
            ui.label(format!("Scanning storage... {} items analyzed", scanned_counter));
            if ui.small_button("Cancel").clicked() {
                action.cancel_scan = true;
            }
        } else if let Some(sum) = summary {
            ui.label(RichText::new(format!(
                "Scanned in {:.2}s  |  {} items  |  Total: {} (On Disk: {})",
                sum.duration.as_secs_f32(),
                sum.total_files,
                format_size(sum.total_size),
                format_size(sum.total_allocated)
            )).small());
        } else {
            ui.label("Ready");
        }

        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            ui.label(
                RichText::new("[Up/Down] Move  |  [Space] Select  |  [Enter] Open  |  [Del] Trash  |  [F1] Help  |  [F4] Sidebar  |  [F5] Rescan  |  [F12] Settings")
                    .small()
                    .color(if is_dark { Color32::DARK_GRAY } else { Color32::from_rgb(120, 120, 120) }),
            );
        });
    });

    action
}
