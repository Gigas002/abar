use std::path::PathBuf;

use clap::Parser;

#[derive(Debug, Parser)]
#[command(version, about = "Wayland status bar")]
pub struct Cli {
    #[arg(
        long,
        value_name = "FILE",
        help = "Path to config.toml (default: XDG …/abar/config.toml)"
    )]
    pub config: Option<PathBuf>,

    #[arg(
        long,
        value_name = "FILE",
        help = "Path to theme.toml (default: resolved from [base].theme)"
    )]
    pub theme: Option<PathBuf>,
}

#[cfg(test)]
mod tests;
