use serde::{Deserialize, Serialize};

/// Versioned request sent to `trayd` over the NDJSON socket.
#[derive(Debug, Clone, Serialize)]
pub struct TrayRequest {
    pub v: u8,
    #[serde(flatten)]
    pub cmd: TrayCmd,
}

impl TrayRequest {
    pub fn subscribe() -> Self {
        Self {
            v: 1,
            cmd: TrayCmd::Subscribe,
        }
    }

    pub fn get_pixmap(app_id: impl Into<String>, size: u32) -> Self {
        Self {
            v: 1,
            cmd: TrayCmd::GetPixmap {
                app_id: app_id.into(),
                size,
            },
        }
    }
}

/// Command variants, inlined into the request object via the `"cmd"` tag.
#[derive(Debug, Clone, Serialize)]
#[serde(tag = "cmd", rename_all = "snake_case")]
pub enum TrayCmd {
    Subscribe,
    GetPixmap { app_id: String, size: u32 },
}

/// Top-level response received from `trayd` — either a typed success or an error envelope.
#[derive(Debug, Clone, Deserialize)]
#[serde(untagged)]
pub enum TrayResponse {
    Ok(TrayOk),
    Err(TrayErrEnvelope),
}

/// Successful response variants, discriminated by the `"type"` field.
#[derive(Debug, Clone, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum TrayOk {
    Event {
        event: TrayEvent,
    },
    Pixmap {
        app_id: String,
        size: u32,
        width: u32,
        height: u32,
        /// Base64-encoded ARGB32 pixels (`width × height × 4`), big-endian per SNI spec.
        data: String,
    },
}

/// Envelope for error responses: `{"v":1,"error":{"code":"...","message":"..."}}`.
#[derive(Debug, Clone, Deserialize)]
pub struct TrayErrEnvelope {
    pub error: TrayIpcError,
}

#[derive(Debug, Clone, Deserialize)]
pub struct TrayIpcError {
    pub code: String,
    pub message: String,
}

/// Event pushed by `trayd` on the subscribe stream.
#[derive(Debug, Clone, Deserialize)]
pub struct TrayEvent {
    pub kind: TrayEventKind,
    pub items: Vec<MinimalTrayItem>,
}

#[derive(Debug, Clone, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum TrayEventKind {
    Update,
    #[serde(other)]
    Unknown,
}

/// Minimal representation of a system tray item as reported by `trayd`.
#[derive(Debug, Clone, Deserialize, PartialEq, Eq)]
pub struct MinimalTrayItem {
    pub app_id: String,
    pub title: Option<String>,
    pub status: TrayItemStatus,
    pub icon_handle: Option<String>,
}

/// SNI item visibility status. `Passive` items are not rendered.
#[derive(Debug, Clone, Deserialize, PartialEq, Eq)]
pub enum TrayItemStatus {
    Active,
    Passive,
    NeedsAttention,
}
