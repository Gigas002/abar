#![allow(dead_code)]

mod events;
mod layout;
mod modules;

use std::path::{Path, PathBuf};

use serde::Deserialize;

#[allow(unused_imports)]
pub use events::Events;
pub use layout::Layout;
#[allow(unused_imports)]
pub use layout::LayoutEntry;
pub use modules::{Clock, Keyboard, Modules, Window, Workspaces};

#[derive(Debug, Clone, Deserialize)]
pub struct Config {
    pub base: Option<Base>,
    pub layout: Option<Layout>,
    pub modules: Option<Modules>,
    pub keyboard: Option<Keyboard>,
    pub clock: Option<Clock>,
    pub workspaces: Option<Workspaces>,
    pub window: Option<Window>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct Base {
    pub font_name: Option<String>,
    pub font_size: Option<f64>,
    pub theme: Option<String>,
}

impl Default for Base {
    fn default() -> Self {
        Self {
            font_name: Some("NotoSans Nerd Font".to_string()),
            font_size: Some(14.0),
            theme: Some("theme.toml".to_string()),
        }
    }
}

impl Default for Config {
    fn default() -> Self {
        Self {
            base: Some(Base::default()),
            layout: Some(Layout::default()),
            modules: Some(Modules::default()),
            keyboard: None,
            clock: None,
            workspaces: None,
            window: None,
        }
    }
}

impl Config {
    pub fn load(path: &Path) -> Self {
        match std::fs::read_to_string(path) {
            Ok(raw) => match toml::from_str(&raw) {
                Ok(config) => config,
                Err(err) => {
                    tracing::warn!(
                        %err,
                        path = %path.display(),
                        "invalid config, using defaults"
                    );
                    Self::default()
                }
            },
            Err(err) => {
                tracing::warn!(
                    %err,
                    path = %path.display(),
                    "config not found, using defaults"
                );
                Self::default()
            }
        }
    }
}

pub fn config_dir() -> PathBuf {
    std::env::var_os("XDG_CONFIG_HOME")
        .filter(|s| !s.is_empty())
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".config")))
        .unwrap_or_else(|| PathBuf::from(".config"))
        .join("abar")
}

pub fn default_config_path() -> PathBuf {
    config_dir().join("config.toml")
}

#[cfg(test)]
mod tests;
