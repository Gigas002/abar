use super::*;

#[test]
fn short_title_unchanged() {
    assert_eq!(truncate_title("Hello", 10), "Hello");
}

#[test]
fn exact_length_unchanged() {
    assert_eq!(truncate_title("Hello", 5), "Hello");
}

#[test]
fn long_title_truncated_with_ellipsis() {
    assert_eq!(truncate_title("Hello, world!", 5), "Hello…");
}

#[test]
fn zero_max_length_means_no_limit() {
    let long = "a".repeat(1000);
    assert_eq!(truncate_title(&long, 0), long);
}

#[test]
fn unicode_scalar_values_counted_not_bytes() {
    assert_eq!(truncate_title("日本語テスト", 3), "日本語…");
    assert_eq!(truncate_title("日本語", 3), "日本語");
}

#[test]
fn empty_title_unchanged() {
    assert_eq!(truncate_title("", 10), "");
    assert_eq!(truncate_title("", 0), "");
}
