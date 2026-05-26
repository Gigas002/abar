use std::io::Write as _;
use std::os::unix::net::UnixStream;
use std::sync::mpsc::SyncSender;
use std::time::Duration;

use serde::de::DeserializeOwned;
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::process::Command;
use tracing::warn;

use crate::modules::ModuleUpdate;

const BACKOFF_INITIAL: Duration = Duration::from_secs(1);
const BACKOFF_MAX: Duration = Duration::from_secs(30);

/// Spawn `command` via `sh -c` and forward each JSON line from its stdout as a `ModuleUpdate`
/// into `tx`. Restarts the script with exponential backoff on unexpected exit.
///
/// Each stdout line is deserialized as `T`; `into_update` converts it to a `ModuleUpdate`.
/// Lines that fail to parse are logged and skipped. A single byte is written to `wakeup` after
/// every successfully forwarded update to unblock the Wayland poll loop.
pub async fn run_exec_handler<T, F>(
    module_id: String,
    command: String,
    tx: SyncSender<ModuleUpdate>,
    mut wakeup: UnixStream,
    into_update: F,
) where
    T: DeserializeOwned,
    F: Fn(T) -> ModuleUpdate + Send + 'static,
{
    let mut backoff = BACKOFF_INITIAL;
    loop {
        match run_once::<T, F>(&module_id, &command, &tx, &mut wakeup, &into_update).await {
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

async fn run_once<T, F>(
    module_id: &str,
    command: &str,
    tx: &SyncSender<ModuleUpdate>,
    wakeup: &mut UnixStream,
    into_update: &F,
) -> Result<(), String>
where
    T: DeserializeOwned,
    F: Fn(T) -> ModuleUpdate,
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
        let line = line.trim().to_string();
        if line.is_empty() {
            continue;
        }
        match serde_json::from_str::<T>(&line) {
            Ok(parsed) => {
                let _ = tx.try_send(into_update(parsed));
                let _ = wakeup.write_all(&[0u8]);
            }
            Err(e) => {
                warn!(%module_id, %line, error = %e, "exec script output is not valid JSON");
            }
        }
    }

    child.wait().await.map_err(|e| format!("wait: {e}"))?;
    Ok(())
}

#[cfg(test)]
mod tests;
