use libabar::color::parse_hex_rgba_to_bgra;
use libabar::{BarColors, BarSpec, BarStyle};

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
        let mut layout = config
            .layout
            .as_ref()
            .map(|l| l.to_bar_layout())
            .unwrap_or_default();
        config::apply_module_events(&mut layout, &config);

        let theme_base = theme.base.unwrap_or_default();
        let config_base = config.base.unwrap_or_default();

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

#[cfg(test)]
mod tests;
