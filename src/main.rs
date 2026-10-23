#![windows_subsystem = "windows"]

mod app;
mod model;
mod platform;
mod scanner;

use app::FreeDataDietApp;

fn main() -> eframe::Result<()> {
    let options = eframe::NativeOptions {
        viewport: eframe::egui::ViewportBuilder::default()
            .with_inner_size([1040.0, 700.0])
            .with_min_inner_size([780.0, 480.0])
            .with_title("FreeDataDiet - Disk Usage Analyzer"),
        ..Default::default()
    };

    eframe::run_native(
        "FreeDataDiet",
        options,
        Box::new(|cc| Ok(Box::new(FreeDataDietApp::new(cc)))),
    )
}
