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
    let config = Config::load(&config_path);

    let theme_name = config
        .base
        .as_ref()
        .and_then(|b| b.theme.as_deref())
        .unwrap_or("theme.toml");
    let theme_path = cli
        .theme
        .clone()
        .unwrap_or_else(|| theme::resolve_path(&config_path, theme_name));
    let theme = Theme::load(&theme_path);

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
