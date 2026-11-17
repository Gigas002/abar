use std::sync::atomic::{AtomicUsize, Ordering};

use jiff::fmt::strtime;
use jiff::tz::TimeZone;
use jiff::{Timestamp, ZonedDateTime};

#[derive(Debug, Clone)]
pub struct ClockConfig {
    pub formats: Vec<String>,
    pub timezones: Vec<TimeZone>,
}

impl Default for ClockConfig {
    fn default() -> Self {
        Self {
            formats: vec!["%H:%M".to_string()],
            timezones: vec![],
        }
    }
}

/// Look up an IANA timezone name (in the system's tz database), logging a
/// warning and returning `None` when it is unknown.
pub fn parse_tz(name: &str) -> Option<TimeZone> {
    match TimeZone::get(name) {
        Ok(tz) => Some(tz),
        Err(err) => {
            tracing::warn!(timezone = %name, %err, "unknown timezone, ignoring");
            None
        }
    }
}

/// Format the current time using the strftime-style `fmt` in `tz`, or in
/// the local timezone if `tz` is `None`.
pub fn current_label(fmt: &str, tz: Option<&TimeZone>) -> String {
    let now = match tz {
        Some(tz) => Timestamp::now().to_zoned_datetime(tz.clone()),
        None => ZonedDateTime::now(),
    };
    format_time(fmt, &now)
}

/// `time` formatted with `fmt`; a format jiff cannot use (e.g. an unknown
/// `%` directive) is shown as written, with a warning, instead of failing.
fn format_time(fmt: &str, time: &ZonedDateTime) -> String {
    strtime::format(fmt, time).unwrap_or_else(|err| {
        tracing::warn!(format = %fmt, %err, "invalid clock format");
        fmt.to_owned()
    })
}

/// Milliseconds until the next full second, minimum 1.
///
/// The clock checks its label every second: formats with seconds tick, and
/// after a suspend the time is right within a second (a sleep until the next
/// minute would run on a clock that stops while suspended).
pub fn ms_until_next_second() -> u64 {
    let ms_past = u64::try_from(Timestamp::now().subsec_nanosecond() / 1_000_000)
        .unwrap_or(0)
        .min(999);
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
            .get(self.timezone.load(Ordering::Relaxed));
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
