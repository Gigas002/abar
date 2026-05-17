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
fn ms_until_next_tick_in_range() {
    let ms = ms_until_next_tick();
    assert!((1..=60_000).contains(&ms));
}
