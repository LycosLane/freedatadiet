use eframe::egui::{self, Color32, RichText, Vec2, Window};

pub fn render_shortcuts_dialog(ctx: &egui::Context, is_open: &mut bool) {
    let mut close_requested = false;

    Window::new("Keyboard Shortcuts Cheat Sheet")
        .open(is_open)
        .collapsible(false)
        .resizable(false)
        .default_size(Vec2::new(540.0, 480.0))
        .show(ctx, |ui| {
            ui.add_space(4.0);
            ui.label(RichText::new("Quick Navigation & Selection:").strong());
            egui::Grid::new("shortcuts_grid_nav")
                .striped(true)
                .spacing([20.0, 6.0])
                .show(ui, |ui| {
                    ui.label(RichText::new("Up / Down").strong().color(Color32::from_rgb(100, 200, 255)));
                    ui.label("Move focus up or down table rows");
                    ui.end_row();

                    ui.label(RichText::new("Shift + Up / Down").strong().color(Color32::from_rgb(100, 200, 255)));
                    ui.label("Expand contiguous selection range");
                    ui.end_row();

                    ui.label(RichText::new("Space").strong().color(Color32::from_rgb(100, 200, 255)));
                    ui.label("Toggle checkbox selection on focused item");
                    ui.end_row();

                    ui.label(RichText::new("Ctrl + A").strong().color(Color32::from_rgb(100, 200, 255)));
                    ui.label("Select all visible items matching current filters");
                    ui.end_row();

                    ui.label(RichText::new("Esc").strong().color(Color32::from_rgb(100, 200, 255)));
                    ui.label("Clear selection or close active modal");
                    ui.end_row();

                    ui.label(RichText::new("Enter").strong().color(Color32::from_rgb(100, 200, 255)));
                    ui.label("Open folder or launch file");
                    ui.end_row();

                    ui.label(RichText::new("Backspace / Left").strong().color(Color32::from_rgb(100, 200, 255)));
                    ui.label("Navigate up to parent directory");
                    ui.end_row();
                });

            ui.add_space(14.0);
            ui.label(RichText::new("Feature Function Keys & Actions:").strong());
            egui::Grid::new("shortcuts_grid_features")
                .striped(true)
                .spacing([20.0, 6.0])
                .show(ui, |ui| {
                    ui.label(RichText::new("F2").strong().color(Color32::from_rgb(100, 200, 255)));
                    ui.label("Toggle search mode (Jump-to-Name vs Live Filter)");
                    ui.end_row();

                    ui.label(RichText::new("F3 / Ctrl + F").strong().color(Color32::from_rgb(100, 200, 255)));
                    ui.label("Focus search / filter input field");
                    ui.end_row();

                    ui.label(RichText::new("F4").strong().color(Color32::from_rgb(100, 200, 255)));
                    ui.label("Toggle left sidebar (Favorites, Drives, Quick Views)");
                    ui.end_row();

                    ui.label(RichText::new("F5").strong().color(Color32::from_rgb(100, 200, 255)));
                    ui.label("Rescan and refresh current directory");
                    ui.end_row();

                    ui.label(RichText::new("F6").strong().color(Color32::from_rgb(100, 200, 255)));
                    ui.label("Toggle recursive subfolder flattening view");
                    ui.end_row();

                    ui.label(RichText::new("Ctrl + D").strong().color(Color32::from_rgb(100, 200, 255)));
                    ui.label("Toggle 'Show Duplicates Only' mode");
                    ui.end_row();

                    ui.label(RichText::new("Ctrl + Shift + D").strong().color(Color32::from_rgb(100, 200, 255)));
                    ui.label("Duplicate Assistant: Select all sibling duplicates in active folder");
                    ui.end_row();

                    ui.label(RichText::new("Ctrl + B").strong().color(Color32::from_rgb(100, 200, 255)));
                    ui.label("Bookmark current folder to Favorites");
                    ui.end_row();

                    ui.label(RichText::new("Del").strong().color(Color32::from_rgb(100, 220, 150)));
                    ui.label("Move selected items to Recycle Bin");
                    ui.end_row();

                    ui.label(RichText::new("Shift + Del").strong().color(Color32::from_rgb(255, 120, 120)));
                    ui.label("Permanently delete selected items (with confirmation)");
                    ui.end_row();
                });

            ui.add_space(14.0);
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if ui.button("Close").clicked() {
                    close_requested = true;
                }
            });
        });

    if close_requested {
        *is_open = false;
    }
}
