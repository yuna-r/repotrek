use std::{
    fs,
    path::{Path, PathBuf},
};

use anyhow::{Context, Result, anyhow};
use directories::ProjectDirs;
use serde::{Deserialize, Serialize};

use crate::theme::ThemeMode;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct Settings {
    pub theme: ThemeMode,
    pub wrap_code: bool,
    pub wrap_diff: bool,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            theme: ThemeMode::Dark,
            wrap_code: false,
            wrap_diff: false,
        }
    }
}

#[derive(Debug)]
pub struct SettingsStore {
    path: PathBuf,
    settings: Settings,
}

impl SettingsStore {
    pub fn load() -> Result<Self> {
        let project_dirs = ProjectDirs::from("dev", "yuna-r", "RepoTrek")
            .ok_or_else(|| anyhow!("Could not determine the RepoTrek configuration directory"))?;
        Self::from_path(project_dirs.config_dir().join("settings.json"))
    }

    fn from_path(path: PathBuf) -> Result<Self> {
        let settings = if path.exists() {
            let bytes = fs::read(&path)
                .with_context(|| format!("Could not read settings: {}", path.display()))?;
            match serde_json::from_slice::<Settings>(&bytes) {
                Ok(settings) => settings,
                Err(error) => {
                    let corrupt_path = path.with_extension("json.corrupt");
                    let _ = fs::rename(&path, &corrupt_path);
                    eprintln!(
                        "RepoTrek: moved invalid settings to {}: {error}",
                        corrupt_path.display()
                    );
                    Settings::default()
                }
            }
        } else {
            Settings::default()
        };
        Ok(Self { path, settings })
    }

    #[must_use]
    pub fn settings(&self) -> &Settings {
        &self.settings
    }

    pub fn replace(&mut self, settings: Settings) -> Result<()> {
        self.settings = settings;
        self.save()
    }

    fn save(&self) -> Result<()> {
        if let Some(parent) = self.path.parent() {
            fs::create_dir_all(parent).with_context(|| {
                format!("Could not create settings directory: {}", parent.display())
            })?;
        }
        let data = serde_json::to_vec_pretty(&self.settings)?;
        let temporary = temporary_path(&self.path);
        fs::write(&temporary, data)
            .with_context(|| format!("Could not write settings: {}", temporary.display()))?;
        replace_file(&temporary, &self.path)
            .with_context(|| format!("Could not finalize settings: {}", self.path.display()))?;
        Ok(())
    }
}

pub fn save_settings(settings: &Settings) -> Result<()> {
    let mut store = SettingsStore::load()?;
    store.replace(settings.clone())
}

fn replace_file(source: &Path, destination: &Path) -> std::io::Result<()> {
    #[cfg(target_os = "windows")]
    if destination.exists() {
        fs::remove_file(destination)?;
    }
    fs::rename(source, destination)
}

fn temporary_path(path: &Path) -> PathBuf {
    let file_name = path
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("settings.json");
    path.with_file_name(format!(".{file_name}.tmp"))
}

#[cfg(test)]
mod tests {
    use std::{fs, time::SystemTime};

    use super::{Settings, SettingsStore};
    use crate::theme::ThemeMode;

    #[test]
    fn persists_theme_and_wrapping() {
        let unique = SystemTime::now()
            .duration_since(SystemTime::UNIX_EPOCH)
            .expect("system clock")
            .as_nanos();
        let path = std::env::temp_dir().join(format!("repotrek-settings-{unique}.json"));
        let mut store = SettingsStore::from_path(path.clone()).expect("settings store");
        store
            .replace(Settings {
                theme: ThemeMode::Light,
                wrap_code: true,
                wrap_diff: true,
            })
            .expect("save settings");
        let loaded = SettingsStore::from_path(path.clone()).expect("reload settings");
        assert_eq!(loaded.settings().theme, ThemeMode::Light);
        assert!(loaded.settings().wrap_code);
        assert!(loaded.settings().wrap_diff);
        let _ = fs::remove_file(path);
    }
}
