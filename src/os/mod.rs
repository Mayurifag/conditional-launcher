use crate::config::AppConfig;
#[cfg(target_os = "linux")]
use anyhow::Context;
use anyhow::Result;
use std::path::Path;
#[cfg(target_os = "linux")]
use std::process::{Command, Stdio};
use std::time::Duration;
use sysinfo::{Disk, Disks, System};

#[cfg(any(target_os = "linux", target_os = "macos"))]
mod autostart;

#[cfg(target_os = "linux")]
mod linux;

#[cfg(target_os = "windows")]
mod windows;

#[cfg(target_os = "macos")]
mod macos;

#[cfg(not(any(target_os = "linux", target_os = "windows", target_os = "macos")))]
compile_error!("Unsupported platform — only Linux, Windows and macOS are supported");

pub struct PartitionInfo {
    pub mount_point: String,
    pub fs_type: String,
    pub size: String,
}

pub trait OsOperations {
    fn check_internet_connection(&self) -> bool {
        shared_check_internet()
    }
    fn is_partition_mounted(&self, path: &str, disks: &Disks) -> bool {
        shared_is_partition_mounted(path, disks)
    }
    fn launch_app(&self, app: &AppConfig) -> Result<()>;
    fn run_now(&self, app: &AppConfig) -> Result<()> {
        self.launch_app(app)
    }
    fn get_autostart_apps(&self) -> Result<Vec<AppConfig>>;
    fn manage_app(&self, app: &AppConfig) -> Result<()>;
    fn unmanage_app(&self, app: &AppConfig) -> Result<()>;
    fn get_partitions(&self) -> Result<Vec<PartitionInfo>>;
    fn add_self_to_autostart(&self, managed_app_count: usize) -> Result<()>;
    fn remove_self_from_autostart(&self) -> Result<()>;
    fn is_app_running(&self, app: &AppConfig, sys: &System) -> bool {
        shared_is_app_running(app, sys)
    }
    /// Returns raw RGBA pixels and [width, height] for the app's icon, if available.
    fn get_app_icon_rgba(&self, _app: &AppConfig) -> Option<(Vec<u8>, [usize; 2])> {
        None
    }
}

// ── Shared helpers used by every platform ────────────────────────────────────

pub fn shared_check_internet() -> bool {
    reqwest::blocking::Client::builder()
        .timeout(Duration::from_secs(5))
        .build()
        .and_then(|client| {
            client
                .get("http://connectivitycheck.gstatic.com/generate_204")
                .send()
        })
        .map(|r| r.status().is_success())
        .unwrap_or(false)
}

pub fn shared_is_partition_mounted(path: &str, disks: &Disks) -> bool {
    let mount_path = Path::new(path);
    disks.iter().any(|disk| disk.mount_point() == mount_path)
}

pub fn shared_is_app_running(app: &AppConfig, sys: &System) -> bool {
    if let Some(process_name) = command_executable(&app.command)
        .and_then(|p| Path::new(p).file_name())
        .and_then(|f| f.to_str())
        && sys
            .processes_by_name(process_name.as_ref())
            .next()
            .is_some()
    {
        return true;
    }
    sys.processes_by_name(app.name.as_ref()).next().is_some()
        || sys
            .processes_by_name(app.name.to_lowercase().as_ref())
            .next()
            .is_some()
}

fn command_executable(command: &str) -> Option<&str> {
    let command = command.trim_start();
    if let Some(quote @ ('"' | '\'')) = command.chars().next() {
        command[1..]
            .split_once(quote)
            .map(|(executable, _)| executable)
    } else {
        command.split_whitespace().next()
    }
}

fn shared_partition_info(disk: &Disk) -> PartitionInfo {
    PartitionInfo {
        mount_point: disk.mount_point().to_string_lossy().into_owned(),
        fs_type: disk.file_system().to_string_lossy().into_owned(),
        size: format!("{:.1} GB", disk.total_space() as f64 / 1_000_000_000.0),
    }
}

#[cfg(target_os = "linux")]
fn shared_spawn(mut command: Command, app: &AppConfig) -> Result<()> {
    if let Some(dir) = &app.working_dir {
        command.current_dir(dir);
    }
    command
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .with_context(|| format!("Launch {}", app.name))?;
    Ok(())
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
