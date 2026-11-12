use super::{OsOperations, PartitionInfo};
use crate::config::AppConfig;
use anyhow::{Context, Result, ensure};
use serde::Serialize;
use std::env;
use std::ffi::CString;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::process::Command;
use sysinfo::Disks;

pub struct MacosOperations;

const LAUNCHER_PLIST: &str = "com.conditional-launcher.plist";

#[derive(Serialize)]
#[serde(rename_all = "PascalCase")]
struct LaunchAgent<'a> {
    label: &'a str,
    program_arguments: Vec<&'a str>,
    run_at_load: bool,
    #[serde(
        rename = "ManagedByConditionalLauncher",
        skip_serializing_if = "std::ops::Not::not"
    )]
    managed: bool,
}

impl LaunchAgent<'_> {
    fn to_xml(&self) -> Result<Vec<u8>> {
        let mut content = Vec::new();
        plist::to_writer_xml(&mut content, self).context("Serialize LaunchAgent")?;
        Ok(content)
    }
}

impl MacosOperations {
    fn launch_agents_dir() -> Result<PathBuf> {
        Ok(dirs::home_dir()
            .context("Cannot find the home directory")?
            .join("Library/LaunchAgents"))
    }

    fn launcher_plist_path() -> Result<PathBuf> {
        Ok(Self::launch_agents_dir()?.join(LAUNCHER_PLIST))
    }

    fn backup_path(original: &Path) -> Result<PathBuf> {
        Ok(dirs::config_dir()
            .context("Cannot find the configuration directory")?
            .join("conditional-launcher/plist-backups")
            .join(
                original
                    .file_name()
                    .context("Missing LaunchAgent file name")?,
            ))
    }

    fn runtime_path(original: &Path) -> Result<PathBuf> {
        Ok(dirs::config_dir()
            .context("Cannot find the configuration directory")?
            .join("conditional-launcher/runtime-agents")
            .join(
                original
                    .file_name()
                    .context("Missing LaunchAgent file name")?,
            ))
    }

    fn app_from_plist(value: &plist::Value, path: &Path) -> Option<AppConfig> {
        let dict = value.as_dictionary()?;
        let name = dict.get("Label")?.as_string()?.to_owned();
        let mut arguments = if let Some(args) = dict
            .get("ProgramArguments")
            .and_then(plist::Value::as_array)
        {
            let args: Option<Vec<_>> = args.iter().map(plist::Value::as_string).collect();
            args?
        } else {
            vec![dict.get("Program")?.as_string()?]
        };
        if arguments.is_empty() {
            return None;
        }
        if let Some(program) = dict.get("Program").and_then(plist::Value::as_string) {
            arguments[0] = program;
        }
        if arguments[0].is_empty() {
            return None;
        }
        let command = shlex::try_join(arguments).ok()?;
        Some(AppConfig {
            name,
            command,
            original_path: Some(path.to_path_buf()),
            working_dir: dict
                .get("WorkingDirectory")
                .and_then(plist::Value::as_string)
                .map(PathBuf::from),
            ..Default::default()
        })
    }

    fn parse_plist_app(path: &Path) -> Option<AppConfig> {
        Self::app_from_plist(&plist::from_file(path).ok()?, path)
    }

    fn managed_agent_path(app: &AppConfig) -> Result<PathBuf> {
        let original = app
            .original_path
            .as_deref()
            .context("Missing LaunchAgent path")?;
        let backup = Self::backup_path(original)?;
        let source = if fs::symlink_metadata(&backup)
            .with_context(|| format!("Read {}", backup.display()))?
            .file_type()
            .is_symlink()
        {
            original
                .parent()
                .context("LaunchAgent path has no parent")?
                .join(fs::read_link(&backup)?)
        } else {
            backup
        };
        let mut value: plist::Value = plist::from_file(&source)
            .with_context(|| format!("Read LaunchAgent {}", source.display()))?;
        let saved = Self::app_from_plist(&value, original).context("Invalid LaunchAgent")?;
        let mut args = shlex::split(&app.command).context("Invalid command quoting")?;
        let saved_args = shlex::split(&saved.command).context("Invalid saved command")?;
        let program = args
            .first()
            .filter(|program| !program.is_empty())
            .context("Empty command")?;
        if Path::new(program).is_absolute() {
            let metadata =
                fs::metadata(program).with_context(|| format!("Find executable {program}"))?;
            ensure!(metadata.is_file(), "Executable {program} is not a file");
            let path = CString::new(program.as_bytes())
                .context("Executable path contains a null character")?;
            // SAFETY: path is a valid null-terminated string and access retains no pointers.
            if unsafe { libc::access(path.as_ptr(), libc::X_OK) } != 0 {
                return Err(io::Error::last_os_error())
                    .with_context(|| format!("Access executable {program}"));
            }
        }
        if let Some(dir) = &app.working_dir {
            ensure!(
                dir.is_dir(),
                "Working directory {} is unavailable",
                dir.display()
            );
        }
        let runs_at_load = value
            .as_dictionary()
            .and_then(|dict| dict.get("RunAtLoad"))
            .and_then(plist::Value::as_boolean)
            .unwrap_or(false);
        if saved_args == args && saved.working_dir == app.working_dir && runs_at_load {
            return Ok(source);
        }

        let dict = value.as_dictionary_mut().context("Invalid LaunchAgent")?;
        dict.insert("RunAtLoad".into(), plist::Value::Boolean(true));
        if saved_args != args {
            if saved_args.first() == args.first() && dict.contains_key("Program") {
                if let Some(argv0) = dict
                    .get("ProgramArguments")
                    .and_then(plist::Value::as_array)
                    .and_then(|arguments| arguments.first())
                    .and_then(plist::Value::as_string)
                {
                    args[0] = argv0.to_owned();
                }
            } else {
                dict.remove("Program");
            }
            dict.insert(
                "ProgramArguments".into(),
                plist::Value::Array(args.into_iter().map(plist::Value::String).collect()),
            );
        }
        if let Some(dir) = &app.working_dir {
            dict.insert(
                "WorkingDirectory".into(),
                plist::Value::String(
                    dir.to_str()
                        .context("Working directory is not UTF-8")?
                        .into(),
                ),
            );
        } else {
            dict.remove("WorkingDirectory");
        }
        let path = Self::runtime_path(original)?;
        let directory = path.parent().context("Runtime agent path has no parent")?;
        fs::create_dir_all(directory).with_context(|| format!("Create {}", directory.display()))?;
        let mut file = tempfile::NamedTempFile::new_in(directory)?;
        value
            .to_writer_xml(&mut file)
            .context("Write LaunchAgent")?;
        file.persist(&path)
            .with_context(|| format!("Replace {}", path.display()))?;
        Ok(path)
    }

    fn launchctl(action: &str, target: &str, path: Option<&Path>) -> Result<()> {
        let mut command = Command::new("/bin/launchctl");
        command.args([action, target]);
        if let Some(path) = path {
            command.arg(path);
        }
        let output = command
            .output()
            .with_context(|| format!("Run launchctl {action}"))?;
        ensure!(
            output.status.success(),
            "launchctl {action} {target}: {}",
            String::from_utf8_lossy(&output.stderr).trim()
        );
        Ok(())
    }

    fn domain() -> String {
        // SAFETY: getuid has no preconditions and does not access Rust memory.
        format!("gui/{}", unsafe { libc::getuid() })
    }
}

impl OsOperations for MacosOperations {
    fn launch_app(&self, app: &AppConfig) -> Result<()> {
        let path = Self::managed_agent_path(app)?;
        let domain = Self::domain();
        let service = format!("{domain}/{}", app.name);
        let bootout = Self::launchctl("bootout", &service, None);
        Self::launchctl("bootstrap", &domain, Some(&path)).with_context(|| {
            bootout.err().map_or_else(
                || format!("Load LaunchAgent {}", app.name),
                |error| format!("Load LaunchAgent {} after: {error:#}", app.name),
            )
        })
    }

    fn run_now(&self, app: &AppConfig) -> Result<()> {
        if app.is_managed {
            return self.launch_app(app);
        }
        let domain = Self::domain();
        let service = format!("{domain}/{}", app.name);
        let error = match Self::launchctl("kickstart", &service, None) {
            Ok(()) => return Ok(()),
            Err(error) => error,
        };
        let original = app
            .original_path
            .as_deref()
            .context("Missing LaunchAgent path")?;
        let mut value: plist::Value = plist::from_file(original)
            .with_context(|| format!("Read LaunchAgent {}", original.display()))?;
        value
            .as_dictionary_mut()
            .context("Invalid LaunchAgent")?
            .insert("RunAtLoad".into(), plist::Value::Boolean(true));
        let mut file = tempfile::Builder::new().suffix(".plist").tempfile()?;
        value
            .to_writer_xml(&mut file)
            .context("Write LaunchAgent")?;
        Self::launchctl("bootstrap", &domain, Some(file.path()))
            .with_context(|| format!("Start LaunchAgent {} after: {error:#}", app.name))
    }

    fn get_autostart_apps(&self) -> Result<Vec<AppConfig>> {
        let dir = Self::launch_agents_dir()?;
        let entries = match fs::read_dir(&dir) {
            Ok(entries) => entries,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
            Err(error) => return Err(error).with_context(|| format!("Read {}", dir.display())),
        };
        let mut apps = Vec::new();
        for entry in entries {
            let path = entry
                .with_context(|| format!("Read an entry in {}", dir.display()))?
                .path();
            if path.file_name().is_some_and(|name| name == LAUNCHER_PLIST)
                || path
                    .extension()
                    .is_none_or(|extension| extension != "plist")
            {
                continue;
            }
            if fs::read_to_string(&path)
                .is_ok_and(|content| content.contains("ManagedByConditionalLauncher"))
            {
                continue;
            }
            if let Some(app) = Self::parse_plist_app(&path) {
                apps.push(app);
            }
        }
        Ok(apps)
    }

    fn manage_app(&self, app: &AppConfig) -> Result<()> {
        let original = app
            .original_path
            .as_deref()
            .context("Missing LaunchAgent path")?;
        let content = LaunchAgent {
            label: &app.name,
            program_arguments: vec!["/usr/bin/true"],
            run_at_load: false,
            managed: true,
        }
        .to_xml()?;
        super::autostart::manage(original, &Self::backup_path(original)?, &content)
    }

    fn unmanage_app(&self, app: &AppConfig) -> Result<()> {
        let original = app
            .original_path
            .as_deref()
            .context("Missing LaunchAgent path")?;
        super::autostart::remove(&Self::runtime_path(original)?)?;
        super::autostart::restore(original, &Self::backup_path(original)?)
    }

    fn get_partitions(&self) -> Result<Vec<PartitionInfo>> {
        Ok(Disks::new_with_refreshed_list()
            .context("Read disk information")?
            .iter()
            .filter(|disk| {
                let fs = disk.file_system().to_string_lossy();
                !matches!(fs.as_ref(), "devfs" | "autofs" | "tmpfs" | "map" | "nullfs")
                    && !fs.starts_with("map ")
            })
            .map(super::shared_partition_info)
            .collect())
    }

    fn add_self_to_autostart(&self, _managed_app_count: usize) -> Result<()> {
        let path = Self::launcher_plist_path()?;
        fs::create_dir_all(Self::launch_agents_dir()?)
            .context("Create the LaunchAgents directory")?;
        let exe = env::current_exe().context("Find the launcher executable")?;
        let exe = exe
            .to_str()
            .context("The launcher executable path is not UTF-8")?;
        let content = LaunchAgent {
            label: "com.conditional-launcher",
            program_arguments: vec![exe, "--hidden"],
            run_at_load: true,
            managed: false,
        }
        .to_xml()?;
        super::autostart::write(&path, &content, None)
    }

    fn remove_self_from_autostart(&self) -> Result<()> {
        super::autostart::remove(&Self::launcher_plist_path()?)
    }
}
