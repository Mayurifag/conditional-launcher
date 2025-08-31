use crate::config::AppConfig;
use sysinfo::{Disks, System};

#[cfg(target_os = "linux")]
pub mod linux;

#[derive(Clone, Debug, Default)]
pub struct PartitionInfo {
    pub mount_point: String,
    pub fs_type: String,
    pub size: String,
}

pub trait OsOperations {
    fn check_internet_connection(&self) -> bool;
    fn is_partition_mounted(&self, path: &str, disks: &Disks) -> bool;
    fn launch_app(&self, app: &AppConfig);
    fn get_autostart_apps(&self) -> Vec<AppConfig>;
    fn manage_app(&self, app: &AppConfig) -> bool;
    fn unmanage_app(&self, app: &AppConfig) -> bool;
    fn get_partitions(&self) -> Vec<PartitionInfo>;
    fn add_self_to_autostart(&self, managed_app_count: usize);
    fn remove_self_from_autostart(&self);
    fn is_app_running(&self, app: &AppConfig, sys: &System) -> bool;
    fn send_exit_notification(&self, launched_apps: &[String]);
}

pub fn get_os_operations() -> Box<dyn OsOperations> {
    #[cfg(target_os = "linux")]
    {
        Box::new(linux::LinuxOperations)
    }
    #[cfg(not(target_os = "linux"))]
    {
        struct UnsupportedOperations;
        impl OsOperations for UnsupportedOperations {
            fn check_internet_connection(&self) -> bool {
                false
            }
            fn is_partition_mounted(&self, _path: &str, _disks: &Disks) -> bool {
                false
            }
            fn launch_app(&self, _app: &AppConfig) {}
            fn get_autostart_apps(&self) -> Vec<AppConfig> {
                vec![]
            }
            fn manage_app(&self, _app: &AppConfig) -> bool {
                false
            }
            fn unmanage_app(&self, _app: &AppConfig) -> bool {
                false
            }
            fn get_partitions(&self) -> Vec<PartitionInfo> {
                vec![]
            }
            fn add_self_to_autostart(&self, _managed_app_count: usize) {}
            fn remove_self_from_autostart(&self) {}
            fn is_app_running(&self, _app: &AppConfig, _sys: &System) -> bool {
                false
            }
            fn send_exit_notification(&self, _launched_apps: &[String]) {}
        }
        Box::new(UnsupportedOperations)
    }
}
