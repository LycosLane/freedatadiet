use std::path::PathBuf;
use eframe::egui::{self, Color32, RichText, Vec2, Window};

use crate::model::format_size;

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum DeleteModalFocus {
    Trash,
    Permanent,
    Cancel,
}

#[derive(Clone)]
pub enum DeleteConfirmState {
    None,
    Single {
        path: PathBuf,
        name: String,
        is_dir: bool,
        size: u64,
        allocated_size: u64,
        item_count: usize,
        is_protected: bool,
    },
    Batch {
        paths: Vec<PathBuf>,
        total_size: u64,
        total_count: usize,
        has_protected: bool,
    },
}

pub struct DeleteDialogAction {
    pub confirmed_permanent: bool,
    pub confirmed_trash: bool,
    pub cancelled: bool,
}

pub fn render_delete_dialog(
    ctx: &egui::Context,
    state: &DeleteConfirmState,
    focus: &mut DeleteModalFocus,
) -> Option<DeleteDialogAction> {
    let mut action = None;

    match state {
        DeleteConfirmState::None => {}
        DeleteConfirmState::Single {
            name,
            size,
            allocated_size,
            item_count,
            is_protected,
            ..
        } => {
            let title = if *is_protected {
                "Protected System Location"
            } else {
                "Confirm Deletion"
            };

            Window::new(RichText::new(title).strong())
                .collapsible(false)
                .resizable(false)
                .anchor(egui::Align2::CENTER_CENTER, Vec2::ZERO)
                .fixed_size(Vec2::new(480.0, 0.0))
                .show(ctx, |ui| {
                    ui.add_space(6.0);

                    if *is_protected {
                        ui.label(
                            RichText::new(
                                "System-critical location: FreeDataDiet protects operating system files, drive roots, and system volumes from deletion.",
                            )
                            .color(Color32::from_rgb(255, 100, 100))
                            .strong(),
                        );
                        ui.add_space(12.0);
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            if ui.button("Close").clicked() {
                                action = Some(DeleteDialogAction {
                                    confirmed_permanent: false,
                                    confirmed_trash: false,
                                    cancelled: true,
                                });
                            }
                        });
                        return;
                    }

                    ui.label(
                        RichText::new(format!(
                            "Permanently delete \"{}\" to immediately reclaim disk space?",
                            name
                        ))
                        .strong(),
                    );
                    ui.add_space(4.0);

                    let details = if *item_count > 1 {
                        format!(
                            "Size: {} (On Disk: {}) across {} items.",
                            format_size(*size),
                            format_size(*allocated_size),
                            item_count
                        )
                    } else {
                        format!(
                            "Size: {} (On Disk: {}).",
                            format_size(*size),
                            format_size(*allocated_size)
                        )
                    };
                    ui.label(RichText::new(details).small());
                    ui.add_space(14.0);

                    ui.horizontal(|ui| {
                        let trash_btn = ui.add(
                            egui::Button::new(
                                RichText::new("Move to Recycle Bin")
                                    .color(if *focus == DeleteModalFocus::Trash {
                                        Color32::from_rgb(100, 220, 150)
                                    } else {
                                        Color32::WHITE
                                    }),
                            ),
                        ).on_hover_text("Safely moves the item to your system Recycle Bin. Can be restored if needed.");

                        if trash_btn.clicked() {
                            action = Some(DeleteDialogAction {
                                confirmed_permanent: false,
                                confirmed_trash: true,
                                cancelled: false,
                            });
                        }

                        let perm_btn = ui.add(
                            egui::Button::new(
                                RichText::new("Delete Permanently")
                                    .color(Color32::from_rgb(255, 120, 120)),
                            ),
                        ).on_hover_text("Immediately reclaims disk space. This action cannot be undone.");

                        if perm_btn.clicked() {
                            action = Some(DeleteDialogAction {
                                confirmed_permanent: true,
                                confirmed_trash: false,
                                cancelled: false,
                            });
                        }

                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            if ui.button("Cancel").clicked() {
                                action = Some(DeleteDialogAction {
                                    confirmed_permanent: false,
                                    confirmed_trash: false,
                                    cancelled: true,
                                });
                            }
                        });
                    });
                });
        }
        DeleteConfirmState::Batch {
            total_size,
            total_count,
            has_protected,
            ..
        } => {
            Window::new(RichText::new("Confirm Batch Deletion").strong())
                .collapsible(false)
                .resizable(false)
                .anchor(egui::Align2::CENTER_CENTER, Vec2::ZERO)
                .fixed_size(Vec2::new(500.0, 0.0))
                .show(ctx, |ui| {
                    ui.add_space(6.0);

                    ui.label(
                        RichText::new(format!(
                            "Delete {} selected items to reclaim {}?",
                            total_count,
                            format_size(*total_size)
                        ))
                        .strong(),
                    );
                    ui.add_space(4.0);

                    if *has_protected {
                        ui.label(
                            RichText::new(
                                "Notice: Protected system files and drive roots will be automatically skipped for safety.",
                            )
                            .color(Color32::from_rgb(255, 200, 80))
                            .small(),
                        );
                        ui.add_space(4.0);
                    }

                    ui.label(
                        RichText::new(
                            "For duplicate clusters, preserved copies in other folders remain untouched.",
                        )
                        .small(),
                    );
                    ui.add_space(14.0);

                    ui.horizontal(|ui| {
                        let trash_btn = ui.add(
                            egui::Button::new(
                                RichText::new(format!("Move {} Items to Recycle Bin", total_count))
                                    .color(Color32::from_rgb(100, 220, 150)),
                            ),
                        ).on_hover_text("Safely moves selected items to the Recycle Bin.");

                        if trash_btn.clicked() {
                            action = Some(DeleteDialogAction {
                                confirmed_permanent: false,
                                confirmed_trash: true,
                                cancelled: false,
                            });
                        }

                        let perm_btn = ui.add(
                            egui::Button::new(
                                RichText::new("Delete Permanently")
                                    .color(Color32::from_rgb(255, 120, 120)),
                            ),
                        ).on_hover_text("Immediately purges files to reclaim disk space. Cannot be undone.");

                        if perm_btn.clicked() {
                            action = Some(DeleteDialogAction {
                                confirmed_permanent: true,
                                confirmed_trash: false,
                                cancelled: false,
                            });
                        }

                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            if ui.button("Cancel").clicked() {
                                action = Some(DeleteDialogAction {
                                    confirmed_permanent: false,
                                    confirmed_trash: false,
                                    cancelled: true,
                                });
                            }
                        });
                    });
                });
        }
    }

    action
}
