use std::path::PathBuf;

use super::{Config, Layout, config_dir_from_env, events::events_for_module, layout::LayoutEntry};

const MINIMAL: &str = r#"
[base]
font_name = "Sans"
font_size = 12
theme = "theme.toml"

[layout]
left = []
center = []
right = []
"#;

const EXAMPLE_CONFIG: &str = include_str!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../examples/config.toml"
));

fn parse(raw: &str) -> Config {
    toml::from_str(raw).expect("config should parse")
}

#[test]
fn deserialize_minimal_ok() {
    let cfg = parse(MINIMAL);
    assert_eq!(
        cfg.base.as_ref().and_then(|b| b.font_name.as_deref()),
        Some("Sans")
    );
    assert_eq!(cfg.base.as_ref().and_then(|b| b.font_size), Some(12.0));
    assert_eq!(
        cfg.base.as_ref().and_then(|b| b.theme.as_deref()),
        Some("theme.toml")
    );
    assert_eq!(
        cfg.layout
            .as_ref()
            .and_then(|l| l.left.as_ref())
            .map(Vec::len),
        Some(0)
    );
}

#[test]
fn example_config_toml_deserializes() {
    let cfg = parse(EXAMPLE_CONFIG);
    assert_eq!(
        cfg.base.as_ref().and_then(|b| b.font_name.as_deref()),
        Some("NotoSans Nerd Font")
    );
    assert_eq!(cfg.base.as_ref().and_then(|b| b.font_size), Some(16.0));
    assert_eq!(
        cfg.modules
            .as_ref()
            .and_then(|m| m.custom.as_ref())
            .map(|c| c.len()),
        Some(5)
    );
    assert!(
        cfg.modules
            .as_ref()
            .and_then(|m| m.custom_by_name("system_info"))
            .is_some()
    );
    assert_eq!(
        cfg.keyboard.as_ref().and_then(|k| k.exec.as_deref()),
        Some("~/.config/abar/scripts/keyboard/hyprland.sh")
    );
    assert_eq!(
        cfg.clock
            .as_ref()
            .and_then(|c| c.formats.as_ref())
            .and_then(|f| f.first().map(String::as_str)),
        Some("%R %Z %d.%m.%Y")
    );
}

#[test]
fn nested_layout_groups_parse() {
    let cfg = parse(EXAMPLE_CONFIG);
    let layout = cfg.layout.as_ref().unwrap();
    assert_eq!(
        layout.left.as_ref().unwrap(),
        &[
            LayoutEntry::Module("system_info".into()),
            LayoutEntry::Module("workspaces".into()),
            LayoutEntry::Module("mpris".into()),
        ]
    );
    assert_eq!(
        layout.center.as_ref().unwrap(),
        &[LayoutEntry::Module("window".into())]
    );
    let right = layout.right.as_ref().unwrap();
    assert_eq!(right.len(), 2);
    match &right[0] {
        LayoutEntry::Group(g) => assert_eq!(g, &["keyboard"]),
        other => panic!("expected group, got {other:?}"),
    }
    match &right[1] {
        LayoutEntry::Group(g) => assert_eq!(
            g,
            &["tray", "clock", "bluetooth", "network", "audio", "leave"]
        ),
        other => panic!("expected group, got {other:?}"),
    }
}

#[test]
fn custom_module_events_parse() {
    let cfg = parse(EXAMPLE_CONFIG);
    let audio = cfg
        .modules
        .as_ref()
        .and_then(|m| m.custom_by_name("audio"))
        .unwrap();
    assert_eq!(audio.icon, "pavucontrol");
    let events = audio.events.as_ref().unwrap();
    assert_eq!(events.on_left_click.as_deref(), Some("pavucontrol"));
    assert_eq!(
        events.on_scroll_up.as_deref(),
        Some("wpctl set-volume @DEFAULT_AUDIO_SINK@ 5%+")
    );
}

#[test]
fn clock_module_events_parse() {
    let cfg = parse(EXAMPLE_CONFIG);
    let clock = cfg.clock.as_ref().unwrap();
    let events = clock.events.as_ref().unwrap();
    assert_eq!(
        events.on_left_click.as_deref(),
        Some("$TERMINAL -e calendar-tui")
    );
    assert_eq!(
        clock.timezones.as_deref(),
        Some(["Asia/Tokyo".to_string(), "Europe/Moscow".to_string()].as_slice(),)
    );
}

#[test]
fn default_config_has_base_and_layout() {
    let cfg = Config::default();
    let base = cfg.base.unwrap();
    assert_eq!(base.font_name.as_deref(), Some("NotoSans Nerd Font"));
    assert_eq!(base.font_size, Some(14.0));
    assert_eq!(base.theme.as_deref(), Some("theme.toml"));
    let layout = cfg.layout.unwrap();
    assert!(layout.left.is_none() && layout.center.is_none() && layout.right.is_none());
}

#[test]
fn config_dir_uses_xdg_config_home() {
    let dir = config_dir_from_env(Some("/custom/config"), None);
    assert_eq!(dir, PathBuf::from("/custom/config/abar"));
}

#[test]
fn config_dir_falls_back_to_home_dot_config() {
    let dir = config_dir_from_env(None, Some("/home/user"));
    assert_eq!(dir, PathBuf::from("/home/user/.config/abar"));
}

#[test]
fn config_dir_ignores_empty_xdg_and_uses_home() {
    let dir = config_dir_from_env(Some(""), Some("/home/user"));
    assert_eq!(dir, PathBuf::from("/home/user/.config/abar"));
}

#[test]
fn config_dir_falls_back_to_relative_when_no_env() {
    let dir = config_dir_from_env(None, None);
    assert_eq!(dir, PathBuf::from(".config/abar"));
}

#[test]
fn default_config_path_appends_config_toml() {
    let dir = config_dir_from_env(Some("/cfg"), None);
    assert_eq!(
        dir.join("config.toml"),
        PathBuf::from("/cfg/abar/config.toml")
    );
}

#[test]
fn to_bar_layout_uses_builtin_segment_labels() {
    let layout = Layout {
        left: Some(vec![LayoutEntry::Module("keyboard".into())]),
        center: Some(vec![LayoutEntry::Module("workspaces".into())]),
        right: Some(vec![LayoutEntry::Module("clock".into())]),
    };
    let bar = layout.to_bar_layout(None);
    assert_eq!(bar.left[0].segments[0].label, "kb");
    assert_eq!(bar.center[0].segments[0].label, "ws");
    assert_eq!(bar.right[0].segments[0].label, "clock");
}

#[test]
fn to_bar_layout_nested_group_is_one_island() {
    let layout = Layout {
        right: Some(vec![LayoutEntry::Group(vec![
            "tray".into(),
            "clock".into(),
        ])]),
        ..Layout::default()
    };
    let bar = layout.to_bar_layout(None);
    assert_eq!(bar.right.len(), 1);
    assert_eq!(bar.right[0].segments.len(), 2);
    assert_eq!(bar.right[0].segments[0].module_id, "tray");
    assert_eq!(bar.right[0].segments[0].label, "tray");
    assert_eq!(bar.right[0].segments[1].label, "clock");
}

#[test]
fn events_for_module_reads_builtin_and_custom_tables() {
    let cfg = parse(EXAMPLE_CONFIG);
    assert_eq!(
        events_for_module(&cfg, "clock").on_left_click.as_deref(),
        Some("$TERMINAL -e calendar-tui")
    );
    assert_eq!(
        events_for_module(&cfg, "system_info")
            .on_left_click
            .as_deref(),
        Some("$TERMINAL -e btm")
    );
    assert!(
        events_for_module(&cfg, "workspaces")
            .on_left_click
            .is_none()
    );
}

#[test]
fn layout_module_names_in_order() {
    let layout = Layout {
        left: Some(vec![LayoutEntry::Module("a".into())]),
        center: Some(vec![LayoutEntry::Group(vec!["b".into(), "c".into()])]),
        right: Some(vec![LayoutEntry::Module("d".into())]),
    };
    assert_eq!(layout.module_names_in_order(), vec!["a", "b", "c", "d"]);
}

#[test]
fn load_missing_file_returns_defaults() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("config.toml");
    // `path` does not exist — dir only creates the directory, not the file.
    let cfg = Config::load(&path);
    let base = cfg.base.unwrap();
    assert_eq!(base.font_name.as_deref(), Some("NotoSans Nerd Font"));
    assert_eq!(base.font_size, Some(14.0));
}
