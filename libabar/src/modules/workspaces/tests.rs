use super::*;

#[test]
fn trim_alpha_strips_last_two_hex_digits() {
    assert_eq!(trim_alpha("#00c1e4FF"), "#00c1e4");
    assert_eq!(trim_alpha("#00c1e4"), "#00c1e4");
    assert_eq!(trim_alpha("red"), "red");
}

#[test]
fn visibility_mode_parse() {
    assert_eq!(
        VisibilityMode::parse("monitor_specific"),
        VisibilityMode::MonitorSpecific
    );
    assert_eq!(
        VisibilityMode::parse("all_monitors"),
        VisibilityMode::AllMonitors
    );
    assert_eq!(
        VisibilityMode::parse("anything"),
        VisibilityMode::AllMonitors
    );
}

#[test]
fn script_environment_follows_the_theme() {
    let cfg = WorkspacesConfig {
        visibility_mode: VisibilityMode::MonitorSpecific,
        active_color: Some("#00c1e4".into()),
        inactive_color: None,
        exec: None,
    };
    assert_eq!(
        cfg.env(),
        [
            (
                "ABAR_WS_VISIBILITY".to_string(),
                "monitor_specific".to_string()
            ),
            ("ABAR_WS_ACTIVE_COLOR".to_string(), "#00c1e4".to_string()),
        ]
    );
}
