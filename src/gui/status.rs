use crate::app::Launcher;
use crate::os;
use anyhow::{Context, Result};
use std::collections::HashMap;
use std::sync::mpsc::{self, Receiver};
use std::time::{Duration, Instant};
use sysinfo::{Disks, ProcessRefreshKind, RefreshKind, System};

const STATUS_INTERVAL: Duration = Duration::from_secs(5);
const AUTOSTART_INTERVAL: Duration = Duration::from_secs(30);

pub(super) struct Status {
    pub has_internet: bool,
    pub keepassxc_unlocked: bool,
    pub disks: Disks,
    pub running: HashMap<String, bool>,
    system: System,
    condition_check: Option<Receiver<Result<(bool, bool)>>>,
    last_refresh: Option<Instant>,
    last_autostart_refresh: Option<Instant>,
}

impl Status {
    pub fn new() -> Result<Self> {
        Ok(Self {
            has_internet: false,
            keepassxc_unlocked: false,
            disks: Disks::new().context("Initialize disk information")?,
            running: HashMap::new(),
            system: System::new().context("Initialize process information")?,
            condition_check: None,
            last_refresh: None,
            last_autostart_refresh: None,
        })
    }

    pub fn refresh(&mut self, app: &mut Launcher) -> Result<()> {
        if let Some(receiver) = &self.condition_check {
            match receiver.try_recv() {
                Ok(result) => {
                    self.condition_check = None;
                    self.has_internet = false;
                    self.keepassxc_unlocked = false;
                    let (connected, unlocked) = result?;
                    self.has_internet = connected;
                    self.keepassxc_unlocked = unlocked;
                }
                Err(mpsc::TryRecvError::Disconnected) => self.condition_check = None,
                Err(mpsc::TryRecvError::Empty) => {}
            }
        }
        if self
            .last_refresh
            .is_some_and(|time| time.elapsed() < STATUS_INTERVAL)
        {
            return Ok(());
        }
        if self.condition_check.is_none() {
            let (sender, receiver) = mpsc::channel();
            self.condition_check = Some(receiver);
            let needs_internet = app.apps.iter().any(|app| app.conditions.internet);
            let needs_keepassxc = app.apps.iter().any(|app| app.conditions.keepassxc_unlocked);
            // Network and D-Bus requests must not block egui's event loop.
            std::thread::spawn(move || {
                let os_ops = os::get_os_operations();
                let status = (|| {
                    let unlocked = needs_keepassxc && os_ops.keepassxc_unlocked()?;
                    let connected = needs_internet && os_ops.check_internet_connection();
                    Ok((connected, unlocked))
                })();
                let _ = sender.send(status);
            });
        }
        self.disks.refresh(true);
        self.system.refresh_specifics(
            RefreshKind::nothing().with_processes(ProcessRefreshKind::everything()),
        );
        if self
            .last_autostart_refresh
            .is_none_or(|time| time.elapsed() >= AUTOSTART_INTERVAL)
        {
            self.last_autostart_refresh = Some(Instant::now());
            app.refresh_autostart_list()?;
        }
        self.running.clear();
        self.running.extend(app.apps.iter().map(|entry| {
            (
                entry.name.clone(),
                app.os_ops.is_app_running(entry, &self.system),
            )
        }));
        self.last_refresh = Some(Instant::now());
        Ok(())
    }
}
