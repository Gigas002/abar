/// Runtime configuration for the window module.
#[derive(Debug, Clone)]
pub struct WindowConfig {
    /// Maximum number of Unicode scalar values to display before appending "…".
    ///
    /// Zero means no limit.
    pub max_length: usize,
    /// Optional exec command (`sh -c <exec>`) whose stdout drives this module.
    pub exec: Option<String>,
}

impl Default for WindowConfig {
    fn default() -> Self {
        Self {
            max_length: 50,
            exec: None,
        }
    }
}

pub use super::truncate_title;

#[cfg(test)]
mod tests;
