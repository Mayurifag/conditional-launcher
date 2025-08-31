#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod app;
mod config;
mod os;

use eframe::egui;

fn main() -> Result<(), eframe::Error> {
    let args: Vec<String> = std::env::args().collect();

    if args.contains(&"--hidden".to_string()) {
        app::run_hidden_process();
        Ok(())
    } else {
        let options = eframe::NativeOptions {
            viewport: egui::ViewportBuilder::default()
                .with_inner_size([550.0, 450.0])
                .with_resizable(false),
            ..Default::default()
        };

        eframe::run_native(
            "Conditional Launcher",
            options,
            Box::new(|cc| Ok(Box::new(app::ConditionalLauncherApp::new(cc)))),
        )
    }
}
