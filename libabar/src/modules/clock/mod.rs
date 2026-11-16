use std::sync::atomic::{AtomicUsize, Ordering};

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

/// Milliseconds until the next full second, minimum 1.
///
/// The clock checks its label every second: formats with seconds tick, and
/// after a suspend the time is right within a second (a sleep until the next
/// minute would run on a clock that stops while suspended).
pub fn ms_until_next_second() -> u64 {
    let ms_past = u64::from(Local::now().nanosecond() / 1_000_000).min(999);
    (1_000 - ms_past).max(1)
}

/// The clock's configuration plus which timezone and format are shown,
/// shared by the tick task and the pointer handlers.
#[derive(Debug)]
pub struct ClockState {
    config: ClockConfig,
    timezone: AtomicUsize,
    format: AtomicUsize,
}

impl ClockState {
    pub fn new(config: ClockConfig) -> Self {
        Self {
            config,
            timezone: AtomicUsize::new(0),
            format: AtomicUsize::new(0),
        }
    }

    /// The label for the current time, timezone and format.
    pub fn label(&self) -> String {
        let tz = self
            .config
            .timezones
            .get(self.timezone.load(Ordering::Relaxed))
            .copied();
        let fmt = self
            .config
            .formats
            .get(self.format.load(Ordering::Relaxed))
            .map_or("%H:%M", String::as_str);
        current_label(fmt, tz)
    }

    /// Show the next (or previous) timezone; `false` with fewer than two.
    pub fn cycle_timezone(&self, forward: bool) -> bool {
        cycle(&self.timezone, self.config.timezones.len(), forward)
    }

    /// Show the next format; `false` with fewer than two.
    pub fn cycle_format(&self) -> bool {
        cycle(&self.format, self.config.formats.len(), true)
    }
}

fn cycle(index: &AtomicUsize, len: usize, forward: bool) -> bool {
    if len < 2 {
        return false;
    }
    let cur = index.load(Ordering::Relaxed) % len;
    let next = if forward {
        (cur + 1) % len
    } else {
        (cur + len - 1) % len
    };
    index.store(next, Ordering::Relaxed);
    true
}

#[cfg(test)]
mod tests;
