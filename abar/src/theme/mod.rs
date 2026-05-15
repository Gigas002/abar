//! Theme schema (`theme.toml`). Fields are consumed in later phases.

#![allow(dead_code)]

use std::path::{Path, PathBuf};

use serde::Deserialize;

use crate::config::config_dir;
use crate::error::Error;

#[derive(Debug, Clone, Deserialize)]
pub struct Theme {
    pub base: Option<Base>,
    pub workspaces: Option<Workspaces>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct Base {
    pub background_color: Option<String>,
    pub foreground_color: Option<String>,
}

#[derive(Debug, Default, Clone, Deserialize)]
pub struct Workspaces {
    pub visibility_mode: Option<String>,
    pub active_color: Option<String>,
    pub inactive_color: Option<String>,
}

impl Default for Base {
    fn default() -> Self {
        Self {
            background_color: Some("#000000FF".to_string()),
            foreground_color: Some("#FFFFFFFF".to_string()),
        }
    }
}

impl Default for Theme {
    fn default() -> Self {
        Self {
            base: Some(Base::default()),
            workspaces: None,
        }
    }
}

pub fn themes_dir() -> PathBuf {
    config_dir().join("themes")
}

impl Theme {
    pub fn load(path: &Path) -> Self {
        match std::fs::read_to_string(path) {
            Ok(raw) => match toml::from_str(&raw) {
                Ok(theme) => theme,
                Err(err) => {
                    tracing::warn!(
                        %err,
                        path = %path.display(),
                        "invalid theme, using defaults"
                    );
                    Self::default()
                }
            },
            Err(err) => {
                tracing::warn!(
                    %err,
                    path = %path.display(),
                    "theme not found, using defaults"
                );
                Self::default()
            }
        }
    }

    pub fn parse_str(raw: &str) -> Result<Self, Error> {
        toml::from_str(raw).map_err(Error::from)
    }
}

pub fn resolve_path(config_path: &Path, theme: &str) -> PathBuf {
    let theme_path = Path::new(theme);
    if theme_path.is_absolute() {
        return theme_path.to_path_buf();
    }

    if let Some(parent) = config_path.parent() {
        let direct = parent.join(theme);
        if direct.is_file() {
            return direct;
        }
        let under_themes = parent.join("themes").join(theme);
        if under_themes.is_file() {
            return under_themes;
        }
    }

    let xdg = themes_dir().join(theme);
    if xdg.is_file() {
        return xdg;
    }

    themes_dir().join(theme)
}

#[cfg(test)]
mod tests;
