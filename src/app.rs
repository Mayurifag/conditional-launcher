use crate::conditions::Conditions;
use crate::config::{AppConfig, ConfigStore};
use crate::os::{OsOperations, PartitionInfo};
use anyhow::{Context, Result};

pub struct Launcher {
    pub apps: Vec<AppConfig>,
    pub os_ops: Box<dyn OsOperations>,
    pub available_partitions: Vec<PartitionInfo>,
    config: ConfigStore,
}

impl Launcher {
    pub fn load() -> Result<Self> {
        Self::new(crate::os::get_os_operations(), ConfigStore::for_user()?)
    }

    pub fn new(os_ops: Box<dyn OsOperations>, config: ConfigStore) -> Result<Self> {
        let apps = config.load()?;
        let available_partitions = os_ops.get_partitions()?;
        let mut app = Self {
            apps,
            os_ops,
            available_partitions,
            config,
        };
        app.refresh_autostart_list()?;
        Ok(app)
    }

    pub fn refresh_autostart_list(&mut self) -> Result<()> {
        let discovered = self.os_ops.get_autostart_apps()?;
        self.apps.retain(|app| app.is_managed);
        for app in discovered {
            if !self.apps.iter().any(|other| other.name == app.name) {
                self.apps.push(app);
            }
        }
        self.apps.sort_by_cached_key(|app| app.name.to_lowercase());
        Ok(())
    }

    pub fn save(&self) -> Result<()> {
        self.config.save(&self.apps)?;
        self.sync_autostart()
    }

    fn sync_autostart(&self) -> Result<()> {
        let count = self.apps.iter().filter(|app| app.is_managed).count();
        if count == 0 {
            self.os_ops.remove_self_from_autostart()
        } else {
            self.os_ops.add_self_to_autostart(count)
        }
    }

    pub fn update_conditions(&mut self, index: usize, conditions: Conditions) -> Result<()> {
        let previous = std::mem::replace(&mut self.apps[index].conditions, conditions);
        let should_manage = self.apps[index].conditions.requires_management();
        if should_manage == self.apps[index].is_managed {
            if let Err(error) = self.config.save(&self.apps) {
                self.apps[index].conditions = previous;
                return Err(error);
            }
            return self.sync_autostart();
        }
        if should_manage {
            self.apps[index].is_managed = true;
            if let Err(error) = self.sync_autostart() {
                self.apps[index].is_managed = false;
                self.apps[index].conditions = previous;
                return Err(error);
            }
            // Save the original command before replacing its system autostart entry.
            if let Err(error) = self.config.save(&self.apps) {
                self.apps[index].is_managed = false;
                self.apps[index].conditions = previous;
                self.sync_autostart()
                    .with_context(|| format!("Restore launcher autostart after: {error:#}"))?;
                return Err(error);
            }
            if let Err(error) = self.os_ops.manage_app(&self.apps[index]) {
                self.apps[index].is_managed = false;
                self.apps[index].conditions = previous;
                self.config
                    .save(&self.apps)
                    .with_context(|| format!("Restore configuration after: {error:#}"))?;
                self.sync_autostart()
                    .with_context(|| format!("Restore launcher autostart after: {error:#}"))?;
                return Err(error);
            }
            return Ok(());
        }
        if let Err(error) = self.os_ops.unmanage_app(&self.apps[index]) {
            self.apps[index].conditions = previous;
            return Err(error);
        }
        self.apps[index].is_managed = false;
        if let Err(error) = self.config.save(&self.apps) {
            self.apps[index].is_managed = true;
            self.apps[index].conditions = previous;
            self.os_ops
                .manage_app(&self.apps[index])
                .with_context(|| format!("Restore management after: {error:#}"))?;
            return Err(error);
        }
        self.sync_autostart()
    }
}
