use crate::model::SegmentEvents;

pub mod ipc;

/// Runtime configuration for the tray module passed into the Wayland run loop.
#[derive(Debug, Clone, Default)]
pub struct TrayConfig {
    /// Optional exec command (`sh -c <exec>`) whose stdout drives this module.
    /// Each stdout line must be a JSON array of `MinimalTrayItem`.
    pub exec: Option<String>,
    /// When `true`, each item's `app_id` is appended to every configured `on_*`
    /// handler when tray segments are rebuilt, e.g. `tray-menu.sh <app_id>`.
    pub feed_id: bool,
    /// Pointer-action handlers applied to every tray item segment.
    pub events: SegmentEvents,
}

#[cfg(test)]
mod tests;
