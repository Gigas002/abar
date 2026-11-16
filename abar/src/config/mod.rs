#![allow(dead_code)]

mod events;
mod layout;
mod modules;

use std::path::{Path, PathBuf};

use serde::Deserialize;
use serde::de::DeserializeOwned;

use crate::error::Error;

#[allow(unused_imports)]
pub use events::Events;
pub(crate) use events::apply_module_events;
#[allow(unused_imports)]
pub(crate) use events::events_for_module;
pub use layout::Layout;
#[allow(unused_imports)]
pub use layout::LayoutEntry;
pub use modules::{Clock, Keyboard, Modules, Mpris, Tray, Window, Workspaces};

#[derive(Debug, Clone, Deserialize)]
pub struct Config {
    pub base: Option<Base>,
    pub icons: Option<Icons>,
    pub layout: Option<Layout>,
    pub modules: Option<Modules>,
    pub keyboard: Option<Keyboard>,
    pub clock: Option<Clock>,
    pub workspaces: Option<Workspaces>,
    pub window: Option<Window>,
    pub mpris: Option<Mpris>,
    pub tray: Option<Tray>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct Base {
    pub font_name: Option<String>,
    pub font_size: Option<f64>,
    pub theme: Option<String>,
}

/// Icon lookup configuration.
#[derive(Debug, Clone, Deserialize)]
pub struct Icons {
    /// `"theme"` = prefer user's icon theme (strip suffixes per-theme).
    /// `"exact"` = strict FreeDesktop spec (default).
    #[serde(default)]
    pub prefer: Option<String>,
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
            icons: None,
            layout: Some(Layout::default()),
            modules: Some(Modules::default()),
            keyboard: None,
            clock: None,
            workspaces: None,
            window: None,
            mpris: None,
            tray: None,
        }
    }
}

impl Config {
    /// Load the config at `path`. A missing file gives the defaults unless
    /// `required` (the user named it with `--config`); a file that is not
    /// valid is an error, so a typo never silently resets the bar.
    pub fn load(path: &Path, required: bool) -> Result<Self, Error> {
        let Some(config) = load_toml::<Self>(path, required)? else {
            tracing::info!(path = %path.display(), "no config file, using defaults");
            return Ok(Self::default());
        };
        for key in config.unknown_module_keys() {
            tracing::warn!(path = %path.display(), %key, "unknown config key ignored");
        }
        Ok(config)
    }

    /// Keys in module tables that no module reads (they would otherwise be
    /// swallowed by the flattened event handlers).
    fn unknown_module_keys(&self) -> Vec<String> {
        let mut tables: Vec<(String, Option<&Events>)> = vec![
            (
                "keyboard".into(),
                self.keyboard.as_ref().and_then(|m| m.events.as_ref()),
            ),
            (
                "clock".into(),
                self.clock.as_ref().and_then(|m| m.events.as_ref()),
            ),
            (
                "workspaces".into(),
                self.workspaces.as_ref().and_then(|m| m.events.as_ref()),
            ),
            (
                "window".into(),
                self.window.as_ref().and_then(|m| m.events.as_ref()),
            ),
            (
                "mpris".into(),
                self.mpris.as_ref().and_then(|m| m.events.as_ref()),
            ),
            (
                "tray".into(),
                self.tray.as_ref().and_then(|m| m.events.as_ref()),
            ),
        ];
        for custom in self.modules.iter().flat_map(|m| m.custom.iter().flatten()) {
            tables.push((
                format!("modules.custom.{}", custom.name),
                custom.events.as_ref(),
            ));
        }
        tables
            .into_iter()
            .flat_map(|(table, events)| {
                events
                    .into_iter()
                    .flat_map(|e| e.unknown.keys())
                    .map(move |key| format!("{table}.{key}"))
            })
            .collect()
    }
}

/// Read and parse a TOML file, warning about keys nothing reads.
///
/// `Ok(None)` when the file does not exist and is not `required`.
pub(crate) fn load_toml<T: DeserializeOwned>(
    path: &Path,
    required: bool,
) -> Result<Option<T>, Error> {
    let raw = match std::fs::read_to_string(path) {
        Ok(raw) => raw,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound && !required => return Ok(None),
        Err(source) => {
            return Err(Error::Read {
                path: path.to_path_buf(),
                source,
            });
        }
    };
    let (value, unknown) = parse_toml(&raw).map_err(|source| Error::Invalid {
        path: path.to_path_buf(),
        source,
    })?;
    for key in unknown {
        tracing::warn!(path = %path.display(), %key, "unknown key ignored");
    }
    Ok(Some(value))
}

/// Parse TOML, also returning the keys that nothing read.
pub(crate) fn parse_toml<T: DeserializeOwned>(
    raw: &str,
) -> Result<(T, Vec<String>), toml::de::Error> {
    let mut unknown = Vec::new();
    let value = serde_ignored::deserialize(toml::Deserializer::parse(raw)?, |key| {
        unknown.push(key_path(&key));
    })?;
    Ok((value, unknown))
}

/// `base.font_sizee`, `modules.custom[0].x`: the key as written in the file.
fn key_path(path: &serde_ignored::Path<'_>) -> String {
    use serde_ignored::Path;
    match path {
        Path::Root => String::new(),
        Path::Seq { parent, index } => format!("{}[{index}]", key_path(parent)),
        Path::Map { parent, key } => match key_path(parent) {
            p if p.is_empty() => key.clone(),
            p => format!("{p}.{key}"),
        },
        Path::Some { parent }
        | Path::NewtypeStruct { parent }
        | Path::NewtypeVariant { parent } => key_path(parent),
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
