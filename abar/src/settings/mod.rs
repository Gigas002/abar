use libabar::color::parse_hex_rgba_to_bgra;
use libabar::{
    BarColors, BarLayout, BarSpec, BarStyle, DisplayMode, default_search_dirs, resolve_icon,
};

use crate::cli::Cli;
use crate::config::{self, Base as ConfigBase, Config};
use crate::error::Error;
use crate::theme::{Base as ThemeBase, Theme};

#[derive(Debug)]
pub struct Settings {
    pub bar: BarSpec,
}

impl Settings {
    pub fn resolve(_cli: &Cli, config: Config, theme: Theme) -> Result<Self, Error> {
        let modules = config.modules.as_ref();

        let mut layout = config
            .layout
            .as_ref()
            .map(|l| l.to_bar_layout(modules))
            .unwrap_or_default();
        config::apply_module_events(&mut layout, &config);

        let theme_base = theme.base.unwrap_or_default();
        let config_base = config.base.clone().unwrap_or_default();

        let background = theme_base
            .background_color
            .unwrap_or_else(|| ThemeBase::default().background_color.unwrap());
        let foreground = theme_base
            .foreground_color
            .unwrap_or_else(|| ThemeBase::default().foreground_color.unwrap());
        let font_name = config_base
            .font_name
            .unwrap_or_else(|| ConfigBase::default().font_name.unwrap());
        let font_size = config_base
            .font_size
            .unwrap_or_else(|| ConfigBase::default().font_size.unwrap());

        // Try to resolve each custom icon; fall back to text for any that are missing.
        apply_icon_fallbacks(&mut layout, font_size);

        Ok(Self {
            bar: BarSpec::new(
                BarColors {
                    background: parse_hex_rgba_to_bgra(&background)?,
                    foreground: parse_hex_rgba_to_bgra(&foreground)?,
                },
                BarStyle {
                    font_name,
                    font_size,
                    ..BarStyle::default()
                },
                layout,
            ),
        })
    }
}

impl Settings {
    pub fn font_name(&self) -> &str {
        &self.bar.style.font_name
    }

    pub fn font_size(&self) -> f64 {
        self.bar.style.font_size
    }
}

/// For every icon-only segment whose icon cannot be resolved, switch it to text mode so the
/// module name is displayed instead. Runs once at startup before the bar spec is finalized.
fn apply_icon_fallbacks(layout: &mut BarLayout, font_size: f64) {
    let search_dirs = default_search_dirs();
    let theme_name = std::env::var("XDG_ICON_THEME").unwrap_or_else(|_| "hicolor".to_string());
    let size = font_size.round() as u32;

    for island in layout
        .left
        .iter_mut()
        .chain(layout.center.iter_mut())
        .chain(layout.right.iter_mut())
    {
        for seg in &mut island.segments {
            if seg.display_mode != DisplayMode::IconOnly {
                continue;
            }
            let Some(icon_name) = &seg.icon_name else {
                continue;
            };
            if resolve_icon(icon_name, size, &search_dirs, &theme_name).is_none() {
                tracing::warn!(
                    module = %seg.module_id,
                    icon = %icon_name,
                    "icon not found in theme, falling back to text"
                );
                seg.display_mode = DisplayMode::TextOnly;
            }
        }
    }
}

#[cfg(test)]
mod tests;
