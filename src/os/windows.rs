use super::{OsOperations, PartitionInfo};
use crate::config::AppConfig;
use std::env;
use std::os::windows::process::CommandExt;
use std::path::PathBuf;
use std::process::{Command, Stdio};
use sysinfo::{Disks, System};
use winreg::RegKey;
use winreg::enums::{HKEY_CURRENT_USER, KEY_READ, KEY_SET_VALUE};

const DETACHED_PROCESS: u32 = 0x00000008;
const AUTORUN_KEY: &str = "Software\\Microsoft\\Windows\\CurrentVersion\\Run";
const LAUNCHER_VALUE: &str = "ConditionalLauncher";

pub struct WindowsOperations;

impl OsOperations for WindowsOperations {
    fn check_internet_connection(&self) -> bool {
        super::shared_check_internet()
    }

    fn is_partition_mounted(&self, path: &str, disks: &Disks) -> bool {
        super::shared_is_partition_mounted(path, disks)
    }

    fn launch_app(&self, app: &AppConfig) {
        let mut cmd = Command::new("cmd");
        cmd.args(["/c", &app.command]);
        if let Some(dir) = &app.working_dir {
            cmd.current_dir(dir);
        }
        cmd.creation_flags(DETACHED_PROCESS);
        let _ = cmd.stdout(Stdio::null()).stderr(Stdio::null()).spawn();
    }

    fn get_autostart_apps(&self) -> Vec<AppConfig> {
        let mut apps = Vec::new();
        let hkcu = RegKey::predef(HKEY_CURRENT_USER);
        if let Ok(run_key) = hkcu.open_subkey_with_flags(AUTORUN_KEY, KEY_READ) {
            for (name, _) in run_key.enum_values().filter_map(Result::ok) {
                if name == LAUNCHER_VALUE {
                    continue;
                }
                if let Ok(cmd) = run_key.get_value::<String, _>(&name) {
                    // Skip no-op placeholder entries we wrote ourselves
                    if cmd.trim() == "cmd /c exit" {
                        continue;
                    }
                    apps.push(AppConfig {
                        name,
                        command: cmd,
                        ..Default::default()
                    });
                }
            }
        }
        apps
    }

    fn manage_app(&self, app: &AppConfig) -> bool {
        // Replace the registry entry with a no-op so the OS won't launch it.
        // The original command is preserved in AppConfig.command / managed_apps.toml.
        let hkcu = RegKey::predef(HKEY_CURRENT_USER);
        if let Ok(run_key) = hkcu.open_subkey_with_flags(AUTORUN_KEY, KEY_READ | KEY_SET_VALUE)
            && run_key.get_value::<String, _>(&app.name).is_ok()
        {
            return run_key
                .set_value(&app.name, &"cmd /c exit".to_string())
                .is_ok();
        }
        false
    }

    fn unmanage_app(&self, app: &AppConfig) -> bool {
        let hkcu = RegKey::predef(HKEY_CURRENT_USER);
        if let Ok(run_key) = hkcu.open_subkey_with_flags(AUTORUN_KEY, KEY_READ | KEY_SET_VALUE) {
            return run_key.set_value(&app.name, &app.command).is_ok();
        }
        false
    }

    fn get_partitions(&self) -> Vec<PartitionInfo> {
        Disks::new_with_refreshed_list()
            .iter()
            .map(|disk| PartitionInfo {
                mount_point: disk.mount_point().to_string_lossy().to_string(),
                fs_type: disk.file_system().to_string_lossy().to_string(),
                size: format!("{:.1} GB", disk.total_space() as f64 / 1_000_000_000.0),
            })
            .collect()
    }

    fn add_self_to_autostart(&self, managed_app_count: usize) {
        if let Ok(exe_path) = env::current_exe() {
            let hkcu = RegKey::predef(HKEY_CURRENT_USER);
            if let Ok(run_key) = hkcu.open_subkey_with_flags(AUTORUN_KEY, KEY_READ | KEY_SET_VALUE)
            {
                let value = format!(
                    "\"{}\" --hidden  ; {} apps",
                    exe_path.display(),
                    managed_app_count
                );
                run_key.set_value(LAUNCHER_VALUE, &value).ok();
            }
        }
    }

    fn remove_self_from_autostart(&self) {
        let hkcu = RegKey::predef(HKEY_CURRENT_USER);
        if let Ok(run_key) = hkcu.open_subkey_with_flags(AUTORUN_KEY, KEY_READ | KEY_SET_VALUE) {
            run_key.delete_value(LAUNCHER_VALUE).ok();
        }
    }

    fn is_app_running(&self, app: &AppConfig, sys: &System) -> bool {
        super::shared_is_app_running(app, sys)
    }

    fn get_app_icon_rgba(&self, _app: &AppConfig) -> Option<(Vec<u8>, [usize; 2])> {
        None
    }
}

#[allow(dead_code)]
fn startup_folder_path() -> Option<PathBuf> {
    dirs::data_dir().map(|d| d.join("Microsoft/Windows/Start Menu/Programs/Startup"))
}
