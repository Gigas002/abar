#![forbid(unsafe_code)]

mod app;
mod cli;
mod config;
mod error;
mod logger;
mod settings;
mod theme;

use std::process::ExitCode;

use clap::Parser;

use crate::cli::Cli;
use crate::config::{Config, default_config_path};
use crate::theme::Theme;

fn main() -> ExitCode {
    logger::init();
    let cli = Cli::parse();

    let config_path = cli.config.clone().unwrap_or_else(default_config_path);
    let config = match Config::load(&config_path, cli.config.is_some()) {
        Ok(config) => config,
        Err(err) => {
            tracing::error!(%err, "cannot load the config");
            return ExitCode::from(1);
        }
    };

    let theme_name = config
        .base
        .as_ref()
        .and_then(|b| b.theme.as_deref())
        .unwrap_or("theme.toml");
    let theme_path = cli
        .theme
        .clone()
        .unwrap_or_else(|| theme::resolve_path(&config_path, theme_name));
    // The theme must exist when it was named on the command line or in a config file.
    let theme_named = cli.theme.is_some()
        || (config_path.is_file() && config.base.as_ref().is_some_and(|b| b.theme.is_some()));
    let theme = match Theme::load(&theme_path, theme_named) {
        Ok(theme) => theme,
        Err(err) => {
            tracing::error!(%err, "cannot load the theme");
            return ExitCode::from(1);
        }
    };

    let settings = match settings::Settings::resolve(&cli, config, theme) {
        Ok(s) => s,
        Err(err) => {
            tracing::error!(%err, "failed to resolve settings");
            return ExitCode::from(1);
        }
    };

    tracing::info!(
        font_name = %settings.font_name(),
        font_size = settings.font_size(),
        theme = %theme_path.display(),
        "abar starting"
    );

    app::run(settings)
}
