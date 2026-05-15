use std::path::PathBuf;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum AbarError {
    #[error("failed to read {path}: {source}")]
    Io {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },

    #[error("failed to connect to Wayland compositor: {0}")]
    WaylandConnect(#[from] wayland_client::ConnectError),

    #[error("Wayland protocol error: {0}")]
    WaylandProtocol(String),

    #[error("render error: {0}")]
    Render(String),

    #[error("async runtime error: {0}")]
    AsyncRuntime(String),
}
