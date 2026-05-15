use std::path::PathBuf;

use clap::Parser;

use super::Cli;

#[test]
fn cli_parses_config_and_theme_paths() {
    let cli = Cli::try_parse_from([
        "abar",
        "--config",
        "/tmp/c.toml",
        "--theme",
        "/tmp/t.toml",
    ])
    .unwrap();
    assert_eq!(cli.config, Some(PathBuf::from("/tmp/c.toml")));
    assert_eq!(cli.theme, Some(PathBuf::from("/tmp/t.toml")));
}

#[test]
fn cli_minimal() {
    Cli::try_parse_from(["abar"]).unwrap();
}
