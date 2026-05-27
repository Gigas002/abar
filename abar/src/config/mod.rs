#![allow(dead_code)]

mod events;
mod layout;
mod modules;

use std::path::{Path, PathBuf};

use serde::Deserialize;

#[allow(unused_imports)]
pub use events::Events;
pub(crate) use events::apply_module_events;
#[allow(unused_imports)]
pub(crate) use events::events_for_module;
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
    config_dir_from_env(
        std::env::var_os("XDG_CONFIG_HOME")
            .as_deref()
            .map(|s| s.to_string_lossy())
            .as_deref(),
        std::env::var_os("HOME")
            .as_deref()
            .map(|s| s.to_string_lossy())
            .as_deref(),
    )
}

pub(crate) fn config_dir_from_env(xdg_config_home: Option<&str>, home: Option<&str>) -> PathBuf {
    xdg_config_home
        .filter(|s| !s.is_empty())
        .map(PathBuf::from)
        .or_else(|| home.map(|h| PathBuf::from(h).join(".config")))
        .unwrap_or_else(|| PathBuf::from(".config"))
        .join("abar")
}

pub fn default_config_path() -> PathBuf {
    config_dir().join("config.toml")
}

#[cfg(test)]
mod tests;
