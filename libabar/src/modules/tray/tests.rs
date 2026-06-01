use std::sync::mpsc;
use std::time::{Duration, Instant};

use crate::modules::tray::ipc::{MinimalTrayItem, TrayItemStatus};
use crate::spawn::ensure_runtime;

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
