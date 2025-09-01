#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod app;
mod config;
mod daemon;
mod gui;
mod os;

fn main() {
    let args: Vec<String> = std::env::args().collect();

    if args.contains(&"--hidden".to_string()) {
        daemon::run_hidden_process();
    } else {
        let os_ops = os::get_os_operations();
        let apps = app::load_all_apps(os_ops.as_ref());
        let mut app = app::ConditionalLauncherApp::new(apps);
        let _ = gui::run(&mut app);
    }
}
