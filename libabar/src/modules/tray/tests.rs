use std::sync::mpsc;
use std::time::{Duration, Instant};

use crate::modules::tray::ipc::{MinimalTrayItem, TrayItemStatus};
use crate::spawn::ensure_runtime;

#[test]
fn minimal_tray_item_optional_fields_default() {
    let item: MinimalTrayItem =
        serde_json::from_str(r#"{"app_id":"x","status":"Active"}"#).unwrap();
    assert_eq!(item.app_id, "x");
    assert_eq!(item.status, TrayItemStatus::Active);
    assert!(!item.item_is_menu);
    assert!(item.category.is_none());
    assert!(item.tooltip_title.is_none());
    assert!(item.tooltip_description.is_none());
    assert!(item.overlay_icon_handle.is_none());
}

#[test]
fn minimal_tray_item_trayd_wire_format() {
    let json = r#"{
        "app_id": "nm-applet",
        "title": "Network",
        "status": "Active",
        "icon_handle": "network-wireless",
        "category": "ApplicationStatus",
        "item_is_menu": false,
        "tooltip_title": "Network Manager",
        "tooltip_description": "Connected",
        "overlay_icon_handle": "network-wireless-encrypted"
    }"#;
    let item: MinimalTrayItem = serde_json::from_str(json).unwrap();
    assert_eq!(item.title.as_deref(), Some("Network"));
    assert_eq!(item.category.as_deref(), Some("ApplicationStatus"));
    assert!(!item.item_is_menu);
    assert_eq!(item.tooltip_title.as_deref(), Some("Network Manager"));
    assert_eq!(item.tooltip_description.as_deref(), Some("Connected"));
    assert_eq!(
        item.overlay_icon_handle.as_deref(),
        Some("network-wireless-encrypted")
    );
}

/// Spawn a trivial script that emits one JSON array line and exits; assert the exec channel
/// receives the item as a `Vec<MinimalTrayItem>`.
#[test]
fn tray_exec_handler_receives_item() {
    ensure_runtime().expect("tokio runtime");

    let (tx, rx) = mpsc::sync_channel::<Vec<MinimalTrayItem>>(16);
    let (wakeup_main, wakeup_exec) = std::os::unix::net::UnixStream::pair().unwrap();
    wakeup_main.set_nonblocking(true).unwrap();
    drop(wakeup_main);

    let handle = ensure_runtime().unwrap();
    handle.spawn(super::run_tray_exec_handler(
        r#"echo '[{"app_id":"x","status":"Active","icon_handle":"network-wireless"}]'"#.to_string(),
        tx,
        wakeup_exec,
    ));

    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        if let Ok(items) = rx.try_recv() {
            assert_eq!(items.len(), 1);
            assert_eq!(items[0].app_id, "x");
            assert_eq!(items[0].status, TrayItemStatus::Active);
            assert_eq!(items[0].icon_handle.as_deref(), Some("network-wireless"));
            return;
        }
        assert!(
            Instant::now() < deadline,
            "timed out waiting for tray update"
        );
        std::thread::sleep(Duration::from_millis(10));
    }
}
