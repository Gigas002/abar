use std::sync::mpsc;
use std::time::{Duration, Instant};

use crate::spawn::ensure_runtime;

/// A script that emits one JSON line and exits must deliver a ModuleUpdate on the channel.
#[test]
fn exec_handler_receives_text() {
    ensure_runtime().expect("tokio runtime");

    let (tx, rx) = mpsc::sync_channel(16);
    let (wakeup_main, wakeup_exec) = std::os::unix::net::UnixStream::pair().unwrap();
    wakeup_main.set_nonblocking(true).unwrap();
    drop(wakeup_main);

    let handle = ensure_runtime().unwrap();
    handle.spawn(super::run_exec_handler(
        "test_module".to_string(),
        r#"echo '{"text": "hello"}'"#.to_string(),
        tx,
        wakeup_exec,
    ));

    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        if let Ok(update) = rx.try_recv() {
            assert_eq!(update.module_id, "test_module");
            assert_eq!(update.text, "hello");
            assert!(!update.use_markup);
            assert!(update.icon.is_none());
            return;
        }
        assert!(
            Instant::now() < deadline,
            "timed out waiting for exec update"
        );
        std::thread::sleep(Duration::from_millis(10));
    }
}

/// Optional fields default correctly when absent from the JSON.
#[test]
fn exec_handler_markup_and_icon() {
    ensure_runtime().expect("tokio runtime");

    let (tx, rx) = mpsc::sync_channel(16);
    let (wakeup_main, wakeup_exec) = std::os::unix::net::UnixStream::pair().unwrap();
    wakeup_main.set_nonblocking(true).unwrap();
    drop(wakeup_main);

    let handle = ensure_runtime().unwrap();
    handle.spawn(super::run_exec_handler(
        "mod2".to_string(),
        r#"echo '{"text": "<b>bold</b>", "markup": true, "icon": "network-wireless"}'"#.to_string(),
        tx,
        wakeup_exec,
    ));

    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        if let Ok(update) = rx.try_recv() {
            assert_eq!(update.text, "<b>bold</b>");
            assert!(update.use_markup);
            assert_eq!(update.icon.as_deref(), Some("network-wireless"));
            return;
        }
        assert!(
            Instant::now() < deadline,
            "timed out waiting for exec update"
        );
        std::thread::sleep(Duration::from_millis(10));
    }
}
