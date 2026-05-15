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

fn main() -> ExitCode {
    logger::init();
    let cli = Cli::parse();

    let config_path = cli.config.clone().unwrap_or_else(config::default_path);
    let config = config::Config::load(&config_path);

    let theme_path = cli.theme.clone().unwrap_or_else(|| {
        let theme_name = config
            .base
            .clone()
            .unwrap_or_default()
            .theme
            .unwrap_or_default();
        theme::resolve_path(&config_path, theme_name.as_str())
    });
    let theme = theme::Theme::load(&theme_path);

    let settings = match settings::Settings::resolve(&cli, config, theme) {
        Ok(s) => s,
        Err(err) => {
            tracing::error!(%err, "failed to resolve settings");
            return ExitCode::from(1);
        }
    };

    tracing::info!(
        font = %settings.font,
        theme = %theme_path.display(),
        "abar starting"
    );

    app::run(settings)
}
