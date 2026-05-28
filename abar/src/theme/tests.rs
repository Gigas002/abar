use super::{Theme, resolve_path};
use libabar::color::{ParseHexRgbaError, parse_hex_rgba_to_bgra};

const THEME: &str = r##"
[base]
background_color = "#161925FF"
foreground_color = "#c74dedFF"
"##;

const EXAMPLE_THEME: &str = include_str!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../examples/theme.toml"
));

#[test]
fn parse_example_theme_colors() {
    let t = Theme::parse_str(THEME).unwrap();
    let bg = t
        .base
        .as_ref()
        .and_then(|b| b.background_color.as_deref())
        .unwrap();
    let px = parse_hex_rgba_to_bgra(bg).unwrap();
    assert_eq!(px, [0x25, 0x19, 0x16, 255]);
}

#[test]
fn rejects_bad_hex_in_settings_path() {
    let s = r##"
[base]
background_color = "161925"
foreground_color = "#FFFFFFFF"
"##;
    let t = Theme::parse_str(s).unwrap();
    let bg = t
        .base
        .as_ref()
        .and_then(|b| b.background_color.clone())
        .unwrap_or_default();
    assert_eq!(
        parse_hex_rgba_to_bgra(&bg),
        Err(ParseHexRgbaError::InvalidFormat)
    );
}

#[test]
fn example_theme_toml_deserializes() {
    let t = Theme::parse_str(EXAMPLE_THEME).unwrap();
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
    assert_eq!(
        t.workspaces
            .as_ref()
            .and_then(|w| w.active_color.as_deref()),
        Some("#00c1e4FF")
    );
}

#[test]
fn resolve_prefers_themes_subdir_next_to_config() {
    let dir = tempfile::tempdir().unwrap();
    let config = dir.path().join("config.toml");
    let themes = dir.path().join("themes");
    std::fs::create_dir_all(&themes).unwrap();
    let theme_file = themes.join("theme.toml");
    std::fs::write(&theme_file, THEME).unwrap();

    let resolved = resolve_path(&config, "theme.toml");
    assert_eq!(resolved, theme_file);
}

#[test]
fn resolve_absolute_theme_path() {
    let dir = tempfile::tempdir().unwrap();
    let theme = dir.path().join("custom.toml");
    std::fs::write(&theme, THEME).unwrap();
    let config = dir.path().join("config.toml");

    let resolved = resolve_path(&config, theme.to_str().unwrap());
    assert_eq!(resolved, theme);
}

#[test]
fn default_theme_has_black_background_and_white_foreground() {
    let t = Theme::default();
    let base = t.base.unwrap();
    assert_eq!(base.background_color.as_deref(), Some("#000000FF"));
    assert_eq!(base.foreground_color.as_deref(), Some("#FFFFFFFF"));
}

#[test]
fn load_missing_theme_returns_defaults() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("theme.toml");
    // `path` does not exist — dir only creates the directory, not the file.
    let t = Theme::load(&path);
    let base = t.base.unwrap();
    assert_eq!(base.background_color.as_deref(), Some("#000000FF"));
    assert_eq!(base.foreground_color.as_deref(), Some("#FFFFFFFF"));
}
