use super::*;

// ---------------------------------------------------------------------------
// parse_layout_names — tested against real keymaps produced by libxkbcommon
// ---------------------------------------------------------------------------

#[test]
fn parse_empty_keymap() {
    assert!(parse_layout_names("").is_empty());
}

#[test]
fn parse_single_layout_us() {
    use xkbcommon::xkb;
    let ctx = xkb::Context::new(xkb::CONTEXT_NO_FLAGS);
    let Some(km) =
        xkb::Keymap::new_from_names(&ctx, "", "", "us", "", None, xkb::KEYMAP_COMPILE_NO_FLAGS)
    else {
        return; // skip if xkb data not available in the test environment
    };
    let names = parse_layout_names(&km.get_as_string(xkb::KEYMAP_FORMAT_TEXT_V1));
    assert_eq!(names.len(), 1);
    assert!(!names[0].is_empty());
}

#[test]
fn parse_two_layouts_us_ru() {
    use xkbcommon::xkb;
    let ctx = xkb::Context::new(xkb::CONTEXT_NO_FLAGS);
    let Some(km) =
        xkb::Keymap::new_from_names(&ctx, "", "", "us,ru", "", None, xkb::KEYMAP_COMPILE_NO_FLAGS)
    else {
        return;
    };
    let names = parse_layout_names(&km.get_as_string(xkb::KEYMAP_FORMAT_TEXT_V1));
    assert_eq!(names.len(), 2);
    assert!(!names[0].is_empty());
    assert!(!names[1].is_empty());
}

// ---------------------------------------------------------------------------
// current_label — pure logic, no compositor needed
// ---------------------------------------------------------------------------

#[test]
fn current_label_prefers_config_over_xkb() {
    let xkb = vec!["English (US)".to_string(), "Russian".to_string()];
    let config = vec!["en".to_string(), "ru".to_string()];
    assert_eq!(current_label(&xkb, &config, 1), "ru");
}

#[test]
fn current_label_falls_back_to_xkb_when_config_empty() {
    let xkb = vec!["English (US)".to_string()];
    assert_eq!(current_label(&xkb, &[], 0), "English (US)");
}

#[test]
fn current_label_falls_back_to_config_when_xkb_empty() {
    let config = vec!["en-US".to_string()];
    assert_eq!(current_label(&[], &config, 0), "en-US");
}

#[test]
fn current_label_unknown_group_returns_question_mark() {
    assert_eq!(current_label(&[], &[], 0), "?");
    assert_eq!(current_label(&["us".to_string()], &[], 5), "?");
}
