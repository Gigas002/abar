/// Runtime configuration for the mpris module passed into the Wayland run loop.
#[derive(Debug, Clone)]
pub struct MprisConfig {
    /// Maximum number of Unicode scalar values to display before appending "…".
    ///
    /// Zero means no limit.
    pub max_length: usize,
    /// Optional exec command (`sh -c <exec>`) whose stdout drives this module.
    pub exec: Option<String>,
}

impl Default for MprisConfig {
    fn default() -> Self {
        Self {
            max_length: 0,
            exec: None,
        }
    }
}

#[cfg(test)]
mod tests;
