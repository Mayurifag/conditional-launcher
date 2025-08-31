use serde::{Deserialize, Serialize};
use std::path::PathBuf;

mod path_serde {
    use serde::{Deserializer, Serializer};
    use std::path::PathBuf;

    pub fn serialize<S>(path: &Option<PathBuf>, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        let s = path.as_ref().map(|p| p.to_string_lossy().to_string());
        serializer.serialize_str(&s.unwrap_or_default())
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

#[derive(Serialize, Deserialize, Default)]
pub struct Config {
    pub apps: Vec<AppConfig>,
}

#[derive(Serialize, Deserialize, Clone, Debug, Default)]
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
    #[serde(skip)]
    pub launched: bool,
    #[serde(skip)]
    pub is_managed: bool,
}

#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq)]
pub struct Conditions {
    pub internet: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub partition_mounted: Option<String>,
}
