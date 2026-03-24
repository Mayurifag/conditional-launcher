use super::{OsOperations, PartitionInfo};
use crate::config::AppConfig;
use std::env;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use sysinfo::{Disks, System};

pub struct MacosOperations;

const LAUNCHER_PLIST: &str = "com.conditional-launcher.plist";

impl MacosOperations {
    fn launch_agents_dir() -> Option<PathBuf> {
        dirs::home_dir().map(|h| h.join("Library/LaunchAgents"))
    }

    fn launcher_plist_path() -> Option<PathBuf> {
        Self::launch_agents_dir().map(|d| d.join(LAUNCHER_PLIST))
    }

    fn backup_directory() -> Option<PathBuf> {
        dirs::config_dir().map(|d| d.join("conditional-launcher/plist-backups"))
    }

    fn get_backup_path(original_path: &Path) -> Option<PathBuf> {
        let backup_dir = Self::backup_directory()?;
        let file_name = original_path.file_name()?;
        Some(backup_dir.join(file_name))
    }

    fn parse_plist_app(path: &Path) -> Option<AppConfig> {
        let value: plist::Value = plist::from_file(path).ok()?;
        let dict = value.as_dictionary()?;

        // label is used as app name
        let label = dict
            .get("Label")
            .and_then(|v| v.as_string())
            .map(|s| s.to_string())?;

        // command comes from ProgramArguments or Program
        let command = if let Some(args) = dict.get("ProgramArguments").and_then(|v| v.as_array()) {
            args.iter()
                .filter_map(|a| a.as_string())
                .collect::<Vec<_>>()
                .join(" ")
        } else if let Some(prog) = dict.get("Program").and_then(|v| v.as_string()) {
            prog.to_string()
        } else {
            return None;
        };

        if command.is_empty() {
            return None;
        }

        Some(AppConfig {
            name: label,
            command,
            original_path: Some(path.to_path_buf()),
            ..Default::default()
        })
    }

    fn is_placeholder_plist(path: &Path) -> bool {
        fs::read_to_string(path)
            .map(|c| c.contains("ManagedByConditionalLauncher"))
            .unwrap_or(false)
    }

    fn write_placeholder_plist(path: &Path, app: &AppConfig) -> std::io::Result<()> {
        let content = format!(
            "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n\
             <!DOCTYPE plist PUBLIC \"-//Apple//DTD PLIST 1.0//EN\" \
             \"http://www.apple.com/DTDs/PropertyList-1.0.dtd\">\n\
             <plist version=\"1.0\">\n\
             <dict>\n\
               <key>Label</key><string>{}</string>\n\
               <!-- ManagedByConditionalLauncher -->\n\
               <key>ProgramArguments</key>\n\
               <array><string>/usr/bin/true</string></array>\n\
               <key>RunAtLoad</key><false/>\n\
             </dict>\n\
             </plist>\n",
            app.name
        );
        fs::write(path, content)
    }
}

impl OsOperations for MacosOperations {
    fn check_internet_connection(&self) -> bool {
        super::shared_check_internet()
    }

    fn is_partition_mounted(&self, path: &str, disks: &Disks) -> bool {
        super::shared_is_partition_mounted(path, disks)
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
        if let Some(dir) = Self::launch_agents_dir() {
            if let Ok(entries) = fs::read_dir(&dir) {
                for entry in entries.filter_map(Result::ok) {
                    let path = entry.path();
                    if path
                        .file_name()
                        .and_then(|n| n.to_str())
                        .is_some_and(|n| n == LAUNCHER_PLIST)
                    {
                        continue;
                    }
                    if path.extension().is_some_and(|e| e == "plist") {
                        if Self::is_placeholder_plist(&path) {
                            continue;
                        }
                        if let Some(app) = Self::parse_plist_app(&path) {
                            apps.push(app);
                        }
                    }
                }
            }
        }
        apps
    }

    fn manage_app(&self, app: &AppConfig) -> bool {
        if let Some(original_path) = &app.original_path {
            if let Some(backup_path) = Self::get_backup_path(original_path) {
                if let Some(backup_dir) = Self::backup_directory() {
                    if fs::create_dir_all(&backup_dir).is_err() {
                        return false;
                    }
                }
                if fs::rename(original_path, &backup_path).is_ok() {
                    if Self::write_placeholder_plist(original_path, app).is_ok() {
                        return true;
                    } else {
                        let _ = fs::rename(&backup_path, original_path);
                    }
                }
            }
        }
        false
    }

    fn unmanage_app(&self, app: &AppConfig) -> bool {
        if let Some(original_path) = &app.original_path {
            if let Some(backup_path) = Self::get_backup_path(original_path) {
                if backup_path.exists() {
                    let _ = fs::remove_file(original_path);
                    return fs::rename(&backup_path, original_path).is_ok();
                }
            }
        }
        false
    }

    fn get_partitions(&self) -> Vec<PartitionInfo> {
        let disks = Disks::new_with_refreshed_list();
        disks
            .iter()
            .filter(|disk| {
                let fs = disk.file_system().to_string_lossy();
                !matches!(fs.as_ref(), "devfs" | "autofs" | "tmpfs" | "map" | "nullfs")
                    && !fs.starts_with("map ")
            })
            .map(|disk| PartitionInfo {
                mount_point: disk.mount_point().to_string_lossy().to_string(),
                fs_type: disk.file_system().to_string_lossy().to_string(),
                size: format!("{:.1} GB", disk.total_space() as f64 / 1_000_000_000.0),
            })
            .collect()
    }

    fn add_self_to_autostart(&self, managed_app_count: usize) {
        if let (Some(plist_path), Ok(exe_path)) = (Self::launcher_plist_path(), env::current_exe())
        {
            if let Some(parent) = plist_path.parent() {
                fs::create_dir_all(parent).ok();
            }
            let content = format!(
                "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n\
                 <!DOCTYPE plist PUBLIC \"-//Apple//DTD PLIST 1.0//EN\" \
                 \"http://www.apple.com/DTDs/PropertyList-1.0.dtd\">\n\
                 <plist version=\"1.0\">\n\
                 <dict>\n\
                   <key>Label</key><string>com.conditional-launcher</string>\n\
                   <key>ProgramArguments</key>\n\
                   <array>\n\
                     <string>{}</string>\n\
                     <string>--hidden</string>\n\
                   </array>\n\
                   <!-- manages {} apps -->\n\
                   <key>RunAtLoad</key><true/>\n\
                 </dict>\n\
                 </plist>\n",
                exe_path.display(),
                managed_app_count
            );
            fs::write(plist_path, content).ok();
        }
    }

    fn remove_self_from_autostart(&self) {
        if let Some(path) = Self::launcher_plist_path() {
            if path.exists() {
                fs::remove_file(path).ok();
            }
        }
    }

    fn is_app_running(&self, app: &AppConfig, sys: &System) -> bool {
        super::shared_is_app_running(app, sys)
    }

    fn get_app_icon_rgba(&self, _app: &AppConfig) -> Option<(Vec<u8>, [usize; 2])> {
        None
    }
}
