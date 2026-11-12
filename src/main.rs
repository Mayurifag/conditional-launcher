#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod app;
mod conditions;
mod config;
mod daemon;
mod gui;
mod os;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    if std::env::args_os().any(|arg| arg == "--hidden") {
        daemon::run().map_err(Into::into)
    } else {
        gui::run()
    }
}
