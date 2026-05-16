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

/// Current workspace display state shared between the background task and the click handler.
#[derive(Debug, Clone, Default)]
pub struct WorkspacesDisplayState {
    pub workspaces: Vec<WorkspaceInfo>,
    pub active_id: i32,
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

/// Determine which workspace ID the user clicked on within the workspaces segment.
///
/// `click_x` is the absolute bar-coordinate x of the pointer.  `seg_x` / `seg_width` are the
/// absolute coordinates of the workspaces segment (from `PlacedSegment`).  The text is centered
/// inside the segment, matching `draw_segment_text` centering logic.
///
/// `measure(text)` returns `(width, height)` for plain text (not markup).  Individual workspace
/// names are measured as plain text regardless of `use_markup`; the separator between workspaces
/// is two spaces `"  "`.
pub fn workspace_at_x(
    click_x: f64,
    seg_x: f64,
    seg_width: f64,
    state: &WorkspacesDisplayState,
    use_markup: bool,
    measure: &impl Fn(&str) -> (f64, f64),
) -> Option<i32> {
    if state.workspaces.is_empty() {
        return None;
    }

    let sep_w = measure("  ").0;

    // Width of each workspace as rendered.
    let ws_widths: Vec<(i32, f64)> = state
        .workspaces
        .iter()
        .map(|ws| {
            let display = if !use_markup && ws.id == state.active_id {
                format!("[{}]", ws.name)
            } else {
                ws.name.clone()
            };
            (ws.id, measure(&display).0)
        })
        .collect();

    let total_w: f64 = ws_widths.iter().map(|(_, w)| w).sum::<f64>()
        + sep_w * ws_widths.len().saturating_sub(1) as f64;

    // Text is centered inside the segment, matching draw_segment_text.
    let text_start = seg_x + (seg_width - total_w) / 2.0;

    let mut cursor = text_start;
    for (id, w) in &ws_widths {
        if click_x >= cursor && click_x <= cursor + w {
            return Some(*id);
        }
        cursor += w + sep_w;
    }
    None
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
