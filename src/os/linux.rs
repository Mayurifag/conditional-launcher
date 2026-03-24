use super::{OsOperations, PartitionInfo};
use crate::config::AppConfig;
use freedesktop_desktop_entry::DesktopEntry;
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

    fn backup_directory() -> Option<PathBuf> {
        dirs::config_dir().map(|d| d.join("conditional-launcher/desktop-backups"))
    }

    fn get_backup_path(original_path: &Path) -> Option<PathBuf> {
        let backup_dir = Self::backup_directory()?;
        let file_name = original_path.file_name()?;
        Some(backup_dir.join(file_name))
    }

    fn create_placeholder_desktop_file(
        original_path: &Path,
        app_config: &AppConfig,
    ) -> Result<(), std::io::Error> {
        let mut content = format!(
            "[Desktop Entry]\n\
             Name={} (Managed by Conditional Launcher)\n\
             Comment=This application is temporarily managed by Conditional Launcher\n\
             Exec=/bin/true\n\
             Type=Application\n\
             Terminal=false\n\
             NoDisplay=true\n",
            app_config.name
        );
        if let Some(icon) = &app_config.icon {
            content.push_str(&format!("Icon={icon}\n"));
        }
        fs::write(original_path, content)
    }

    fn is_placeholder_file(path: &Path) -> bool {
        fs::read_to_string(path)
            .map(|c| c.contains("Managed by Conditional Launcher"))
            .unwrap_or(false)
    }
}

impl OsOperations for LinuxOperations {
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
                    if path
                        .file_name()
                        .and_then(|n| n.to_str())
                        .is_some_and(|n| n == "conditional-launcher.desktop")
                    {
                        continue;
                    }
                    if path.extension().is_some_and(|e| e == "desktop") {
                        if Self::is_placeholder_file(&path) {
                            continue;
                        }
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
        if let Some(original_path) = &app.original_path {
            if let Some(backup_path) = Self::get_backup_path(original_path) {
                if let Some(backup_dir) = Self::backup_directory() {
                    if fs::create_dir_all(&backup_dir).is_err() {
                        return false;
                    }
                }
                if fs::rename(original_path, &backup_path).is_ok() {
                    if Self::create_placeholder_desktop_file(original_path, app).is_ok() {
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
        let mut partitions = Vec::new();
        for disk in disks.iter() {
            let mount_point_str = disk.mount_point().to_string_lossy();
            if !mount_point_str.starts_with('/') {
                continue;
            }
            let fs_type_str = disk.file_system().to_string_lossy();
            if matches!(
                fs_type_str.as_ref(),
                s if s.starts_with("squashfs")
                    || s.starts_with("overlay")
                    || s.starts_with("tmpfs")
                    || s.starts_with("devtmpfs")
                    || s.starts_with("fuse.")
            ) {
                continue;
            }
            partitions.push(PartitionInfo {
                mount_point: mount_point_str.to_string(),
                fs_type: fs_type_str.to_string(),
                size: format!("{:.1} GB", disk.total_space() as f64 / 1_000_000_000.0),
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
            let content = format!(
                "[Desktop Entry]\n\
                 Name=Conditional launch {managed_app_count} apps\n\
                 Exec=\"{}\" --hidden\n\
                 Icon=conditional-launcher\n\
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

    fn is_app_running(&self, app: &AppConfig, sys: &System) -> bool {
        super::shared_is_app_running(app, sys)
    }

    fn get_app_icon_rgba(&self, app: &AppConfig) -> Option<(Vec<u8>, [usize; 2])> {
        let icon_name = app.icon.as_deref()?;
        let path = freedesktop_icons::lookup(icon_name).with_size(32).find()?;
        let image_data = std::fs::read(&path).ok()?;

        if path.extension().is_some_and(|e| e == "svg") {
            let rtree = usvg::Tree::from_data(&image_data, &usvg::Options::default()).ok()?;
            let svg_size = rtree.size();
            let (w, h) = (32u32, 32u32);
            let mut pixmap = resvg::tiny_skia::Pixmap::new(w, h)?;
            let transform = resvg::tiny_skia::Transform::from_scale(
                w as f32 / svg_size.width(),
                h as f32 / svg_size.height(),
            );
            resvg::render(&rtree, transform, &mut pixmap.as_mut());
            Some((
                pixmap.data().to_vec(),
                [pixmap.width() as usize, pixmap.height() as usize],
            ))
        } else {
            let img = image::load_from_memory(&image_data).ok()?;
            let rgba = img.to_rgba8();
            let (w, h) = (img.width() as usize, img.height() as usize);
            Some((rgba.into_raw(), [w, h]))
        }
    }
}
