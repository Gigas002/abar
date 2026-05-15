use libabar::color::parse_hex_rgba_to_bgra;

use crate::cli::Cli;
use crate::config::Config;
use crate::error::Error;
use crate::theme::Theme;

#[derive(Debug)]
pub struct Settings {
    pub font: String,
    pub background_color: [u8; 4],
    #[allow(dead_code)]
    pub foreground_color: [u8; 4],
}

impl Settings {
    #[allow(unused_variables)]
    pub fn resolve(cli: &Cli, config: Config, theme: Theme) -> Result<Self, Error> {
        let theme_base = theme.base.unwrap_or_default();
        let background = theme_base.background_color.unwrap_or_default();
        let foreground = theme_base.foreground_color.unwrap_or_default();

        let font = config.base.unwrap_or_default().font.unwrap_or_default();

        Ok(Self {
            font,
            background_color: parse_hex_rgba_to_bgra(&background)?,
            foreground_color: parse_hex_rgba_to_bgra(&foreground)?,
        })
    }
}

#[cfg(test)]
mod tests;
