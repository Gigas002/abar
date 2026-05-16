use super::*;

fn ws(id: i32, name: &str) -> WorkspaceInfo {
    WorkspaceInfo {
        id,
        name: name.to_string(),
    }
}

#[test]
fn plain_text_marks_active_with_brackets() {
    let cfg = WorkspacesConfig::default();
    let workspaces = vec![ws(1, "1"), ws(2, "2"), ws(3, "3")];
    let (label, use_markup) = format_label(&workspaces, 2, &cfg);
    assert!(!use_markup);
    assert_eq!(label, "1  [2]  3");
}

#[test]
fn empty_workspace_list_returns_empty_label() {
    let cfg = WorkspacesConfig::default();
    let (label, use_markup) = format_label(&[], 1, &cfg);
    assert!(!use_markup);
    assert!(label.is_empty());
}

#[test]
fn markup_mode_with_colors() {
    let cfg = WorkspacesConfig {
        visibility_mode: VisibilityMode::AllMonitors,
        active_color: Some("#00c1e4".into()),
        inactive_color: Some("#c74ded".into()),
    };
    let workspaces = vec![ws(1, "1"), ws(2, "2")];
    let (label, use_markup) = format_label(&workspaces, 1, &cfg);
    assert!(use_markup, "expected markup when colors are set");
    assert!(
        label.contains("foreground=\"#00c1e4\""),
        "active color in label; got: {label}"
    );
    assert!(
        label.contains("foreground=\"#c74ded\""),
        "inactive color in label; got: {label}"
    );
}

#[test]
fn pango_escape_chars() {
    let workspaces = vec![ws(1, "a&b")];
    let cfg = WorkspacesConfig {
        active_color: Some("#fff".into()),
        ..Default::default()
    };
    let (label, _) = format_label(&workspaces, 1, &cfg);
    assert!(label.contains("&amp;"), "ampersand must be escaped");
}

#[test]
fn trim_alpha_strips_last_two_hex_digits() {
    assert_eq!(trim_alpha("#00c1e4FF"), "#00c1e4");
    assert_eq!(trim_alpha("#c74dedFF"), "#c74ded");
    assert_eq!(trim_alpha("#fff"), "#fff");
}

#[test]
fn visibility_mode_parse() {
    assert_eq!(
        VisibilityMode::parse("monitor_specific"),
        VisibilityMode::MonitorSpecific
    );
    assert_eq!(VisibilityMode::parse("all"), VisibilityMode::AllMonitors);
    assert_eq!(
        VisibilityMode::parse("unknown"),
        VisibilityMode::AllMonitors
    );
}
