/// Filter controlling which workspaces are shown when the compositor reports monitor information.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub enum VisibilityMode {
    /// Show all workspaces across every monitor.
    #[default]
    AllMonitors,
    /// Show only workspaces on the same monitor as the currently active workspace.
    MonitorSpecific,
}

impl VisibilityMode {
    pub fn parse(s: &str) -> Self {
        match s {
            "monitor_specific" => Self::MonitorSpecific,
            _ => Self::AllMonitors,
        }
    }
}

/// Runtime configuration for the workspaces module passed into the Wayland run loop.
#[derive(Debug, Clone, Default)]
pub struct WorkspacesConfig {
    pub visibility_mode: VisibilityMode,
    /// Pango-compatible foreground color (`#rrggbb`) for the active workspace.
    pub active_color: Option<String>,
    /// Pango-compatible foreground color (`#rrggbb`) for inactive workspaces.
    pub inactive_color: Option<String>,
}

/// Minimal workspace descriptor used for label formatting and testing.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WorkspaceInfo {
    pub id: i32,
    pub name: String,
}

/// Format a workspace label from `workspaces` sorted by id.
///
/// Returns `(label, use_markup)`.  When colors are configured the label contains Pango markup and
/// `use_markup` is `true`; otherwise plain text with `[name]` for the active workspace is used.
pub fn format_label(
    workspaces: &[WorkspaceInfo],
    active_id: i32,
    config: &WorkspacesConfig,
) -> (String, bool) {
    if workspaces.is_empty() {
        return (String::new(), false);
    }

    let has_colors = config.active_color.is_some() || config.inactive_color.is_some();

    if has_colors {
        let parts: Vec<String> = workspaces
            .iter()
            .map(|ws| {
                let color = if ws.id == active_id {
                    config.active_color.as_deref().unwrap_or("")
                } else {
                    config.inactive_color.as_deref().unwrap_or("")
                };
                let name = pango_escape(&ws.name);
                if color.is_empty() {
                    name
                } else {
                    format!(r#"<span foreground="{color}">{name}</span>"#)
                }
            })
            .collect();
        (parts.join("  "), true)
    } else {
        let parts: Vec<String> = workspaces
            .iter()
            .map(|ws| {
                if ws.id == active_id {
                    format!("[{}]", ws.name)
                } else {
                    ws.name.clone()
                }
            })
            .collect();
        (parts.join("  "), false)
    }
}

/// Escape special XML/Pango characters in a workspace name.
fn pango_escape(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&apos;")
}

/// Trim an 8-digit hex color (`#RRGGBBAA`) to the 6-digit form Pango expects.
pub fn trim_alpha(hex: &str) -> String {
    if hex.starts_with('#') && hex.len() == 9 {
        hex[..7].to_string()
    } else {
        hex.to_string()
    }
}

#[cfg(test)]
mod tests;
