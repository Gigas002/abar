/// Runtime configuration for the window module.
#[derive(Debug, Clone)]
pub struct WindowConfig {
    /// Maximum number of Unicode scalar values to display before appending "…".
    ///
    /// Zero means no limit.
    pub max_length: usize,
}

impl Default for WindowConfig {
    fn default() -> Self {
        Self { max_length: 50 }
    }
}

/// Truncate a window title to at most `max_length` Unicode scalar values, appending "…" if cut.
///
/// When `max_length` is zero the title is returned unchanged.
pub fn truncate_title(title: &str, max_length: usize) -> String {
    if max_length == 0 {
        return title.to_string();
    }
    let mut chars = title.chars();
    let truncated: String = chars.by_ref().take(max_length).collect();
    if chars.next().is_some() {
        format!("{truncated}…")
    } else {
        truncated
    }
}

#[cfg(test)]
mod tests;
