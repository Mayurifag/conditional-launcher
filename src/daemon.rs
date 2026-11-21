use crate::{
    config::{AppConfig, ConfigStore},
    os::{OsOperations, get_os_operations},
};
use anyhow::{Context, Result, ensure};
use std::time::{Duration, Instant};
use sysinfo::Disks;

pub fn perform_launch_checks(
    os_ops: &dyn OsOperations,
    apps: &mut [AppConfig],
    has_internet: bool,
    keepassxc_unlocked: bool,
) -> Result<()> {
    let disks = Disks::new_with_refreshed_list().context("Read disk information")?;
    for app in apps.iter_mut().filter(|app| !app.launched) {
        if app
            .conditions
            .check(os_ops, has_internet, &disks, keepassxc_unlocked)
            .ready()
        {
            match os_ops.launch_app(app) {
                Ok(()) => app.launched = true,
                Err(error) => eprintln!("{error:#}"),
            }
        }
    }
    Ok(())
}

pub fn run() -> Result<()> {
    let os_ops = get_os_operations();
    let mut apps = ConfigStore::for_user()?.load()?;
    let needs_keepassxc = apps.iter().any(|app| app.conditions.keepassxc_unlocked);
    let mut watcher = if needs_keepassxc {
        os_ops.watch_keepassxc()?
    } else {
        None
    };
    ensure!(
        !needs_keepassxc || watcher.is_some(),
        "The KeePassXC unlock condition requires Linux with Secret Service integration"
    );
    let mut has_internet = false;
    let mut last_internet_check: Option<Instant> = None;
    while apps.iter().any(|app| !app.launched) {
        if apps
            .iter()
            .any(|app| !app.launched && app.conditions.internet)
            && last_internet_check.is_none_or(|time| time.elapsed() >= Duration::from_secs(5))
        {
            has_internet = os_ops.check_internet_connection();
            last_internet_check = Some(Instant::now());
        }
        let keepassxc_unlocked = watcher
            .as_ref()
            .map_or(Ok(false), |watcher| watcher.keepassxc_unlocked())?;
        perform_launch_checks(os_ops.as_ref(), &mut apps, has_internet, keepassxc_unlocked)?;
        let pending: Vec<_> = apps.iter().filter(|app| !app.launched).collect();
        if pending.is_empty() {
            break;
        }
        if !pending.iter().any(|app| app.conditions.keepassxc_unlocked) {
            watcher = None;
        }
        let only_waiting_for_unlock = !keepassxc_unlocked
            && pending.iter().all(|app| {
                app.conditions.keepassxc_unlocked
                    && !app.conditions.internet
                    && app.conditions.partition_mounted.is_none()
            });
        if let Some(watcher) = &mut watcher {
            watcher.wait((!only_waiting_for_unlock).then_some(Duration::from_secs(5)))?;
        } else {
            std::thread::sleep(Duration::from_secs(5));
        }
    }
    Ok(())
}
