use super::{OsOperations, PartitionInfo};
use crate::config::AppConfig;
use freedesktop_desktop_entry::DesktopEntry;
use std::collections::HashMap;
use std::convert::TryInto;
use std::fs;
use std::path::PathBuf;
use std::process::{Command, Stdio};
use zbus::blocking::Connection;

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
}

impl OsOperations for LinuxOperations {
    fn check_internet_connection(&self) -> bool {
        Command::new("ping")
            .args(["-c", "1", "8.8.8.8"])
            .output()
            .map_or(false, |o| o.status.success())
    }

    fn is_partition_mounted(&self, path: &str) -> bool {
        fs::read_to_string("/proc/mounts").map_or(false, |m| m.lines().any(|l| l.contains(path)))
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
        let connection = match Connection::system() {
            Ok(c) => c,
            Err(_) => return vec![],
        };

        let proxy = match zbus::blocking::Proxy::new(
            &connection,
            "org.freedesktop.UDisks2",
            "/org/freedesktop/UDisks2",
            "org.freedesktop.DBus.ObjectManager",
        ) {
            Ok(p) => p,
            Err(_) => return vec![],
        };

        type ManagedObjects = HashMap<
            zbus::zvariant::OwnedObjectPath,
            HashMap<String, HashMap<String, zbus::zvariant::OwnedValue>>,
        >;
        let objects: ManagedObjects = match proxy.call_method("GetManagedObjects", &()) {
            Ok(msg) => msg.body().deserialize().unwrap_or_default(),
            Err(_) => return vec![],
        };

        let mut partitions = Vec::new();
        for (_, interfaces) in objects {
            if let (Some(fs_props), Some(block_props)) = (
                interfaces.get("org.freedesktop.UDisks2.Filesystem"),
                interfaces.get("org.freedesktop.UDisks2.Block"),
            ) {
                if let Some(mount_points_value) = fs_props.get("MountPoints") {
                    if let Ok(mount_points) =
                        TryInto::<Vec<Vec<u8>>>::try_into(mount_points_value.clone())
                    {
                        if let Some(bytes) = mount_points.get(0) {
                            let mount_point = String::from_utf8_lossy(bytes).to_string();
                            if mount_point.is_empty() {
                                continue;
                            }

                            let label: String = block_props
                                .get("IdLabel")
                                .and_then(|v| v.clone().try_into().ok())
                                .unwrap_or_default();
                            let size_bytes: u64 = block_props
                                .get("Size")
                                .and_then(|v| v.clone().try_into().ok())
                                .unwrap_or(0);
                            let size = format!("{:.1} GB", size_bytes as f64 / 1_000_000_000.0);
                            let fs_type: String = fs_props
                                .get("IdType")
                                .and_then(|v| v.clone().try_into().ok())
                                .unwrap_or_default();

                            partitions.push(PartitionInfo {
                                mount_point,
                                label,
                                fs_type,
                                size,
                            });
                        }
                    }
                }
            }
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
}
