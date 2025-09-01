#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod app;
mod config;
mod daemon;
mod gui;
mod os;

use eframe::egui;

fn main() -> Result<(), eframe::Error> {
    let args: Vec<String> = std::env::args().collect();

    if args.contains(&"--hidden".to_string()) {
        daemon::run_hidden_process();
        Ok(())
    } else {
        let os_ops = os::get_os_operations();
        let apps = app::load_all_apps(os_ops.as_ref());

        const HEIGHT_PER_APP: f32 = 95.0;
        const PADDING: f32 = 30.0;
        const MIN_HEIGHT: f32 = 150.0;
        const MAX_HEIGHT: f32 = 700.0;

        let height = if apps.is_empty() {
            MIN_HEIGHT
        } else {
            (apps.len() as f32 * HEIGHT_PER_APP + PADDING).min(MAX_HEIGHT)
        };

        let options = eframe::NativeOptions {
            viewport: egui::ViewportBuilder::default()
                .with_inner_size([550.0, height])
                .with_resizable(true),
            ..Default::default()
        };

        eframe::run_native(
            "Conditional Launcher",
            options,
            Box::new(|cc| Ok(Box::new(gui::GuiApp::new(cc, apps)))),
        )
    }
}
