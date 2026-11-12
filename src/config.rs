use crate::conditions::Conditions;
use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::fs;
use std::io::Write;
use std::path::PathBuf;

mod path_serde {
    use serde::ser::Error;
    use serde::{Deserializer, Serializer};
    use std::path::PathBuf;

    pub fn serialize<S>(path: &Option<PathBuf>, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        let value = match path {
            Some(path) => path
                .to_str()
                .ok_or_else(|| S::Error::custom("Cannot store a non-UTF-8 path in TOML"))?,
            None => "",
        };
        serializer.serialize_str(value)
    }

    pub fn deserialize<'de, D>(deserializer: D) -> Result<Option<PathBuf>, D::Error>
    where
        D: Deserializer<'de>,
    {
        let s: String = serde::Deserialize::deserialize(deserializer)?;
        if s.is_empty() {
            Ok(None)
        } else {
            Ok(Some(PathBuf::from(s)))
        }
    }
}

#[derive(Deserialize)]
struct Config {
    apps: Vec<AppConfig>,
}

#[derive(Serialize, Deserialize, Default)]
pub struct AppConfig {
    pub name: String,
    pub command: String,
    pub conditions: Conditions,
    #[serde(with = "path_serde", default)]
    pub original_path: Option<PathBuf>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub icon: Option<String>,
    #[serde(with = "path_serde", default)]
    pub working_dir: Option<PathBuf>,
    // Legacy configs did not retain the registry value type.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub expand_environment: Option<bool>,
    #[serde(skip)]
    pub launched: bool,
    #[serde(skip)]
    pub is_managed: bool,
}

pub struct ConfigStore {
    path: PathBuf,
}

impl ConfigStore {
    pub fn new(path: PathBuf) -> Self {
        Self { path }
    }

    pub fn for_user() -> Result<Self> {
        Ok(Self::new(
            dirs::config_dir()
                .context("Cannot find the user configuration directory")?
                .join("conditional-launcher/managed_apps.toml"),
        ))
    }

    pub fn load(&self) -> Result<Vec<AppConfig>> {
        let path = &self.path;
        let content = match fs::read_to_string(path) {
            Ok(content) => content,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
            Err(error) => return Err(error).with_context(|| format!("Read {}", path.display())),
        };
        let mut config: Config =
            toml::from_str(&content).with_context(|| format!("Parse {}", path.display()))?;
        for app in &mut config.apps {
            app.is_managed = true;
        }
        Ok(config.apps)
    }

    pub fn save(&self, apps: &[AppConfig]) -> Result<()> {
        #[derive(Serialize)]
        struct ManagedApps<'a> {
            apps: Vec<&'a AppConfig>,
        }

        let config = ManagedApps {
            apps: apps.iter().filter(|app| app.is_managed).collect(),
        };
        let content = toml::to_string_pretty(&config).context("Serialize managed applications")?;
        let path = &self.path;
        let parent = path.parent().context("Configuration path has no parent")?;
        fs::create_dir_all(parent).with_context(|| format!("Create {}", parent.display()))?;
        let mut file = tempfile::NamedTempFile::new_in(parent)
            .with_context(|| format!("Create a temporary config in {}", parent.display()))?;
        file.write_all(content.as_bytes())
            .with_context(|| format!("Write {}", path.display()))?;
        file.as_file()
            .sync_all()
            .context("Flush managed applications")?;
        file.persist(path)
            .with_context(|| format!("Replace {}", path.display()))?;
        Ok(())
    }
}
