use serde::Deserialize;
use std::collections::HashMap;
use std::path::Path;

#[derive(Debug, Clone, Deserialize)]
#[serde(default)]
pub struct Config {
    pub general: GeneralConfig,
    pub keybindings: HashMap<String, String>,
    pub appearance: AppearanceConfig,
    pub history: HistoryConfig,
    pub devtools: DevToolsConfig,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(default)]
pub struct GeneralConfig {
    pub startup_url: String,
    pub search_engine: String,
    pub autosave_interval_secs: u64,
    pub restore_workspace_on_startup: bool,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(default)]
pub struct AppearanceConfig {
    pub hud_opacity: f32,
    pub font_size: u32,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(default)]
pub struct HistoryConfig {
    pub max_entries: usize,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(default)]
pub struct DevToolsConfig {
    pub enabled: bool,
    pub tcp_port: u16,
    pub ws_port: u16,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            general: GeneralConfig::default(),
            keybindings: HashMap::new(),
            appearance: AppearanceConfig::default(),
            history: HistoryConfig::default(),
            devtools: DevToolsConfig::default(),
        }
    }
}

impl Default for GeneralConfig {
    fn default() -> Self {
        Self {
            startup_url: "https://servo.org".into(),
            search_engine: "https://duckduckgo.com/?q={}".into(),
            autosave_interval_secs: 60,
            restore_workspace_on_startup: true,
        }
    }
}

impl Default for AppearanceConfig {
    fn default() -> Self {
        Self {
            hud_opacity: 0.9,
            font_size: 14,
        }
    }
}

impl Default for HistoryConfig {
    fn default() -> Self {
        Self {
            max_entries: 10000,
        }
    }
}

impl Default for DevToolsConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            tcp_port: 7000,
            ws_port: 9222,
        }
    }
}

impl Config {
    pub fn load(path: &Path) -> Self {
        log::info!("Config::load: attempting to load from {}", path.display());
        match std::fs::read_to_string(path) {
            Ok(contents) => match toml::from_str(&contents) {
                Ok(config) => {
                    log::info!("Config::load: loaded successfully from {}", path.display());
                    config
                }
                Err(e) => {
                    log::warn!("Config::load: failed to parse {}: {}. Using defaults.", path.display(), e);
                    Config::default()
                }
            },
            Err(e) => {
                log::info!("Config::load: no config file at {} ({}). Using defaults.", path.display(), e);
                Config::default()
            }
        }
    }

    pub fn search_url(&self, query: &str) -> String {
        let url = self.general.search_engine.replace("{}", query);
        log::trace!("Config::search_url: query='{}' -> url='{}'", query, url);
        url
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_config_has_sane_values() {
        let config = Config::default();
        assert_eq!(config.general.startup_url, "https://servo.org");
        assert_eq!(config.general.autosave_interval_secs, 60);
        assert!(config.general.restore_workspace_on_startup);
        assert_eq!(config.history.max_entries, 10000);
    }

    #[test]
    fn parse_minimal_toml() {
        let toml_str = r#"
[general]
startup_url = "https://example.com"
"#;
        let config: Config = toml::from_str(toml_str).unwrap();
        assert_eq!(config.general.startup_url, "https://example.com");
        assert_eq!(config.general.autosave_interval_secs, 60);
        assert_eq!(config.history.max_entries, 10000);
    }

    #[test]
    fn parse_keybindings() {
        let toml_str = r#"
[keybindings]
focus_left = "a"
split_vertical = "ctrl+v"
"#;
        let config: Config = toml::from_str(toml_str).unwrap();
        assert_eq!(config.keybindings.get("focus_left").unwrap(), "a");
        assert_eq!(config.keybindings.get("split_vertical").unwrap(), "ctrl+v");
    }

    #[test]
    fn search_url_substitution() {
        let config = Config::default();
        let url = config.search_url("rust lang");
        assert_eq!(url, "https://duckduckgo.com/?q=rust lang");
    }

    #[test]
    fn missing_file_returns_defaults() {
        let config = Config::load(Path::new("/nonexistent/path/config.toml"));
        assert_eq!(config.general.startup_url, "https://servo.org");
    }

    #[test]
    fn empty_toml_returns_defaults() {
        let config: Config = toml::from_str("").unwrap();
        assert_eq!(config.general.startup_url, "https://servo.org");
    }
}
