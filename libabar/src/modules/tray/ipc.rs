use serde::Deserialize;

/// One tray item as reported by the tray exec script (one JSON array per stdout line).
#[derive(Debug, Clone, Deserialize, PartialEq, Eq)]
pub struct MinimalTrayItem {
    pub app_id: String,
    pub title: Option<String>,
    pub status: TrayItemStatus,
    pub icon_handle: Option<String>,
}

/// SNI item visibility status. `Passive` items are not rendered.
#[derive(Debug, Clone, Deserialize, PartialEq, Eq)]
pub enum TrayItemStatus {
    Active,
    Passive,
    NeedsAttention,
}
