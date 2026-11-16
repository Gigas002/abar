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
    let label = current_label("%H:%M", Some(tz));
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
