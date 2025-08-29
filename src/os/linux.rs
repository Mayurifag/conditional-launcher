use super::{OsOperations, PartitionInfo};
use crate::config::AppConfig;
use freedesktop_desktop_entry::DesktopEntry;
use std::env;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use sysinfo::Disks;

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
        })
    }

    fn launcher_desktop_file_path() -> Option<PathBuf> {
        dirs::config_dir().map(|d| d.join("autostart/conditional-launcher.desktop"))
    }
}

impl OsOperations for LinuxOperations {
    fn check_internet_connection(&self) -> bool {
        Command::new("ping")
            .args(["-c", "1", "8.8.8.8"])
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()
            .map_or(false, |s| s.success())
    }

    fn is_partition_mounted(&self, path: &str) -> bool {
        // Use the `Disks` struct from `sysinfo` for a reliable check.
        let disks = Disks::new_with_refreshed_list();
        let mount_path = Path::new(path);
        disks.iter().any(|disk| disk.mount_point() == mount_path)
    }

    fn launch_app(&self, app: &AppConfig) {
        let mut cmd = Command::new("sh");
        cmd.arg("-c").arg(&app.command);
        if let Some(dir) = &app.working_dir {
            cmd.current_dir(dir);
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
        let disks = Disks::new_with_refreshed_list();
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
            let label = disk.name().to_string_lossy().to_string();
            let size_bytes = disk.total_space();
            let size = format!("{:.1} GB", size_bytes as f64 / 1_000_000_000.0);
            let fs_type = fs_type_str.to_string();

            partitions.push(PartitionInfo {
                mount_point,
                label,
                fs_type,
                size,
            });
        }
        partitions
    }

    fn open_config_dir(&self) {
        if let Some(config_dir) = dirs::config_dir() {
            let path = config_dir.join("conditional-launcher");
            if fs::create_dir_all(&path).is_ok() {
                let _ = Command::new("xdg-open").arg(path).spawn();
            }
        }
    }

    fn add_self_to_autostart(&self) {
        if let (Some(path), Ok(exe_path)) = (Self::launcher_desktop_file_path(), env::current_exe())
        {
            if let Some(parent) = path.parent() {
                fs::create_dir_all(parent).ok();
            }
            let content = format!(
                "[Desktop Entry]\n\
                 Name=Conditional Launcher\n\
                 Exec=\"{}\" --hidden\n\
                 Type=Application\n\
                 Terminal=false\n",
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
}
