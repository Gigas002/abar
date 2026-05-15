use std::process::ExitCode;

use crate::settings::Settings;

pub fn run(settings: Settings) -> ExitCode {
    match libabar::wayland::run_bar(settings.bar) {
        Ok(()) => ExitCode::SUCCESS,
        Err(err) => {
            tracing::error!(%err, "Wayland session ended with an error");
            ExitCode::from(1)
        }
    }
}
