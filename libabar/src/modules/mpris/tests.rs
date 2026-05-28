use super::*;

#[test]
fn default_config_has_no_exec_and_no_limit() {
    let cfg = MprisConfig::default();
    assert!(cfg.exec.is_none());
    assert_eq!(cfg.max_length, 0);
}

#[test]
fn config_stores_exec_and_max_length() {
    let cfg = MprisConfig {
        exec: Some("playerctl.sh".to_string()),
        max_length: 40,
    };
    assert_eq!(cfg.exec.as_deref(), Some("playerctl.sh"));
    assert_eq!(cfg.max_length, 40);
}
