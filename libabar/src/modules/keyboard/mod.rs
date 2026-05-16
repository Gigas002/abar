/// Configuration for the keyboard module passed into the Wayland run loop.
#[derive(Debug, Clone, Default)]
pub struct KeyboardConfig {
    /// Fallback layout labels from config, used before the compositor reports the keymap.
    pub layouts: Vec<String>,
}

/// Extract layout names from an XKB keymap string.
///
/// Scans for `name[GroupN] = "…";` lines inside the `xkb_symbols` section.
/// Returns names sorted by group number (Group1 first).
pub fn parse_layout_names(keymap: &str) -> Vec<String> {
    let mut layouts: Vec<(usize, String)> = Vec::new();

    for line in keymap.lines() {
        let t = line.trim();
        let Some(rest) = t.strip_prefix("name[Group") else {
            continue;
        };
        let Some(bracket) = rest.find(']') else {
            continue;
        };
        let Ok(group_num) = rest[..bracket].parse::<usize>() else {
            continue;
        };
        let Some(q_start) = rest.find('"') else {
            continue;
        };
        let after_q = &rest[q_start + 1..];
        let Some(q_end) = after_q.find('"') else {
            continue;
        };
        layouts.push((group_num, after_q[..q_end].to_string()));
    }

    layouts.sort_by_key(|(n, _)| *n);
    layouts.into_iter().map(|(_, name)| name).collect()
}

/// Return the display label for the given layout group index.
///
/// Prefers `xkb_layouts` (parsed from the compositor keymap) over `config_layouts`
/// (static fallback from config). Falls back to `"?"` when both slices are exhausted.
pub fn current_label(xkb_layouts: &[String], config_layouts: &[String], group: u32) -> String {
    let layouts = if !xkb_layouts.is_empty() {
        xkb_layouts
    } else {
        config_layouts
    };
    layouts
        .get(group as usize)
        .cloned()
        .unwrap_or_else(|| "?".to_string())
}

#[cfg(test)]
mod tests;
