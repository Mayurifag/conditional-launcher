use crate::config::{AppConfig, Config};
use crate::os::{OsOperations, PartitionInfo};
use sysinfo::Disks;

pub struct ConditionStatus {
    pub internet_ok: bool,
    pub partition_ok: bool,
}

pub fn check_app_conditions(
    os_ops: &dyn OsOperations,
    app: &AppConfig,
    has_internet: bool,
    disks: &Disks,
) -> ConditionStatus {
    let partition_ok = app
        .conditions
        .partition_mounted
        .as_ref()
        .map_or(true, |p| os_ops.is_partition_mounted(p, disks));
    let internet_ok = !app.conditions.internet || has_internet;
    ConditionStatus {
        internet_ok,
        partition_ok,
    }
}

pub fn perform_launch_checks(os_ops: &dyn OsOperations, managed_apps: &mut [AppConfig]) {
    let has_internet = os_ops.check_internet_connection();
    let mut disks = Disks::new();
    disks.refresh(true);

    for app in managed_apps.iter_mut() {
        if app.launched {
            continue;
        }

        let status = check_app_conditions(os_ops, app, has_internet, &disks);

        if status.internet_ok && status.partition_ok {
            os_ops.launch_app(app);
            app.launched = true;
        }
    }
}

pub struct ConditionalLauncherApp {
    pub apps: Vec<AppConfig>,
    pub os_ops: Box<dyn OsOperations>,
    pub available_partitions: Vec<PartitionInfo>,
}

pub fn load_all_apps(os_ops: &dyn OsOperations) -> Vec<AppConfig> {
    let mut managed_apps = ConditionalLauncherApp::load_config();
    for app in &mut managed_apps {
        app.is_managed = true;
    }

    let unmanaged_apps = os_ops.get_autostart_apps();

    let mut apps = managed_apps;
    for unmanaged_app in unmanaged_apps {
        if !apps.iter().any(|a| a.name == unmanaged_app.name) {
            apps.push(unmanaged_app);
        }
    }

    apps.sort_by(|a, b| a.name.to_lowercase().cmp(&b.name.to_lowercase()));
    apps
}

impl ConditionalLauncherApp {
    pub fn load_config() -> Vec<AppConfig> {
        Config::load_config()
    }

    pub fn new(apps: Vec<AppConfig>) -> Self {
        let os_ops = crate::os::get_os_operations();
        let available_partitions = os_ops.get_partitions();

        Self {
            apps,
            os_ops,
            available_partitions,
        }
    }

    pub fn save_config(&mut self) {
        Config::save_config(&self.apps);

        let managed_app_count = self.apps.iter().filter(|a| a.is_managed).count();
        if managed_app_count == 0 {
            self.os_ops.remove_self_from_autostart();
        } else {
            self.os_ops.add_self_to_autostart(managed_app_count);
        }
    }
}
