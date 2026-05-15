use std::path::PathBuf;

use clap::Parser;

#[derive(Debug, Parser)]
#[command(version, about = "Wayland status bar")]
pub struct Cli {
    #[arg(long, value_name = "FILE")]
    pub config: Option<PathBuf>,

    #[arg(long, value_name = "FILE")]
    pub theme: Option<PathBuf>,
}

#[cfg(test)]
mod tests;
