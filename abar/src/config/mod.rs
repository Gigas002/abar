use std::path::PathBuf;

use serde::Deserialize;

/// Application `config.toml`: `[base]`, `[layout]`, etc.
#[derive(Debug, Clone, Deserialize)]
pub struct Config {
    #[serde(default)]
    pub base: Option<Base>,
    #[serde(default)]
    #[allow(dead_code)]
    pub layout: Option<Layout>,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            base: Some(Base::default()),
            layout: Some(Layout::default()),
        }
    }
}

#[derive(Debug, Clone, Deserialize)]
pub struct Base {
    pub font: Option<String>,
    pub theme: Option<String>,
}

impl Default for Base {
    fn default() -> Self {
        Self {
            font: Some("NotoSans Nerd Font".to_string()),
            theme: Some("theme.toml".to_string()),
        }
    }
}

#[derive(Debug, Clone, Deserialize)]
#[allow(dead_code)]
pub struct Layout {
    pub left: Option<Vec<String>>,
    pub center: Option<Vec<String>>,
    pub right: Option<Vec<String>>,
}

impl Default for Layout {
    fn default() -> Self {
        Self {
            left: Some(Vec::new()),
            center: Some(Vec::new()),
            right: Some(Vec::new()),
        }
    }
}

impl Config {
    pub fn load(path: &std::path::Path) -> Self {
        let s = std::fs::read_to_string(path).unwrap_or_default();
        toml::from_str(&s).unwrap_or_default()
    }
}

pub fn default_path() -> PathBuf {
    std::env::var_os("XDG_CONFIG_HOME")
        .filter(|s| !s.is_empty())
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".config")))
        .unwrap_or_else(|| PathBuf::from(".config"))
        .join("abar")
        .join("config.toml")
}

#[cfg(test)]
mod tests;
