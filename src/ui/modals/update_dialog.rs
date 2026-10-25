use eframe::egui::{self, Color32, RichText, Vec2, Window};

use crate::updater::{UpdateManager, UpdateStatus};

pub fn render_update_dialog(
    ctx: &egui::Context,
    updater: &mut UpdateManager,
    is_open: &mut bool,
) {
    if !*is_open {
        return;
    }

    let mut close_requested = false;

    Window::new("Software Update")
        .open(is_open)
        .collapsible(false)
        .resizable(false)
        .anchor(egui::Align2::CENTER_CENTER, Vec2::ZERO)
        .fixed_size(Vec2::new(500.0, 360.0))
        .show(ctx, |ui| {
            ui.add_space(4.0);

            match updater.status() {
                UpdateStatus::Available(ref update) => {
                    ui.label(
                        RichText::new(format!(
                            "FreeDataDiet v{} is available!",
                            update.latest_version
                        ))
                        .color(Color32::from_rgb(60, 220, 120))
                        .heading(),
                    );
                    ui.label(RichText::new(format!("Current version: v{}", update.current_version)).small());
                    ui.add_space(10.0);

                    ui.label(RichText::new("Changelog:").strong());
                    egui::ScrollArea::vertical()
                        .max_height(160.0)
                        .show(ui, |ui| {
                            let notes = if update.release_notes.trim().is_empty() {
                                "Performance improvements, memory optimizations, and duplicate scanner refinements.".to_string()
                            } else {
                                update.release_notes.clone()
                            };
                            ui.label(notes);
                        });

                    ui.add_space(14.0);
                    ui.horizontal(|ui| {
                        #[cfg(windows)]
                        {
                            if update.download_url.is_some() {
                                if ui
                                    .add(
                                        egui::Button::new(
                                            RichText::new("Download and Restart")
                                                .color(Color32::WHITE)
                                                .background_color(Color32::from_rgb(40, 160, 80))
                                                .strong(),
                                        ),
                                    )
                                    .clicked()
                                {
                                    updater.start_download((**update).clone());
                                    close_requested = true;
                                }
                            }
                        }

                        #[cfg(not(windows))]
                        {
                            ui.label(RichText::new("Update via your package manager: apt update && apt upgrade freedatadiet").small());
                        }

                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            if ui.button("Close").clicked() {
                                close_requested = true;
                            }
                        });
                    });
                }
                UpdateStatus::Downloading {
                    progress_pct,
                    ref message,
                } => {
                    ui.label(RichText::new("Downloading Update").heading());
                    ui.add_space(10.0);
                    ui.add(egui::ProgressBar::new(progress_pct).show_percentage());
                    ui.add_space(6.0);
                    ui.label(message);
                }
                UpdateStatus::DownloadedReadyToRestart { ref new_exe_path } => {
                    ui.label(RichText::new("Update Ready to Apply").heading());
                    ui.add_space(10.0);
                    ui.label("The new version has been downloaded and verified.");
                    ui.add_space(14.0);

                    if ui
                        .add(
                            egui::Button::new(
                                RichText::new("Restart to Apply Update")
                                    .color(Color32::WHITE)
                                    .background_color(Color32::from_rgb(40, 160, 80))
                                    .strong(),
                            ),
                        )
                        .clicked()
                    {
                        updater.restart_and_apply(new_exe_path.clone());
                    }
                }
                UpdateStatus::Failed(ref err) => {
                    ui.label(RichText::new("Update Failed").color(Color32::RED).heading());
                    ui.add_space(10.0);
                    ui.label(err);
                    ui.add_space(14.0);
                    if ui.button("Close").clicked() {
                        close_requested = true;
                    }
                }
                _ => {
                    ui.label("FreeDataDiet is up to date.");
                    ui.add_space(14.0);
                    if ui.button("Close").clicked() {
                        close_requested = true;
                    }
                }
            }
        });

    if close_requested {
        *is_open = false;
    }
}
