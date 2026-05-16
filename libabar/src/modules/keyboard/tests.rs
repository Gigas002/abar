use super::*;

const FIXTURE_TWO_LAYOUTS: &str = r#"
xkb_keymap {
xkb_symbols "pc+us+ru(phonetic):2" {
    name[Group1]="English (US)";
    name[Group2]="Russian (Phonetic)";
};
};
"#;

#[test]
fn parse_two_layouts() {
    let names = parse_layout_names(FIXTURE_TWO_LAYOUTS);
    assert_eq!(names, vec!["English (US)", "Russian (Phonetic)"]);
}

#[test]
fn parse_empty_keymap() {
    assert!(parse_layout_names("").is_empty());
}

#[test]
fn parse_single_layout() {
    let keymap = r#"    name[Group1]="us";"#;
    let names = parse_layout_names(keymap);
    assert_eq!(names, vec!["us"]);
}

#[test]
fn parse_ignores_unrelated_lines() {
    let keymap = "xkb_symbols {\n    key <A> { ... };\n    name[Group1]=\"de\";\n};\n";
    assert_eq!(parse_layout_names(keymap), vec!["de"]);
}

#[test]
fn parse_out_of_order_groups() {
    // Should still return in group-number order regardless of line order.
    let keymap = "    name[Group2]=\"ru\";\n    name[Group1]=\"us\";\n";
    assert_eq!(parse_layout_names(keymap), vec!["us", "ru"]);
}

#[test]
fn current_label_prefers_xkb_over_config() {
    let xkb = vec!["English (US)".to_string(), "Russian".to_string()];
    let config = vec!["en".to_string(), "ru".to_string()];
    assert_eq!(current_label(&xkb, &config, 1), "Russian");
}

#[test]
fn current_label_falls_back_to_config() {
    let config = vec!["en-US".to_string()];
    assert_eq!(current_label(&[], &config, 0), "en-US");
}

#[test]
fn current_label_unknown_group_returns_question_mark() {
    assert_eq!(current_label(&[], &[], 0), "?");
    assert_eq!(current_label(&["us".to_string()], &[], 5), "?");
}
