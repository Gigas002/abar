/// An update to a module's displayed label sent from a background task.
pub struct ModuleUpdate {
    pub module_id: String,
    pub label: String,
}

/// Per-module runtime configuration passed into the Wayland run loop.
#[derive(Debug, Default)]
pub struct ModuleConfigs {
    #[cfg(feature = "clock")]
    pub clock: Option<clock::ClockConfig>,
}

#[cfg(feature = "clock")]
pub mod clock;
