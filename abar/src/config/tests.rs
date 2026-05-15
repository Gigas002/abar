use clap::Parser;

use super::{Config, default_path};
use crate::cli::Cli;
use crate::settings::Settings;
use crate::theme::Theme;

const MINIMAL: &str = r#"
[base]
font = "Sans"
theme = "theme.toml"

[layout]
left = []
center = []
right = []
"#;

#[test]
fn deserialize_minimal_ok() {
    let cfg: Config = toml::from_str(MINIMAL).unwrap();
    let b = cfg.base.unwrap_or_default();
    assert_eq!(b.font.as_deref(), Some("Sans"));
    assert_eq!(b.theme.as_deref(), Some("theme.toml"));
}

#[test]
fn no_base_section_uses_default_font() {
    let raw = r#"
[layout]
left = []
center = []
right = []
"#;
    let cfg: Config = toml::from_str(raw).unwrap();
    let cli = Cli::try_parse_from(["abar"]).unwrap();
    let resolved = Settings::resolve(&cli, cfg, Theme::default()).unwrap();
    assert_eq!(resolved.font, "NotoSans Nerd Font");
}

#[test]
fn deserialize_allows_extra_top_level_tables() {
    let s = r#"
[base]
font = "Sans"
theme = "theme.toml"

[layout]
left = []
center = []
right = []

[custom_modules]
x = { icon = "x" }
"#;
    let cfg: Config = toml::from_str(s).unwrap();
    assert_eq!(cfg.base.unwrap_or_default().font.as_deref(), Some("Sans"));
}

#[test]
fn load_missing_file_yields_default_struct() {
    let cfg = Config::load(std::path::Path::new("/nonexistent/abar/config.toml"));
    let cli = Cli::try_parse_from(["abar"]).unwrap();
    let s = Settings::resolve(&cli, cfg, Theme::default()).unwrap();
    assert_eq!(s.font, "NotoSans Nerd Font");
}

#[test]
fn default_path_ends_with_abar_config() {
    let p = default_path();
    assert!(p.ends_with("abar/config.toml"));
}
