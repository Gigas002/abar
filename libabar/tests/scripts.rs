//! Exec scripts must not outlive the bar. Runs in its own test binary because
//! `stop_scripts` acts on every script in the process.

use std::sync::mpsc;
use std::time::{Duration, Instant};

use libabar::{ScriptLine, ensure_runtime, exec};

fn alive(pid: u32) -> bool {
    // A killed child of a dead shell is reparented and reaped; a zombie counts as gone.
    std::fs::read_to_string(format!("/proc/{pid}/stat")).is_ok_and(|stat| {
        !stat
            .rsplit(')')
            .next()
            .unwrap_or("")
            .trim_start()
            .starts_with('Z')
    })
}

#[test]
fn stopping_scripts_also_stops_what_they_started() {
    let (tx, rx) = mpsc::channel();
    // The script starts a long-running helper and reports its pid.
    ensure_runtime().unwrap().spawn(exec::run_exec_handler(
        "probe".into(),
        r#"sleep 300 & echo "{\"text\":\"$!\"}"; wait"#.into(),
        move |line: ScriptLine| {
            let _ = tx.send(line.text);
        },
    ));
    let helper: u32 = rx
        .recv_timeout(Duration::from_secs(5))
        .expect("script output")
        .parse()
        .unwrap();
    assert!(alive(helper));

    exec::stop_scripts();

    let deadline = Instant::now() + Duration::from_secs(3);
    while alive(helper) {
        assert!(
            Instant::now() < deadline,
            "helper {helper} survived stop_scripts"
        );
        std::thread::sleep(Duration::from_millis(20));
    }
}
