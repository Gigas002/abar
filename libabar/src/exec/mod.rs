use std::sync::Mutex;
use std::time::Duration;

use rustix::process::{Pid, Signal, kill_process_group};

use serde::de::DeserializeOwned;
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::process::Command;
use tracing::warn;

const BACKOFF_INITIAL: Duration = Duration::from_secs(1);
const BACKOFF_MAX: Duration = Duration::from_secs(30);

/// Process groups of the scripts that are running now.
static SCRIPT_GROUPS: Mutex<Vec<Pid>> = Mutex::new(Vec::new());

fn groups() -> std::sync::MutexGuard<'static, Vec<Pid>> {
    SCRIPT_GROUPS.lock().unwrap_or_else(|e| e.into_inner())
}

/// Send `SIGTERM` to every running exec script and everything it started.
///
/// Scripts run in their own process groups, so this also reaches the
/// long-running helpers they start (`socat`, `trayctl subscribe`, …), which
/// would otherwise outlive the bar.
pub fn stop_scripts() {
    for group in groups().drain(..) {
        let _ = kill_process_group(group, Signal::TERM);
    }
}

/// Spawn `command` via `sh -c` and hand each JSON line from its stdout to `deliver`.
/// Restarts the script with exponential backoff when it exits.
///
/// Each stdout line is deserialized as `T`; lines that fail to parse are logged and skipped.
/// `module_id` only names the module in log messages.
pub async fn run_exec_handler<T, F>(module_id: String, command: String, deliver: F)
where
    T: DeserializeOwned,
    F: Fn(T) + Send + 'static,
{
    let mut backoff = BACKOFF_INITIAL;
    loop {
        match run_once::<T, F>(&module_id, &command, &deliver).await {
            Ok(()) => {
                warn!(%module_id, %command, "exec script exited; restarting");
            }
            Err(e) => {
                warn!(%module_id, %command, error = %e, "exec script error; restarting after backoff");
            }
        }
        tokio::time::sleep(backoff).await;
        backoff = (backoff * 2).min(BACKOFF_MAX);
    }
}

async fn run_once<T, F>(module_id: &str, command: &str, deliver: &F) -> Result<(), String>
where
    T: DeserializeOwned,
    F: Fn(T),
{
    let mut child = Command::new("sh")
        .arg("-c")
        .arg(command)
        .stdout(std::process::Stdio::piped())
        // stdin pipe reserved for future back-channel signals
        .stdin(std::process::Stdio::piped())
        // Own process group, so the script and its helpers can be stopped together.
        .process_group(0)
        .kill_on_drop(true)
        .spawn()
        .map_err(|e| format!("spawn: {e}"))?;
    let group = child
        .id()
        .and_then(|id| Pid::from_raw(id as i32))
        .ok_or("child has no pid")?;
    groups().push(group);
    let result = read_lines(module_id, &mut child, deliver).await;

    // Helpers left behind by a script that exited (or closed its stdout)
    // would keep running next to its restarted copy.
    let _ = kill_process_group(group, Signal::TERM);
    groups().retain(|g| *g != group);
    result
}

async fn read_lines<T, F>(
    module_id: &str,
    child: &mut tokio::process::Child,
    deliver: &F,
) -> Result<(), String>
where
    T: DeserializeOwned,
    F: Fn(T),
{
    let stdout = child.stdout.take().ok_or("child has no stdout")?;
    let mut lines = BufReader::new(stdout).lines();

    while let Some(line) = lines.next_line().await.map_err(|e| format!("read: {e}"))? {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        match serde_json::from_str::<T>(line) {
            Ok(parsed) => deliver(parsed),
            Err(e) => {
                warn!(%module_id, %line, error = %e, "exec script output is not valid JSON");
            }
        }
    }

    child.wait().await.map_err(|e| format!("wait: {e}"))?;
    Ok(())
}

#[cfg(test)]
pub(crate) mod tests;
