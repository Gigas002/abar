/// Which workspaces the script should list; passed to it as `ABAR_WS_VISIBILITY`.
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
    /// Optional exec command (`sh -c <exec>`) whose stdout drives this module.
    pub exec: Option<String>,
}

impl WorkspacesConfig {
    /// Environment for the workspaces script, so it can follow the theme:
    /// `ABAR_WS_ACTIVE_COLOR`, `ABAR_WS_INACTIVE_COLOR` (when set) and
    /// `ABAR_WS_VISIBILITY` (`all_monitors` or `monitor_specific`).
    pub fn env(&self) -> Vec<(String, String)> {
        let mut env = vec![(
            "ABAR_WS_VISIBILITY".to_string(),
            match self.visibility_mode {
                VisibilityMode::AllMonitors => "all_monitors",
                VisibilityMode::MonitorSpecific => "monitor_specific",
            }
            .to_string(),
        )];
        for (key, value) in [
            ("ABAR_WS_ACTIVE_COLOR", &self.active_color),
            ("ABAR_WS_INACTIVE_COLOR", &self.inactive_color),
        ] {
            if let Some(value) = value {
                env.push((key.to_string(), value.clone()));
            }
        }
        env
    }
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
