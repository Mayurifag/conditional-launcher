use crate::{
    config::{AppConfig, ConfigStore},
    os::{OsOperations, get_os_operations},
};
use anyhow::{Context, Result};
use std::time::Duration;
use sysinfo::Disks;

pub fn perform_launch_checks(os_ops: &dyn OsOperations, apps: &mut [AppConfig]) -> Result<()> {
    let has_internet = apps
        .iter()
        .any(|app| !app.launched && app.conditions.internet)
        && os_ops.check_internet_connection();
    let disks = Disks::new_with_refreshed_list().context("Read disk information")?;
    for app in apps.iter_mut().filter(|app| !app.launched) {
        if app.conditions.check(os_ops, has_internet, &disks).ready() {
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
    while apps.iter().any(|app| !app.launched) {
        perform_launch_checks(os_ops.as_ref(), &mut apps)?;
        if apps.iter().any(|app| !app.launched) {
            std::thread::sleep(Duration::from_secs(5));
        }
    }
    Ok(())
}
