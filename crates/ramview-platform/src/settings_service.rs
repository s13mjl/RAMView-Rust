use std::path::PathBuf;

use ramview_core::models::AppSettings;

/// Persists and loads user settings locally via JSON (§19, §20).
pub struct SettingsService {
    settings_path: PathBuf,
    current_settings: AppSettings,
}

impl Default for SettingsService {
    fn default() -> Self {
        Self::new(None)
    }
}

impl SettingsService {
    pub fn new(custom_path: Option<PathBuf>) -> Self {
        let settings_path = match custom_path {
            Some(path) if !path.as_os_str().is_empty() => path,
            _ => {
                let app_data = std::env::var_os("LOCALAPPDATA")
                    .map(PathBuf::from)
                    .unwrap_or_default();
                app_data.join("RAMView").join("settings.json")
            }
        };

        Self {
            settings_path,
            current_settings: AppSettings::default(),
        }
    }

    pub fn settings_path(&self) -> &PathBuf {
        &self.settings_path
    }

    pub fn current_settings(&self) -> &AppSettings {
        &self.current_settings
    }

    pub fn current_settings_mut(&mut self) -> &mut AppSettings {
        &mut self.current_settings
    }

    /// Loads settings from disk, falling back to defaults on any I/O or parse error.
    pub fn load_settings(&mut self) -> AppSettings {
        if self.settings_path.is_file() {
            if let Ok(json) = std::fs::read_to_string(&self.settings_path) {
                if let Ok(loaded) = serde_json::from_str::<AppSettings>(&json) {
                    self.current_settings = loaded;
                    return self.current_settings.clone();
                }
            }
        }

        self.current_settings = AppSettings::default();
        self.current_settings.clone()
    }

    /// Saves settings with the same indented PascalCase JSON System.Text.Json writes.
    pub fn save_settings(&mut self, settings: AppSettings) {
        self.current_settings = settings;

        if let Some(dir) = self.settings_path.parent() {
            if !dir.as_os_str().is_empty() && !dir.is_dir() {
                let _ = std::fs::create_dir_all(dir);
            }
        }

        if let Ok(json) = serde_json::to_string_pretty(&self.current_settings) {
            let _ = std::fs::write(&self.settings_path, json);
        }
    }
}
