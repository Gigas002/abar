use std::io::Write as _;
use std::os::unix::net::UnixStream;
use std::sync::mpsc::SyncSender;
use std::time::Duration;

use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::process::Command;
use tracing::warn;

use crate::modules::tray::ipc::MinimalTrayItem;

use crate::model::SegmentEvents;

pub mod ipc;

const BACKOFF_INITIAL: Duration = Duration::from_secs(1);
const BACKOFF_MAX: Duration = Duration::from_secs(30);

/// Runtime configuration for the tray module passed into the Wayland run loop.
#[derive(Debug, Clone, Default)]
pub struct TrayConfig {
    /// Optional exec command (`sh -c <exec>`) whose stdout drives this module.
    /// Each stdout line must be a JSON array of `MinimalTrayItem`.
    pub exec: Option<String>,
    /// When `true`, each item's `app_id` is appended to every configured `on_*`
    /// handler when tray segments are rebuilt, e.g. `tray-menu.sh <app_id>`.
    pub feed_id: bool,
    /// Pointer-action handlers applied to every tray item segment.
    pub events: SegmentEvents,
}

/// Spawn `command` via `sh -c` and forward each JSON-array line from its stdout as a
/// `Vec<MinimalTrayItem>` into `tx`. Restarts the script with exponential backoff on unexpected exit.
///
/// Lines that fail to parse are logged and skipped. A single byte is written to `wakeup` after
/// every successfully forwarded update to unblock the Wayland poll loop.
pub async fn run_tray_exec_handler(
    command: String,
    tx: SyncSender<Vec<MinimalTrayItem>>,
    mut wakeup: UnixStream,
) {
    let mut backoff = BACKOFF_INITIAL;
    loop {
        match run_tray_once(&command, &tx, &mut wakeup).await {
            Ok(()) => {
                warn!(%command, "tray exec script exited; restarting");
            }
            Err(e) => {
                warn!(%command, error = %e, "tray exec script error; restarting after backoff");
            }
        }
        tokio::time::sleep(backoff).await;
        backoff = (backoff * 2).min(BACKOFF_MAX);
    }
}

async fn run_tray_once(
    command: &str,
    tx: &SyncSender<Vec<MinimalTrayItem>>,
    wakeup: &mut UnixStream,
) -> Result<(), String> {
    let mut child = Command::new("sh")
        .arg("-c")
        .arg(command)
        .stdout(std::process::Stdio::piped())
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
        match serde_json::from_str::<Vec<MinimalTrayItem>>(&line) {
            Ok(items) => {
                let _ = tx.try_send(items);
                let _ = wakeup.write_all(&[0u8]);
            }
            Err(e) => {
                warn!(%line, error = %e, "tray exec script output is not valid JSON");
            }
        }
    }

    child.wait().await.map_err(|e| format!("wait: {e}"))?;
    Ok(())
}

#[cfg(test)]
mod tests;
