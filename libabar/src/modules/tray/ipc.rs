use serde::Deserialize;

/// One tray item as reported by the tray exec script (one JSON array per stdout line).
#[derive(Debug, Clone, Deserialize, PartialEq, Eq)]
pub struct MinimalTrayItem {
    pub app_id: String,
    pub title: Option<String>,
    pub status: TrayItemStatus,
    pub icon_handle: Option<String>,
    #[serde(default)]
    pub tooltip_title: Option<String>,
    #[serde(default)]
    pub tooltip_description: Option<String>,
    /// The app's own icon directory (SNI `IconThemePath`), searched as
    /// [`AppIconTheme`](super::AppIconTheme) says.
    #[serde(default)]
    pub icon_theme_path: Option<String>,
}

/// SNI item visibility status. `Passive` items are not rendered.
#[derive(Debug, Clone, Deserialize, PartialEq, Eq)]
pub enum TrayItemStatus {
    Active,
    Passive,
    NeedsAttention,
    /// Any other value; shown like `Active`, so one unexpected status
    /// does not make the whole tray update unreadable.
    #[serde(other)]
    Unknown,
}

/// Quote `s` as one `sh` word, so it reaches the command verbatim.
pub fn shell_quote(s: &str) -> String {
    format!("'{}'", s.replace('\'', "'\\''"))
}
