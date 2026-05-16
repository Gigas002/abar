use serde::Deserialize;

use super::events::Events;

#[derive(Debug, Clone, Deserialize)]
pub struct Custom {
    pub name: String,
    pub icon: String,
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
    pub layouts: Option<Vec<String>>,
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
    #[serde(flatten)]
    pub events: Option<Events>,
}

#[derive(Debug, Default, Clone, Deserialize)]
pub struct Window {}
