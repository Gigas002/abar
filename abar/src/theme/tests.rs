use super::Theme;
use libabar::color::{ParseHexRgbaError, parse_hex_rgba_to_bgra};

const THEME: &str = r##"
[base]
background_color = "#161925FF"
foreground_color = "#c74dedFF"
"##;

fn from_toml(s: &str) -> Theme {
    toml::from_str(s).unwrap()
}

#[test]
fn parse_example_theme_colors() {
    let t = from_toml(THEME);
    let px = parse_hex_rgba_to_bgra(
        &t.base
            .unwrap_or_default()
            .background_color
            .unwrap_or_default(),
    )
    .unwrap();
    assert_eq!(px, [0x25, 0x19, 0x16, 255]);
}

#[test]
fn rejects_bad_hex() {
    let s = r#"
[base]
background_color = "161925"
"#;
    let t = from_toml(s);
    let bg = t
        .base
        .unwrap_or_default()
        .background_color
        .unwrap_or_default();
    assert_eq!(
        parse_hex_rgba_to_bgra(&bg),
        Err(ParseHexRgbaError::InvalidFormat)
    );
}

#[test]
fn default_theme_resolves_pixel() {
    let t = Theme::default();
    let px = parse_hex_rgba_to_bgra(
        &t.base
            .unwrap_or_default()
            .background_color
            .unwrap_or_default(),
    )
    .unwrap();
    assert_eq!(px, [0, 0, 0, 255]);
}

#[test]
fn load_missing_file_yields_defaults() {
    let t = Theme::load(std::path::Path::new("/nonexistent/abar/theme.toml"));
    parse_hex_rgba_to_bgra(
        &t.base
            .unwrap_or_default()
            .background_color
            .unwrap_or_default(),
    )
    .unwrap();
}

#[test]
fn example_theme_toml_deserializes() {
    let raw = include_str!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../examples/theme.toml"
    ));
    let t = from_toml(raw);
    assert_eq!(
        t.base.as_ref().and_then(|b| b.background_color.as_deref()),
        Some("#161925FF")
    );
    assert_eq!(
        t.workspaces
            .as_ref()
            .and_then(|w| w.visibility_mode.as_deref()),
        Some("monitor_specific")
    );
}
