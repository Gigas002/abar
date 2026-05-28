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
fn resolve_builds_layout_with_builtin_modules() {
    // Uses only built-in module names so icon validation is skipped (no custom modules).
    let raw = r#"
[base]
font_name = "NotoSans Nerd Font"
font_size = 16

[layout]
left = ["workspaces"]
center = ["window"]
right = [
    ["keyboard"],
    ["tray", "clock"],
]

[clock]
formats = ["%R %Z %d.%m.%Y"]
on_left_click = "rusti-cal"
"#;
    let cfg: Config = toml::from_str(raw).unwrap();
    let theme = Theme::default();
    let cli = Cli::try_parse_from(["abar"]).unwrap();
    let s = Settings::resolve(&cli, cfg, theme).unwrap();
    assert!(!s.bar.layout.left.is_empty());
    assert_eq!(s.bar.layout.center.len(), 1);
    assert!(!s.bar.layout.right.is_empty());
    // When the clock feature is active the segment shows live time, not the placeholder.
    #[cfg(feature = "clock")]
    assert_ne!(s.bar.layout.right[1].segments[1].label, "clock");
    #[cfg(not(feature = "clock"))]
    assert_eq!(s.bar.layout.right[1].segments[1].label, "clock");
    assert_eq!(s.bar.layout.right[1].segments[1].module_id, "clock");
    assert_eq!(
        s.bar.layout.right[1].segments[1]
            .events
            .on_left_click
            .as_deref(),
        Some("rusti-cal")
    );
    assert_eq!(s.font_size(), 16.0);
}

#[test]
fn resolve_island_style_from_theme() {
    let cfg: Config = toml::from_str(
        r#"
[base]
font_name = "sans-serif"
font_size = 14
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
background_color = "#000000FF"
foreground_color = "#FFFFFFFF"
island_padding_x = 20
island_padding_y = 8
island_radius = 6
"##,
    )
    .unwrap();
    let cli = Cli::try_parse_from(["abar"]).unwrap();
    let s = Settings::resolve(&cli, cfg, theme).unwrap();
    assert_eq!(s.bar.style.island_padding_x, 20.0);
    assert_eq!(s.bar.style.island_padding_y, 8.0);
    assert_eq!(s.bar.style.island_radius, 6.0);
}

#[test]
fn resolve_island_style_uses_defaults_when_absent() {
    use libabar::BarStyle;
    let cfg: Config = toml::from_str(
        r#"
[base]
font_name = "sans-serif"
font_size = 14
[layout]
left = []
center = []
right = []
"#,
    )
    .unwrap();
    let cli = Cli::try_parse_from(["abar"]).unwrap();
    let s = Settings::resolve(&cli, cfg, Theme::default()).unwrap();
    let defaults = BarStyle::default();
    assert_eq!(s.bar.style.island_padding_x, defaults.island_padding_x);
    assert_eq!(s.bar.style.island_padding_y, defaults.island_padding_y);
    assert_eq!(s.bar.style.island_radius, defaults.island_radius);
}

#[test]
fn resolve_falls_back_to_text_for_missing_icon() {
    use libabar::{BarLayout, DisplayMode, Island, Segment};

    use crate::settings::apply_icon_fallbacks_with_dirs;

    let seg = Segment::icon_only("mymod", "nonexistent-icon-xyz");
    let mut layout = BarLayout {
        left: vec![Island {
            segments: vec![seg],
        }],
        ..BarLayout::default()
    };

    // Empty search dirs: no icon can be resolved regardless of system state.
    apply_icon_fallbacks_with_dirs(&mut layout, 14.0, &[], "hicolor");

    let seg = &layout.left[0].segments[0];
    assert_eq!(
        seg.display_mode,
        DisplayMode::TextOnly,
        "should fall back to text"
    );
    assert_eq!(seg.label, "mymod");
}
