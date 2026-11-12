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
    pub disks: Disks,
    pub running: HashMap<String, bool>,
    system: System,
    network_check: Option<Receiver<bool>>,
    last_refresh: Option<Instant>,
    last_autostart_refresh: Option<Instant>,
}

impl Status {
    pub fn new() -> Result<Self> {
        Ok(Self {
            has_internet: false,
            disks: Disks::new().context("Initialize disk information")?,
            running: HashMap::new(),
            system: System::new().context("Initialize process information")?,
            network_check: None,
            last_refresh: None,
            last_autostart_refresh: None,
        })
    }

    pub fn refresh(&mut self, app: &mut Launcher) -> Result<()> {
        if let Some(receiver) = &self.network_check {
            match receiver.try_recv() {
                Ok(connected) => {
                    self.has_internet = connected;
                    self.network_check = None;
                }
                Err(mpsc::TryRecvError::Disconnected) => self.network_check = None,
                Err(mpsc::TryRecvError::Empty) => {}
            }
        }
        if self
            .last_refresh
            .is_some_and(|time| time.elapsed() < STATUS_INTERVAL)
        {
            return Ok(());
        }
        if self.network_check.is_none() {
            let (sender, receiver) = mpsc::channel();
            self.network_check = Some(receiver);
            // A network request must not block egui's event loop.
            std::thread::spawn(move || {
                let _ = sender.send(os::get_os_operations().check_internet_connection());
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
