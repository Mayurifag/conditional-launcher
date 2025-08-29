#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod app;
mod config;
mod os;

use eframe::egui;

fn main() -> Result<(), eframe::Error> {
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([550.0, 450.0]) // Reduced height
            .with_resizable(false),
        ..Default::default()
    };

    eframe::run_native(
        "Conditional Launcher",
        options,
        Box::new(|cc| Ok(Box::new(app::ConditionalLauncherApp::new(cc)))),
    )
}
