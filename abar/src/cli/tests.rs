use std::path::PathBuf;

use clap::Parser;

use super::Cli;

#[test]
fn cli_parses_config_and_theme_paths() {
    let cli = Cli::try_parse_from([
        "abar",
        "--config",
        "/custom/c.toml",
        "--theme",
        "/custom/t.toml",
    ])
    .unwrap();
    assert_eq!(cli.config, Some(PathBuf::from("/custom/c.toml")));
    assert_eq!(cli.theme, Some(PathBuf::from("/custom/t.toml")));
}

#[test]
fn cli_minimal() {
    Cli::try_parse_from(["abar"]).unwrap();
}
