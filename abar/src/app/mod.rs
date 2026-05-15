use std::process::ExitCode;

use crate::settings::Settings;

pub fn run(settings: Settings) -> ExitCode {
    match libabar::wayland::run_layer_strip(settings.background_color) {
        Ok(()) => ExitCode::SUCCESS,
        Err(err) => {
            tracing::error!(%err, "Wayland session ended with an error");
            ExitCode::from(1)
        }
    }
}
