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
