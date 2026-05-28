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
}

impl ModuleUpdate {
    /// Create a plain-text update (no markup, no icon).
    pub fn text(module_id: impl Into<String>, text: impl Into<String>) -> Self {
        Self {
            module_id: module_id.into(),
            text: text.into(),
            use_markup: false,
            icon: None,
        }
    }

    /// Create a markup-enabled update (no icon).
    pub fn markup(module_id: impl Into<String>, text: impl Into<String>) -> Self {
        Self {
            module_id: module_id.into(),
            text: text.into(),
            use_markup: true,
            icon: None,
        }
    }

    /// Build from a deserialized exec-script line.
    pub fn from_script(module_id: impl Into<String>, line: ScriptLine) -> Self {
        Self {
            module_id: module_id.into(),
            text: line.text,
            use_markup: line.markup,
            icon: line.icon,
        }
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
}

#[cfg(feature = "clock")]
pub mod clock;
#[cfg(feature = "keyboard")]
pub mod keyboard;
#[cfg(feature = "mpris")]
pub mod mpris;
#[cfg(feature = "window")]
pub mod window;
#[cfg(feature = "workspaces")]
pub mod workspaces;
