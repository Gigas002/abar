use chrono::{Local, Timelike, Utc};
use chrono_tz::Tz;

#[derive(Debug, Clone)]
pub struct ClockConfig {
    pub formats: Vec<String>,
    pub timezones: Vec<Tz>,
}

impl Default for ClockConfig {
    fn default() -> Self {
        Self {
            formats: vec!["%H:%M".to_string()],
            timezones: vec![],
        }
    }
}

/// Parse an IANA timezone name into a `Tz`, logging a warning and returning `None` on failure.
pub fn parse_tz(name: &str) -> Option<Tz> {
    match name.parse::<Tz>() {
        Ok(tz) => Some(tz),
        Err(_) => {
            tracing::warn!(timezone = %name, "unknown timezone, ignoring");
            None
        }
    }
}

/// Format the current time using `fmt` in `tz`, or local time if `tz` is `None`.
pub fn current_label(fmt: &str, tz: Option<Tz>) -> String {
    match tz {
        Some(tz) => Utc::now().with_timezone(&tz).format(fmt).to_string(),
        None => Local::now().format(fmt).to_string(),
    }
}

/// Milliseconds until the next full minute boundary, minimum 1.
pub fn ms_until_next_tick() -> u64 {
    let now = Local::now();
    let secs_past = now.second() as u64;
    let nanos_past = now.nanosecond() as u64;
    ((60 - secs_past) * 1_000)
        .saturating_sub(nanos_past / 1_000_000)
        .max(1)
}

#[cfg(test)]
mod tests;
