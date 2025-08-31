use super::{OsOperations, PartitionInfo};
use crate::config::AppConfig;
use freedesktop_desktop_entry::DesktopEntry;
use notify_rust::Notification;
use reqwest;
use std::env;
use std::fs;
use std::os::unix::process::CommandExt;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use sysinfo::{Disks, System};

pub struct LinuxOperations;

impl LinuxOperations {
    fn parse_desktop_file(path: PathBuf) -> Option<AppConfig> {
        let entry = DesktopEntry::from_path(&path, Some(&[] as &[&str])).ok()?;
        let name = entry.name(&[] as &[&str]).map(|s| s.to_string())?;
        let command = entry.exec().map(|s| s.to_string())?;

        Some(AppConfig {
            name,
            command,
            icon: entry.icon().map(|s| s.to_string()),
            working_dir: entry.path().map(|s| PathBuf::from(s.to_string())),
            original_path: Some(path),
            conditions: Default::default(),
            launched: false,
            is_managed: false,
        })
    }

    fn launcher_desktop_file_path() -> Option<PathBuf> {
        dirs::config_dir().map(|d| d.join("autostart/conditional-launcher.desktop"))
    }
}

impl OsOperations for LinuxOperations {
    fn check_internet_connection(&self) -> bool {
        match reqwest::blocking::get("http://connectivitycheck.gstatic.com/generate_204") {
            Ok(response) => response.status().is_success(),
            Err(_) => false,
        }
    }

    fn is_partition_mounted(&self, path: &str, disks: &Disks) -> bool {
        let mount_path = Path::new(path);
        disks.iter().any(|disk| disk.mount_point() == mount_path)
    }

    fn launch_app(&self, app: &AppConfig) {
        let mut cmd = Command::new("sh");
        cmd.arg("-c").arg(&app.command);
        if let Some(dir) = &app.working_dir {
            cmd.current_dir(dir);
        }

        unsafe {
            cmd.pre_exec(|| {
                if libc::setsid() == -1 {
                    Err(std::io::Error::last_os_error())
                } else {
                    Ok(())
                }
            });
        }

        let _ = cmd.stdout(Stdio::null()).stderr(Stdio::null()).spawn();
    }

    fn get_autostart_apps(&self) -> Vec<AppConfig> {
        let mut apps = Vec::new();
        if let Some(config_dir) = dirs::config_dir() {
            let autostart_dir = config_dir.join("autostart");
            if let Ok(entries) = fs::read_dir(autostart_dir) {
                for entry in entries.filter_map(Result::ok) {
                    let path = entry.path();
                    if let Some(file_name) = path.file_name().and_then(|n| n.to_str()) {
                        if file_name == "conditional-launcher.desktop" {
                            continue;
                        }
                    }
                    if path.extension().map_or(false, |e| e == "desktop") {
                        if let Some(app_config) = Self::parse_desktop_file(path) {
                            apps.push(app_config);
                        }
                    }
                }
            }
        }
        apps
    }

    fn manage_app(&self, app: &AppConfig) -> bool {
        if let Some(path) = &app.original_path {
            let mut disabled_path = path.as_os_str().to_owned();
            disabled_path.push(".disabled");
            return fs::rename(path, PathBuf::from(disabled_path)).is_ok();
        }
        false
    }

    fn unmanage_app(&self, app: &AppConfig) -> bool {
        if let Some(path) = &app.original_path {
            let mut disabled_path = path.as_os_str().to_owned();
            disabled_path.push(".disabled");
            return fs::rename(PathBuf::from(disabled_path), path).is_ok();
        }
        false
    }

    fn get_partitions(&self) -> Vec<PartitionInfo> {
        let mut disks = Disks::new();
        disks.refresh(true);
        let mut partitions = Vec::new();
        for disk in disks.iter() {
            let mount_point_str = disk.mount_point().to_string_lossy();
            if !mount_point_str.starts_with('/') {
                continue; // Skip non-standard mount points
            }

            let fs_type_str = disk.file_system().to_string_lossy();

            // Filter out unwanted virtual/temporary filesystems by name
            if fs_type_str.starts_with("squashfs")
                || fs_type_str.starts_with("overlay")
                || fs_type_str.starts_with("tmpfs")
                || fs_type_str.starts_with("devtmpfs")
                || fs_type_str.starts_with("fuse.")
            {
                continue;
            }

            let mount_point = mount_point_str.to_string();
            let size_bytes = disk.total_space();
            let size = format!("{:.1} GB", size_bytes as f64 / 1_000_000_000.0);
            let fs_type = fs_type_str.to_string();

            partitions.push(PartitionInfo {
                mount_point,
                fs_type,
                size,
            });
        }
        partitions
    }

    fn add_self_to_autostart(&self, managed_app_count: usize) {
        if let (Some(path), Ok(exe_path)) = (Self::launcher_desktop_file_path(), env::current_exe())
        {
            if let Some(parent) = path.parent() {
                fs::create_dir_all(parent).ok();
            }
            let name = format!("Conditional launch {} apps", managed_app_count);
            let content = format!(
                "[Desktop Entry]\n\
                 Name={}\n\
                 Exec=\"{}\" --hidden\n\
                 Type=Application\n\
                 Terminal=false\n",
                name,
                exe_path.display()
            );
            fs::write(path, content).ok();
        }
    }

    fn remove_self_from_autostart(&self) {
        if let Some(path) = Self::launcher_desktop_file_path() {
            if path.exists() {
                fs::remove_file(path).ok();
            }
        }
    }

    fn is_app_running(&self, app: &AppConfig, sys: &System) -> bool {
        if let Some(process_name) = app
            .command
            .split_whitespace()
            .next()
            .and_then(|p| Path::new(p).file_name())
            .and_then(|f| f.to_str())
        {
            if sys
                .processes_by_name(process_name.as_ref())
                .next()
                .is_some()
            {
                return true;
            }
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

    fn send_exit_notification(&self, launched_apps: &[String]) {
        if launched_apps.is_empty() {
            return;
        }
        let body = format!("Launched: {}", launched_apps.join(", "));
        let _ = Notification::new()
            .summary("Conditional Launcher Finished")
            .body(&body)
            .show();
    }
}
