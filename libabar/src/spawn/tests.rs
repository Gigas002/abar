use std::path::PathBuf;
use std::time::{Duration, Instant};

use super::{ensure_runtime, spawn_shell_command};

#[test]
fn spawn_runs_command_without_blocking_caller() {
    ensure_runtime().expect("tokio runtime");

    let dir = tempfile::tempdir().expect("tempdir");
    let marker: PathBuf = dir.path().join("spawned");
    let path = marker.display().to_string();
    let script = format!("touch '{path}'");

    let start = Instant::now();
    spawn_shell_command(&script);
    assert!(
        start.elapsed() < Duration::from_millis(50),
        "spawn should return immediately"
    );

    let deadline = Instant::now() + Duration::from_secs(2);
    while Instant::now() < deadline {
        if marker.is_file() {
            return;
        }
        std::thread::sleep(Duration::from_millis(10));
    }
    panic!("expected marker file at {}", marker.display());
}
