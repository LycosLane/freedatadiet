#![cfg_attr(not(test), windows_subsystem = "windows")]

pub mod config;
pub mod duplicate;
pub mod locations;
pub mod model;
pub mod platform;
pub mod scanner;
pub mod ui;
pub mod updater;

use ui::FreeDataDietApp;

fn main() -> eframe::Result<()> {
    let options = eframe::NativeOptions {
        viewport: eframe::egui::ViewportBuilder::default()
            .with_inner_size([1120.0, 720.0])
            .with_min_inner_size([800.0, 500.0])
            .with_title("FreeDataDiet - Disk Usage Analyzer"),
        ..Default::default()
    };

    eframe::run_native(
        "FreeDataDiet",
        options,
        Box::new(|cc| Ok(Box::new(FreeDataDietApp::new(cc)))),
    )
}
