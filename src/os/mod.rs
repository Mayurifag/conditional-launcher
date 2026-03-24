use crate::config::AppConfig;
use std::path::Path;
use sysinfo::{Disks, System};

#[cfg(target_os = "linux")]
pub mod linux;

#[cfg(target_os = "windows")]
pub mod windows;

#[cfg(target_os = "macos")]
pub mod macos;

#[cfg(not(any(target_os = "linux", target_os = "windows", target_os = "macos")))]
compile_error!("Unsupported platform — only Linux, Windows and macOS are supported");

#[derive(Clone, Debug, Default)]
pub struct PartitionInfo {
    pub mount_point: String,
    pub fs_type: String,
    pub size: String,
}

pub trait OsOperations {
    fn check_internet_connection(&self) -> bool;
    fn is_partition_mounted(&self, path: &str, disks: &Disks) -> bool;
    fn launch_app(&self, app: &AppConfig);
    fn get_autostart_apps(&self) -> Vec<AppConfig>;
    fn manage_app(&self, app: &AppConfig) -> bool;
    fn unmanage_app(&self, app: &AppConfig) -> bool;
    fn get_partitions(&self) -> Vec<PartitionInfo>;
    fn add_self_to_autostart(&self, managed_app_count: usize);
    fn remove_self_from_autostart(&self);
    fn is_app_running(&self, app: &AppConfig, sys: &System) -> bool;
    /// Returns raw RGBA pixels and [width, height] for the app's icon, if available.
    fn get_app_icon_rgba(&self, app: &AppConfig) -> Option<(Vec<u8>, [usize; 2])>;
}

// ── Shared helpers used by every platform ────────────────────────────────────

pub fn shared_check_internet() -> bool {
    reqwest::blocking::get("http://connectivitycheck.gstatic.com/generate_204")
        .map(|r| r.status().is_success())
        .unwrap_or(false)
}

pub fn shared_is_partition_mounted(path: &str, disks: &Disks) -> bool {
    let mount_path = Path::new(path);
    disks.iter().any(|disk| disk.mount_point() == mount_path)
}

pub fn shared_is_app_running(app: &AppConfig, sys: &System) -> bool {
    if let Some(process_name) = app
        .command
        .split_whitespace()
        .next()
        .and_then(|p| Path::new(p).file_name())
        .and_then(|f| f.to_str())
        && sys
            .processes_by_name(process_name.as_ref())
            .next()
            .is_some()
    {
        return true;
    }
    if sys.processes_by_name(app.name.as_ref()).next().is_some() {
        return true;
    }
    if sys
        .processes_by_name(app.name.to_lowercase().as_ref())
        .next()
        .is_some()
    {
        return true;
    }
    false
}

// ── Platform dispatch ─────────────────────────────────────────────────────────

#[cfg(target_os = "linux")]
pub fn get_os_operations() -> Box<dyn OsOperations> {
    Box::new(linux::LinuxOperations)
}

#[cfg(target_os = "windows")]
pub fn get_os_operations() -> Box<dyn OsOperations> {
    Box::new(windows::WindowsOperations)
}

#[cfg(target_os = "macos")]
pub fn get_os_operations() -> Box<dyn OsOperations> {
    Box::new(macos::MacosOperations)
}
