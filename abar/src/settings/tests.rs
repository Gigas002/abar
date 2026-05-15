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
font_name = "File Font"
font_size = 18
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
    assert_eq!(s.font_name(), "File Font");
    assert_eq!(s.font_size(), 18.0);
    assert_eq!(s.bar.colors.background, [0x33, 0x22, 0x11, 255]);
    assert_eq!(s.bar.colors.foreground, [255, 255, 255, 255]);
}

#[test]
fn resolve_builds_layout_from_example_config() {
    let raw = include_str!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../examples/config.toml"
    ));
    let cfg: Config = toml::from_str(raw).unwrap();
    let theme = Theme::default();
    let cli = Cli::try_parse_from(["abar"]).unwrap();
    let s = Settings::resolve(&cli, cfg, theme).unwrap();
    assert!(!s.bar.layout.left.is_empty());
    assert_eq!(s.bar.layout.center.len(), 1);
    assert!(!s.bar.layout.right.is_empty());
    assert_eq!(s.bar.layout.right[1].segments[1].label, "clock");
    assert_eq!(s.bar.layout.right[1].segments[1].module_id, "clock");
    assert_eq!(
        s.bar.layout.right[1].segments[1]
            .events
            .on_left_click
            .as_deref(),
        Some("rusti-cal")
    );
    assert_eq!(
        s.bar.layout.left[0].segments[0]
            .events
            .on_left_click
            .as_deref(),
        Some("btm")
    );
    assert_eq!(s.font_size(), 16.0);
}
