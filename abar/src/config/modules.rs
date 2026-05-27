use serde::Deserialize;

use super::events::Events;

#[derive(Debug, Clone, Deserialize)]
pub struct SubmenuItem {
    pub content: String,
    pub action: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct Custom {
    pub name: String,
    pub icon: String,
    #[serde(default)]
    pub submenu: Vec<SubmenuItem>,
    #[serde(flatten)]
    pub events: Option<Events>,
}

#[derive(Debug, Default, Clone, Deserialize)]
pub struct Modules {
    pub custom: Option<Vec<Custom>>,
}

impl Modules {
    pub fn custom_by_name(&self, name: &str) -> Option<&Custom> {
        self.custom.as_ref()?.iter().find(|m| m.name == name)
    }
}

#[derive(Debug, Default, Clone, Deserialize)]
pub struct Keyboard {
    pub exec: Option<String>,
    #[serde(flatten)]
    pub events: Option<Events>,
}

#[derive(Debug, Default, Clone, Deserialize)]
pub struct Clock {
    pub formats: Option<Vec<String>>,
    pub timezones: Option<Vec<String>>,
    #[serde(flatten)]
    pub events: Option<Events>,
}

#[derive(Debug, Default, Clone, Deserialize)]
pub struct Workspaces {
    pub exec: Option<String>,
    #[serde(flatten)]
    pub events: Option<Events>,
}

#[derive(Debug, Default, Clone, Deserialize)]
pub struct Window {
    pub max_length: Option<usize>,
    pub exec: Option<String>,
    #[serde(flatten)]
    pub events: Option<Events>,
}

#[derive(Debug, Default, Clone, Deserialize)]
pub struct Mpris {
    pub max_length: Option<usize>,
    pub exec: Option<String>,
    #[serde(flatten)]
    pub events: Option<Events>,
}
