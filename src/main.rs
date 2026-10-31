#![cfg_attr(not(test), windows_subsystem = "windows")]

pub mod app;
pub mod config;
pub mod duplicate;
pub mod locations;
pub mod model;
pub mod platform;
pub mod scanner;
pub mod updater;

use app::FreeDataDietApp;

fn load_app_icon() -> Option<eframe::egui::IconData> {
    let png_bytes = include_bytes!("../assets/icons/app_icon.png");
    let image = image::load_from_memory(png_bytes).ok()?.into_rgba8();
    let (width, height) = image.dimensions();
    Some(eframe::egui::IconData {
        rgba: image.into_raw(),
        width,
        height,
    })
}

fn main() -> eframe::Result<()> {
    let mut viewport = eframe::egui::ViewportBuilder::default()
        .with_inner_size([1120.0, 720.0])
        .with_min_inner_size([800.0, 500.0])
        .with_title("FreeDataDiet - Disk Usage Analyzer");

    if let Some(icon) = load_app_icon() {
        viewport = viewport.with_icon(icon);
    }

    let options = eframe::NativeOptions {
        viewport,
        ..Default::default()
    };

    eframe::run_native(
        "FreeDataDiet",
        options,
        Box::new(|cc| Ok(Box::new(FreeDataDietApp::new(cc)))),
    )
}
