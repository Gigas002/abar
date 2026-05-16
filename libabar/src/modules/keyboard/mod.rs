/// Configuration for the keyboard module passed into the Wayland run loop.
#[derive(Debug, Clone, Default)]
pub struct KeyboardConfig {
    /// Fallback layout labels from config, used before the compositor reports the keymap.
    pub layouts: Vec<String>,
}

/// Extract layout names from an XKB keymap string using libxkbcommon.
///
/// Returns one name per layout group, in group order.
pub fn parse_layout_names(keymap: &str) -> Vec<String> {
    use xkbcommon::xkb;
    let ctx = xkb::Context::new(xkb::CONTEXT_NO_FLAGS);
    let Some(km) = xkb::Keymap::new_from_string(
        &ctx,
        keymap.to_string(),
        xkb::KEYMAP_FORMAT_TEXT_V1,
        xkb::KEYMAP_COMPILE_NO_FLAGS,
    ) else {
        return Vec::new();
    };
    (0..km.num_layouts()).map(|i| km.layout_get_name(i).to_string()).collect()
}

/// Return the display label for the given layout group index.
///
/// Config labels take priority: if the user configured `layouts = [...]`, those are shown.
/// Falls back to `xkb_layouts` (parsed from the compositor keymap) when config is absent,
/// and to `"?"` when both slices are exhausted.
pub fn current_label(xkb_layouts: &[String], config_layouts: &[String], group: u32) -> String {
    let layouts = if !config_layouts.is_empty() {
        config_layouts
    } else {
        xkb_layouts
    };
    layouts
        .get(group as usize)
        .cloned()
        .unwrap_or_else(|| "?".to_string())
}

#[cfg(test)]
mod tests;
