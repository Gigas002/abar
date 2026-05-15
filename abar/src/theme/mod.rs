use std::path::{Path, PathBuf};

use serde::Deserialize;

#[derive(Debug, Clone, Deserialize)]
pub struct Theme {
    pub base: Option<Base>,
    #[allow(dead_code)]
    pub workspaces: Option<Workspaces>,
}

impl Default for Theme {
    fn default() -> Self {
        Self {
            base: Some(Base::default()),
            workspaces: Some(Workspaces::default()),
        }
    }
}

#[derive(Debug, Clone, Deserialize)]
pub struct Base {
    pub background_color: Option<String>,
    pub foreground_color: Option<String>,
}

impl Default for Base {
    fn default() -> Self {
        Self {
            background_color: Some("#000000FF".to_string()),
            foreground_color: Some("#FFFFFFFF".to_string()),
        }
    }
}

#[allow(dead_code)]
#[derive(Debug, Clone, Deserialize)]
pub struct Workspaces {
    pub visibility_mode: Option<String>,
    pub active_color: Option<String>,
    pub inactive_color: Option<String>,
}

impl Default for Workspaces {
    fn default() -> Self {
        Self {
            visibility_mode: Some(String::new()),
            active_color: Some("#00000000".to_string()),
            inactive_color: Some("#00000000".to_string()),
        }
    }
}

impl Theme {
    pub fn load(path: &Path) -> Self {
        let s = std::fs::read_to_string(path).unwrap_or_default();
        toml::from_str(&s).unwrap_or_default()
    }
}

pub fn resolve_path(config_path: &Path, theme: &str) -> PathBuf {
    let theme_path = Path::new(theme);
    if theme_path.is_absolute() {
        return theme_path.to_path_buf();
    }
    let base_dir = config_path
        .parent()
        .map(Path::to_path_buf)
        .unwrap_or_else(|| PathBuf::from("."));

    let direct = base_dir.join(theme);
    if direct.exists() {
        return direct;
    }

    base_dir.join("themes").join(theme)
}

#[cfg(test)]
mod tests;
