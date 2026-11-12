use super::{OsOperations, PartitionInfo};
use crate::config::AppConfig;
use anyhow::{Context, Result, ensure};
use std::env;
use std::io;
use std::os::windows::ffi::OsStrExt;
use std::os::windows::io::{FromRawHandle, OwnedHandle};
use sysinfo::Disks;
use windows_sys::Win32::System::Environment::ExpandEnvironmentStringsW;
use windows_sys::Win32::System::Threading::{
    CREATE_NO_WINDOW, CreateProcessW, PROCESS_INFORMATION, STARTUPINFOW,
};
use winreg::RegKey;
use winreg::enums::{HKEY_CURRENT_USER, KEY_READ, KEY_SET_VALUE, REG_EXPAND_SZ, REG_SZ};
use winreg::types::{FromRegValue, ToRegValue};

const AUTORUN_KEY: &str = "Software\\Microsoft\\Windows\\CurrentVersion\\Run";
const LAUNCHER_VALUE: &str = "ConditionalLauncher";
const PLACEHOLDER: &str = "cmd /c exit";

pub struct WindowsOperations;

fn autorun_key(access: u32) -> Result<RegKey> {
    RegKey::predef(HKEY_CURRENT_USER)
        .open_subkey_with_flags(AUTORUN_KEY, access)
        .context("Open the current user's Run registry key")
}

fn expand_environment(command: &str) -> Result<String> {
    ensure!(!command.contains('\0'), "Command contains a null character");
    let source: Vec<u16> = command.encode_utf16().chain(Some(0)).collect();
    // SAFETY: source is null-terminated; a null destination requests the required length.
    let size = unsafe { ExpandEnvironmentStringsW(source.as_ptr(), std::ptr::null_mut(), 0) };
    if size == 0 {
        return Err(io::Error::last_os_error()).context("Expand command environment variables");
    }
    let mut expanded = vec![0; size as usize];
    // SAFETY: expanded has space for size UTF-16 units, and source remains valid.
    let written =
        unsafe { ExpandEnvironmentStringsW(source.as_ptr(), expanded.as_mut_ptr(), size) };
    if written == 0 {
        return Err(io::Error::last_os_error()).context("Expand command environment variables");
    }
    ensure!(
        written <= size,
        "Environment changed while expanding command"
    );
    String::from_utf16(&expanded[..written as usize - 1]).context("Decode expanded command")
}

impl OsOperations for WindowsOperations {
    fn launch_app(&self, app: &AppConfig) -> Result<()> {
        let expanded;
        // Older configs used cmd, which expanded environment variables without recording the type.
        let value = if app.expand_environment.unwrap_or(true) {
            expanded = expand_environment(&app.command)?;
            &expanded
        } else {
            &app.command
        };
        ensure!(!value.trim().is_empty(), "Empty command");
        ensure!(!value.contains('\0'), "Command contains a null character");
        let mut command: Vec<u16> = value.encode_utf16().chain(Some(0)).collect();
        let directory = app.working_dir.as_ref().map(|path| {
            path.as_os_str()
                .encode_wide()
                .chain(Some(0))
                .collect::<Vec<_>>()
        });
        if let Some(dir) = &directory {
            ensure!(
                !dir[..dir.len() - 1].contains(&0),
                "Working directory contains a null character"
            );
        }
        let startup = STARTUPINFOW {
            cb: std::mem::size_of::<STARTUPINFOW>() as u32,
            ..Default::default()
        };
        let mut process = PROCESS_INFORMATION::default();
        // SAFETY: command is writable and null-terminated; all other pointers are either null
        // or valid for the duration of the call. The returned handles are owned on success.
        let created = unsafe {
            CreateProcessW(
                std::ptr::null(),
                command.as_mut_ptr(),
                std::ptr::null(),
                std::ptr::null(),
                0,
                CREATE_NO_WINDOW,
                std::ptr::null(),
                directory
                    .as_ref()
                    .map_or(std::ptr::null(), |dir| dir.as_ptr()),
                &startup,
                &mut process,
            )
        };
        if created == 0 {
            return Err(io::Error::last_os_error()).with_context(|| format!("Launch {}", app.name));
        }
        // SAFETY: a successful CreateProcessW returns two valid handles owned by this process.
        let _process = unsafe { OwnedHandle::from_raw_handle(process.hProcess) };
        // SAFETY: the thread handle is distinct and is closed when this guard is dropped.
        let _thread = unsafe { OwnedHandle::from_raw_handle(process.hThread) };
        Ok(())
    }

    fn get_autostart_apps(&self) -> Result<Vec<AppConfig>> {
        let key = match RegKey::predef(HKEY_CURRENT_USER)
            .open_subkey_with_flags(AUTORUN_KEY, KEY_READ)
        {
            Ok(key) => key,
            Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(Vec::new()),
            Err(error) => return Err(error).context("Read the current user's Run registry key"),
        };
        let mut apps = Vec::new();
        for entry in key.enum_values() {
            let (name, value) = entry.context("Read an autostart registry value")?;
            if name == LAUNCHER_VALUE {
                continue;
            }
            if !matches!(value.vtype, REG_SZ | REG_EXPAND_SZ) {
                continue;
            }
            let Ok(command) = String::from_reg_value(&value) else {
                continue;
            };
            if command.trim() == PLACEHOLDER {
                continue;
            }
            apps.push(AppConfig {
                name,
                command,
                expand_environment: Some(value.vtype == REG_EXPAND_SZ),
                ..Default::default()
            });
        }
        Ok(apps)
    }

    fn manage_app(&self, app: &AppConfig) -> Result<()> {
        let key = autorun_key(KEY_READ | KEY_SET_VALUE)?;
        let value = key
            .get_raw_value(&app.name)
            .with_context(|| format!("Read autostart entry {}", app.name))?;
        ensure!(
            matches!(value.vtype, REG_SZ | REG_EXPAND_SZ)
                && (value.vtype == REG_EXPAND_SZ) == app.expand_environment.unwrap_or(true)
                && String::from_reg_value(&value)? == app.command,
            "Autostart entry {} changed; refresh the app list and retry",
            app.name
        );
        key.set_value(&app.name, &PLACEHOLDER)
            .with_context(|| format!("Disable autostart for {}", app.name))
    }

    fn unmanage_app(&self, app: &AppConfig) -> Result<()> {
        let mut value = app.command.to_reg_value();
        if app.expand_environment.unwrap_or(true) {
            value.vtype = REG_EXPAND_SZ;
        }
        autorun_key(KEY_SET_VALUE)?
            .set_raw_value(&app.name, &value)
            .with_context(|| format!("Restore autostart for {}", app.name))
    }

    fn get_partitions(&self) -> Result<Vec<PartitionInfo>> {
        Ok(Disks::new_with_refreshed_list()
            .context("Read disk information")?
            .iter()
            .map(super::shared_partition_info)
            .collect())
    }

    fn add_self_to_autostart(&self, _managed_app_count: usize) -> Result<()> {
        let exe = env::current_exe().context("Find the launcher executable")?;
        let (key, _) = RegKey::predef(HKEY_CURRENT_USER)
            .create_subkey(AUTORUN_KEY)
            .context("Create the current user's Run registry key")?;
        key.set_value(LAUNCHER_VALUE, &format!("\"{}\" --hidden", exe.display()))
            .context("Add Conditional Launcher to autostart")
    }

    fn remove_self_from_autostart(&self) -> Result<()> {
        let key = match RegKey::predef(HKEY_CURRENT_USER)
            .open_subkey_with_flags(AUTORUN_KEY, KEY_SET_VALUE)
        {
            Ok(key) => key,
            Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(()),
            Err(error) => return Err(error).context("Open the Run registry key"),
        };
        match key.delete_value(LAUNCHER_VALUE) {
            Ok(()) => Ok(()),
            Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(()),
            Err(error) => Err(error).context("Remove Conditional Launcher from autostart"),
        }
    }
}
