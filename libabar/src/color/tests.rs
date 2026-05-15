use super::*;

#[test]
fn parses_opaque_white() {
    assert_eq!(parse_hex_rgba("#FFFFFFFF").unwrap(), [255, 255, 255, 255]);
}

#[test]
fn parses_black_full_alpha() {
    assert_eq!(parse_hex_rgba("#000000FF").unwrap(), [0, 0, 0, 255]);
}

#[test]
fn parses_lowercase() {
    assert_eq!(parse_hex_rgba("#161925ff").unwrap(), [22, 25, 37, 255]);
}

#[test]
fn rejects_rgb_only() {
    assert_eq!(parse_hex_rgba("#161925"), Err(ParseHexRgbaError::InvalidFormat));
}

#[test]
fn rgba_to_bgra_reorders_channels() {
    assert_eq!(rgba_to_bgra([0x16, 0x19, 0x25, 255]), [0x25, 0x19, 0x16, 255]);
}

#[test]
fn parse_hex_to_bgra_matches_manual() {
    assert_eq!(
        parse_hex_rgba_to_bgra("#161925FF").unwrap(),
        rgba_to_bgra(parse_hex_rgba("#161925FF").unwrap()),
    );
}

#[test]
fn rejects_missing_hash() {
    assert_eq!(
        parse_hex_rgba("161925ff"),
        Err(ParseHexRgbaError::InvalidFormat)
    );
}
