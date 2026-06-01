pub mod ipc;

/// Runtime configuration for the tray module passed into the Wayland run loop.
#[derive(Debug, Clone)]
pub struct TrayConfig {
    /// Path to the trayd Unix socket. `None` defaults to `$XDG_RUNTIME_DIR/trayd.sock`.
    pub socket_path: Option<String>,
    /// Icon pixel size requested via `get_pixmap`.
    pub icon_size: u32,
    /// dmenu-compatible command forwarded to `trayctl` as `--dmenu-cmd`. `None` uses trayctl's default.
    pub dmenu_cmd: Option<String>,
}

impl Default for TrayConfig {
    fn default() -> Self {
        Self {
            socket_path: None,
            icon_size: 22,
            dmenu_cmd: None,
        }
    }
}

#[cfg(test)]
mod tests;
