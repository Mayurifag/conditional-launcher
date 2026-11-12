use super::{OsOperations, PartitionInfo};
use crate::config::AppConfig;
use anyhow::{Context, Result, ensure};
use freedesktop_desktop_entry::DesktopEntry;
use std::env;
use std::fs;
use std::os::unix::process::CommandExt;
use std::path::{Path, PathBuf};
use std::process::Command;
use sysinfo::Disks;

pub struct LinuxOperations;

impl LinuxOperations {
    fn parse_desktop_file(path: PathBuf) -> Option<AppConfig> {
        let mut entry = DesktopEntry::from_path(&path, Some(&[] as &[&str])).ok()?;
        let name = entry.name(&[] as &[&str]).map(|s| s.to_string())?;
        let icon = entry.icon().map(str::to_owned);
        let mut args = entry.parse_exec().ok()?;
        if let Some(icon) = &icon
            && entry.exec().is_some_and(|exec| exec.contains("%i"))
        {
            // The parser omits --icon; compare a second expansion to identify %i arguments.
            entry.add_desktop_entry("Icon".into(), format!("{icon}_"));
            let marked_args = entry.parse_exec().ok()?;
            if args.len() != marked_args.len() {
                return None;
            }
            let mut corrected = Vec::new();
            for (arg, marked) in args.into_iter().zip(marked_args) {
                if arg == marked {
                    corrected.push(arg);
                } else if !icon.is_empty() {
                    corrected.push("--icon".into());
                    corrected.push(arg);
                }
            }
            args = corrected;
        }
        let command = shlex::try_join(args.iter().map(String::as_str)).ok()?;

        Some(AppConfig {
            name,
            command,
            icon,
            working_dir: entry.path().map(PathBuf::from),
            original_path: Some(path),
            ..Default::default()
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

    fn placeholder_desktop_file(app_config: &AppConfig) -> String {
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
        content
    }

    fn is_placeholder_file(path: &Path) -> bool {
        fs::read_to_string(path)
            .map(|c| c.contains("Managed by Conditional Launcher"))
            .unwrap_or(false)
    }
}

impl OsOperations for LinuxOperations {
    fn launch_app(&self, app: &AppConfig) -> Result<()> {
        ensure!(!app.command.trim().is_empty(), "Empty command");
        let mut cmd = Command::new("sh");
        cmd.arg("-c").arg(&app.command);
        // SAFETY: the pre-exec closure calls only the async-signal-safe setsid
        // syscall and constructs an OS error; it neither allocates nor takes locks.
        unsafe {
            cmd.pre_exec(|| {
                if libc::setsid() == -1 {
                    Err(std::io::Error::last_os_error())
                } else {
                    Ok(())
                }
            });
        }
        super::shared_spawn(cmd, app)
    }

    fn get_autostart_apps(&self) -> Result<Vec<AppConfig>> {
        let mut apps = Vec::new();
        let dir = dirs::config_dir()
            .context("Cannot find the configuration directory")?
            .join("autostart");
        let entries = match fs::read_dir(&dir) {
            Ok(entries) => entries,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(apps),
            Err(error) => return Err(error).with_context(|| format!("Read {}", dir.display())),
        };
        for entry in entries {
            let path = entry
                .with_context(|| format!("Read an entry in {}", dir.display()))?
                .path();
            if path
                .file_name()
                .is_some_and(|name| name == "conditional-launcher.desktop")
                || path
                    .extension()
                    .is_none_or(|extension| extension != "desktop")
                || Self::is_placeholder_file(&path)
            {
                continue;
            }
            if let Some(app) = Self::parse_desktop_file(path) {
                apps.push(app);
            }
        }
        Ok(apps)
    }

    fn manage_app(&self, app: &AppConfig) -> Result<()> {
        let original = app
            .original_path
            .as_deref()
            .context("Missing desktop entry path")?;
        let backup =
            Self::get_backup_path(original).context("Cannot find the desktop backup path")?;
        super::autostart::manage(
            original,
            &backup,
            Self::placeholder_desktop_file(app).as_bytes(),
        )
    }

    fn unmanage_app(&self, app: &AppConfig) -> Result<()> {
        let original = app
            .original_path
            .as_deref()
            .context("Missing desktop entry path")?;
        let backup =
            Self::get_backup_path(original).context("Cannot find the desktop backup path")?;
        super::autostart::restore(original, &backup)
    }

    fn get_partitions(&self) -> Result<Vec<PartitionInfo>> {
        let disks = Disks::new_with_refreshed_list().context("Read disk information")?;
        let mut partitions = Vec::new();
        for disk in disks.iter() {
            let mount_point_str = disk.mount_point().to_string_lossy();
            if !mount_point_str.starts_with('/') {
                continue;
            }
            let fs_type_str = disk.file_system().to_string_lossy();
            if fs_type_str.starts_with("squashfs")
                || fs_type_str.starts_with("overlay")
                || fs_type_str.starts_with("tmpfs")
                || fs_type_str.starts_with("devtmpfs")
                || fs_type_str.starts_with("fuse.")
            {
                continue;
            }
            partitions.push(super::shared_partition_info(disk));
        }
        Ok(partitions)
    }

    fn add_self_to_autostart(&self, managed_app_count: usize) -> Result<()> {
        let path =
            Self::launcher_desktop_file_path().context("Cannot find the autostart directory")?;
        let exe_path = env::current_exe().context("Find the launcher executable")?;
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).with_context(|| format!("Create {}", parent.display()))?;
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
        super::autostart::write(&path, content.as_bytes(), None)
    }

    fn remove_self_from_autostart(&self) -> Result<()> {
        let path =
            Self::launcher_desktop_file_path().context("Cannot find the autostart directory")?;
        super::autostart::remove(&path)
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
