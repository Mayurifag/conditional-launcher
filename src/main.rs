#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod app;
mod config;
mod os;

use crate::os::get_os_operations;
use eframe::egui;

fn main() -> Result<(), eframe::Error> {
    let args: Vec<String> = std::env::args().collect();

    if args.contains(&"--hidden".to_string()) {
        let os_ops = get_os_operations();
        let mut managed_apps = app::ConditionalLauncherApp::load_config();
        app::perform_launch_checks(os_ops.as_ref(), &mut managed_apps);
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
