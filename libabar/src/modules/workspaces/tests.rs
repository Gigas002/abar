use super::*;

fn ws(id: i32, name: &str) -> WorkspaceInfo {
    WorkspaceInfo {
        id,
        name: name.to_string(),
    }
}

/// Fixed-width measure: each character = 8 px wide, height = 16 px.
fn fixed(text: &str) -> (f64, f64) {
    (text.len() as f64 * 8.0, 16.0)
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
fn workspace_at_x_plain_text_hits_each_workspace() {
    // Display: "1  [2]  3"
    // char widths @ 8px: "1"=8, "  "=16, "[2]"=24, "  "=16, "3"=8  → total=72
    // seg: x=0, width=100 → text_start = (100-72)/2 = 14
    // "1": 14..22, sep: 22..38, "[2]": 38..62, sep: 62..78, "3": 78..86
    let state = WorkspacesDisplayState {
        workspaces: vec![ws(1, "1"), ws(2, "2"), ws(3, "3")],
        active_id: 2,
    };
    assert_eq!(workspace_at_x(15.0, 0.0, 100.0, &state, false, &fixed), Some(1));
    assert_eq!(workspace_at_x(50.0, 0.0, 100.0, &state, false, &fixed), Some(2));
    assert_eq!(workspace_at_x(82.0, 0.0, 100.0, &state, false, &fixed), Some(3));
    // Between workspaces (separator region) → None
    assert_eq!(workspace_at_x(30.0, 0.0, 100.0, &state, false, &fixed), None);
}

#[test]
fn workspace_at_x_markup_mode_uses_plain_names() {
    // In markup mode all names are unstyled for measurement: "1", "2", "3"
    // Display widths @ 8px: "1"=8, "  "=16, "2"=8, "  "=16, "3"=8  → total=56
    // seg: x=0, width=80 → text_start = (80-56)/2 = 12
    // "1": 12..20, sep: 20..36, "2": 36..44, sep: 44..60, "3": 60..68
    let state = WorkspacesDisplayState {
        workspaces: vec![ws(1, "1"), ws(2, "2"), ws(3, "3")],
        active_id: 1,
    };
    assert_eq!(workspace_at_x(14.0, 0.0, 80.0, &state, true, &fixed), Some(1));
    assert_eq!(workspace_at_x(40.0, 0.0, 80.0, &state, true, &fixed), Some(2));
    assert_eq!(workspace_at_x(64.0, 0.0, 80.0, &state, true, &fixed), Some(3));
}

#[test]
fn workspace_at_x_empty_state_returns_none() {
    let state = WorkspacesDisplayState::default();
    assert_eq!(workspace_at_x(10.0, 0.0, 100.0, &state, false, &fixed), None);
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
