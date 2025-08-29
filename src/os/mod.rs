use crate::config::AppConfig;

#[cfg(target_os = "linux")]
pub mod linux;

#[derive(Clone, Debug, Default)]
pub struct PartitionInfo {
    pub mount_point: String,
    pub label: String,
    pub fs_type: String,
    pub size: String,
}

pub trait OsOperations {
    fn check_internet_connection(&self) -> bool;
    fn is_partition_mounted(&self, path: &str) -> bool;
    fn launch_app(&self, app: &AppConfig);
    fn get_autostart_apps(&self) -> Vec<AppConfig>;
    fn manage_app(&self, app: &AppConfig) -> bool;
    fn unmanage_app(&self, app: &AppConfig) -> bool;
    fn get_partitions(&self) -> Vec<PartitionInfo>;
    fn open_config_dir(&self);
    fn add_self_to_autostart(&self);
    fn remove_self_from_autostart(&self);
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
            fn is_partition_mounted(&self, _path: &str) -> bool {
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
            fn open_config_dir(&self) {}
            fn add_self_to_autostart(&self) {}
            fn remove_self_from_autostart(&self) {}
        }
        Box::new(UnsupportedOperations)
    }
}
