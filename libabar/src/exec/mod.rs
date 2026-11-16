use std::time::Duration;

use serde::de::DeserializeOwned;
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::process::Command;
use tracing::warn;

const BACKOFF_INITIAL: Duration = Duration::from_secs(1);
const BACKOFF_MAX: Duration = Duration::from_secs(30);

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
        .spawn()
        .map_err(|e| format!("spawn: {e}"))?;

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
