use std::sync::OnceLock;

use tokio::runtime::{Handle, Runtime};
use tracing::warn;

use crate::error::AbarError;

static RUNTIME: OnceLock<Runtime> = OnceLock::new();

fn build_runtime() -> Result<Runtime, AbarError> {
    tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .thread_name("abar-async")
        .worker_threads(2)
        .build()
        .map_err(|err| AbarError::AsyncRuntime(err.to_string()))
}

/// Start the shared Tokio runtime used for shell commands and other background async work.
pub fn ensure_runtime() -> Result<&'static Handle, AbarError> {
    if let Some(runtime) = RUNTIME.get() {
        return Ok(runtime.handle());
    }
    let runtime = build_runtime()?;
    let _ = RUNTIME.set(runtime);
    Ok(RUNTIME.get().expect("runtime initialized").handle())
}

/// Run `command` via `sh -c` on the Tokio runtime without blocking the caller.
pub fn spawn_shell_command(command: &str) {
    let Ok(handle) = ensure_runtime() else {
        warn!(%command, "tokio runtime unavailable, dropping command");
        return;
    };
    let command = command.to_owned();
    handle.spawn(async move {
        run_shell_command(&command).await;
    });
}

async fn run_shell_command(command: &str) {
    match tokio::process::Command::new("sh")
        .arg("-c")
        .arg(command)
        .status()
        .await
    {
        Ok(status) if !status.success() => {
            warn!(%command, ?status, "command exited with failure");
        }
        Err(err) => warn!(%err, %command, "failed to spawn command"),
        Ok(_) => {}
    }
}

#[cfg(test)]
mod tests;
