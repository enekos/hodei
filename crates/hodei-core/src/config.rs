use serde::Deserialize;
use std::collections::HashMap;
use std::path::Path;

#[derive(Debug, Clone, Default, Deserialize)]
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
        // URL-escape the query so characters like `&`, `#`, `?`, space survive
        // the round trip to the search engine; the template is responsible for
        // placing `{}` in a URL position where a percent-encoded value is safe.
        let escaped = urlencoding_like(query);
        let engine = &self.general.search_engine;
        let url = if engine.contains("{}") {
            engine.replace("{}", &escaped)
        } else {
            // Template forgot the placeholder — fall back to appending so the
            // query isn't silently swallowed. This matches the behaviour users
            // expect when they paste a URL like `https://ddg.com/?q=` without
            // the trailing `{}`.
            format!("{}{}", engine, escaped)
        };
        log::trace!("Config::search_url: query='{}' -> url='{}'", query, url);
        url
    }

    /// Log warnings for obviously-broken config values. Returns the number of
    /// warnings emitted so tests can assert on them.
    pub fn validate(&self) -> usize {
        let mut warnings = 0;
        if !self.general.search_engine.contains("{}") {
            log::warn!(
                "config: search_engine '{}' has no '{{}}' placeholder — query will be appended verbatim",
                self.general.search_engine
            );
            warnings += 1;
        }
        if !(0.0..=1.0).contains(&self.appearance.hud_opacity) {
            log::warn!(
                "config: appearance.hud_opacity={} out of [0, 1]; clamp on render",
                self.appearance.hud_opacity
            );
            warnings += 1;
        }
        if self.devtools.tcp_port == self.devtools.ws_port {
            log::warn!(
                "config: devtools.tcp_port == ws_port ({}); servers will fail to bind",
                self.devtools.tcp_port
            );
            warnings += 1;
        }
        warnings
    }
}

/// Tiny percent-encoder for the unsafe characters that show up in browser
/// queries. Avoids pulling a crate for ~12 characters. Not RFC-complete;
/// anything not in this set is left alone (including UTF-8 bytes), matching
/// what most search engines tolerate.
fn urlencoding_like(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for b in s.bytes() {
        match b {
            b' ' => out.push_str("%20"),
            b'#' => out.push_str("%23"),
            b'&' => out.push_str("%26"),
            b'?' => out.push_str("%3F"),
            b'+' => out.push_str("%2B"),
            b'=' => out.push_str("%3D"),
            _ => out.push(b as char),
        }
    }
    out
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
    fn search_url_substitution_percent_encodes_space() {
        let config = Config::default();
        let url = config.search_url("rust lang");
        assert_eq!(url, "https://duckduckgo.com/?q=rust%20lang");
    }

    #[test]
    fn search_url_encodes_special_chars() {
        let config = Config::default();
        let url = config.search_url("a&b #c");
        assert_eq!(url, "https://duckduckgo.com/?q=a%26b%20%23c");
    }

    #[test]
    fn search_url_missing_placeholder_appends_query() {
        let mut config = Config::default();
        config.general.search_engine = "https://example.com/?q=".into();
        let url = config.search_url("hello world");
        assert_eq!(url, "https://example.com/?q=hello%20world");
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

    #[test]
    fn partial_override_keeps_other_defaults() {
        let toml_str = r#"
[appearance]
hud_opacity = 0.5
"#;
        let config: Config = toml::from_str(toml_str).unwrap();
        assert!((config.appearance.hud_opacity - 0.5).abs() < 1e-6);
        // Untouched fields keep their defaults.
        assert_eq!(config.general.startup_url, "https://servo.org");
        assert_eq!(config.devtools.tcp_port, 7000);
    }

    #[test]
    fn malformed_toml_falls_back_via_load() {
        // load() swallows parse errors and returns defaults.
        let dir = std::env::temp_dir().join("hodei-test-malformed");
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("config.toml");
        std::fs::write(&path, "[[[not toml").unwrap();
        let config = Config::load(&path);
        assert_eq!(config.general.startup_url, "https://servo.org");
        std::fs::remove_file(&path).ok();
    }

    #[test]
    fn validate_flags_missing_placeholder() {
        let mut config = Config::default();
        config.general.search_engine = "https://example.com/".into();
        assert!(config.validate() >= 1);
    }

    #[test]
    fn validate_flags_out_of_range_opacity() {
        let mut config = Config::default();
        config.appearance.hud_opacity = 1.5;
        assert!(config.validate() >= 1);
    }

    #[test]
    fn validate_flags_port_collision() {
        let mut config = Config::default();
        config.devtools.ws_port = config.devtools.tcp_port;
        assert!(config.validate() >= 1);
    }

    #[test]
    fn validate_clean_config_is_silent() {
        assert_eq!(Config::default().validate(), 0);
    }

    #[test]
    fn devtools_port_defaults() {
        let d = DevToolsConfig::default();
        assert!(d.enabled);
        assert_eq!(d.tcp_port, 7000);
        assert_eq!(d.ws_port, 9222);
    }
}
