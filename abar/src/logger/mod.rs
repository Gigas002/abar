use std::io::IsTerminal;

use tracing_subscriber::EnvFilter;

/// Log to stderr (stdout is for output, not diagnostics), with colours only
/// when stderr is a terminal so redirected logs stay plain text.
pub fn init() {
    let filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info"));
    tracing_subscriber::fmt()
        .with_env_filter(filter)
        .with_writer(std::io::stderr)
        .with_ansi(std::io::stderr().is_terminal())
        .init();
}
