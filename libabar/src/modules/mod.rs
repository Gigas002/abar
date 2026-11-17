use std::io::Write as _;
use std::os::unix::net::UnixStream;
use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, Ordering};

use serde::Deserialize;

/// Newline-delimited JSON record emitted by an exec script to update a module.
///
/// Unknown fields are silently ignored so scripts can include extra metadata.
#[derive(Debug, Clone, Deserialize)]
pub struct ScriptLine {
    pub text: String,
    /// When true, `text` is treated as Pango markup.
    #[serde(default)]
    pub markup: bool,
    /// Optional FreeDesktop icon name or absolute path.
    pub icon: Option<String>,
    /// Optional tooltip, shown while the pointer rests on the module.
    #[serde(default)]
    pub tooltip: Option<String>,
}

/// Internal channel message sent from background tasks (exec handlers or built-in timers)
/// to the Wayland run loop.
#[derive(Debug, Clone)]
pub struct ModuleUpdate {
    /// Identifies which segment to update (matches `Segment::module_id`).
    pub module_id: String,
    /// Text to display (may contain Pango markup when `use_markup` is true).
    pub text: String,
    /// Whether `text` contains Pango markup.
    pub use_markup: bool,
    /// Optional icon override; `None` leaves the segment icon unchanged.
    pub icon: Option<String>,
    /// Tooltip text; `None` shows no tooltip.
    pub tooltip: Option<String>,
}

impl ModuleUpdate {
    /// Create a plain-text update (no markup, no icon).
    pub fn text(module_id: impl Into<String>, text: impl Into<String>) -> Self {
        Self {
            module_id: module_id.into(),
            text: text.into(),
            use_markup: false,
            icon: None,
            tooltip: None,
        }
    }

    /// Create a markup-enabled update (no icon).
    pub fn markup(module_id: impl Into<String>, text: impl Into<String>) -> Self {
        Self {
            module_id: module_id.into(),
            text: text.into(),
            use_markup: true,
            icon: None,
            tooltip: None,
        }
    }

    /// Build from a deserialized exec-script line.
    pub fn from_script(module_id: impl Into<String>, line: ScriptLine) -> Self {
        Self {
            module_id: module_id.into(),
            text: line.text,
            use_markup: line.markup,
            icon: line.icon,
            tooltip: line.tooltip.filter(|t| !t.trim().is_empty()),
        }
    }
}

/// Updates waiting for the Wayland loop: the latest one per module.
///
/// Background tasks [`post`](Self::post) into it and the loop
/// [`take`](Self::take)s everything at once. Posting replaces an unread
/// update for the same module, so a burst of script output never loses the
/// final state, never queues stale ones, and costs one repaint.
#[derive(Debug)]
pub struct Mailbox {
    pending: Mutex<Pending>,
    quit: AtomicBool,
    wakeup: UnixStream,
}

/// Everything posted since the last [`Mailbox::take`].
#[derive(Debug, Default)]
pub struct Pending {
    pub modules: Vec<ModuleUpdate>,
    #[cfg(feature = "tray")]
    pub tray: Option<Vec<tray::ipc::MinimalTrayItem>>,
}

impl Pending {
    pub fn is_empty(&self) -> bool {
        #[cfg(feature = "tray")]
        if self.tray.is_some() {
            return false;
        }
        self.modules.is_empty()
    }
}

impl Mailbox {
    /// `wakeup` is the write end of the socket the Wayland loop polls; it is
    /// made non-blocking so a busy loop never stalls a background task.
    pub fn new(wakeup: UnixStream) -> std::io::Result<Self> {
        wakeup.set_nonblocking(true)?;
        Ok(Self {
            pending: Mutex::new(Pending::default()),
            quit: AtomicBool::new(false),
            wakeup,
        })
    }

    pub fn post(&self, update: ModuleUpdate) {
        {
            let mut pending = self.lock();
            match pending
                .modules
                .iter_mut()
                .find(|u| u.module_id == update.module_id)
            {
                Some(slot) => *slot = update,
                None => pending.modules.push(update),
            }
        }
        self.wake();
    }

    #[cfg(feature = "tray")]
    pub fn post_tray(&self, items: Vec<tray::ipc::MinimalTrayItem>) {
        self.lock().tray = Some(items);
        self.wake();
    }

    /// Ask the Wayland loop to stop (e.g. on `SIGTERM`).
    pub fn request_quit(&self) {
        self.quit.store(true, Ordering::Relaxed);
        self.wake();
    }

    pub fn quit_requested(&self) -> bool {
        self.quit.load(Ordering::Relaxed)
    }

    pub fn take(&self) -> Pending {
        std::mem::take(&mut *self.lock())
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, Pending> {
        self.pending.lock().unwrap_or_else(|e| e.into_inner())
    }

    fn wake(&self) {
        // A full socket already holds a pending wakeup; nothing to add.
        let _ = (&self.wakeup).write(&[0u8]);
    }
}

/// Truncate a title to at most `max_length` Unicode scalar values, appending "…" if cut.
///
/// When `max_length` is zero the title is returned unchanged.
pub fn truncate_title(title: &str, max_length: usize) -> String {
    if max_length == 0 {
        return title.to_string();
    }
    let mut chars = title.chars();
    let truncated: String = chars.by_ref().take(max_length).collect();
    if chars.next().is_some() {
        format!("{truncated}…")
    } else {
        truncated
    }
}

/// Per-module runtime configuration passed into the Wayland run loop.
#[derive(Debug, Default)]
pub struct ModuleConfigs {
    #[cfg(feature = "clock")]
    pub clock: Option<clock::ClockConfig>,
    #[cfg(feature = "keyboard")]
    pub keyboard: Option<keyboard::KeyboardConfig>,
    #[cfg(feature = "workspaces")]
    pub workspaces: Option<workspaces::WorkspacesConfig>,
    #[cfg(feature = "window")]
    pub window: Option<window::WindowConfig>,
    #[cfg(feature = "mpris")]
    pub mpris: Option<mpris::MprisConfig>,
    #[cfg(feature = "tray")]
    pub tray: Option<tray::TrayConfig>,
}

#[cfg(feature = "clock")]
pub mod clock;
#[cfg(feature = "keyboard")]
pub mod keyboard;
#[cfg(feature = "mpris")]
pub mod mpris;
#[cfg(feature = "tray")]
pub mod tray;
#[cfg(feature = "window")]
pub mod window;
#[cfg(feature = "workspaces")]
pub mod workspaces;

#[cfg(test)]
mod tests;
