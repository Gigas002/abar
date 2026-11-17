use super::*;

#[test]
fn local_default_format_is_hm() {
    let label = current_label("%H:%M", None);
    assert_eq!(label.len(), 5);
    assert!(label.contains(':'));
}

#[test]
fn local_year_format() {
    let label = current_label("%Y", None);
    assert_eq!(label.len(), 4);
    let year: u32 = label.parse().unwrap();
    assert!(year >= 2024);
}

#[test]
fn tz_format() {
    let tz = parse_tz("Europe/Moscow").unwrap();
    let label = current_label("%H:%M", Some(&tz));
    assert_eq!(label.len(), 5);
    assert!(label.contains(':'));
}

#[test]
fn unknown_tz_returns_none() {
    assert!(parse_tz("Not/ATimezone").is_none());
}

#[test]
fn ms_until_next_second_in_range() {
    let ms = ms_until_next_second();
    assert!((1..=1_000).contains(&ms));
}

#[test]
fn formats_and_timezones_cycle() {
    let state = ClockState::new(ClockConfig {
        formats: vec!["%Y".into(), "fixed".into()],
        timezones: vec![parse_tz("UTC").unwrap(), parse_tz("Asia/Tokyo").unwrap()],
    });
    assert_eq!(state.label().len(), 4);
    assert!(state.cycle_format());
    assert_eq!(state.label(), "fixed");
    assert!(state.cycle_format());
    assert_eq!(state.label().len(), 4, "wraps around");

    let hm = ClockState::new(ClockConfig {
        formats: vec!["%Z".into()],
        timezones: vec![parse_tz("UTC").unwrap(), parse_tz("Asia/Tokyo").unwrap()],
    });
    assert_eq!(hm.label(), "UTC");
    assert!(hm.cycle_timezone(true));
    assert_eq!(hm.label(), "JST");
    assert!(hm.cycle_timezone(false));
    assert_eq!(hm.label(), "UTC");
    assert!(hm.cycle_timezone(false), "backwards wraps too");
    assert_eq!(hm.label(), "JST");
    assert!(!hm.cycle_format(), "a single format does not cycle");
}

/// The directives clock configs use render as they did with chrono.
#[test]
fn strftime_directives_render_as_before() {
    let at: jiff::ZonedDateTime = "2026-03-28T21:35:07[Europe/Moscow]".parse().unwrap();
    for (fmt, want) in [
        ("%R %Z %d.%m.%Y", "21:35 MSK 28.03.2026"),
        ("%H:%M:%S", "21:35:07"),
        ("%H:%M", "21:35"),
        ("%T", "21:35:07"),
        ("%a %d %b", "Sat 28 Mar"),
        ("%A, %B %e", "Saturday, March 28"),
        ("%I:%M %p", "09:35 PM"),
        ("%-d.%-m.%y", "28.3.26"),
        ("%F", "2026-03-28"),
        ("%j", "087"),
        ("%z", "+0300"),
        ("%Y-%m-%d %%", "2026-03-28 %"),
    ] {
        assert_eq!(format_time(fmt, &at), want, "{fmt}");
    }
}

/// chrono panicked on an unknown directive; it is now shown as written.
#[test]
fn an_invalid_format_is_shown_instead_of_panicking() {
    let at: jiff::ZonedDateTime = "2026-03-28T21:35:07[UTC]".parse().unwrap();
    assert_eq!(format_time("%H %J", &at), "%H %J");
    let clock = ClockState::new(ClockConfig {
        formats: vec!["%J".into()],
        timezones: vec![],
    });
    assert_eq!(clock.label(), "%J");
}
