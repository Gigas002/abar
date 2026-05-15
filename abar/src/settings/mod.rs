use libabar::color::parse_hex_rgba_to_bgra;

use crate::cli::Cli;
use crate::config::{Base as ConfigBase, Config};
use crate::error::Error;
use crate::theme::{Base as ThemeBase, Theme};

#[derive(Debug)]
#[allow(dead_code)]
pub struct Settings {
    pub font: String,
    pub background_color: [u8; 4],
    pub foreground_color: [u8; 4],
}

impl Settings {
    pub fn resolve(_cli: &Cli, config: Config, theme: Theme) -> Result<Self, Error> {
        let theme_base = theme.base.unwrap_or_default();
        let config_base = config.base.unwrap_or_default();

        let background = theme_base
            .background_color
            .unwrap_or_else(|| ThemeBase::default().background_color.unwrap());
        let foreground = theme_base
            .foreground_color
            .unwrap_or_else(|| ThemeBase::default().foreground_color.unwrap());
        let font = config_base
            .font
            .unwrap_or_else(|| ConfigBase::default().font.unwrap());

        Ok(Self {
            font,
            background_color: parse_hex_rgba_to_bgra(&background)?,
            foreground_color: parse_hex_rgba_to_bgra(&foreground)?,
        })
    }
}

#[cfg(test)]
mod tests;
