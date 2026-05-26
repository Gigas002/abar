use serde::Deserialize;

/// JSON record emitted by the keyboard exec script.
#[derive(Deserialize)]
pub struct KeyboardData {
    pub label: String,
}

/// Configuration for the keyboard module passed into the Wayland run loop.
#[derive(Debug, Clone, Default)]
pub struct KeyboardConfig {
    /// Optional exec command (`sh -c <exec>`) whose stdout drives this module.
    pub exec: Option<String>,
}

#[cfg(test)]
mod tests;
