use serde::Deserialize;

/// One tray item as reported by the tray exec script (one JSON array per stdout line).
///
/// Matches trayd IPC v1 `MinimalTrayItem` (`docs/IPC.md` in the trayd repo).
#[derive(Debug, Clone, Deserialize, PartialEq, Eq)]
pub struct MinimalTrayItem {
    pub app_id: String,
    pub title: Option<String>,
    pub status: TrayItemStatus,
    pub icon_handle: Option<String>,
    /// SNI category (e.g. `"ApplicationStatus"`, `"Communications"`).
    #[serde(default)]
    pub category: Option<String>,
    /// `true` when the item is a pure menu (primary action is the menu, not activation).
    #[serde(default)]
    pub item_is_menu: bool,
    #[serde(default)]
    pub tooltip_title: Option<String>,
    #[serde(default)]
    pub tooltip_description: Option<String>,
    /// Overlay icon theme name (badge drawn on top of the normal icon).
    #[serde(default)]
    pub overlay_icon_handle: Option<String>,
}

/// SNI item visibility status. `Passive` items are not rendered.
#[derive(Debug, Clone, Deserialize, PartialEq, Eq)]
pub enum TrayItemStatus {
    Active,
    Passive,
    NeedsAttention,
}
