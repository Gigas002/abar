use crate::exec::tests::first_line;
use crate::modules::tray::ipc::{MinimalTrayItem, TrayItemStatus};

/// A tray script line is read as a `Vec<MinimalTrayItem>`.
#[test]
fn tray_exec_handler_receives_item() {
    let items: Vec<MinimalTrayItem> =
        first_line(r#"echo '[{"app_id":"x","status":"Active","icon_handle":"network-wireless"}]'"#);
    assert_eq!(items.len(), 1);
    assert_eq!(items[0].app_id, "x");
    assert_eq!(items[0].status, TrayItemStatus::Active);
    assert_eq!(items[0].icon_handle.as_deref(), Some("network-wireless"));
}

#[test]
fn unknown_status_does_not_drop_the_update() {
    let items: Vec<MinimalTrayItem> = serde_json::from_str(
        r#"[{"app_id":"a","status":"Active","icon_handle":null},
            {"app_id":"b","status":"Blinking","icon_handle":null}]"#,
    )
    .unwrap();
    assert_eq!(items[1].status, TrayItemStatus::Unknown);
}

#[test]
fn app_ids_are_passed_to_sh_verbatim() {
    use crate::modules::tray::ipc::shell_quote;
    for id in [
        "plain",
        "with space",
        "it's",
        "$(touch /tmp/x)",
        "`id`;rm",
        "",
    ] {
        let out = std::process::Command::new("sh")
            .arg("-c")
            .arg(format!("printf %s {}", shell_quote(id)))
            .output()
            .unwrap();
        assert_eq!(String::from_utf8(out.stdout).unwrap(), id);
    }
}
