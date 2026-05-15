use clap::Parser;

use crate::cli::Cli;
use crate::config::Config;
use crate::settings::Settings;
use crate::theme::Theme;

#[test]
fn resolve_uses_config_font_and_theme_colors() {
    let cfg: Config = toml::from_str(
        r#"
[base]
font = "File Font"
theme = "t.toml"
[layout]
left = []
center = []
right = []
"#,
    )
    .unwrap();
    let theme = Theme::parse_str(
        r##"
[base]
background_color = "#112233FF"
foreground_color = "#FFFFFFFF"
"##,
    )
    .unwrap();
    let cli = Cli::try_parse_from(["abar"]).unwrap();
    let s = Settings::resolve(&cli, cfg, theme).unwrap();
    assert_eq!(s.font, "File Font");
    assert_eq!(s.background_color, [0x33, 0x22, 0x11, 255]);
    assert_eq!(s.foreground_color, [255, 255, 255, 255]);
}
